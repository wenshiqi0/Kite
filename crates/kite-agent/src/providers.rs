//! Host-verified provider availability and operation metadata; no native UI rules.
use crate::composer::{self, Error};
use kite_herdr::api::Agent;
use kite_ssh::Session;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Provider {
    pub id: String,
    pub managed_chat: bool,
    pub models: bool,
    pub attachments: bool,
    pub skills: bool,
    pub usage: bool,
    pub pause: bool,
    pub terminal: bool,
    #[serde(default)]
    pub release: bool,
}

#[derive(Debug, Deserialize)]
pub struct Scan {
    pub providers: Vec<Provider>,
    pub agents: Vec<Agent>,
}

pub fn harness_owner(id: &str) -> bool {
    matches!(id, "dsh" | "ply" | "antigravity")
}

pub fn registered(id: &str) -> bool {
    id == "claude" || managed(id)
}

pub fn managed(id: &str) -> bool {
    id == "codex" || harness_owner(id)
}

pub async fn scan(ssh: &Session) -> Result<Scan, Error> {
    let output = tokio::time::timeout(
        Duration::from_secs(55),
        ssh.exec(&composer::helper_command("harness-scan"), &[]),
    )
    .await
    .map_err(|_| Error::Timeout)??;
    if output.exit_status != Some(0) || output.stdout.len() > 2 * 1024 * 1024 {
        return Err(Error::Remote("provider discovery failed".into()));
    }
    let value: Scan = serde_json::from_slice(&output.stdout)?;
    let mut ids = std::collections::BTreeSet::new();
    if value
        .providers
        .iter()
        .any(|p| !registered(&p.id) || !ids.insert(&p.id))
        || value.agents.len() > 1024
        || value.agents.iter().any(|a| {
            !a.agent.as_deref().is_some_and(harness_owner)
                || !a.pane_id.starts_with("kite-harness-")
        })
    {
        return Err(Error::Invalid("invalid provider inventory"));
    }
    Ok(value)
}
