//! Durable host-owned conversation metadata. Provider history remains authoritative.
use crate::{
    composer::{self, Error},
    fingerprint, history,
};
use kite_herdr::api::{Agent, AgentSession};
use kite_ssh::Session;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub provider: String,
    pub reference: Option<AgentSession>,
    pub title: String,
    pub automatic_title: bool,
    pub preview: String,
    pub cwd: Option<String>,
    pub archived: bool,
    pub created_ms: i64,
    pub updated_ms: i64,
    pub terminal_id: String,
    pub pane_id: String,
    pub workspace_id: String,
    pub agent_name: String,
}
impl Record {
    pub fn matches(&self, agent: &Agent) -> bool {
        self.reference
            .as_ref()
            .zip(agent.agent_session.as_ref())
            .is_some_and(|(a, b)| a == b && a.agent == self.provider)
            || (self.reference.is_none()
                && !self.terminal_id.is_empty()
                && self.terminal_id == agent.terminal_id)
    }
    pub fn draft(
        id: String,
        provider: String,
        title: String,
        cwd: Option<String>,
        now: i64,
    ) -> Self {
        Self {
            id,
            provider,
            reference: None,
            automatic_title: title.is_empty(),
            title,
            cwd,
            preview: String::new(),
            archived: false,
            created_ms: now,
            updated_ms: now,
            terminal_id: String::new(),
            pane_id: String::new(),
            workspace_id: String::new(),
            agent_name: String::new(),
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Catalog {
    pub revision: u64,
    pub now_ms: i64,
    pub sessions: Vec<Record>,
}

pub struct Repository<'a> {
    ssh: &'a Session,
}
impl<'a> Repository<'a> {
    pub fn new(ssh: &'a Session) -> Self {
        Self { ssh }
    }
    pub async fn read(&self) -> Result<Catalog, Error> {
        let output = self
            .ssh
            .exec(&composer::helper_command("catalog-read"), &[])
            .await?;
        if output.exit_status != Some(0) {
            return Err(Error::Remote("session catalog unavailable".into()));
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    pub async fn change<T>(
        &self,
        mut change: impl FnMut(&mut Vec<Record>, i64) -> Result<T, Error>,
    ) -> Result<(Catalog, T), Error> {
        for _ in 0..4 {
            let mut catalog = self.read().await?;
            let original = catalog.sessions.clone();
            let value = change(&mut catalog.sessions, catalog.now_ms)?;
            if original == catalog.sessions {
                return Ok((catalog, value));
            }
            if catalog.sessions.len() > 1024 {
                return Err(Error::Invalid("session catalog capacity"));
            }
            let payload = serde_json::to_vec(&catalog)?;
            if payload.len() > 1024 * 1024 {
                return Err(Error::Invalid("session catalog size"));
            }
            let output = self
                .ssh
                .exec(&composer::helper_command("catalog-write"), &payload)
                .await?;
            if output.exit_status != Some(0) {
                return Err(Error::Remote("session catalog write failed".into()));
            }
            let response: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            if response["conflict"] == true {
                continue;
            }
            return Ok((serde_json::from_value(response)?, value));
        }
        Err(Error::Conflict)
    }
    pub async fn observe(&self, agents: &[Agent]) -> Result<Catalog, Error> {
        self.change(|records, now| {
            for agent in agents {
                let Some(reference) = agent
                    .agent_session
                    .as_ref()
                    .filter(|r| matches!(r.agent.as_str(), "codex" | "claude"))
                else {
                    continue;
                };
                let index = records.iter().position(|r| r.matches(agent));
                let record = if let Some(index) = index {
                    &mut records[index]
                } else {
                    let mut record = Record::draft(
                        history::key(reference),
                        reference.agent.clone(),
                        agent.name.clone().unwrap_or_default(),
                        agent.cwd.clone(),
                        now,
                    );
                    record.automatic_title = false;
                    records.push(record);
                    records.last_mut().expect("record inserted")
                };
                record.reference = Some(reference.clone());
                record.terminal_id = agent.terminal_id.clone();
                record.pane_id = agent.pane_id.clone();
                record.workspace_id = agent.workspace_id.clone();
                record.agent_name = agent.name.clone().unwrap_or_default();
                if record.cwd.is_none() {
                    record.cwd = agent.cwd.clone();
                }
            }
            Ok(())
        })
        .await
        .map(|(c, ())| c)
    }
    pub async fn archive(&self, id: &str, archived: bool) -> Result<Catalog, Error> {
        self.change(|records, _| {
            let record = records
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or(Error::Invalid("session unavailable"))?;
            record.archived = archived;
            Ok(())
        })
        .await
        .map(|(c, ())| c)
    }
    pub async fn create(
        &self,
        id: String,
        provider: String,
        title: String,
        cwd: Option<String>,
    ) -> Result<Catalog, Error> {
        if !matches!(provider.as_str(), "codex" | "claude") || title.len() > 512 {
            return Err(Error::Invalid("session parameters"));
        }
        self.change(|records, now| {
            if !records.iter().any(|r| r.id == id) {
                records.push(Record::draft(
                    id.clone(),
                    provider.clone(),
                    title.clone(),
                    cwd.clone(),
                    now,
                ));
            }
            Ok(())
        })
        .await
        .map(|(c, ())| c)
    }
    pub async fn note(&self, id: &str, text: &str, first_user: bool) -> Result<Catalog, Error> {
        self.change(|records, now| {
            if let Some(record) = records.iter_mut().find(|r| r.id == id) {
                let preview: String = text.trim().chars().take(120).collect();
                if record.preview != preview {
                    record.preview = preview;
                    record.updated_ms = now;
                }
                if first_user && record.automatic_title && !text.trim().is_empty() {
                    record.title = text
                        .trim()
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .take(48)
                        .collect();
                    record.automatic_title = false;
                }
            }
            Ok(())
        })
        .await
        .map(|(c, ())| c)
    }
    pub async fn resume(&self, id: &str, herdr: &str) -> Result<Agent, Error> {
        let result = self
            .ssh
            .exec(
                &composer::helper_command(&format!(
                    "session-open --id {} --herdr {}",
                    composer::quote(id),
                    composer::quote(herdr)
                )),
                &[],
            )
            .await?;
        if result.exit_status != Some(0) {
            return Err(Error::Remote(
                String::from_utf8_lossy(&result.stderr)
                    .chars()
                    .take(2048)
                    .collect(),
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(&result.stdout)?;
        Ok(serde_json::from_value(value["agent"].clone())?)
    }
}

pub fn cache_key(host: &str, id: &str) -> String {
    fingerprint(&format!("{host}/{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_is_part_of_session_identity_and_drafts_are_not_provider_history() {
        let a = AgentSession {
            source: "herdr:codex".into(),
            agent: "codex".into(),
            kind: "id".into(),
            value: "same".into(),
        };
        let mut b = a.clone();
        b.agent = "claude".into();
        assert_ne!(history::key(&a), history::key(&b));
        let draft = Record::draft("draft".into(), "claude".into(), "".into(), None, 5);
        assert!(draft.reference.is_none() && !draft.archived && draft.automatic_title);
        assert_ne!(cache_key("host-a", "draft"), cache_key("host-b", "draft"));
    }
}
