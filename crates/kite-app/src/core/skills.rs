use super::*;

impl AppCore {
    /// Refresh only on explicit picker entry or retry; normal chat polling stays cheap.
    pub async fn refresh_skills(self: &Arc<Self>, id: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            let generation = core.generation.load(Ordering::SeqCst);
            let agent = core
                .snapshot()
                .agents
                .into_iter()
                .find(|a| a.terminal_id == id)
                .ok_or_else(|| AppError::State("agent unavailable".into()))?;
            let key = AgentKey::new(&c.host.id, &agent);
            let conversation = core.agent_conversation(&c.host.id, &agent);
            let client = core
                .chat_clients
                .lock()
                .await
                .get(&key)
                .cloned()
                .ok_or_else(|| AppError::State("chat backend unavailable".into()))?;
            {
                let mut drafts = lock(&core.skill_drafts);
                let draft = drafts.entry(key.clone()).or_default();
                draft.loading = true;
                draft.details.clear();
            }
            core.publish_skills(&conversation, &key);
            let result = match agent.cwd.as_deref() {
                Some(cwd) => client.list_skills(cwd).await,
                None => Err(composer::Error::Invalid(
                    "session working directory unavailable",
                )),
            };
            lock(&core.skill_drafts)
                .entry(key.clone())
                .or_default()
                .finish(result);
            if core.generation.load(Ordering::SeqCst) == generation {
                core.publish_skills(&conversation, &key);
            }
            Ok(())
        })
        .await
    }

    pub async fn select_skill(
        self: &Arc<Self>,
        id: String,
        path: String,
        selected: bool,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            let agent = core
                .snapshot()
                .agents
                .into_iter()
                .find(|a| a.terminal_id == id)
                .ok_or_else(|| AppError::State("agent unavailable".into()))?;
            let key = AgentKey::new(&c.host.id, &agent);
            lock(&core.skill_drafts)
                .entry(key.clone())
                .or_default()
                .select(&path, selected)?;
            core.publish_skills(&core.agent_conversation(&c.host.id, &agent), &key);
            Ok(())
        })
        .await
    }

    pub(super) fn publish_skills(&self, conversation: &ConversationKey, key: &AgentKey) {
        let draft = lock(&self.skill_drafts)
            .get(key)
            .cloned()
            .unwrap_or_default();
        self.update_live(conversation, |live| {
            if live.agent.as_ref().is_some_and(|a| a.key == *key) {
                live.composer.skills = draft;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChatSkill;

    struct Secrets;
    impl SecretStorage for Secrets {
        fn read(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
        fn write(&self, _: &str, _: &[u8]) -> Result<(), String> {
            Ok(())
        }
    }
    struct Quiet;
    impl Observer for Quiet {
        fn changed(&self, _: Snapshot) {}
    }

    #[test]
    fn stale_catalog_and_selection_updates_cannot_cross_hosts_or_threads() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Quiet));
        let agent: Agent = serde_json::from_value(serde_json::json!({
            "terminal_id":"same-pane", "agent":"codex", "agent_status":"idle", "workspace_id":"w", "tab_id":"t", "pane_id":"p", "cwd":"/project", "revision":1,
            "agent_session":{"source":"herdr:codex","agent":"codex","kind":"id","value":"thread-a"}
        })).unwrap();
        let key = AgentKey::new("host-a", &agent);
        let skill = ChatSkill {
            name: "report".into(),
            description: "Write a report".into(),
            path: "/skills/report/SKILL.md".into(),
            display_name: Some("Project progress report".into()),
            short_description: Some("Summarize project progress".into()),
        };
        lock(&core.skill_drafts).insert(
            key.clone(),
            SkillDraft {
                available: vec![skill.clone()],
                selected: vec![skill.clone()],
                loaded: true,
                ..SkillDraft::default()
            },
        );
        let conversation = ConversationKey::new("host-a", &agent.terminal_id);
        core.select(Some(conversation.clone()), |s| {
            s.selected_host = Some("host-a".into());
            s.agents = vec![agent];
        });
        core.publish_skills(&conversation, &key);
        assert_eq!(
            core.snapshot().composer.skills.selected.as_slice(),
            std::slice::from_ref(&skill)
        );
        // A late update for the first host stays with that host's session.
        core.select(Some(ConversationKey::new("host-b", "same-pane")), |s| {
            s.selected_host = Some("host-b".into());
        });
        core.publish_skills(&conversation, &key);
        assert!(core.snapshot().composer.skills.selected.is_empty());
        // The pane moved to another thread, so its old draft no longer applies.
        core.select(Some(conversation.clone()), |s| {
            s.selected_host = Some("host-a".into());
            s.agents[0].agent_session.as_mut().unwrap().value = "thread-b".into();
        });
        core.publish_skills(&conversation, &key);
        assert!(core.snapshot().composer.skills.available.is_empty());
        core.update(|s| {
            s.agents[0].agent_session.as_mut().unwrap().value = "thread-a".into();
        });
        core.publish_skills(&conversation, &key);
        assert_eq!(core.snapshot().composer.skills.selected, [skill]);
    }
}
