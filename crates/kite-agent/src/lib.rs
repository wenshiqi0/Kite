//! Provider conversation records and explicit terminal choices. No UI or FFI.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub mod history;
pub mod process;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    User,
    Assistant,
    Tool,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: Role,
    pub text: String,
    pub tool: Option<Tool>,
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

fn add_text(messages: &mut Vec<Message>, id: String, role: Role, text: String) {
    if text.trim().is_empty() {
        return;
    }
    if let Some(m) = messages.iter_mut().find(|m| m.id == id) {
        m.text = text;
        return;
    }
    if messages
        .last()
        .is_some_and(|m| m.role == role && m.text == text && m.tool.is_none())
    {
        return;
    }
    messages.push(Message {
        id,
        role,
        text,
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

fn finish_tool(messages: &mut [Message], id: &str, output: String, failed: bool) {
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

/// Parse bounded JSONL records. Partial writes and non-conversation metadata are ignored.
pub fn parse_history(provider: &str, bytes: &[u8]) -> Vec<Message> {
    let mut messages = Vec::new();
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
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
            "codex" => {
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
                        let text = text_content(&p["content"]);
                        if text.starts_with("<environment_context>")
                            || text.starts_with("<turn_aborted>")
                        {
                            continue;
                        }
                        add_text(
                            &mut messages,
                            p["id"].as_str().unwrap_or(&id).to_owned(),
                            role,
                            text,
                        );
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
                            finish_tool(&mut messages, call, text_content(&p["output"]), false);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if messages.len() > 300 {
        messages.drain(..messages.len() - 300);
    }
    // Keep snapshots small enough that desktop statistics do not copy megabytes
    // of historical tool output through the state channel on every update.
    let mut budget = 256 * 1024;
    let mut keep = messages.len();
    for m in messages.iter_mut().rev() {
        fn limit(s: &mut String, max: usize) {
            if s.len() > max {
                let mut end = max;
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                s.truncate(end);
                s.push('…');
            }
        }
        limit(&mut m.text, 16 * 1024);
        if let Some(t) = &mut m.tool {
            limit(&mut t.arguments, 8 * 1024);
            limit(&mut t.output, 16 * 1024);
        }
        let size = m.text.len()
            + m.tool
                .as_ref()
                .map(|t| t.arguments.len() + t.output.len())
                .unwrap_or(0);
        if size > budget {
            break;
        }
        budget -= size;
        keep -= 1;
    }
    messages.drain(..keep);
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
    pub choices: Vec<Choice>,
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
        choices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
