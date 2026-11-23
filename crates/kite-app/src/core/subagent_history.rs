//! Read-only inspection of a recorded descendant, without selecting or resuming it.
use super::*;
use kite_agent::sessions::Record;
use kite_agent::subagents::{AssociatedSession, AssociatedTask, Subagent, delegated_status};
use kite_herdr::api::AgentSession;

#[derive(Clone, Debug)]
pub struct SubagentInspection {
    pub items: Vec<Item>,
    pub has_older: bool,
}

pub(super) fn associate(workspaces: &mut [HostConnection], hosts: &[Host]) {
    for workspace in workspaces.iter_mut() {
        for record in &mut workspace.sessions {
            record.associated_tasks = record.subagents.iter().cloned().map(Into::into).collect();
            record.delegated_child = false;
        }
    }
    let mut links = Vec::new();
    for (child_host, workspace) in workspaces.iter().enumerate() {
        for (child_row, child) in workspace.sessions.iter().enumerate() {
            let Some(delegation) = &child.delegation else {
                continue;
            };
            let Some(parent) = &delegation.parent else {
                continue;
            };
            let exact = workspaces
                .iter()
                .position(|entry| entry.host_id == parent.machine);
            let mut candidates = workspaces.iter().enumerate().filter_map(|(index, entry)| {
                let machine_matches = exact.map_or_else(
                    || {
                        entry.machine.as_deref() == Some(parent.machine.as_str())
                            || hosts
                                .iter()
                                .any(|host| host.id == entry.host_id && host.host == parent.machine)
                    },
                    |exact| exact == index,
                );
                if !machine_matches {
                    return None;
                }
                let row = entry.sessions.iter().position(|record| {
                    record.id == parent.session_id
                        && parent
                            .provider
                            .as_ref()
                            .is_none_or(|provider| &record.provider == provider)
                })?;
                Some((index, row))
            });
            let Some((host, row)) = candidates.next() else {
                continue;
            };
            if candidates.next().is_some() || (host, row) == (child_host, child_row) {
                continue;
            }
            let connected = workspace.connection == ConnectionPhase::Connected;
            let target = AssociatedSession {
                host_id: workspace.host_id.clone(),
                session_id: child.id.clone(),
                machine: workspace
                    .machine
                    .clone()
                    .or_else(|| {
                        hosts
                            .iter()
                            .find(|h| h.id == workspace.host_id)
                            .map(|h| h.host.clone())
                    })
                    .unwrap_or_else(|| workspace.host_id.clone()),
                provider: child.provider.clone(),
                connected,
            };
            let task = AssociatedTask {
                activity: Subagent {
                    id: format!("delegated/{}/{}", workspace.host_id, child.id),
                    parent_id: parent.session_id.clone(),
                    name: delegation
                        .label
                        .clone()
                        .unwrap_or_else(|| child.title.clone()),
                    task: delegation.role.clone().unwrap_or_default(),
                    status: delegated_status(connected, child.archived, &child.status).into(),
                    updated_ms: child.updated_ms,
                },
                session: Some(target),
            };
            links.push((host, row, child_host, child_row, task));
        }
    }
    for (host, row, child_host, child_row, task) in links {
        let parent = &mut workspaces[host].sessions[row];
        let visible = !parent.archived
            && parent
                .delegation
                .as_ref()
                .is_none_or(|group| group.parent.is_none());
        parent.associated_tasks.push(task);
        workspaces[child_host].sessions[child_row].delegated_child = visible;
    }
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
    pub(super) async fn inspect_associated_session(
        self: &Arc<Self>,
        host: String,
        id: String,
    ) -> Result<SubagentInspection, AppError> {
        let core = self.clone();
        dispatch(async move {
            let record = core
                .session_record(&host, &id)
                .ok_or_else(|| AppError::State("associated session unavailable".into()))?;
            let c = core.connection()?;
            if c.host.id != host {
                return Err(AppError::State("workspace changed".into()));
            }
            let reference = record
                .reference
                .as_ref()
                .ok_or_else(|| AppError::State("associated session history unavailable".into()))?;
            let history = history::read(&c.ssh, reference, None).await;
            if !history.available {
                return Err(AppError::State(
                    "associated session history unavailable".into(),
                ));
            }
            Ok(SubagentInspection {
                items: history.items.clone(),
                has_older: history.page.as_ref().is_some_and(|p| p.has_older),
            })
        })
        .await
    }

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

    #[test]
    fn cross_provider_tasks_join_by_machine_and_keep_offline_facts_out_of_activity() {
        use kite_agent::sessions::{DelegationPolicy, SessionDelegation, SessionParent};

        let parent = record();
        let mut child = Record::draft("remote".into(), "ply".into(), "Remote task".into(), None, 2);
        child.delegation = Some(SessionDelegation {
            parent: Some(SessionParent {
                machine: "real-parent-host".into(),
                session_id: parent.id.clone(),
                provider: Some("codex".into()),
            }),
            label: Some("Review change".into()),
            role: Some("reviewer".into()),
            policy: DelegationPolicy::HumanInTheLoop,
            worktree_isolated: None,
            write_lock: Some(true),
        });
        let workspace = |host: &str, machine: &str, record: Record, connection| HostConnection {
            host_id: host.into(),
            machine: Some(machine.into()),
            connection,
            connection_step: ConnectionStep::Idle,
            failure: None,
            resources: None,
            sessions: sessions::views(&[record], &[], SessionSort::Newest),
            selected_session: None,
            providers: vec![],
            session_sort: SessionSort::Newest,
        };
        let mut workspaces = [
            workspace(
                "saved-parent",
                "real-parent-host",
                parent,
                ConnectionPhase::Connected,
            ),
            workspace(
                "saved-child",
                "real-child-host",
                child,
                ConnectionPhase::Disconnected,
            ),
        ];
        workspaces[1].sessions[0].status = "working".into();
        associate(&mut workspaces, &[]);
        assert!(workspaces[1].sessions[0].delegated_child);
        let activities = &workspaces[0].sessions[0].associated_tasks;
        assert_eq!(activities.len(), 3);
        assert!(activities[..2].iter().all(|task| task.session.is_none()));
        let task = &activities[2];
        assert_eq!(task.activity.status, "offline");
        assert_eq!(task.activity.name, "Review change");
        let target = task.session.as_ref().unwrap();
        assert_eq!(target.host_id, "saved-child");
        assert_eq!(target.session_id, "remote");
        assert_eq!(target.provider, "ply");
        assert!(!target.connected);
        workspaces[1].connection = ConnectionPhase::Connected;
        workspaces[1].sessions[0].status = "question".into();
        associate(&mut workspaces, &[]);
        assert_eq!(
            workspaces[0].sessions[0].associated_tasks[2]
                .activity
                .status,
            "question"
        );
    }
}
