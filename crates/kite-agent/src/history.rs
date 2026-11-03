//! Bounded reads of the selected provider session, through the existing SSH connection.
use crate::{Message, parse_history};
use kite_herdr::api::AgentSession;
use kite_ssh::Session;
use std::{sync::Arc, time::Duration};

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
    pub source_pending: bool,
    pub problem: String,
    pub messages: Vec<Message>,
    pub async_input: crate::async_input::History,
    pub page: Option<Page>,
    pub positions: std::collections::BTreeMap<String, (u64, u32)>,
}

pub fn key(reference: &AgentSession) -> String {
    crate::fingerprint(&format!(
        "{}:{}:{}:{}",
        reference.source, reference.agent, reference.kind, reference.value
    ))
}

pub async fn read(
    ssh: &Session,
    reference: &AgentSession,
    previous: Option<Arc<History>>,
) -> Arc<History> {
    request(ssh, reference, previous, None).await
}

pub async fn page(
    ssh: &Session,
    reference: &AgentSession,
    cursor: &str,
    newer: bool,
) -> Arc<History> {
    if !crate::providers::managed(&reference.agent)
        || cursor.len() > 128
        || !cursor.bytes().all(|c| c.is_ascii_digit() || c == b':')
    {
        return Arc::new(History::default());
    }
    request(ssh, reference, None, Some((cursor, newer))).await
}

async fn request(
    ssh: &Session,
    reference: &AgentSession,
    previous: Option<Arc<History>>,
    page: Option<(&str, bool)>,
) -> Arc<History> {
    let cache_key = key(reference);
    let previous = previous.filter(|p| p.key == cache_key);
    if matches!(reference.agent.as_str(), "dsh" | "ply") {
        if reference.source != "kite:harness"
            || reference.kind != "id"
            || reference.value.len() != 36
            || !reference
                .value
                .bytes()
                .all(|c| c.is_ascii_hexdigit() || c == b'-')
        {
            return unavailable(&cache_key, "invalid harness history reference".into());
        }
        let mut command = format!(
            "harness-history --thread {}",
            crate::composer::quote(&reference.value)
        );
        if let Some(old) = &previous {
            command.push_str(&format!(" --name {}", crate::composer::quote(&old.stamp)));
        }
        if let Some((cursor, newer)) = page {
            command.push_str(&format!(
                " --{} {}",
                if newer { "after" } else { "before" },
                crate::composer::quote(cursor)
            ));
        }
        let output = match tokio::time::timeout(
            Duration::from_secs(30),
            ssh.exec(&crate::composer::helper_command(&command), &[]),
        )
        .await
        {
            Ok(Ok(output)) => output,
            _ => return unavailable(&cache_key, "harness history request failed".into()),
        };
        return response(reference, previous, output);
    }
    if !matches!(reference.agent.as_str(), "claude" | "codex")
        || !matches!(reference.kind.as_str(), "id" | "path")
    {
        return Arc::new(History::default());
    }
    let value = &reference.value;
    if value.is_empty() || value.len() > 1024 || value.contains(['\0', '\n', '\r']) {
        return Arc::new(History::default());
    }
    if reference.kind == "id"
        && !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return Arc::new(History::default());
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
if ! test -d "$root"; then printf 'kite-history-pending-v1\n'; exit 0; fi
root=$(readlink -f "$root")
if test "$kind" = path; then
  file=$(readlink -f "$value")
  case "$file" in "$root"/*.jsonl) ;; *) exit 2;; esac
else
  files=$(find "$root" -maxdepth 6 -type f -name "$pattern" -print | head -n 2)
  count=$(printf '%s\n' "$files" | grep -c . || true)
  if test "$count" = 0; then printf 'kite-history-pending-v1\n'; exit 0; fi
  test "$count" = 1
  file=$files
fi
if ! test -f "$file"; then printf 'kite-history-pending-v1\n'; exit 0; fi
stamp=$(stat -c '%d:%i:%s:%y' "$file")
printf 'kite-history-v1\n%s\n' "$stamp"
if test "$stamp" = "$cached"; then exit 0; fi
if test "$provider" = codex; then
  "$HOME/.local/bin/kite-node" backend history --compressed --file "$file"{page_args}
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
    let output = match tokio::time::timeout(
        Duration::from_secs(30),
        ssh.exec("/bin/sh -s", script.as_bytes()),
    )
    .await
    {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => return unavailable(&cache_key, format!("history transport: {error}")),
        Err(_) => return unavailable(&cache_key, "history request timed out".into()),
    };
    response(reference, previous, output)
}

fn unavailable(key: &str, problem: String) -> Arc<History> {
    Arc::new(History {
        key: key.into(),
        problem,
        ..History::default()
    })
}

fn response(
    reference: &AgentSession,
    previous: Option<Arc<History>>,
    output: kite_ssh::ExecOutput,
) -> Arc<History> {
    let cache_key = key(reference);
    if output.exit_status != Some(0) || output.stdout.len() > 4 * 1024 * 1024 {
        return unavailable(
            &cache_key,
            format!(
                "history source read failed (exit {:?}, {} bytes)",
                output.exit_status,
                output.stdout.len()
            ),
        );
    }
    if output.stdout == b"kite-history-pending-v1\n" {
        return Arc::new(History {
            key: cache_key,
            source_pending: true,
            ..History::default()
        });
    }
    let mut lines = output.stdout.splitn(3, |b| *b == b'\n');
    if lines.next() != Some(b"kite-history-v1".as_slice()) {
        return unavailable(&cache_key, "invalid history response header".into());
    }
    let Some(stamp) = lines.next().and_then(|b| std::str::from_utf8(b).ok()) else {
        return unavailable(&cache_key, "invalid history source stamp".into());
    };
    if let Some(previous) = previous
        && previous.stamp == stamp
    {
        return previous;
    }
    let bytes = lines.next().unwrap_or_default();
    let decoded;
    let bytes = if crate::providers::managed(&reference.agent) {
        let Some(value) = decode_history(bytes) else {
            return unavailable(&cache_key, "invalid compressed history response".into());
        };
        decoded = value;
        decoded.as_slice()
    } else {
        bytes
    };
    let (controls, bytes, page) = if crate::providers::managed(&reference.agent) {
        let Some(parts) = codex_projection(bytes) else {
            return unavailable(&cache_key, "invalid provider history projection".into());
        };
        let Some((page, display)) = display_page(parts.1) else {
            return unavailable(&cache_key, "invalid history page".into());
        };
        (parts.0, display, Some(page))
    } else {
        (&b""[..], bytes, None)
    };
    Arc::new(History {
        key: cache_key,
        stamp: stamp.into(),
        available: true,
        source_pending: false,
        problem: String::new(),
        messages: parse_history(&reference.agent, bytes),
        async_input: if reference.agent == "codex" {
            crate::async_input::history(&reference.value, controls)
        } else {
            Default::default()
        },
        positions: record_positions(bytes),
        page,
    })
}

fn decode_history(bytes: &[u8]) -> Option<Vec<u8>> {
    use std::io::Read;
    let decoder = flate2::read::GzDecoder::new(bytes);
    let mut output = Vec::new();
    decoder
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut output)
        .ok()?;
    (output.len() <= 4 * 1024 * 1024).then_some(output)
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
pub fn merge_message(older: &Message, newer: &Message) -> Message {
    let mut next = newer.clone();
    if next.question_replies.is_empty() {
        next.question_replies = older.question_replies.clone();
    }
    if let (Some(old), Some(new)) = (&older.tool, &mut next.tool) {
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
    next
}

pub fn merge(older: &[Message], newer: &[Message]) -> Vec<Message> {
    let mut result = older.to_vec();
    let mut positions: std::collections::BTreeMap<_, _> = result
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id.clone(), i))
        .collect();
    for incoming in newer {
        if let Some(&i) = positions.get(&incoming.id) {
            result[i] = merge_message(&result[i], incoming);
        } else {
            positions.insert(incoming.id.clone(), result.len());
            result.push(incoming.clone());
        }
    }
    result
}

fn record_positions(bytes: &[u8]) -> std::collections::BTreeMap<String, (u64, u32)> {
    use serde_json::Value;
    let mut result = std::collections::BTreeMap::new();
    for line in bytes.split(|b| *b == b'\n') {
        let Ok(mut value) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let Some(offset) = value
            .as_object_mut()
            .and_then(|v| v.remove("_kite_offset"))
            .and_then(|v| v.as_u64())
        else {
            continue;
        };
        let p = &value["payload"];
        let id = if let Some(id) = crate::turn_error_id(&value) {
            id
        } else if p["type"] == "message" || p["type"] == "image_generation_call" {
            p["id"]
                .as_str()
                .or_else(|| value["uuid"].as_str())
                .or_else(|| value["id"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| crate::fingerprint(&value.to_string()))
        } else if let Some(id) = p["call_id"].as_str() {
            id.into()
        } else {
            continue;
        };
        result
            .entry(id.clone())
            .and_modify(|old| *old = std::cmp::min(*old, (offset, 0)))
            .or_insert((offset, 0));
        if p["type"] == "function_call_output" {
            result.insert(format!("{id}-answer"), (offset, 1));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_source_is_distinct_from_failed_transport_or_invalid_output() {
        let reference = AgentSession {
            source: "herdr:codex".into(),
            agent: "codex".into(),
            kind: "id".into(),
            value: "thread".into(),
        };
        let output = |stdout: &[u8], exit| kite_ssh::ExecOutput {
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
            exit_status: exit,
        };
        let pending = response(
            &reference,
            None,
            output(b"kite-history-pending-v1\n", Some(0)),
        );
        assert!(pending.source_pending);
        assert!(!pending.available);
        assert!(pending.problem.is_empty());
        assert_eq!(pending.key, key(&reference));
        for bad in [
            output(b"kite-history-pending-v1\n", None),
            output(b"kite-history-pending-v1\n", Some(1)),
            output(b"kite-history-pending-v1\ntruncated", Some(0)),
        ] {
            let failed = response(&reference, None, bad);
            assert!(!failed.source_pending);
            assert!(!failed.available);
            assert!(!failed.problem.is_empty());
        }
    }
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

#[cfg(test)]
mod transport_tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn terminal_error_identity_and_source_position_survive_history_backfill() {
        let record = serde_json::json!({"type":"event_msg","_kite_offset":4321,"payload":{"type":"task_complete","turn_id":"failed","error":{"message":"model unavailable","codex_error_info":"server_overloaded"}}});
        let projected = serde_json::to_vec(&record).unwrap();
        let messages = parse_history("codex", &projected);
        let positions = record_positions(&projected);
        assert_eq!(positions[&messages[0].id], (4321, 0));
        let mut original = record;
        original.as_object_mut().unwrap().remove("_kite_offset");
        let original = parse_history("codex", &serde_json::to_vec(&original).unwrap());
        assert_eq!(messages, original);
        assert_eq!(merge(&messages, &original), messages);
    }
    #[test]
    fn compressed_pages_decode_with_a_hard_bound() {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(b"projected records").unwrap();
        assert_eq!(
            decode_history(&encoder.finish().unwrap()).unwrap(),
            b"projected records"
        );
        let mut bomb = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        bomb.write_all(&vec![b'x'; 5 * 1024 * 1024]).unwrap();
        assert!(decode_history(&bomb.finish().unwrap()).is_none());
        assert!(decode_history(b"not gzip").is_none());
    }
    #[test]
    fn transport_offsets_do_not_change_provider_message_identity() {
        let original=br#"{"timestamp":"2026-10-31T00:00:00Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"same message"}]}}"#;
        let mut projected: serde_json::Value = serde_json::from_slice(original).unwrap();
        projected["_kite_offset"] = 12345.into();
        let projected = serde_json::to_vec(&projected).unwrap();
        assert_eq!(
            parse_history("codex", original),
            parse_history("codex", &projected)
        );
        assert_eq!(
            record_positions(&projected).values().next(),
            Some(&(12345, 0))
        );
    }
}
