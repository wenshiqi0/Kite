//! One identity for everything scoped to a conversation (RFC 0027). Other keys
//! derive from here; the derivations of persisted keys must stay byte-identical.
use kite_agent::{fingerprint, history};
use kite_herdr::api::{Agent, AgentSession};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConversationKey {
    pub host: String,
    /// A catalog `Record.id`, or the terminal ID of a live pane without a record yet.
    pub conversation: String,
}

impl ConversationKey {
    pub fn new(host: impl Into<String>, conversation: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            conversation: conversation.into(),
        }
    }
    /// Opaque transcript identity shared with native deltas and attachment reads.
    pub fn context(&self) -> String {
        format!("{}/{}", self.host, self.conversation)
    }
    /// The transcript identity of a host with nothing selected.
    pub fn unselected_context(host: Option<&str>) -> String {
        format!("{}/", host.unwrap_or_default())
    }
    /// Identity of this conversation's entry in the session cache.
    pub(super) fn cache_key(&self) -> String {
        fingerprint(&self.context())
    }
    /// Name of this conversation's persisted cache document.
    pub(super) fn cache_document(&self) -> String {
        Self::cache_document_of(&self.cache_key())
    }
    /// A namespaced 256-bit key encoded as letters fits Store's existing path contract.
    pub(super) fn cache_document_of(cache_key: &str) -> String {
        fingerprint(&format!("conversation-cache/{cache_key}"))
            .bytes()
            .map(|b| {
                (if b.is_ascii_digit() {
                    b'a' + b - b'0'
                } else {
                    b'k' + b - b'a'
                }) as char
            })
            .collect()
    }
    /// Identity of a provider source; `kite-agent` derives it for record and history IDs.
    pub(super) fn history_key(reference: &AgentSession) -> String {
        history::key(reference)
    }
    /// Scope of one transcript: it restarts when a draft gains its provider source.
    pub(super) fn history_scope(&self, reference: Option<&AgentSession>) -> String {
        match reference {
            Some(reference) => Self::new(&self.host, Self::history_key(reference)).context(),
            None => self.context(),
        }
    }
}

/// Internal lookup of the agent behind a conversation. Clients, uploads and model
/// settings follow the provider thread; the persisted `chat-settings` map uses it as is.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct AgentKey(String);

impl AgentKey {
    pub(super) fn new(host: &str, agent: &Agent) -> Self {
        Self(ConversationKey::new(host, agent_source(agent)).context())
    }
}

/// Provider source of an agent; a pane without one is identified by its terminal.
pub(super) fn agent_source(agent: &Agent) -> String {
    agent
        .agent_session
        .as_ref()
        .map(ConversationKey::history_key)
        .unwrap_or_else(|| fingerprint(&agent.terminal_id))
}

/// A pane on a host; the persisted `paused-processes` map uses it as is.
pub(super) fn pane_key(host: &str, terminal: &str) -> String {
    ConversationKey::new(host, terminal).context()
}

/// One approval request shown by a pane; each is submitted at most once.
pub(super) fn approval_token(host: &str, terminal: &str, request: &str) -> String {
    format!("{}/{request}", pane_key(host, terminal))
}

/// Optimistic rows stay with the pane and provider source that accepted them.
pub(super) fn pending_scope(host: &str, agent: &Agent) -> String {
    format!(
        "{}/{}",
        pane_key(host, &agent.terminal_id),
        agent_source(agent)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(session: Option<&str>) -> Agent {
        let mut value = serde_json::json!({
            "terminal_id":"terminal", "agent_status":"idle", "workspace_id":"w", "tab_id":"t", "pane_id":"p", "revision":1
        });
        if let Some(session) = session {
            value["agent_session"] = serde_json::json!({
                "source":"herdr:codex", "agent":"codex", "kind":"id", "value":session
            });
        }
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn saved_caches_settings_and_pauses_keep_their_existing_names() {
        let key = ConversationKey::new("host", "session");
        assert_eq!(key.context(), "host/session");
        assert_eq!(key.cache_key(), fingerprint("host/session"));
        assert_eq!(
            key.cache_document(),
            "pobpeonfieonpelgakcmghpbdchefoickcbpjamgfjlghdplbohfbgjoaddpffho"
        );
        assert_eq!(pane_key("host", "terminal"), "host/terminal");
        let source = fingerprint("herdr:codex:codex:id:thread");
        assert_eq!(
            serde_json::to_value(AgentKey::new("host", &agent(Some("thread")))).unwrap(),
            format!("host/{source}")
        );
        assert_eq!(
            AgentKey::new("host", &agent(None)),
            AgentKey(format!("host/{}", fingerprint("terminal")))
        );
    }

    #[test]
    fn hosts_never_share_a_conversation_identity() {
        let (a, b) = (
            ConversationKey::new("host-a", "draft"),
            ConversationKey::new("host-b", "draft"),
        );
        assert_ne!(a.cache_key(), b.cache_key());
        assert_ne!(a.cache_document(), b.cache_document());
        assert_ne!(a.context(), b.context());
    }

    #[test]
    fn a_draft_scope_restarts_when_it_gains_a_provider_source() {
        let key = ConversationKey::new("host", "draft");
        let bound = agent(Some("thread"));
        let reference = bound.agent_session.as_ref();
        assert_eq!(key.history_scope(None), "host/draft");
        assert_eq!(
            key.history_scope(reference),
            format!("host/{}", ConversationKey::history_key(reference.unwrap()))
        );
    }
}
