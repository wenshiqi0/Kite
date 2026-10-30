//! Bounded reads of the selected provider session, through the existing SSH connection.
use crate::{Message, parse_history};
use kite_herdr::api::AgentSession;
use kite_ssh::Session;
use std::time::Duration;

#[derive(Clone, Debug, Default, serde::Deserialize)]
pub struct Page {
    pub start: String,
    pub end: String,
    pub has_older: bool,
    pub has_newer: bool,
}

#[derive(Clone, Debug, Default)]
pub struct History {
    pub key: String,
    pub stamp: String,
    pub available: bool,
    pub messages: Vec<Message>,
    pub async_input: crate::async_input::History,
    pub page: Option<Page>,
}

pub fn key(reference: &AgentSession) -> String {
    crate::fingerprint(&format!(
        "{}:{}:{}:{}",
        reference.source, reference.agent, reference.kind, reference.value
    ))
}

pub async fn read(ssh: &Session, reference: &AgentSession, previous: Option<History>) -> History {
    request(ssh, reference, previous, None).await
}

pub async fn page(ssh: &Session, reference: &AgentSession, cursor: &str, newer: bool) -> History {
    if reference.agent != "codex"
        || cursor.len() > 128
        || !cursor.bytes().all(|c| c.is_ascii_digit() || c == b':')
    {
        return History::default();
    }
    request(ssh, reference, None, Some((cursor, newer))).await
}

async fn request(
    ssh: &Session,
    reference: &AgentSession,
    previous: Option<History>,
    page: Option<(&str, bool)>,
) -> History {
    let cache_key = key(reference);
    let previous = previous.filter(|p| p.key == cache_key);
    if !matches!(reference.agent.as_str(), "claude" | "codex")
        || !matches!(reference.kind.as_str(), "id" | "path")
    {
        return History::default();
    }
    let value = &reference.value;
    if value.is_empty() || value.len() > 1024 || value.contains(['\0', '\n', '\r']) {
        return History::default();
    }
    if reference.kind == "id"
        && !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return History::default();
    }
    let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    let page_args = page
        .map(|(cursor, newer)| {
            format!(
                " --{} {}",
                if newer { "after" } else { "before" },
                quote(cursor)
            )
        })
        .unwrap_or_default();
    let script = format!(
        r#"set -eu
provider={provider}
kind={kind}
value={value}
cached={cached}
if test "$provider" = claude; then root="${{CLAUDE_CONFIG_DIR:-$HOME/.claude}}/projects"; pattern="$value.jsonl";
else root="${{CODEX_HOME:-$HOME/.codex}}/sessions"; pattern="rollout-*-$value.jsonl"; fi
test -d "$root"
root=$(readlink -f "$root")
if test "$kind" = path; then
  file=$(readlink -f "$value")
  case "$file" in "$root"/*.jsonl) ;; *) exit 2;; esac
else
  files=$(find "$root" -maxdepth 6 -type f -name "$pattern" -print | head -n 2)
  count=$(printf '%s\n' "$files" | grep -c .)
  test "$count" = 1
  file=$files
fi
test -f "$file"
stamp=$(stat -c '%d:%i:%s:%y' "$file")
printf 'kite-history-v1\n%s\n' "$stamp"
if test "$stamp" = "$cached"; then exit 0; fi
if test "$provider" = codex; then
  python3 "$HOME/.local/lib/kite/backend-v3.py" history --file "$file"{page_args}
else
  tail -c 2097152 "$file"
fi
"#,
        provider = quote(&reference.agent),
        kind = quote(&reference.kind),
        value = quote(value),
        cached = quote(previous.as_ref().map(|p| p.stamp.as_str()).unwrap_or("")),
        page_args = page_args,
    );
    let Ok(Ok(output)) = tokio::time::timeout(
        Duration::from_secs(10),
        ssh.exec("/bin/sh -s", script.as_bytes()),
    )
    .await
    else {
        return History::default();
    };
    if output.exit_status != Some(0) || output.stdout.len() > 2097280 {
        return History::default();
    }
    let mut lines = output.stdout.splitn(3, |b| *b == b'\n');
    if lines.next() != Some(b"kite-history-v1".as_slice()) {
        return History::default();
    }
    let Some(stamp) = lines.next().and_then(|b| std::str::from_utf8(b).ok()) else {
        return History::default();
    };
    if let Some(previous) = previous
        && previous.stamp == stamp
    {
        return previous;
    }
    let bytes = lines.next().unwrap_or_default();
    let (controls, bytes, page) = if reference.agent == "codex" {
        let Some(parts) = codex_projection(bytes) else {
            return History::default();
        };
        let Some((page, display)) = display_page(parts.1) else {
            return History::default();
        };
        (parts.0, display, Some(page))
    } else {
        (&b""[..], bytes, None)
    };
    History {
        key: cache_key,
        stamp: stamp.into(),
        available: true,
        messages: parse_history(&reference.agent, bytes),
        async_input: if reference.agent == "codex" {
            crate::async_input::history(&reference.value, controls)
        } else {
            Default::default()
        },
        page,
    }
}

fn display_page(bytes: &[u8]) -> Option<(Page, &[u8])> {
    let split = bytes.iter().position(|c| *c == b'\n')?;
    let value: serde_json::Value = serde_json::from_slice(&bytes[..split]).ok()?;
    if value["type"] != "kite_history_page" {
        return None;
    }
    let page: Page = serde_json::from_value(value).ok()?;
    if page.start.len() > 128 || page.end.len() > 128 {
        return None;
    }
    Some((page, &bytes[split + 1..]))
}

fn codex_projection(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut parts = bytes.splitn(3, |b| *b == b'\n');
    if parts.next()? != b"kite-codex-history-v2" {
        return None;
    }
    let size = std::str::from_utf8(parts.next()?)
        .ok()?
        .parse::<usize>()
        .ok()?;
    let body = parts.next()?;
    if size > 1024 * 1024 || size > body.len() {
        return None;
    }
    Some(body.split_at(size))
}

/// Merge chronological pages, retaining tool details split across a page boundary.
pub fn merge(older: &[Message], newer: &[Message]) -> Vec<Message> {
    let mut result = older.to_vec();
    let mut positions: std::collections::BTreeMap<String, usize> = result
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id.clone(), i))
        .collect();
    for incoming in newer {
        if let Some(&i) = positions.get(&incoming.id) {
            let mut next = incoming.clone();
            if let (Some(old), Some(new)) = (&result[i].tool, &mut next.tool) {
                if new.name == "tool" {
                    new.name = old.name.clone();
                }
                if new.arguments.is_empty() {
                    new.arguments = old.arguments.clone();
                }
                if !new.complete && old.complete {
                    new.output = old.output.clone();
                }
                new.complete |= old.complete;
                new.failed |= old.failed;
                new.computer |= old.computer;
            }
            result[i] = next;
        } else {
            positions.insert(incoming.id.clone(), result.len());
            result.push(incoming.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_history_is_separate_from_the_bounded_display_and_frames_are_checked() {
        let data = b"kite-codex-history-v2\n8\ncontrol\nrecent-chat";
        let (control, display) = codex_projection(data).unwrap();
        assert_eq!(control, b"control\n");
        assert_eq!(display, b"recent-chat");
        for invalid in [
            b"old unframed history".as_slice(),
            b"kite-codex-history-v2\n99\nx",
            b"kite-codex-history-v2\n2000000\nx",
            b"kite-codex-history-v2\nno\nx",
        ] {
            assert!(codex_projection(invalid).is_none());
        }
    }

    #[test]
    fn page_metadata_is_separate_from_messages_and_required_fields_are_checked() {
        let data = br#"{"type":"kite_history_page","start":"1:2:0:12","end":"1:2:12:12","has_older":false,"has_newer":false}
records
"#;
        let (page, records) = display_page(data).unwrap();
        assert_eq!(page.end, "1:2:12:12");
        assert_eq!(records, b"records\n");
        assert!(display_page(b"{}\n").is_none());
        assert!(display_page(b"not json\n").is_none());
    }

    #[test]
    fn a_tool_call_and_its_output_can_span_pages_without_losing_details() {
        let call = parse_history("codex", br#"{"type":"response_item","payload":{"type":"function_call","call_id":"call","name":"Bash","arguments":"echo hello"}}
"#);
        let output = parse_history("codex", br#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"call","output":"hello"}}
"#);
        let messages = merge(&call, &output);
        assert_eq!(messages.len(), 1);
        let tool = messages[0].tool.as_ref().unwrap();
        assert_eq!(tool.name, "Bash");
        assert_eq!(tool.arguments, "echo hello");
        assert_eq!(tool.output, "hello");
        assert!(tool.complete);
    }
}
