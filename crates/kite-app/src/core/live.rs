//! One conversation's runtime (RFC 0027). Switching conversations changes which
//! object the snapshot projects; nothing is cleared field by field.
use super::*;
use kite_agent::interaction::RuntimeState;
use sessions::CachedConversation;
use std::collections::VecDeque;

/// Conversations kept live at once, matching the in-memory transcript cache.
const LIVE_LIMIT: usize = 4;

/// The pane currently serving a conversation.
#[derive(Clone, Debug)]
pub(super) struct AgentBinding {
    pub(super) terminal: String,
    /// The pane's current lookup key; it changes when the pane learns its thread.
    pub(super) key: AgentKey,
    /// The last thread the pane reported.
    thread: Option<String>,
}
impl AgentBinding {
    fn new(host: &str, agent: &Agent) -> Self {
        Self {
            terminal: agent.terminal_id.clone(),
            key: AgentKey::new(host, agent),
            thread: agent
                .agent_session
                .as_ref()
                .map(ConversationKey::history_key),
        }
    }
    /// A pane that learns or briefly loses its thread is still the same agent; only
    /// another terminal or a different thread replaces it.
    fn same_agent(&self, next: &Self) -> bool {
        self.terminal == next.terminal
            && (self.thread.is_none() || next.thread.is_none() || self.thread == next.thread)
    }
    fn follow(&mut self, next: Self) {
        self.key = next.key;
        if next.thread.is_some() {
            self.thread = next.thread;
        }
    }
}

pub(super) struct PendingItem {
    pub(super) item: Item,
    pub(super) previous: BTreeSet<String>,
    pub(super) receipt: Option<ItemRef>,
}

pub(super) struct LiveSession {
    pub(super) agent: Option<AgentBinding>,
    /// The pane `session-open` returned for this conversation, until it reports a thread.
    pub(super) launching: Option<String>,
    pub(super) history: Option<Arc<history::History>>,
    pub(super) transcript: conversation::Transcript,
    pub(super) sync_job: Option<JoinHandle<()>>,
    pub(super) pending: Vec<PendingItem>,
    pub(super) composer: Composer,
    pub(super) approval: Option<Approval>,
    pub(super) question: Option<QuestionPrompt>,
    pub(super) response_activity: ResponseActivity,
    pub(super) agent_pause: AgentPause,
    pub(super) agent_output: String,
    pub(super) conversation: SharedItems,
    pub(super) journal: Arc<VecDeque<ConversationEdit>>,
    pub(super) revision: u64,
    pub(super) available: bool,
    pub(super) history_status: HistoryStatus,
    viewed: u64,
}
impl LiveSession {
    fn new(revision: u64, seed: Option<CachedConversation>) -> Self {
        let (conversation, available) = seed
            .map(|cache| (cache.messages, cache.available))
            .unwrap_or_default();
        Self {
            agent: None,
            launching: None,
            history: None,
            transcript: conversation::Transcript::default(),
            sync_job: None,
            pending: Vec::new(),
            composer: Composer::default(),
            approval: None,
            question: None,
            response_activity: ResponseActivity::Idle,
            agent_pause: AgentPause::Running,
            agent_output: String::new(),
            conversation,
            journal: Arc::new(VecDeque::new()),
            revision,
            available,
            history_status: HistoryStatus::default(),
            viewed: 0,
        }
    }
    /// A request that only this conversation can show keeps it out of eviction.
    fn needs_attention(&self) -> bool {
        self.approval.is_some() || self.question.is_some()
    }
    /// Returns whether the serving agent changed, which starts it from an empty runtime.
    fn rebind(
        &mut self,
        next: Option<AgentBinding>,
        paused_token: impl FnOnce(&AgentBinding) -> bool,
    ) -> bool {
        match (&mut self.agent, next) {
            (None, None) => false,
            (Some(current), Some(next)) if current.same_agent(&next) => {
                current.follow(next);
                false
            }
            (_, next) => {
                let token = next.as_ref().is_some_and(paused_token);
                self.bind(next, token);
                true
            }
        }
    }
    /// What an agent left behind does not apply to the one serving the conversation next.
    fn bind(&mut self, agent: Option<AgentBinding>, paused_token: bool) {
        self.agent = agent;
        self.composer = Composer::default();
        self.approval = None;
        self.question = None;
        self.response_activity = ResponseActivity::Idle;
        self.agent_output.clear();
        self.agent_pause = if paused_token {
            AgentPause::Unknown
        } else {
            AgentPause::Running
        };
    }
    fn route(&mut self, runtime: &RuntimeState) {
        self.approval = runtime.approval();
        self.question = runtime.question();
        self.response_activity = runtime.phase();
    }
}
impl Drop for LiveSession {
    fn drop(&mut self) {
        if let Some(job) = self.sync_job.take() {
            job.abort();
        }
    }
}

#[derive(Default)]
pub(super) struct LiveSessions {
    sessions: BTreeMap<ConversationKey, LiveSession>,
    clock: u64,
    /// A reopened conversation restarts above every revision native may hold for it.
    revision_floor: u64,
}
impl LiveSessions {
    pub(super) fn get(&self, key: &ConversationKey) -> Option<&LiveSession> {
        self.sessions.get(key)
    }
    pub(super) fn get_mut(&mut self, key: &ConversationKey) -> Option<&mut LiveSession> {
        self.sessions.get_mut(key)
    }
    pub(super) fn bound_to(&self, host: &str, terminal: &str) -> Option<ConversationKey> {
        self.sessions
            .iter()
            .find(|(key, live)| {
                key.host == host && live.agent.as_ref().is_some_and(|a| a.terminal == terminal)
            })
            .map(|(key, _)| key.clone())
    }
    fn open(
        &mut self,
        key: &ConversationKey,
        seed: Option<CachedConversation>,
    ) -> &mut LiveSession {
        let revision = self.revision_floor.wrapping_add(1);
        self.sessions
            .entry(key.clone())
            .or_insert_with(|| LiveSession::new(revision, seed))
    }
    /// Only a selection counts as viewing; routing keeps the eviction order.
    fn view(&mut self, key: &ConversationKey, seed: Option<CachedConversation>) {
        self.clock += 1;
        let clock = self.clock;
        self.open(key, seed).viewed = clock;
    }
    /// The least recently viewed sessions over the bound. The caller persists them
    /// before dropping them; when everything is protected the bound waits.
    fn evict(&mut self, selected: Option<&ConversationKey>) -> Vec<(ConversationKey, LiveSession)> {
        let mut evicted = Vec::new();
        while self.sessions.len() > LIVE_LIMIT {
            let Some(key) = self
                .sessions
                .iter()
                .filter(|(key, live)| Some(*key) != selected && !live.needs_attention())
                .min_by_key(|(_, live)| live.viewed)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(live) = self.sessions.remove(&key) {
                self.revision_floor = self.revision_floor.max(live.revision);
                evicted.push((key, live));
            }
        }
        evicted
    }
    /// The caller persists the sessions before dropping them.
    pub(super) fn drain(&mut self) -> Vec<(ConversationKey, LiveSession)> {
        let sessions = std::mem::take(&mut self.sessions);
        for live in sessions.values() {
            self.revision_floor = self.revision_floor.max(live.revision);
        }
        sessions.into_iter().collect()
    }
}

/// Everything one publication covers: the snapshot is only ever a projection of
/// the global fields plus the selected session.
#[derive(Default)]
pub(super) struct CoreState {
    pub(super) snapshot: Snapshot,
    pub(super) selected: Option<ConversationKey>,
    pub(super) live: LiveSessions,
}
impl CoreState {
    fn project(&mut self) {
        let Self {
            snapshot: s,
            selected,
            live,
        } = self;
        s.context = match selected {
            Some(key) => key.context(),
            None => ConversationKey::unselected_context(s.selected_host.as_deref()),
        };
        if let Some(session) = selected.as_ref().and_then(|key| live.get(key)) {
            s.selected_agent = session.agent.as_ref().map(|a| a.terminal.clone());
            s.agent_output.clone_from(&session.agent_output);
            s.conversation = session.conversation.clone();
            s.conversation_journal = session.journal.clone();
            s.conversation_revision = session.revision;
            s.conversation_available = session.available;
            s.history_status.clone_from(&session.history_status);
            s.approval.clone_from(&session.approval);
            s.question.clone_from(&session.question);
            s.response_activity = session.response_activity;
            s.agent_pause = session.agent_pause;
            s.composer.clone_from(&session.composer);
        } else {
            s.selected_agent = None;
            s.agent_output.clear();
            s.conversation = Arc::new(Vec::new());
            s.conversation_journal = Arc::new(VecDeque::new());
            s.conversation_revision = live.revision_floor;
            s.conversation_available = false;
            s.history_status = HistoryStatus::default();
            s.approval = None;
            s.question = None;
            s.response_activity = ResponseActivity::Idle;
            s.agent_pause = AgentPause::Running;
            s.composer = Composer::default();
        }
    }
}

impl AppCore {
    pub(super) fn update_state<T>(&self, f: impl FnOnce(&mut CoreState) -> T) -> T {
        let mut state = lock(&self.state);
        let value = f(&mut state);
        self.publish(state);
        value
    }
    /// Apply a change to one conversation; only the selected one is published.
    pub(super) fn update_live<T>(
        &self,
        key: &ConversationKey,
        f: impl FnOnce(&mut LiveSession) -> T,
    ) -> Option<T> {
        let mut state = lock(&self.state);
        let value = f(state.live.get_mut(key)?);
        if state.selected.as_ref() == Some(key) {
            self.publish(state);
        }
        Some(value)
    }
    pub(super) fn read_live<T>(
        &self,
        key: &ConversationKey,
        f: impl FnOnce(&LiveSession) -> T,
    ) -> Option<T> {
        lock(&self.state).live.get(key).map(f)
    }
    pub(super) fn selected(&self) -> Option<ConversationKey> {
        lock(&self.state).selected.clone()
    }
    fn publish(&self, mut state: std::sync::MutexGuard<'_, CoreState>) {
        self.reconcile(&mut state);
        state.snapshot.revision = state.snapshot.revision.saturating_add(1);
        let snapshot = state.snapshot.clone();
        drop(state);
        self.observer.changed(snapshot);
    }
    /// The catalog and live agents decide which pane serves each conversation; every
    /// publication re-derives that and projects the selected session from it. Returns
    /// whether that changed what the selected session shows.
    fn reconcile(&self, state: &mut CoreState) -> bool {
        let mut changed = false;
        if state.selected.as_ref().map(|key| &key.host) != state.snapshot.selected_host.as_ref()
            && state.selected.take().is_some()
        {
            changed = true;
        }
        let book = lock(&self.session_book);
        let paused = lock(&self.paused_processes);
        let agents = &state.snapshot.agents;
        for (key, live) in &mut state.live.sessions {
            // Once the pane reports a thread, the record alone decides.
            if live.launching.as_ref().is_some_and(|terminal| {
                agents
                    .iter()
                    .any(|a| &a.terminal_id == terminal && a.agent_session.is_some())
            }) {
                live.launching = None;
            }
            let agent = sessions::conversation_agent(key, &book, agents, live.launching.as_deref())
                .map(|agent| AgentBinding::new(&key.host, agent));
            let rebound = live.rebind(agent, |a| {
                paused.contains_key(&pane_key(&key.host, &a.terminal))
            });
            changed |= rebound && state.selected.as_ref() == Some(key);
        }
        drop(paused);
        let session = state
            .selected
            .as_ref()
            .and_then(|key| sessions::saved_record(key, &book))
            .map(|record| record.id.clone());
        drop(book);
        changed |= state.snapshot.selected_session != session;
        state.snapshot.selected_session = session;
        state.project();
        changed
    }
    /// A conversation's saved cache, read outside the state lock because it can be
    /// a whole transcript on disk. Live sessions already hold theirs.
    fn live_seed(&self, key: &ConversationKey) -> Option<CachedConversation> {
        if self.read_live(key, |_| ()).is_some() {
            return None;
        }
        self.cached_history(key)
    }
    /// Point the selection at `next` and publish `f` with it. Everything session
    /// scoped comes along with that session's object.
    pub(super) fn select(&self, next: Option<ConversationKey>, f: impl FnOnce(&mut Snapshot)) {
        self.select_state(next, |state| f(&mut state.snapshot));
    }
    pub(super) fn select_state(
        &self,
        next: Option<ConversationKey>,
        f: impl FnOnce(&mut CoreState),
    ) {
        let seed = next.as_ref().and_then(|key| self.live_seed(key));
        let evicted = self.update_state(|state| {
            if let Some(key) = &next {
                state.live.view(key, seed);
            }
            state.selected = next;
            f(state);
            state.live.evict(state.selected.as_ref())
        });
        self.retire(evicted);
    }
    /// Approvals and questions reach their own conversation while another is on
    /// screen. The selected one also has terminal state, read by `refresh_agent`.
    pub(super) fn route_runtime(&self, routed: Vec<(ConversationKey, RuntimeState)>) {
        let selected = self.selected();
        let routed: Vec<_> = routed
            .into_iter()
            .filter(|(key, _)| Some(key) != selected.as_ref())
            .filter(|(key, runtime)| {
                runtime.approval().is_some()
                    || runtime.question().is_some()
                    || self.read_live(key, |_| ()).is_some()
            })
            .map(|(key, runtime)| {
                let seed = self.live_seed(&key);
                (key, runtime, seed)
            })
            .collect();
        if routed.is_empty() {
            return;
        }
        let evicted = {
            let mut state = lock(&self.state);
            let mut runtimes = Vec::with_capacity(routed.len());
            for (key, runtime, seed) in routed {
                state.live.open(&key, seed);
                runtimes.push((key, runtime));
            }
            // Bind first: a new binding starts from an empty runtime.
            let changed = self.reconcile(&mut state);
            for (key, runtime) in runtimes {
                if let Some(live) = state.live.get_mut(&key).filter(|l| l.agent.is_some()) {
                    live.route(&runtime);
                }
            }
            let selected = state.selected.clone();
            let evicted = state.live.evict(selected.as_ref());
            // Nothing routed here is on screen, but this reconcile may have changed it.
            if changed {
                self.publish(state);
            }
            evicted
        };
        self.retire(evicted);
    }
    /// Sessions leaving the bounded set keep their transcript in the saved cache.
    pub(super) fn retire(&self, sessions: Vec<(ConversationKey, LiveSession)>) {
        if sessions.is_empty() {
            return;
        }
        for (key, live) in &sessions {
            if !live.conversation.is_empty()
                && self.session_record(&key.host, &key.conversation).is_some()
                && let Err(error) = self.cache_conversation(key, &live.conversation, live.available)
            {
                self.report(&error);
            }
        }
        if let Err(error) = self.flush_history_caches(true) {
            self.report(&error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kite_agent::sessions::{Catalog, Record};

    struct Secrets;
    impl SecretStorage for Secrets {
        fn read(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
        fn write(&self, _: &str, _: &[u8]) -> Result<(), String> {
            Ok(())
        }
    }
    struct Silent;
    impl Observer for Silent {
        fn changed(&self, _: Snapshot) {}
    }
    fn key(id: &str) -> ConversationKey {
        ConversationKey::new("host", id)
    }
    fn message(id: &str) -> ItemRef {
        Arc::new(Item::new(
            id.into(),
            Role::Assistant,
            vec![Block::Text(format!("body of {id}"))],
        ))
    }
    fn approval(id: &str) -> Approval {
        Approval {
            id: id.into(),
            prompt: "Proceed?".into(),
            explanation: None,
            command: None,
            working_directory: None,
            operation: None,
            details: Vec::new(),
            choices: Vec::new(),
        }
    }
    /// A core on one host whose catalog holds the given saved conversations.
    fn core(ids: &[&str]) -> (Arc<AppCore>, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        *lock(&core.store) = Some(Store::open(directory.path()).unwrap());
        core.install_catalog(
            "host",
            &Catalog {
                revision: 1,
                now_ms: 1,
                sessions: ids
                    .iter()
                    .map(|id| Record::draft((*id).into(), "codex".into(), String::new(), None, 1))
                    .collect(),
            },
        )
        .unwrap();
        core.update(|s| s.selected_host = Some("host".into()));
        (core, directory)
    }
    fn show(core: &AppCore, id: &str) {
        core.select(Some(key(id)), |_| {});
    }

    #[test]
    fn switching_away_and_back_keeps_the_same_runtime_without_clearing_it() {
        let (core, _directory) = core(&["a", "b"]);
        show(&core, "a");
        core.update_live(&key("a"), |live| {
            live.replace_conversation(vec![message("kept")], true);
            live.approval = Some(approval("kept"));
            live.response_activity = ResponseActivity::Tool;
            live.agent_output = "pane text".into();
        });
        let before = core.snapshot();
        assert_eq!(before.conversation_context(), "host/a");
        show(&core, "b");
        let other = core.snapshot();
        assert_eq!(other.conversation_context(), "host/b");
        assert!(other.conversation.is_empty() && other.approval.is_none());
        assert!(other.agent_output.is_empty());
        assert_eq!(other.response_activity, ResponseActivity::Idle);
        show(&core, "a");
        let after = core.snapshot();
        assert!(Arc::ptr_eq(&before.conversation, &after.conversation));
        assert_eq!(after.approval.unwrap().id, "kept");
        assert_eq!(after.agent_output, "pane text");
        assert_eq!(after.response_activity, ResponseActivity::Tool);
    }

    #[test]
    fn unselected_changes_are_not_published_and_never_reach_the_snapshot() {
        let (core, _directory) = core(&["a", "b"]);
        show(&core, "a");
        show(&core, "b");
        let published = core.snapshot().revision;
        core.update_live(&key("a"), |live| {
            live.replace_conversation(vec![message("late")], true);
            live.question = None;
        });
        let snapshot = core.snapshot();
        assert_eq!(snapshot.revision, published);
        assert!(snapshot.conversation.is_empty());
    }

    #[test]
    fn eviction_is_bounded_persists_first_and_keeps_selected_and_waiting_sessions() {
        let ids = ["a", "b", "c", "d", "e", "f", "g"];
        let (core, _directory) = core(&ids);
        show(&core, "a");
        core.update_live(&key("a"), |live| {
            live.replace_conversation(vec![message("waiting-history")], true);
            live.approval = Some(approval("waiting"));
        });
        for id in &ids[1..] {
            show(&core, id);
            core.update_live(&key(id), |live| {
                live.replace_conversation(vec![message(id)], true);
            });
        }
        let live: Vec<_> = {
            let state = lock(&core.state);
            state.live.sessions.keys().cloned().collect()
        };
        assert_eq!(live.len(), LIVE_LIMIT);
        assert!(live.contains(&key("a")), "pending approval evicted");
        assert!(live.contains(&key("g")), "selected session evicted");
        // Oldest unprotected first, and its transcript is already saved.
        assert!(!live.contains(&key("b")));
        lock(&core.session_book).histories.clear();
        let saved = core.cached_history(&key("b")).unwrap();
        assert_eq!(saved.messages[0].id, "b");
        // Reopening restores the transcript above any revision native could hold.
        let before = core.snapshot().conversation_revision;
        show(&core, "b");
        let reopened = core.snapshot();
        assert_eq!(reopened.conversation[0].id, "b");
        assert!(reopened.conversation_revision >= 1);
        assert!(core.conversation_since("host/b", before).reset);
    }

    #[test]
    fn a_full_set_of_waiting_sessions_is_retained_over_the_bound() {
        let ids = ["a", "b", "c", "d", "e", "f"];
        let (core, _directory) = core(&ids);
        for id in ids {
            show(&core, id);
            core.update_live(&key(id), |live| live.approval = Some(approval(id)));
        }
        assert_eq!(lock(&core.state).live.sessions.len(), ids.len());
    }

    #[test]
    fn a_new_agent_starts_from_an_empty_runtime_on_the_same_conversation() {
        let (core, _directory) = core(&["a"]);
        let mut record = Record::draft("a".into(), "codex".into(), String::new(), None, 1);
        record.reference = Some(kite_herdr::api::AgentSession {
            source: "herdr:codex".into(),
            agent: "codex".into(),
            kind: "id".into(),
            value: "thread".into(),
        });
        core.install_catalog(
            "host",
            &Catalog {
                revision: 2,
                now_ms: 1,
                sessions: vec![record],
            },
        )
        .unwrap();
        let agent = |terminal: &str| -> Agent {
            serde_json::from_value(serde_json::json!({
                "terminal_id":terminal, "agent":"codex", "agent_status":"idle", "workspace_id":"w", "tab_id":"t", "pane_id":"p", "revision":1,
                "agent_session":{"source":"herdr:codex","agent":"codex","kind":"id","value":"thread"}
            }))
            .unwrap()
        };
        core.select(Some(key("a")), |s| s.agents = vec![agent("one")]);
        assert_eq!(core.snapshot().selected_agent.as_deref(), Some("one"));
        core.update_live(&key("a"), |live| {
            live.replace_conversation(vec![message("kept")], true);
            live.approval = Some(approval("old"));
            live.composer.ready = true;
        });
        core.update(|s| s.agents = vec![agent("two")]);
        let moved = core.snapshot();
        assert_eq!(moved.selected_agent.as_deref(), Some("two"));
        assert!(moved.approval.is_none() && !moved.composer.ready);
        assert_eq!(moved.agent_pause, AgentPause::Running);
        assert_eq!(moved.conversation[0].id, "kept");
        lock(&core.paused_processes).insert(
            pane_key("host", "three"),
            process::Token {
                group: 1,
                started: 1,
                shell: None,
                detached_backend: None,
            },
        );
        core.update(|s| s.agents = vec![agent("three")]);
        assert_eq!(core.snapshot().agent_pause, AgentPause::Unknown);
    }

    fn pane(terminal: &str, thread: Option<&str>) -> Agent {
        let mut value = serde_json::json!({
            "terminal_id":terminal, "agent":"claude", "agent_status":"working", "workspace_id":"w", "tab_id":"t", "pane_id":"p", "revision":1
        });
        if let Some(thread) = thread {
            value["agent_session"] = serde_json::json!({
                "source":"herdr:claude", "agent":"claude", "kind":"id", "value":thread
            });
        }
        serde_json::from_value(value).unwrap()
    }
    fn busy(core: &AppCore, id: &str) {
        core.update_live(&key(id), |live| {
            live.response_activity = ResponseActivity::Sending;
            live.agent_output = "pane text".into();
            live.approval = Some(approval("waiting"));
        });
    }
    fn assert_kept(core: &AppCore, terminal: &str) {
        let s = core.snapshot();
        assert_eq!(s.selected_agent.as_deref(), Some(terminal));
        assert_eq!(s.response_activity, ResponseActivity::Sending);
        assert_eq!(s.agent_output, "pane text");
        assert_eq!(s.approval.unwrap().id, "waiting");
    }
    fn assert_reset(core: &AppCore) {
        let s = core.snapshot();
        assert_eq!(s.response_activity, ResponseActivity::Idle);
        assert!(s.agent_output.is_empty() && s.approval.is_none());
    }
    /// A draft record bound to its pane by terminal, as `session-open` leaves it.
    fn draft_on_pane(id: &str, terminal: &str) -> (Arc<AppCore>, tempfile::TempDir) {
        let (core, directory) = core(&[]);
        let mut record = Record::draft(id.into(), "claude".into(), String::new(), None, 1);
        record.terminal_id = terminal.into();
        core.install_catalog(
            "host",
            &Catalog {
                revision: 2,
                now_ms: 1,
                sessions: vec![record],
            },
        )
        .unwrap();
        (core, directory)
    }

    #[test]
    fn a_pane_learning_its_thread_keeps_the_running_conversation() {
        let (core, _directory) = draft_on_pane("draft", "T");
        core.select(Some(key("draft")), |s| s.agents = vec![pane("T", None)]);
        busy(&core, "draft");
        assert_kept(&core, "T");
        // The lookup key moves to the thread while everything on screen stays.
        core.update(|s| s.agents = vec![pane("T", Some("thread"))]);
        assert_kept(&core, "T");
        let expected = AgentKey::new("host", &pane("T", Some("thread")));
        assert_eq!(
            core.read_live(&key("draft"), |live| live
                .agent
                .as_ref()
                .unwrap()
                .key
                .clone()),
            Some(expected)
        );
    }

    #[test]
    fn another_thread_on_the_same_pane_is_another_agent_even_after_a_gap() {
        let (core, _directory) = draft_on_pane("draft", "T");
        core.select(Some(key("draft")), |s| {
            s.agents = vec![pane("T", Some("first"))]
        });
        busy(&core, "draft");
        // A brief loss of the thread is not a new agent.
        core.update(|s| s.agents = vec![pane("T", None)]);
        assert_kept(&core, "T");
        core.update(|s| s.agents = vec![pane("T", Some("first"))]);
        assert_kept(&core, "T");
        core.update(|s| s.agents = vec![pane("T", None)]);
        core.update(|s| s.agents = vec![pane("T", Some("second"))]);
        assert_eq!(core.snapshot().selected_agent.as_deref(), Some("T"));
        assert_reset(&core);
        busy(&core, "draft");
        core.update(|s| s.agents = vec![pane("T", Some("third"))]);
        assert_reset(&core);
    }

    /// A saved conversation whose thread is known, resumed on a pane still launching.
    fn saved_on_launching_pane(thread: &str) -> (Arc<AppCore>, tempfile::TempDir) {
        let (core, directory) = core(&[]);
        let mut record = Record::draft("saved".into(), "claude".into(), String::new(), None, 1);
        record.reference = pane("old", Some(thread)).agent_session;
        record.terminal_id = "T".into();
        core.install_catalog(
            "host",
            &Catalog {
                revision: 2,
                now_ms: 1,
                sessions: vec![record],
            },
        )
        .unwrap();
        core.select_state(Some(key("saved")), |state| {
            state.snapshot.agents = vec![pane("T", None)];
            state.live.get_mut(&key("saved")).unwrap().launching = Some("T".into());
        });
        (core, directory)
    }

    #[test]
    fn a_resumed_pane_serves_its_conversation_before_it_reports_the_thread() {
        let (core, _directory) = saved_on_launching_pane("thread");
        assert_eq!(core.snapshot().selected_agent.as_deref(), Some("T"));
        busy(&core, "saved");
        core.update(|s| s.agents = vec![pane("T", Some("thread"))]);
        assert_kept(&core, "T");
        assert_eq!(
            core.read_live(&key("saved"), |live| live.launching.clone()),
            Some(None)
        );
    }

    #[test]
    fn a_resumed_pane_reporting_another_thread_stops_serving_the_conversation() {
        let (core, _directory) = saved_on_launching_pane("thread");
        busy(&core, "saved");
        core.update(|s| s.agents = vec![pane("T", Some("another"))]);
        assert!(core.snapshot().selected_agent.is_none());
        assert_reset(&core);
    }

    #[test]
    fn an_unrelated_session_less_pane_never_binds_to_a_saved_conversation() {
        let (core, _directory) = saved_on_launching_pane("thread");
        core.update_live(&key("saved"), |live| live.launching = None);
        core.update(|s| s.agents = vec![pane("T", None)]);
        assert!(core.snapshot().selected_agent.is_none());
    }

    #[test]
    fn routing_a_session_does_not_count_as_viewing_it() {
        let (core, _directory) = core(&["a", "b", "c", "d", "e"]);
        for id in ["a", "b", "c", "d"] {
            show(&core, id);
        }
        core.route_runtime(vec![(key("a"), RuntimeState::default())]);
        show(&core, "e");
        let state = lock(&core.state);
        assert!(!state.live.sessions.contains_key(&key("a")));
        assert!(state.live.sessions.contains_key(&key("b")));
    }

    #[derive(Default)]
    struct Recording(Mutex<Vec<Snapshot>>);
    impl Observer for Recording {
        fn changed(&self, snapshot: Snapshot) {
            lock(&self.0).push(snapshot);
        }
    }

    #[test]
    fn routing_publishes_only_when_it_changed_what_the_selected_session_shows() {
        let observed = Arc::new(Recording::default());
        let core = AppCore::new(Arc::new(Secrets), observed.clone());
        let mut record = Record::draft("a".into(), "claude".into(), String::new(), None, 1);
        record.reference = pane("T", Some("thread")).agent_session;
        let catalog = |revision, sessions| Catalog {
            revision,
            now_ms: 1,
            sessions,
        };
        let other = Record::draft("b".into(), "claude".into(), String::new(), None, 1);
        core.install_catalog("host", &catalog(1, vec![record.clone(), other.clone()]))
            .unwrap();
        core.select(Some(key("b")), |s| {
            s.selected_host = Some("host".into());
            s.agents = vec![pane("T", Some("thread"))];
        });
        core.select(Some(key("a")), |_| {});
        assert_eq!(core.snapshot().selected_agent.as_deref(), Some("T"));
        let published = lock(&observed.0).len();
        core.route_runtime(vec![(key("b"), RuntimeState::default())]);
        assert_eq!(lock(&observed.0).len(), published);
        // The catalog changed underneath: this reconcile is the first to see it.
        record.archived = true;
        core.install_catalog("host", &catalog(2, vec![record, other]))
            .unwrap();
        core.route_runtime(vec![(key("b"), RuntimeState::default())]);
        assert_eq!(lock(&observed.0).len(), published + 1);
        assert!(lock(&observed.0).last().unwrap().selected_agent.is_none());
    }
}
