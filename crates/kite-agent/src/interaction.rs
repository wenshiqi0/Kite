//! Live app-server interaction state. Requests are correlated to one thread and turn.
use crate::{Approval, Choice, composer::Error, fingerprint};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Activity {
    #[default]
    Idle,
    Sending,
    Thinking,
    Tool,
    Responding,
    Waiting,
    Failed,
}
#[derive(Clone, Debug, Deserialize)]
pub struct OptionItem {
    pub label: String,
    pub description: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub header: String,
    pub question: String,
    #[serde(default)]
    pub is_other: bool,
    #[serde(default)]
    pub is_secret: bool,
    pub options: Option<Vec<OptionItem>>,
}
#[derive(Clone, Debug)]
pub struct QuestionPrompt {
    pub id: String,
    pub questions: Vec<Question>,
    pub blocking: bool,
}
#[derive(Clone, Debug)]
pub struct Answer {
    pub question_id: String,
    pub value: String,
}
#[derive(Clone)]
struct PendingQuestion {
    raw_id: Value,
    turn: String,
    prompt: QuestionPrompt,
}
#[derive(Clone)]
struct PendingApproval {
    raw_id: Value,
    turn: String,
    approval: Approval,
    decisions: Vec<Value>,
}
#[derive(Clone, Default)]
pub struct RuntimeState {
    pub activity: Activity,
    pub error: Option<String>,
    questions: Vec<PendingQuestion>,
    approval: Option<PendingApproval>,
}
impl RuntimeState {
    pub fn question(&self) -> Option<QuestionPrompt> {
        self.questions.first().map(|q| q.prompt.clone())
    }
    pub fn approval(&self) -> Option<Approval> {
        self.approval.as_ref().map(|a| a.approval.clone())
    }
    pub fn phase(&self) -> Activity {
        if self.questions.iter().any(|q| q.prompt.blocking) || self.approval.is_some() {
            Activity::Waiting
        } else {
            self.activity
        }
    }
    pub fn begin(&mut self) {
        self.activity = Activity::Sending;
        self.error = None;
    }
    pub fn accepted(&mut self) {
        if self.activity == Activity::Sending {
            self.activity = Activity::Thinking;
        }
    }
    pub fn failed(&mut self, details: String) {
        self.activity = Activity::Failed;
        self.error = Some(details);
    }
    pub fn disconnected(&mut self) {
        self.questions.clear();
        self.approval = None;
        self.failed("chat connection closed".into());
    }
    pub fn ingest(&mut self, value: &Value, thread: &str) {
        let Some(method) = value["method"].as_str() else {
            return;
        };
        let p = &value["params"];
        if method == "serverRequest/resolved" {
            if p.get("threadId").is_some() && p["threadId"] != thread {
                return;
            }
            self.questions.retain(|q| q.raw_id != p["requestId"]);
            if self
                .approval
                .as_ref()
                .is_some_and(|a| a.raw_id == p["requestId"])
            {
                self.approval = None;
            }
            return;
        }
        if p["threadId"] != thread {
            return;
        }
        match method {
            "item/tool/requestUserInput" if value.get("id").is_some() => {
                let Ok(questions) = serde_json::from_value::<Vec<Question>>(p["questions"].clone())
                else {
                    return;
                };
                if questions.is_empty()
                    || questions.len() > 12
                    || questions.iter().any(|q| q.id.is_empty())
                {
                    return;
                }
                let identity = json!({"thread":p["threadId"],"turn":p["turnId"],"item":p["itemId"],"questions":p["questions"]});
                let id = format!("question-{}", fingerprint(&identity.to_string()));
                let pending = PendingQuestion {
                    raw_id: value["id"].clone(),
                    turn: p["turnId"].as_str().unwrap_or_default().into(),
                    prompt: QuestionPrompt {
                        id: id.clone(),
                        questions,
                        blocking: p["isBlocking"].as_bool().unwrap_or(true),
                    },
                };
                if let Some(previous) = self.questions.iter_mut().find(|q| q.prompt.id == id) {
                    *previous = pending;
                } else if self.questions.len() < 8 {
                    self.questions.push(pending);
                }
            }
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
                if value.get("id").is_some() =>
            {
                let offered = p["availableDecisions"].as_array();
                let decisions: Vec<_> = ["accept", "decline"]
                    .into_iter()
                    .filter(|d| offered.is_none_or(|a| a.contains(&json!(d))))
                    .map(|d| json!(d))
                    .collect();
                if decisions.is_empty() {
                    return;
                }
                let prompt = [
                    p["reason"].as_str(),
                    p["command"].as_str(),
                    p["cwd"].as_str(),
                    p["grantRoot"].as_str(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("\n");
                // File-change requests lacking a concrete description stay in the TUI.
                if prompt.trim().is_empty() {
                    return;
                }
                self.approval = Some(PendingApproval {
                    raw_id: value["id"].clone(),
                    turn: p["turnId"].as_str().unwrap_or_default().into(),
                    approval: Approval {
                        id: format!("rpc-{}", fingerprint(&value.to_string())),
                        prompt,
                        choices: decisions
                            .iter()
                            .map(|d| Choice {
                                label: if d == "accept" { "Yes" } else { "No" }.into(),
                                keys: vec![],
                            })
                            .collect(),
                    },
                    decisions,
                });
            }
            "turn/started" => {
                self.activity = Activity::Thinking;
                self.error = None;
            }
            "turn/completed" => {
                let turn = p["turn"]["id"].as_str().unwrap_or_default();
                let terminal_failure =
                    p["turn"]["status"] == "failed" || p["turn"]["status"] == "interrupted";
                self.questions
                    .retain(|q| q.turn != turn || (!q.prompt.blocking && !terminal_failure));
                if self.approval.as_ref().is_some_and(|a| a.turn == turn) {
                    self.approval = None;
                }
                self.activity = if p["turn"]["status"] == "failed" {
                    Activity::Failed
                } else {
                    Activity::Idle
                };
                self.error = p["turn"]["error"]["message"].as_str().map(str::to_owned);
            }
            "item/started" => match p["item"]["type"].as_str() {
                Some("reasoning") => self.activity = Activity::Thinking,
                Some(
                    "commandExecution" | "fileChange" | "mcpToolCall" | "dynamicToolCall"
                    | "webSearch" | "imageView",
                ) => self.activity = Activity::Tool,
                Some("agentMessage") => self.activity = Activity::Responding,
                _ => {}
            },
            "item/agentMessage/delta" => self.activity = Activity::Responding,
            "thread/status/changed" => {
                if p["status"]["type"] == "idle" || p["status"]["type"] == "notLoaded" {
                    self.activity = Activity::Idle;
                } else if p["status"]["type"] == "active" && self.activity == Activity::Idle {
                    self.activity = Activity::Thinking;
                }
            }
            "error" if p["willRetry"] != true => {
                self.failed(
                    p["error"]["message"]
                        .as_str()
                        .unwrap_or("agent turn failed")
                        .into(),
                );
            }
            _ => {}
        }
    }
    pub fn question_reply(&self, id: &str, answers: &[Answer]) -> Result<Value, Error> {
        let pending = self
            .questions
            .iter()
            .find(|q| q.prompt.id == id)
            .ok_or(Error::Invalid("question expired"))?;
        if answers.len() != pending.prompt.questions.len() {
            return Err(Error::Invalid("answer every question"));
        }
        let mut values = serde_json::Map::new();
        for question in &pending.prompt.questions {
            let matching: Vec<_> = answers
                .iter()
                .filter(|a| a.question_id == question.id)
                .collect();
            if matching.len() != 1 {
                return Err(Error::Invalid("question answer mismatch"));
            }
            let answer = if question.is_secret {
                matching[0].value.as_str()
            } else {
                matching[0].value.trim()
            };
            if answer.is_empty() || answer.len() > 16384 {
                return Err(Error::Invalid("answer length"));
            }
            if !question.is_other
                && question.options.as_ref().is_some_and(|options| {
                    !options.is_empty() && !options.iter().any(|o| o.label == answer)
                })
            {
                return Err(Error::Invalid("answer must be an offered option"));
            }
            values.insert(question.id.clone(), json!({"answers":[answer]}));
        }
        Ok(json!({"id":pending.raw_id,"result":{"answers":values}}))
    }
    pub fn approval_reply(&self, id: &str, choice: u32) -> Result<Value, Error> {
        let pending = self
            .approval
            .as_ref()
            .filter(|a| a.approval.id == id)
            .ok_or(Error::Invalid("approval expired"))?;
        let decision = pending
            .decisions
            .get(choice as usize)
            .ok_or(Error::Invalid("approval choice"))?;
        Ok(json!({"id":pending.raw_id,"result":{"decision":decision}}))
    }
    pub fn answered(&mut self, id: &str) {
        self.questions.retain(|q| q.prompt.id != id);
        if self.approval.as_ref().is_some_and(|a| a.approval.id == id) {
            self.approval = None;
        }
        if !matches!(self.activity, Activity::Idle | Activity::Failed) {
            self.activity = Activity::Thinking;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> Value {
        json!({"id":7,"method":"item/tool/requestUserInput","params":{"threadId":"t","turnId":"turn","isBlocking":true,"questions":[{"id":"colour","header":"Colour","question":"Which colour?","isOther":true,"options":[{"label":"Blue","description":"Blue"},{"label":"Green","description":"Green"}]}]}})
    }
    #[test]
    fn model_questions_accept_choices_or_free_text_and_expire_on_resolution() {
        let mut state = RuntimeState::default();
        state.ingest(&request(), "t");
        let prompt = state.question().unwrap();
        assert_eq!(state.phase(), Activity::Waiting);
        let reply = state
            .question_reply(
                &prompt.id,
                &[Answer {
                    question_id: "colour".into(),
                    value: "Custom colour".into(),
                }],
            )
            .unwrap();
        assert_eq!(
            reply["result"]["answers"]["colour"]["answers"][0],
            "Custom colour"
        );
        assert_eq!(reply["id"], 7);
        state.ingest(
            &json!({"method":"serverRequest/resolved","params":{"requestId":7}}),
            "t",
        );
        assert!(state.question().is_none());
        assert!(
            state
                .question_reply(
                    &prompt.id,
                    &[Answer {
                        question_id: "colour".into(),
                        value: "Blue".into()
                    }]
                )
                .is_err()
        );
    }
    #[test]
    fn requests_and_answers_cannot_cross_threads_or_question_ids() {
        let mut state = RuntimeState::default();
        state.ingest(&request(), "other");
        assert!(state.question().is_none());
        state.ingest(&request(), "t");
        let id = state.question().unwrap().id;
        assert!(state.question_reply(&id, &[]).is_err());
        assert!(
            state
                .question_reply(
                    &id,
                    &[Answer {
                        question_id: "wrong".into(),
                        value: "Blue".into()
                    }]
                )
                .is_err()
        );
    }
    #[test]
    fn thinking_stops_on_completion_failure_and_waiting() {
        let mut state = RuntimeState::default();
        state.begin();
        assert_eq!(state.phase(), Activity::Sending);
        state.accepted();
        assert_eq!(state.phase(), Activity::Thinking);
        state.ingest(&request(), "t");
        assert_eq!(state.phase(), Activity::Waiting);
        state.ingest(&json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"turn","status":"completed"}}}),"t");
        assert_eq!(state.phase(), Activity::Idle);
        assert!(state.question().is_none());
        state.failed("offline".into());
        assert_eq!(state.phase(), Activity::Failed);
    }
    #[test]
    fn restricted_approval_never_exposes_an_unoffered_accept() {
        let mut state = RuntimeState::default();
        state.ingest(&json!({"id":8,"method":"item/commandExecution/requestApproval","params":{"threadId":"t","turnId":"turn","command":"python3 task.py","availableDecisions":["decline","cancel"]}}),"t");
        let approval = state.approval().unwrap();
        assert_eq!(approval.choices.len(), 1);
        assert_eq!(approval.choices[0].label, "No");
        assert_eq!(
            state.approval_reply(&approval.id, 0).unwrap()["result"]["decision"],
            "decline"
        );
    }

    #[test]
    fn replayed_question_keeps_its_form_identity_and_nonblocking_questions_remain_answerable() {
        let mut value = request();
        value["params"]["isBlocking"] = json!(false);
        let mut state = RuntimeState::default();
        state.ingest(&value, "t");
        let id = state.question().unwrap().id;
        value["id"] = json!(99);
        state.ingest(&value, "t");
        assert_eq!(state.question().unwrap().id, id);
        state.ingest(&json!({"method":"turn/completed","params":{"threadId":"t","turn":{"id":"turn","status":"completed"}}}),"t");
        assert_eq!(state.phase(), Activity::Idle);
        let reply = state
            .question_reply(
                &id,
                &[Answer {
                    question_id: "colour".into(),
                    value: "Blue".into(),
                }],
            )
            .unwrap();
        assert_eq!(reply["id"], 99);
    }
}
