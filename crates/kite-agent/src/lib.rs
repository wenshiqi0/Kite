//! Provider conversation records and explicit terminal choices. No UI or FFI.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod async_input;
pub mod composer;
pub mod history;
pub mod interaction;
pub mod notifications;
mod presentation;
pub mod process;
pub mod providers;
pub mod sessions;
pub mod usage;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
    Tool,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub arguments: String,
    pub output: String,
    pub complete: bool,
    pub failed: bool,
    pub computer: bool,
}
impl Tool {
    pub fn label(&self) -> String {
        if self.computer {
            "computer".into()
        } else if matches!(
            self.name.as_str(),
            "Bash" | "exec_command" | "shell_command" | "shell"
        ) {
            "Bash".into()
        } else {
            self.name.clone()
        }
    }
    pub fn summary(&self) -> String {
        let value = serde_json::from_str::<Value>(&self.arguments).ok();
        let text = value
            .as_ref()
            .and_then(|v| {
                ["command", "cmd", "description", "action", "text"]
                    .into_iter()
                    .find_map(|key| v[key].as_str())
                    .or_else(|| {
                        v["questions"]
                            .as_array()
                            .and_then(|q| q.first())
                            .and_then(|q| q["question"].as_str())
                    })
            })
            .unwrap_or_else(|| {
                if self.computer {
                    self.name.rsplit("__").next().unwrap_or(&self.name)
                } else {
                    self.arguments.lines().next().unwrap_or_default()
                }
            });
        text.lines()
            .next()
            .unwrap_or_default()
            .chars()
            .take(120)
            .collect()
    }
}

/// A submitted answer keeps its originating question across history and reconnects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionReply {
    pub id: String,
    pub question: String,
    pub answer: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub text: String,
    pub intermediate: bool,
    pub tool: Option<Tool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub question_replies: Vec<QuestionReply>,
}

impl Message {
    /// Keep original records for paging/cache merges, but expose user-facing
    /// messages and report bodies to native clients instead of machine receipts.
    pub fn presentation(&self) -> std::borrow::Cow<'_, Self> {
        if self.role == Role::Assistant
            && !self.intermediate
            && self.tool.is_none()
            && let Some(text) = presentation::report_markdown(&self.text)
        {
            return std::borrow::Cow::Owned(Self {
                text,
                ..self.clone()
            });
        }
        let Some(tool) = &self.tool else {
            return std::borrow::Cow::Borrowed(self);
        };
        if tool.name != "send_message_to_user_async" || tool.failed {
            return std::borrow::Cow::Borrowed(self);
        }
        let Ok(arguments) = serde_json::from_str::<Value>(&tool.arguments) else {
            return std::borrow::Cow::Borrowed(self);
        };
        let Some(text) = arguments["message"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
        else {
            return std::borrow::Cow::Borrowed(self);
        };
        std::borrow::Cow::Owned(Self {
            id: self.id.clone(),
            role: Role::Assistant,
            text: text.into(),
            intermediate: false,
            tool: None,
            question_replies: Vec::new(),
        })
    }
}

pub fn fingerprint(text: &str) -> String {
    hex::encode(openssl::sha::sha256(text.as_bytes()))
}

fn computer(name: &str) -> bool {
    let name = name.to_lowercase();
    name == "computer"
        || name == "computer_call"
        || name == "browser"
        || name.starts_with("mcp__tactero")
        || name.starts_with("mcp__playwright")
        || name.starts_with("mcp__computer")
        || name.starts_with("mcp__browser")
}

fn text_content(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_owned();
    }
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn codex_message_text(v: &Value) -> String {
    let Some(blocks) = v.as_array() else {
        return text_content(v);
    };
    let mut text = Vec::new();
    let mut i = 0;
    while i < blocks.len() {
        // Codex persists an image wrapper around each native image input. It is
        // transport metadata, not part of the user's text or attachment manifest.
        if blocks[i]["text"]
            .as_str()
            .is_some_and(|s| s.starts_with("<image name=[Image #") && s.ends_with('>'))
            && blocks
                .get(i + 1)
                .is_some_and(|b| b["type"] == "input_image")
            && blocks.get(i + 2).is_some_and(|b| b["text"] == "</image>")
        {
            i += 3;
            continue;
        }
        if let Some(value) = blocks[i]["text"].as_str() {
            text.push(value);
        }
        i += 1;
    }
    text.join("\n")
}

fn add_text(messages: &mut Vec<Message>, id: String, role: Role, text: String) {
    if text.trim().is_empty() && role != Role::Error {
        return;
    }
    if let Some(m) = messages.iter_mut().find(|m| m.id == id) {
        m.text = text;
        return;
    }
    messages.push(Message {
        id,
        role,
        text,
        intermediate: false,
        question_replies: Vec::new(),
        tool: None,
    });
}

fn add_tool(messages: &mut Vec<Message>, id: String, name: &str, arguments: String) {
    if messages.iter().any(|m| m.id == id) {
        return;
    }
    messages.push(Message {
        id,
        role: Role::Tool,
        text: String::new(),
        intermediate: false,
        question_replies: Vec::new(),
        tool: Some(Tool {
            name: name.into(),
            arguments,
            output: String::new(),
            complete: false,
            failed: false,
            computer: computer(name),
        }),
    });
}

fn finish_tool(messages: &mut Vec<Message>, id: &str, output: String, failed: bool) {
    if !messages.iter().any(|m| m.id == id) {
        add_tool(messages, id.into(), "tool", String::new());
    }
    if let Some(tool) = messages
        .iter_mut()
        .find(|m| m.id == id)
        .and_then(|m| m.tool.as_mut())
    {
        tool.output = output;
        tool.complete = true;
        tool.failed = failed;
    }
}

pub fn question_reply_text(replies: &[QuestionReply]) -> String {
    replies
        .iter()
        .map(|r| format!("{}\n{}", r.question, r.answer))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn model_question_answer(
    messages: &[Message],
    call: &str,
    output: &str,
) -> Option<Vec<QuestionReply>> {
    let tool = messages.iter().find(|m| m.id == call)?.tool.as_ref()?;
    if !matches!(
        tool.name.as_str(),
        "request_user_input" | "request_user_input_async"
    ) {
        return None;
    }
    let arguments: Value = serde_json::from_str(&tool.arguments).ok()?;
    let result: Value = serde_json::from_str(output).ok()?;
    let mut replies = Vec::new();
    for question in arguments["questions"].as_array()? {
        let id = question["id"].as_str()?;
        let values = result["answers"][id]["answers"].as_array()?;
        let text = if question["isSecret"] == true || question["is_secret"] == true {
            "••••".into()
        } else {
            values
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let label = question["header"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(id);
        replies.push(QuestionReply {
            id: format!("{call}/{id}"),
            question: question["question"]
                .as_str()
                .filter(|q| !q.is_empty())
                .unwrap_or(label)
                .into(),
            answer: text,
        });
    }
    if replies.is_empty() {
        None
    } else {
        Some(replies)
    }
}

/// Terminal errors are records in their original turn, independent of page boundaries.
fn turn_error_id(record: &Value) -> Option<String> {
    let payload = &record["payload"];
    if record["type"] != "event_msg"
        || !matches!(
            payload["type"].as_str(),
            Some("task_complete" | "task_aborted")
        )
        || payload.get("error").is_none_or(Value::is_null)
    {
        return None;
    }
    let turn = payload["turn_id"].as_str()?;
    (!turn.is_empty() && turn.len() <= 128).then(|| format!("turn-error-{turn}"))
}

/// Parse bounded JSONL records. Partial writes and non-conversation metadata are ignored.
pub fn parse_history(provider: &str, bytes: &[u8]) -> Vec<Message> {
    let mut messages = Vec::new();
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(mut v) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        if let Some(record) = v.as_object_mut() {
            record.remove("_kite_offset");
        }
        let id = v["uuid"]
            .as_str()
            .or_else(|| v["id"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| fingerprint(&v.to_string()));
        match provider {
            "claude" => {
                let kind = v["type"].as_str().unwrap_or_default();
                if !matches!(kind, "user" | "assistant") {
                    continue;
                }
                let body = &v["message"]["content"];
                let role = if kind == "user" {
                    Role::User
                } else {
                    Role::Assistant
                };
                add_text(&mut messages, id.clone(), role, text_content(body));
                if kind == "assistant"
                    && body
                        .as_array()
                        .is_some_and(|blocks| blocks.iter().any(|b| b["type"] == "tool_use"))
                    && let Some(message) = messages.iter_mut().find(|m| m.id == id)
                {
                    message.intermediate = true;
                }
                if let Some(blocks) = body.as_array() {
                    for block in blocks {
                        match block["type"].as_str() {
                            Some("tool_use") => {
                                if let (Some(call), Some(name)) =
                                    (block["id"].as_str(), block["name"].as_str())
                                {
                                    add_tool(
                                        &mut messages,
                                        call.into(),
                                        name,
                                        serde_json::to_string_pretty(&block["input"])
                                            .unwrap_or_default(),
                                    );
                                }
                            }
                            Some("tool_result") => {
                                if let Some(call) = block["tool_use_id"].as_str() {
                                    finish_tool(
                                        &mut messages,
                                        call,
                                        text_content(&block["content"]),
                                        block["is_error"].as_bool().unwrap_or(false),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "codex" | "dsh" | "ply" => {
                if let Some(id) = turn_error_id(&v) {
                    add_text(
                        &mut messages,
                        id,
                        Role::Error,
                        interaction::error_details(&v["payload"]["error"]).unwrap_or_default(),
                    );
                    continue;
                }
                if v["type"] != "response_item" {
                    continue;
                }
                let p = &v["payload"];
                match p["type"].as_str() {
                    Some("message") => {
                        let role = match p["role"].as_str() {
                            Some("user") => Role::User,
                            Some("assistant") => Role::Assistant,
                            _ => continue,
                        };
                        let text = codex_message_text(&p["content"]);
                        let replies = if role == Role::User {
                            async_input::replies(&text)
                                .map(|replies| {
                                    replies
                                        .into_iter()
                                        .map(|r| QuestionReply {
                                            id: r.question_item_id,
                                            question: r.question,
                                            answer: r.answer,
                                        })
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        let text = if replies.is_empty() {
                            text
                        } else {
                            question_reply_text(&replies)
                        };
                        if text.starts_with("<environment_context>")
                            || text.starts_with("<turn_aborted>")
                            // Codex injects the explicitly selected skill body as
                            // user-role context; keep the actual $name prompt visible.
                            || (role == Role::User
                                && text.starts_with("<skill>\n<name>")
                                && text.contains("</name>\n<path>")
                                && text.trim_end().ends_with("</skill>"))
                        {
                            continue;
                        }
                        let message_id = p["id"].as_str().unwrap_or(&id).to_owned();
                        add_text(&mut messages, message_id.clone(), role, text);
                        if let Some(message) = messages.iter_mut().find(|m| m.id == message_id) {
                            message.question_replies = replies;
                            if message.role == Role::Assistant {
                                message.intermediate =
                                    p["phase"] == "commentary" || p["channel"] == "commentary";
                            }
                        }
                    }
                    Some("image_generation_call") => {
                        let image_id = p["id"].as_str().unwrap_or(&id).to_owned();
                        let files = p["_kite_images"]
                            .as_array()
                            .and_then(|_| {
                                serde_json::from_value::<Vec<composer::Attachment>>(
                                    p["_kite_images"].clone(),
                                )
                                .ok()
                            })
                            .unwrap_or_default();
                        let label = if !files.is_empty() {
                            ""
                        } else if p["_kite_image_error"].is_string() {
                            "图片加载失败，请重试同步"
                        } else if p["status"] == "failed" {
                            "图片生成失败"
                        } else if p["status"] == "completed" {
                            "生成图片暂不可用"
                        } else {
                            "正在生成图片…"
                        };
                        let text =
                            composer::compose_text(label, &files).unwrap_or_else(|_| label.into());
                        add_text(&mut messages, image_id, Role::Assistant, text);
                    }
                    Some("function_call" | "custom_tool_call" | "computer_call") => {
                        if let Some(call) = p["call_id"].as_str() {
                            let name = p["name"].as_str().unwrap_or("computer_call");
                            let args = p["arguments"]
                                .as_str()
                                .or_else(|| p["input"].as_str())
                                .map(str::to_owned)
                                .unwrap_or_else(|| p["action"].to_string());
                            add_tool(&mut messages, call.into(), name, args);
                        }
                    }
                    Some(
                        "function_call_output" | "custom_tool_call_output" | "computer_call_output",
                    ) => {
                        if let Some(call) = p["call_id"].as_str() {
                            let output = text_content(&p["output"]);
                            let answer = model_question_answer(&messages, call, &output);
                            finish_tool(
                                &mut messages,
                                call,
                                output,
                                p["failed"].as_bool().unwrap_or(false),
                            );
                            if let Some(answer) = answer {
                                add_text(
                                    &mut messages,
                                    format!("{call}-answer"),
                                    Role::User,
                                    question_reply_text(&answer),
                                );
                                if let Some(message) = messages
                                    .iter_mut()
                                    .find(|m| m.id == format!("{call}-answer"))
                                {
                                    message.question_replies = answer;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    messages
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    pub keys: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub id: String,
    pub prompt: String,
    pub explanation: Option<String>,
    pub command: Option<String>,
    pub working_directory: Option<String>,
    pub operation: Option<String>,
    pub details: Vec<ApprovalDetail>,
    pub choices: Vec<Choice>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalDetail {
    pub name: String,
    pub value: String,
}

/// Only expose numbered choices from an actually blocked terminal. Never invent approval semantics.
pub fn terminal_choices(blocked: bool, provider: &str, text: &str) -> Option<Approval> {
    if !blocked || !matches!(provider, "claude" | "codex") {
        return None;
    }
    let mut choices = Vec::new();
    let mut selected = None;
    for line in text.lines() {
        let line = line.trim();
        let marked = line.starts_with(['❯', '›']);
        let line = line.trim_start_matches(['❯', '›']).trim();
        let Some((number, label)) = line.split_once(". ") else {
            continue;
        };
        if number.len() != 1
            || !matches!(number.as_bytes()[0], b'1'..=b'9')
            || label.trim().is_empty()
        {
            continue;
        }
        if number.parse::<usize>().ok() != Some(choices.len() + 1) {
            choices.clear();
            selected = None;
        }
        if number != (choices.len() + 1).to_string() {
            continue;
        }
        if marked {
            selected = Some(choices.len());
        }
        choices.push(Choice {
            label: label.trim().into(),
            keys: Vec::new(),
        });
    }
    if !(2..=9).contains(&choices.len()) {
        return None;
    }
    let selected = selected?;
    for (index, choice) in choices.iter_mut().enumerate() {
        let direction = if index < selected { "Up" } else { "Down" };
        choice.keys = std::iter::repeat_n(direction.to_owned(), index.abs_diff(selected))
            .chain(["Enter".to_owned()])
            .collect();
    }
    let lower = text.to_lowercase();
    if !["proceed", "allow", "permission", "approve", "确认", "允许"]
        .iter()
        .any(|s| lower.contains(s))
    {
        return None;
    }
    Some(Approval {
        id: fingerprint(text),
        prompt: text.into(),
        explanation: None,
        command: None,
        working_directory: None,
        operation: None,
        details: vec![],
        choices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn async_messages_are_readable_before_and_after_delivery_without_changing_history() {
        let text = "可以。**共享配置**保存在 `node.json`。\n\n接下来继续验证。";
        let call = serde_json::json!({"type":"response_item","payload":{"type":"function_call","call_id":"async-message","name":"send_message_to_user_async","arguments":serde_json::json!({"message":text}).to_string()}});
        let output = serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"async-message","output":"{\"accepted\":true}"}});
        for records in [
            call.to_string(),
            format!("{call}\n{output}\n{call}\n{output}"),
        ] {
            let messages = parse_history("codex", records.as_bytes());
            assert_eq!(messages.len(), 1);
            let original = &messages[0];
            let displayed = original.presentation();
            assert_eq!(displayed.id, "async-message");
            assert_eq!(displayed.role, Role::Assistant);
            assert_eq!(displayed.text, text);
            assert!(!displayed.intermediate);
            assert!(displayed.tool.is_none());
            assert_eq!(original.role, Role::Tool);
            assert_eq!(
                original.tool.as_ref().unwrap().name,
                "send_message_to_user_async"
            );

            // Previously cached tool records gain the same presentation offline.
            let restored: Message =
                serde_json::from_slice(&serde_json::to_vec(original).unwrap()).unwrap();
            assert_eq!(restored.presentation(), displayed);
        }
    }

    #[test]
    fn malformed_async_messages_failed_delivery_and_other_tools_keep_their_details() {
        for (name, arguments, failed) in [
            ("send_message_to_user_async", "not json", false),
            ("send_message_to_user_async", "{}", false),
            ("send_message_to_user_async", r#"{"message":null}"#, false),
            ("send_message_to_user_async", r#"{"message":42}"#, false),
            ("send_message_to_user_async", r#"{"message":"  "}"#, false),
            ("send_message_to_user_async", r#"{"message":"hello"}"#, true),
            (
                "mcp__other__send_message_to_user_async",
                r#"{"message":"hello"}"#,
                false,
            ),
            ("exec_command", r#"{"message":"hello"}"#, false),
            (
                "request_user_input_async",
                r#"{"questions":[{"title":"Choose"}]}"#,
                false,
            ),
        ] {
            let mut messages = Vec::new();
            add_tool(&mut messages, "call".into(), name, arguments.into());
            finish_tool(&mut messages, "call", "receipt".into(), failed);
            assert!(matches!(
                messages[0].presentation(),
                std::borrow::Cow::Borrowed(_)
            ));
            assert_eq!(messages[0].presentation().as_ref(), &messages[0]);
        }
    }

    #[test]
    fn failed_turns_remain_distinct_history_records_after_continuation() {
        let failure = serde_json::json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"failed-turn","last_agent_message":null,"error":{"message":"Selected model is at capacity. Please try a different model.","codex_error_info":"server_overloaded"}}});
        let next = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"continued"}]}});
        let bytes = format!("{failure}\n{failure}\n{next}\n");
        let messages = parse_history("codex", bytes.as_bytes());
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, "turn-error-failed-turn");
        assert_eq!(messages[0].role, Role::Error);
        assert!(messages[0].text.starts_with("server_overloaded:"));
        assert!(messages[0].tool.is_none());
        assert!(!messages[0].intermediate);
        assert_eq!(messages[1].text, "continued");
        let restored: Vec<Message> =
            serde_json::from_slice(&serde_json::to_vec(&messages).unwrap()).unwrap();
        assert_eq!(restored, messages);
    }

    #[test]
    fn successful_turns_and_user_cancellations_do_not_invent_history_errors() {
        for payload in [
            serde_json::json!({"type":"task_complete","turn_id":"success","error":null}),
            serde_json::json!({"type":"task_complete","turn_id":"success"}),
            serde_json::json!({"type":"task_aborted","turn_id":"cancelled"}),
        ] {
            let record = serde_json::json!({"type":"event_msg","payload":payload});
            assert!(parse_history("codex", record.to_string().as_bytes()).is_empty());
        }
        let record = serde_json::json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"no-details","error":{}}});
        let messages = parse_history("codex", record.to_string().as_bytes());
        assert_eq!(messages[0].role, Role::Error);
        assert!(messages[0].text.is_empty());
    }

    #[test]
    fn selected_skill_instructions_do_not_appear_as_extra_user_prompts() {
        let context = "<skill>\n<name>report</name>\n<path>/skills/report/SKILL.md</path>\nWrite a report.\n</skill>";
        let wire = |role: &str, text: &str| {
            serde_json::json!({"type":"response_item","payload":{"type":"message","role":role,"content":[{"type":"input_text","text":text}]}}).to_string()
        };
        let lines = [
            wire("user", "$report\n\nSummarize changes"),
            wire("user", context),
            wire("assistant", context),
            wire("user", "Explain <skill> tags"),
        ]
        .join("\n");
        let messages = parse_history("codex", lines.as_bytes());
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].text, "$report\n\nSummarize changes");
        assert_eq!(messages[1].role, Role::Assistant);
        assert_eq!(messages[1].text, context);
        assert_eq!(messages[2].text, "Explain <skill> tags");
    }
    #[test]
    fn generated_images_render_as_assistant_attachments_with_stable_identity() {
        let bytes = br#"{"type":"response_item","_kite_offset":100,"payload":{"type":"image_generation_call","id":"ig-test","status":"completed","_kite_images":[{"id":"abc","name":"generated.png","mime":"image/png","size":1024,"path":"kite-image:abc"}]}}
{"type":"response_item","payload":{"type":"message","id":"final","role":"assistant","content":[{"text":"Here is your image"}]}}
"#;
        let messages = parse_history("codex", bytes);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].id, "ig-test");
        assert_eq!(messages[0].role, Role::Assistant);
        assert!(!messages[0].intermediate);
        let (body, files) = composer::split_attachments(&messages[0].text);
        assert!(body.is_empty());
        assert_eq!(files[0].path, "kite-image:abc");
        assert_eq!(messages[1].id, "final");
        let failed = parse_history("codex", br#"{"type":"response_item","payload":{"type":"image_generation_call","id":"bad","status":"failed"}}"#);
        assert_eq!(failed.len(), 1);
        assert!(!failed[0].text.is_empty());
    }
    #[test]
    fn actual_model_question_answers_are_retained_as_user_replies() {
        let input = serde_json::json!({"type":"response_item","payload":{"type":"function_call","call_id":"question","name":"request_user_input","arguments":serde_json::json!({"questions":[{"id":"colour","header":"Colour"}]}).to_string()}});
        let output = serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"question","output":serde_json::json!({"answers":{"colour":{"answers":["Blue"]}}}).to_string()}});
        let messages = parse_history("codex", format!("{input}\n{output}\n").as_bytes());
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].role, Role::User);
        assert_eq!(messages[1].text, "Colour\nBlue");
        assert_eq!(messages[1].question_replies[0].question, "Colour");
        assert_eq!(messages[1].question_replies[0].answer, "Blue");
        assert!(messages[0].tool.as_ref().unwrap().complete);
    }
    #[test]
    fn async_question_replies_keep_each_question_paired_with_its_answer() {
        let replies = serde_json::json!([
            {"questionItemId":"[\"request_user_input_async\",\"call-1\",0]","question":"Which colour?","answer":"Blue"},
            {"questionItemId":"[\"request_user_input_async\",\"call-1\",1]","question":"Which size?","answer":"Large"}
        ]);
        let body = format!(
            "<send_user_message_question_reply>\n{replies}\n</send_user_message_question_reply>"
        );
        let event = serde_json::json!({"type":"response_item","payload":{"type":"message","id":"reply-1","role":"user","content":[{"text":body}]}});
        let messages = parse_history("codex", event.to_string().as_bytes());
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].question_replies.len(), 2);
        assert_eq!(messages[0].question_replies[0].question, "Which colour?");
        assert_eq!(messages[0].question_replies[0].answer, "Blue");
        assert_eq!(messages[0].question_replies[1].question, "Which size?");
        assert_eq!(messages[0].question_replies[1].answer, "Large");
        assert!(!messages[0].text.contains("questionItemId"));
        let restored: Message =
            serde_json::from_str(&serde_json::to_string(&messages[0]).unwrap()).unwrap();
        assert_eq!(restored, messages[0]);
        let ordinary = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"text":"Which colour?\nBlue"}]}});
        assert!(
            parse_history("codex", ordinary.to_string().as_bytes())[0]
                .question_replies
                .is_empty()
        );
    }
    #[test]
    fn blocking_question_history_retains_the_full_prompt_and_masks_secret_answers() {
        let input = serde_json::json!({"type":"response_item","payload":{"type":"function_call","call_id":"q","name":"request_user_input","arguments":serde_json::json!({"questions":[{"id":"key","header":"Key","question":"Enter your key","isSecret":true}]}).to_string()}});
        let output = serde_json::json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"q","output":serde_json::json!({"answers":{"key":{"answers":["private-key"]}}}).to_string()}});
        let messages = parse_history("codex", format!("{input}\n{output}\n").as_bytes());
        assert_eq!(messages[1].question_replies[0].question, "Enter your key");
        assert_eq!(messages[1].question_replies[0].answer, "••••");
        assert!(!messages[1].text.contains("private-key"));
        // Old saved records still deserialize; no history is discarded during upgrade.
        let old: Message = serde_json::from_value(serde_json::json!({"id":"old","role":"User","text":"hello","intermediate":false,"tool":null})).unwrap();
        assert!(old.question_replies.is_empty());
    }
    #[test]
    fn empty_progress_does_not_hide_a_final_reply_and_commentary_is_classified() {
        let data=br#"{"id":"a","type":"response_item","payload":{"id":"answer","type":"message","role":"assistant","channel":"final","content":[{"text":"answer"}]}}
{"id":"empty","type":"response_item","payload":{"type":"message","role":"assistant","channel":"commentary","content":[]}}
{"id":"p","type":"response_item","payload":{"id":"progress","type":"message","role":"assistant","phase":"commentary","content":[{"text":"checking"}]}}"#;
        let messages = parse_history("codex", data);
        assert_eq!(messages.len(), 2);
        assert!(!messages[0].intermediate);
        assert!(messages[1].intermediate);
    }
    #[test]
    fn claude_links_real_tool_results_without_user_bubbles_for_tool_output() {
        let data = br#"{"type":"user","uuid":"u","message":{"content":"hello"}}
{"type":"assistant","uuid":"a","message":{"content":[{"type":"text","text":"Opening"},{"type":"tool_use","id":"t","name":"computer","input":{"action":"click"}}]}}
{"type":"user","uuid":"r","message":{"content":[{"type":"tool_result","tool_use_id":"t","content":"done"}]}}
{"type":"assistant","uuid":"partial""#;
        let m = parse_history("claude", data);
        assert_eq!(m.len(), 3);
        let t = m[2].tool.as_ref().unwrap();
        assert!(t.complete && t.computer);
        assert_eq!(t.output, "done");
    }
    #[test]
    fn codex_ignores_metadata_and_correlates_calls() {
        let data = br#"{"type":"session_meta","payload":{"id":"session"}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"test"}]}}
{"type":"response_item","payload":{"type":"function_call","call_id":"c","name":"exec_command","arguments":"{}"}}
{"type":"response_item","payload":{"type":"function_call_output","call_id":"c","output":"ok"}}
{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"done"}]}}"#;
        let m = parse_history("codex", data);
        assert_eq!(m.len(), 3);
        assert!(m[1].tool.as_ref().unwrap().complete);
        assert_eq!(m[2].text, "done");
    }
    #[test]
    fn codex_native_image_wrappers_do_not_break_attachment_reconciliation() {
        let files = vec![composer::Attachment {
            id: "f".into(),
            name: "photo.png".into(),
            mime: "image/png".into(),
            size: 12,
            path: "/home/user/photo.png".into(),
        }];
        let text = composer::compose_text("Describe", &files).unwrap();
        let input = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":text},{"type":"input_text","text":"<image name=[Image #1] path=\"/home/user/photo.png\">"},{"type":"input_image","image_url":"data:image/png;base64,AA=="},{"type":"input_text","text":"</image>"}]}});
        let messages = parse_history("codex", input.to_string().as_bytes());
        assert_eq!(messages[0].text, text);
        assert_eq!(composer::split_attachments(&messages[0].text).1, files);
    }
    #[test]
    fn approval_requires_blocked_state_and_explicit_actual_choices() {
        let t = "Do you want to proceed?\n ❯ 1. Yes\n  2. No";
        assert!(terminal_choices(false, "claude", t).is_none());
        let a = terminal_choices(true, "claude", t).unwrap();
        assert_eq!(a.choices[0].keys, ["Enter"]);
        assert_eq!(a.choices[1].keys, ["Down", "Enter"]);
        assert!(terminal_choices(true, "claude", "Proceed?\n 1. Yes\n 2. No").is_none());
        assert_ne!(a.id, fingerprint("different question"));
        assert!(terminal_choices(true, "claude", "ordinary output").is_none());
    }
}

#[cfg(test)]
mod complete_history_tests {
    use super::*;
    #[test]
    fn full_records_and_repeated_text_are_not_cut_to_the_render_window() {
        let long = "完整内容 ".repeat(12000);
        let mut rows = Vec::new();
        for i in 0..1000 {
            rows.push(serde_json::json!({"id":format!("m-{i}"),"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":if i==0 {&long}else{"same repeated body"}}]}}).to_string());
        }
        let parsed = parse_history("codex", rows.join("\n").as_bytes());
        assert_eq!(parsed.len(), 1000);
        assert_eq!(parsed[0].text, long);
        assert_eq!(parsed[999].id, "m-999");
    }
}
