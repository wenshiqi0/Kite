//! Bounded reads of the selected provider session, through the existing SSH connection.
use crate::{Message, parse_history};
use kite_herdr::api::AgentSession;
use kite_ssh::Session;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct History {
    pub key: String,
    pub stamp: String,
    pub available: bool,
    pub messages: Vec<Message>,
}

pub fn key(reference: &AgentSession) -> String {
    crate::fingerprint(&format!(
        "{}:{}:{}:{}",
        reference.source, reference.agent, reference.kind, reference.value
    ))
}

pub async fn read(ssh: &Session, reference: &AgentSession, previous: Option<History>) -> History {
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
tail -c 2097152 "$file"
"#,
        provider = quote(&reference.agent),
        kind = quote(&reference.kind),
        value = quote(value),
        cached = quote(previous.as_ref().map(|p| p.stamp.as_str()).unwrap_or(""))
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
    History {
        key: cache_key,
        stamp: stamp.into(),
        available: true,
        messages: parse_history(&reference.agent, lines.next().unwrap_or_default()),
    }
}
