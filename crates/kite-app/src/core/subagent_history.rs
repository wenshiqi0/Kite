//! Read-only inspection of a recorded descendant, without selecting or resuming it.
use super::*;
use kite_agent::sessions::Record;
use kite_herdr::api::AgentSession;

#[derive(Clone, Debug)]
pub struct SubagentInspection {
    pub items: Vec<Item>,
    pub has_older: bool,
}

fn descendant(record: &Record, id: &str) -> Result<AgentSession, AppError> {
    let unavailable = || AppError::State("subagent unavailable in this conversation".into());
    let reference = record.reference.as_ref().ok_or_else(unavailable)?;
    if record.provider != "codex"
        || reference.agent != "codex"
        || reference.kind != "id"
        || reference.value.is_empty()
    {
        return Err(unavailable());
    }
    let agent = record
        .subagents
        .iter()
        .find(|a| a.id == id)
        .ok_or_else(unavailable)?;
    let mut node = agent;
    let mut seen = BTreeSet::new();
    loop {
        if node.id == reference.value || !seen.insert(&node.id) || seen.len() > 32 {
            return Err(unavailable());
        }
        if node.parent_id == reference.value {
            break;
        }
        node = record
            .subagents
            .iter()
            .find(|a| a.id == node.parent_id)
            .ok_or_else(unavailable)?;
    }
    let mut source = reference.clone();
    source.value = agent.id.clone();
    Ok(source)
}

impl AppCore {
    pub async fn inspect_subagent(
        self: &Arc<Self>,
        host: String,
        parent: String,
        child: String,
    ) -> Result<SubagentInspection, AppError> {
        let core = self.clone();
        dispatch(async move {
            let key = ConversationKey::new(host, parent);
            let c = core.connection()?;
            if c.host.id != key.host {
                return Err(AppError::State("workspace changed".into()));
            }
            let record = core
                .session_record(&key.host, &key.conversation)
                .ok_or_else(|| AppError::State("parent conversation unavailable".into()))?;
            let reference = descendant(&record, &child)?;
            // The existing bounded provider-history reader performs no owner/turn actions.
            let history = history::read(&c.ssh, &reference, None).await;
            if !history.available {
                return Err(AppError::State("subagent history unavailable".into()));
            }
            Ok(SubagentInspection {
                items: history.items.clone(),
                has_older: history.page.as_ref().is_some_and(|p| p.has_older),
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kite_agent::subagents::Subagent;
    fn record() -> Record {
        let mut record = Record::draft(
            "parent-session".into(),
            "codex".into(),
            String::new(),
            None,
            0,
        );
        record.reference = Some(AgentSession {
            source: "herdr:codex".into(),
            agent: "codex".into(),
            kind: "id".into(),
            value: "parent-thread".into(),
        });
        record.subagents = [("child", "parent-thread"), ("grandchild", "child")]
            .into_iter()
            .map(|(id, parent)| Subagent {
                id: id.into(),
                parent_id: parent.into(),
                name: id.into(),
                task: "inspect".into(),
                status: "done".into(),
                updated_ms: 1,
            })
            .collect();
        record
    }
    #[test]
    fn reads_child_and_grandchild_with_the_original_provider_source() {
        let record = record();
        let before = record.clone();
        for id in ["child", "grandchild"] {
            let reference = descendant(&record, id).unwrap();
            assert_eq!(reference.value, id);
            assert_eq!(reference.source, "herdr:codex");
        }
        assert_eq!(record, before);
    }
    #[test]
    fn rejects_other_conversations_missing_ancestry_and_cycles() {
        let mut record = record();
        assert!(descendant(&record, "other-thread").is_err());
        assert!(descendant(&record, "parent-thread").is_err());
        record.subagents[0].parent_id = "another-parent".into();
        assert!(descendant(&record, "grandchild").is_err());
        record.subagents[0].parent_id = "grandchild".into();
        assert!(descendant(&record, "grandchild").is_err());
    }
    #[test]
    fn rejects_missing_sources_and_other_provider_kinds() {
        let mut record = record();
        record.reference.as_mut().unwrap().kind = "path".into();
        assert!(descendant(&record, "child").is_err());
        record.reference.as_mut().unwrap().kind = "id".into();
        record.provider = "claude".into();
        assert!(descendant(&record, "child").is_err());
        record.provider = "codex".into();
        record.reference = None;
        assert!(descendant(&record, "child").is_err());
    }
}
