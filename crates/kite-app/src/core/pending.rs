//! Reconcile optimistic rows and provider echoes at the serialized publication boundary.
use super::*;

fn selected_key(s: &Snapshot) -> Option<String> {
    let host = s.selected_host.as_ref()?;
    let id = s.selected_agent.as_ref()?;
    let agent = s.agents.iter().find(|a| &a.terminal_id == id)?;
    Some(pending_scope(host, agent))
}

impl AppCore {
    /// Called inside update: the canonical snapshot cannot change between the
    /// echo check and adding an optimistic row. All producers use the same path.
    pub(super) fn stage_pending_message(&self, s: &mut Snapshot, message: PendingMessage) {
        let key = message.key.clone();
        {
            let mut pending = lock(&self.pending_messages);
            if pending.iter().any(|p| p.message.id == message.message.id) {
                return;
            }
            if pending.len() >= 32 {
                let index = pending
                    .iter()
                    .position(|p| p.receipt.is_some())
                    .unwrap_or(0);
                pending.remove(index);
            }
            pending.push(message);
        }
        if selected_key(s).as_ref() == Some(&key) {
            self.publish_conversation(s, s.conversation.as_ref().clone(), true);
        }
    }

    /// Keep a bounded acknowledgement receipt so an already prepared history
    /// page cannot resurrect a local row or briefly erase its canonical echo.
    pub(super) fn publish_conversation(
        &self,
        s: &mut Snapshot,
        mut proposed: Vec<MessageRef>,
        available: bool,
    ) {
        let key = selected_key(s);
        proposed.retain(|m| !m.id.starts_with("local-"));
        let mut messages = proposed;
        let mut pending = lock(&self.pending_messages);
        let active = |p: &&PendingMessage| Some(&p.key) == key.as_ref();
        if !pending.iter().any(|p| Some(&p.key) == key.as_ref()) {
            drop(pending);
            s.replace_shared_conversation(messages, available);
            return;
        }
        let mut acknowledged: BTreeSet<_> = pending
            .iter()
            .filter(active)
            .filter_map(|p| p.receipt.as_ref().map(|m| m.id.clone()))
            .collect();
        for p in pending.iter_mut().filter(|p| Some(&p.key) == key.as_ref()) {
            if let Some(receipt) = &p.receipt {
                if let Some(current) = messages.iter().find(|m| m.id == receipt.id) {
                    p.receipt = Some(current.clone());
                }
            } else if let Some(echo) = messages.iter().find(|m| {
                m.role == Role::User
                    && m.text == p.message.text
                    && !p.previous.contains(&m.id)
                    && !acknowledged.contains(&m.id)
            }) {
                acknowledged.insert(echo.id.clone());
                p.receipt = Some(echo.clone());
                p.previous.clear();
            }
        }
        let visible: std::collections::HashSet<_> =
            messages.iter().map(|m| m.id.as_str()).collect();
        let missing: Vec<_> = pending
            .iter()
            .filter(active)
            .map(|p| {
                p.receipt
                    .clone()
                    .unwrap_or_else(|| Arc::new(p.message.clone()))
            })
            .filter(|row| !visible.contains(row.id.as_str()))
            .collect();
        messages.extend(missing);
        drop(pending);
        s.replace_shared_conversation(messages, available);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Secrets;
    impl SecretStorage for Secrets {
        fn read(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
        fn write(&self, _: &str, _: &[u8]) -> Result<(), String> {
            Ok(())
        }
    }
    #[derive(Default)]
    struct Recording(Mutex<Vec<Vec<String>>>);
    impl Observer for Recording {
        fn changed(&self, s: Snapshot) {
            lock(&self.0).push(s.conversation.iter().map(|m| m.id.clone()).collect());
        }
    }
    fn core() -> (Arc<AppCore>, Arc<Recording>) {
        let observer = Arc::new(Recording::default());
        let core = AppCore::new(Arc::new(Secrets), observer.clone());
        core.update(|s| {
            s.selected_host = Some("host".into());
            s.selected = Some(ConversationKey::new("host", "terminal"));
            s.agents = vec![serde_json::from_value(serde_json::json!({
                "terminal_id":"terminal","agent_status":"working","workspace_id":"workspace","tab_id":"tab","pane_id":"workspace:p1","revision":1
            })).unwrap()];
        });
        (core, observer)
    }
    fn row(id: &str) -> MessageRef {
        Arc::new(Message {
            id: id.into(),
            role: Role::User,
            text: "owned repeated prompt".into(),
            intermediate: false,
            tool: None,
            question_replies: Vec::new(),
        })
    }
    fn stage(core: &AppCore, id: &str, previous: &[&str]) {
        core.update(|s| {
            core.stage_pending_message(
                s,
                PendingMessage {
                    key: selected_key(s).unwrap(),
                    message: row(id).as_ref().clone(),
                    previous: previous.iter().map(|id| (*id).into()).collect(),
                    receipt: None,
                },
            );
        });
    }
    fn publish(core: &AppCore, rows: Vec<MessageRef>) {
        core.update(|s| core.publish_conversation(s, rows, true));
    }
    fn ids(core: &AppCore) -> Vec<String> {
        core.snapshot()
            .conversation
            .iter()
            .map(|m| m.id.clone())
            .collect()
    }
    #[test]
    fn prompt_transport_is_locked_until_launch_and_chat_setup_complete() {
        let (core, _) = core();
        let mut snapshot = core.snapshot();
        let mut agent = snapshot.agents[0].clone();
        agent.agent = Some("codex".into());
        agent.launch_pending = true;
        snapshot.composer.ready = true;
        assert!(require_prompt_ready(&snapshot, &agent).is_err());
        agent.launch_pending = false;
        snapshot.composer.ready = false;
        assert!(require_prompt_ready(&snapshot, &agent).is_err());
        snapshot.composer.ready = true;
        assert!(require_prompt_ready(&snapshot, &agent).is_ok());
    }
    #[test]
    fn echo_before_send_callback_never_publishes_a_second_bubble() {
        let (core, observed) = core();
        publish(&core, vec![row("server")]);
        stage(&core, "local-prompt", &[]);
        assert_eq!(ids(&core), ["server"]);
        assert!(lock(&observed.0).iter().all(|rows| rows.len() <= 1));
    }
    #[test]
    fn local_first_is_replaced_once_and_a_prepared_page_cannot_resurrect_it() {
        let (core, observed) = core();
        stage(&core, "local-prompt", &[]);
        let prepared_page = core.snapshot().conversation.as_ref().clone();
        let before = core.snapshot().conversation_revision;
        publish(&core, vec![row("server")]);
        let acknowledged_revision = core.snapshot().conversation_revision;
        let delta = core.conversation_since("host/terminal", before);
        assert_eq!(delta.edits.len(), 1);
        assert_eq!(delta.edits[0].remove, 1);
        assert_eq!(delta.edits[0].items[0].id, "server");
        publish(&core, prepared_page);
        assert_eq!(ids(&core), ["server"]);
        assert_eq!(core.snapshot().conversation_revision, acknowledged_revision);
        assert!(lock(&observed.0).iter().skip(1).all(|rows| rows.len() == 1));
    }
    #[test]
    fn two_deliberate_identical_sends_each_keep_their_own_echo() {
        let (core, _) = core();
        publish(&core, vec![row("old")]);
        stage(&core, "local-first", &["old"]);
        stage(&core, "local-second", &["old", "local-first"]);
        publish(&core, vec![row("old"), row("echo-first")]);
        assert_eq!(ids(&core), ["old", "echo-first", "local-second"]);
        publish(
            &core,
            vec![row("old"), row("echo-first"), row("echo-second")],
        );
        assert_eq!(ids(&core), ["old", "echo-first", "echo-second"]);
    }
    #[test]
    fn another_host_or_assistant_text_cannot_acknowledge_the_prompt() {
        let (core, _) = core();
        stage(&core, "local-prompt", &[]);
        core.update(|s| s.selected_host = Some("another-host".into()));
        publish(&core, vec![row("other-host-echo")]);
        assert_eq!(ids(&core), ["other-host-echo"]);
        core.update(|s| {
            s.selected_host = Some("host".into());
            s.selected = Some(ConversationKey::new("host", "terminal"));
        });
        let mut assistant = row("assistant").as_ref().clone();
        assistant.role = Role::Assistant;
        publish(&core, vec![Arc::new(assistant)]);
        assert_eq!(ids(&core), ["assistant", "local-prompt"]);
        assert!(lock(&core.pending_messages)[0].receipt.is_none());
    }
    #[test]
    fn simultaneous_echo_and_send_callbacks_publish_only_one_row() {
        for _ in 0..100 {
            let (core, observed) = core();
            let barrier = std::sync::Barrier::new(2);
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    barrier.wait();
                    stage(&core, "local-prompt", &[]);
                });
                scope.spawn(|| {
                    barrier.wait();
                    publish(&core, vec![row("server")]);
                });
            });
            assert_eq!(ids(&core), ["server"]);
            assert!(lock(&observed.0).iter().all(|rows| rows.len() <= 1));
        }
    }
    #[test]
    fn acknowledged_question_answers_retain_their_paired_metadata() {
        let (core, _) = core();
        let mut answer = row("local-answer").as_ref().clone();
        answer.question_replies = vec![kite_agent::QuestionReply {
            id: "q".into(),
            question: "Which choice?".into(),
            answer: "First".into(),
        }];
        core.update(|s| {
            core.stage_pending_message(
                s,
                PendingMessage {
                    key: selected_key(s).unwrap(),
                    message: answer.clone(),
                    previous: BTreeSet::new(),
                    receipt: None,
                },
            )
        });
        let mut echo = answer.clone();
        echo.id = "answer-echo".into();
        publish(&core, vec![Arc::new(echo)]);
        publish(&core, vec![Arc::new(answer)]);
        let snapshot = core.snapshot();
        assert_eq!(snapshot.conversation.len(), 1);
        assert_eq!(snapshot.conversation[0].id, "answer-echo");
        assert_eq!(snapshot.conversation[0].question_replies[0].answer, "First");
    }
}
