//! Shared subagent activity rules; native adapters publish only provider facts.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subagent {
    pub id: String,
    pub parent_id: String,
    pub name: String,
    pub task: String,
    pub status: String,
    pub updated_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssociatedSession {
    pub host_id: String,
    pub session_id: String,
    pub machine: String,
    pub provider: String,
    pub connected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssociatedTask {
    pub activity: Subagent,
    pub session: Option<AssociatedSession>,
}

impl From<Subagent> for AssociatedTask {
    fn from(activity: Subagent) -> Self {
        Self {
            activity,
            session: None,
        }
    }
}

pub fn delegated_status(connected: bool, archived: bool, status: &str) -> &str {
    if !connected {
        return "offline";
    }
    match status {
        "saved" if archived => "released",
        "saved" | "" => "unknown",
        value => value,
    }
}

impl From<kite_herdr::api::Subagent> for Subagent {
    fn from(value: kite_herdr::api::Subagent) -> Self {
        let status = match value.runtime_status.as_str() {
            "active" if value.active_flags.iter().any(|f| f == "waitingOnUserInput") => "question",
            "active" if value.active_flags.iter().any(|f| f == "waitingOnApproval") => "blocked",
            "active" => "working",
            "systemError" => "failed",
            _ => match value.last_outcome.as_deref() {
                Some("completed") => "done",
                Some("failed") => "failed",
                Some("aborted") => "stopped",
                _ if value.runtime_status == "idle" => "idle",
                _ => "unknown",
            },
        };
        Self {
            id: value.id,
            parent_id: value.parent_id,
            name: value.name,
            task: value.task,
            status: status.into(),
            updated_ms: value.updated_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_facts_override_earlier_outcomes_and_unloaded_does_not_imply_running() {
        for (runtime, flag, outcome, expected) in [
            ("active", "", "completed", "working"),
            ("active", "waitingOnApproval", "failed", "blocked"),
            ("active", "waitingOnUserInput", "completed", "question"),
            ("systemError", "", "completed", "failed"),
            ("notLoaded", "", "completed", "done"),
            ("notLoaded", "", "failed", "failed"),
            ("notLoaded", "", "aborted", "stopped"),
            ("notLoaded", "", "", "unknown"),
            ("idle", "", "", "idle"),
        ] {
            let agent = Subagent::from(kite_herdr::api::Subagent {
                id: "child".into(),
                parent_id: "parent".into(),
                name: "Worker".into(),
                task: "research".into(),
                runtime_status: runtime.into(),
                active_flags: vec![flag.into()],
                last_outcome: Some(outcome.into()),
                updated_ms: 1,
            });
            assert_eq!(agent.status, expected, "{runtime}/{flag}/{outcome}");
        }
    }
}
