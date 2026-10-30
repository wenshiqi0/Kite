//! Nonblocking Codex questions use the TUI's correlated user-message envelope.
use crate::{
    codex_message_text, fingerprint,
    interaction::{OptionItem, Question, QuestionPrompt},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct Pending {
    pub turn: String,
    pub prompt: QuestionPrompt,
}
#[derive(Clone, Debug, Default)]
pub struct History {
    pub pending: Vec<Pending>,
    pub answered: Vec<String>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub question_item_id: String,
    pub question: String,
    pub answer: String,
}

pub fn replies(text: &str) -> Option<Vec<Reply>> {
    let body = text
        .trim()
        .strip_prefix("<send_user_message_question_reply>")?
        .strip_suffix("</send_user_message_question_reply>")?
        .trim();
    let values: Vec<Reply> = serde_json::from_str(body).ok()?;
    if values.is_empty()
        || values.len() > 12
        || values.iter().any(|r| {
            let id = serde_json::from_str::<Value>(&r.question_item_id).unwrap_or(Value::Null);
            id.as_array().is_none_or(|a| a.len() != 3)
                || id[0] != "request_user_input_async"
                || id[1].as_str().is_none_or(str::is_empty)
                || id[2].as_u64().is_none()
                || r.question.len() > 16384
                || r.answer.len() > 16384
        })
    {
        return None;
    }
    Some(values)
}

pub fn from_item(thread: &str, turn: &str, item: &Value) -> Option<Pending> {
    if item["type"] != "agentMessage" || item["delivery"] != "async" {
        return None;
    }
    from_questions(thread, turn, item["id"].as_str()?, &item["questions"])
}

fn from_questions(thread: &str, turn: &str, call: &str, value: &Value) -> Option<Pending> {
    let fields = value.as_array()?;
    if call.is_empty() || call.len() > 256 || fields.is_empty() || fields.len() > 12 {
        return None;
    }
    let mut questions = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let title = field["title"].as_str()?;
        if title.trim().is_empty() || title.len() > 16384 {
            return None;
        }
        let options = match field.get("options").filter(|v| !v.is_null()) {
            Some(value) => {
                let values = value.as_array()?;
                if values.len() > 32 {
                    return None;
                }
                let mut options = Vec::new();
                for value in values {
                    let label = value.as_str()?;
                    if label.trim().is_empty() || label.len() > 2048 {
                        return None;
                    }
                    options.push(OptionItem {
                        label: label.into(),
                        description: String::new(),
                    });
                }
                Some(options)
            }
            None => None,
        };
        questions.push(Question {
            id: json!(["request_user_input_async", call, index]).to_string(),
            header: format!("Q{}", index + 1),
            question: title.into(),
            options,
            is_other: true,
            is_secret: false,
        });
    }
    Some(Pending {
        turn: turn.into(),
        prompt: QuestionPrompt {
            id: format!("async-{}", fingerprint(&json!([thread, call]).to_string())),
            questions,
            blocking: false,
        },
    })
}

/// Rebuild unanswered forms from bounded provider history, including after reconnect.
pub fn history(thread: &str, bytes: &[u8]) -> History {
    let mut pending = Vec::<(String, Pending)>::new();
    let mut accepted = Vec::new();
    let mut answered = Vec::new();
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if v["type"] != "response_item" {
            continue;
        }
        let p = &v["payload"];
        match p["type"].as_str() {
            Some("function_call") if p["name"] == "request_user_input_async" => {
                let Some(call) = p["call_id"].as_str() else {
                    continue;
                };
                let args = p["arguments"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .unwrap_or(Value::Null);
                let turn = p["internal_chat_message_metadata_passthrough"]["turn_id"]
                    .as_str()
                    .unwrap_or_default();
                if let Some(question) = from_questions(thread, turn, call, &args["questions"]) {
                    if let Some(existing) = pending.iter_mut().find(|(id, _)| id == call) {
                        existing.1 = question;
                    } else {
                        pending.push((call.into(), question));
                    }
                }
            }
            Some("function_call_output") => {
                let output = p["output"]
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok());
                if output.is_some_and(|v| v["accepted"] == true)
                    && let Some(call) = p["call_id"].as_str()
                {
                    accepted.push(call.to_owned());
                }
            }
            Some("message") if p["role"] == "user" => {
                let text = codex_message_text(&p["content"]);
                if text.starts_with("<environment_context>") || text.starts_with("<turn_aborted>") {
                    continue;
                }
                if let Some(replies) = replies(&text) {
                    answered.extend(replies.into_iter().map(|r| r.question_item_id));
                } else {
                    // A new ordinary user submission supersedes
                    // earlier queued follow-up questions. Correlated replies
                    // above resolve only the specifically answered fields.
                    answered.extend(
                        pending
                            .iter()
                            .flat_map(|(_, p)| p.prompt.questions.iter().map(|q| q.id.clone())),
                    );
                    pending.clear();
                }
            }
            _ => {}
        }
    }
    pending.retain(|(id, _)| accepted.contains(id));
    let mut result = History {
        pending: pending.into_iter().map(|(_, p)| p).collect(),
        answered,
    };
    for pending in &mut result.pending {
        pending
            .prompt
            .questions
            .retain(|q| !result.answered.contains(&q.id));
    }
    result.pending.retain(|p| !p.prompt.questions.is_empty());
    result.pending.truncate(8);
    result.answered = result.answered.into_iter().rev().take(2048).collect();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{Answer, RuntimeState};

    fn fixture() -> Vec<Value> {
        vec![
            json!({"type":"response_item","payload":{"type":"function_call","name":"request_user_input_async","call_id":"call_async","arguments":json!({"questions":[{"title":"Choose a colour","options":["Blue","Green"]},{"title":"Add a note"}]}).to_string(),"internal_chat_message_metadata_passthrough":{"turn_id":"turn"}}}),
            json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call_async","output":"{\"accepted\":true}"}}),
        ]
    }
    fn bytes(rows: &[Value]) -> Vec<u8> {
        rows.iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            .into_bytes()
    }
    fn answer(state: &RuntimeState) -> (String, Vec<Answer>) {
        let q = state.question().unwrap();
        let answers = q
            .questions
            .iter()
            .enumerate()
            .map(|(i, q)| Answer {
                question_id: q.id.clone(),
                value: if i == 0 {
                    "Green"
                } else {
                    "来自手机的说明"
                }
                .into(),
            })
            .collect();
        (q.id, answers)
    }

    #[test]
    fn accepted_async_forms_recover_without_a_live_rpc_and_normalize_answer_history() {
        let mut rows = fixture();
        let h = history("thread", &bytes(&rows));
        assert_eq!(h.pending.len(), 1);
        let mut state = RuntimeState::default();
        state.reconcile_async(&h);
        let prompt = state.question().unwrap();
        assert!(!prompt.blocking);
        assert_eq!(
            prompt.questions[0].options.as_ref().unwrap()[0].label,
            "Blue"
        );
        assert!(prompt.questions[0].is_other);
        let (id, answers) = answer(&state);
        assert!(state.question_reply(&id, &answers).is_err());
        let text = state.async_reply(&id, &answers).unwrap().unwrap();
        let parsed = replies(&text).unwrap();
        assert_eq!(
            parsed[0].question_item_id,
            json!(["request_user_input_async", "call_async", 0]).to_string()
        );
        assert_eq!(parsed[1].answer, "来自手机的说明");
        rows.push(json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":text}]}}));
        let recovered = history("thread", &bytes(&rows));
        assert!(recovered.pending.is_empty());
        let messages = crate::parse_history("codex", &bytes(&rows));
        assert!(messages.iter().any(|m| m.role == crate::Role::User
            && m.text.contains("Green")
            && m.text.contains("来自手机的说明")
            && !m.text.contains("send_user_message_question_reply")));
    }

    #[test]
    fn asynchronous_answer_survives_completion_and_does_not_reappear_from_a_stale_snapshot() {
        let h = history("thread", &bytes(&fixture()));
        let mut state = RuntimeState::default();
        state.reconcile_async(&h);
        let original = state.question().unwrap().id;
        state.ingest(&json!({"method":"turn/completed","params":{"threadId":"thread","turn":{"id":"turn","status":"completed"}}}),"thread");
        assert_eq!(state.question().unwrap().id, original);
        let (id, answers) = answer(&state);
        state.async_reply(&id, &answers).unwrap();
        // Preparing or failing a request cannot consume it; only acknowledgement does.
        assert!(state.question().is_some());
        state.answered(&id);
        state.reconcile_async(&h);
        assert!(state.question().is_none());
        assert!(state.async_reply(&id, &answers).is_err());
    }

    #[test]
    fn live_questions_and_history_use_one_identity_and_blocking_rpc_has_priority() {
        let h = history("thread", &bytes(&fixture()));
        let mut state = RuntimeState::default();
        state.ingest(&json!({"method":"item/completed","params":{"threadId":"thread","turnId":"turn","item":{"type":"agentMessage","id":"call_async","delivery":"async","questions":[{"title":"Choose a colour","options":["Blue","Green"]},{"title":"Add a note"}]}}}),"thread");
        assert_eq!(state.question().unwrap().id, h.pending[0].prompt.id);
        state.reconcile_async(&h);
        state.ingest(&json!({"id":88,"method":"item/tool/requestUserInput","params":{"threadId":"thread","turnId":"turn","questions":[{"id":"native","header":"Native","question":"Native choice?","options":null}]}}),"thread");
        let q = state.question().unwrap();
        assert!(q.blocking);
        let reply = state
            .question_reply(
                &q.id,
                &[Answer {
                    question_id: "native".into(),
                    value: "Yes".into(),
                }],
            )
            .unwrap();
        assert_eq!(reply["id"], 88);
        state.answered(&q.id);
        assert_eq!(state.question().unwrap().id, h.pending[0].prompt.id);
        assert_ne!(
            history("other-thread", &bytes(&fixture())).pending[0]
                .prompt
                .id,
            h.pending[0].prompt.id
        );
    }

    #[test]
    fn incomplete_or_malformed_questions_and_wrong_answers_are_not_submitted() {
        assert!(
            history("thread", &bytes(&fixture()[..1]))
                .pending
                .is_empty()
        );
        let mut state = RuntimeState::default();
        state.reconcile_async(&history("thread", &bytes(&fixture())));
        let (id, mut answers) = answer(&state);
        answers[0].question_id = "wrong".into();
        assert!(state.async_reply(&id, &answers).is_err());
        answers[0].question_id = answers[1].question_id.clone();
        assert!(state.async_reply(&id, &answers).is_err());
        assert!(state.async_reply(&id, &[]).is_err());
        assert!(replies("<send_user_message_question_reply>[{\"questionItemId\":\"[1,2,3]\",\"question\":\"q\",\"answer\":\"a\"}]</send_user_message_question_reply>").is_none());
        let mut rows = fixture();
        rows.push(json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"<send_user_message_question_reply>[]</send_user_message_question_reply>"}]}}));
        assert_eq!(history("thread", &bytes(&rows)).pending.len(), 1);
    }

    #[test]
    fn new_user_input_supersedes_old_questions_but_correlated_replies_only_resolve_their_fields() {
        let original = fixture();
        let mut context = original.clone();
        context.push(json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"<environment_context>metadata</environment_context>"}]}}));
        assert_eq!(history("thread", &bytes(&context)).pending.len(), 1);
        let old_id = history("thread", &bytes(&original)).pending[0]
            .prompt
            .questions[0]
            .id
            .clone();
        let mut rows = original.clone();
        rows.push(
            json!({"type":"response_item","payload":{"type":"message","role":"user","content":[]}}),
        );
        let h = history("thread", &bytes(&rows));
        assert!(h.pending.is_empty());
        assert!(h.answered.contains(&old_id));
        let mut newer = fixture();
        newer[0]["payload"]["call_id"] = json!("new-question");
        newer[1]["payload"]["call_id"] = json!("new-question");
        rows.extend(newer);
        let h = history("thread", &bytes(&rows));
        assert_eq!(h.pending.len(), 1);
        assert!(h.pending[0].prompt.questions[0].id.contains("new-question"));
        let first_id = h.pending[0].prompt.questions[0].id.clone();
        let reply = format!(
            "<send_user_message_question_reply>\n{}\n</send_user_message_question_reply>",
            json!([{"questionItemId":first_id,"question":"Choose a colour","answer":"Blue"}])
        );
        rows.push(json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":reply}]}}));
        let h = history("thread", &bytes(&rows));
        assert_eq!(h.pending.len(), 1);
        assert_eq!(h.pending[0].prompt.questions.len(), 1);
        assert_eq!(h.pending[0].prompt.questions[0].header, "Q2");
    }
}
