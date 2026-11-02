use super::*;
use kite_agent::sessions::{Catalog, Record, Repository};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionCleanup {
    pub reclaimed: u32,
    pub retained: u32,
    pub failed: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct CachedConversation {
    pub(super) messages: SharedItems,
    pub(super) available: bool,
    viewed_ms: i64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct SessionBook {
    catalogs: BTreeMap<String, Vec<Record>>,
    selected: BTreeMap<String, Option<String>>,
    revisions: BTreeMap<String, u64>,
    #[serde(default, skip_serializing)]
    pub(super) histories: BTreeMap<String, CachedConversation>,
}
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}
pub(super) fn views(records: &[Record], agents: &[Agent]) -> Vec<ConversationSession> {
    let mut result: Vec<_> = records
        .iter()
        .map(|r| {
            let live = agents.iter().find(|a| r.matches(a));
            ConversationSession {
                id: r.id.clone(),
                provider: r.provider.clone(),
                title: r.title.clone(),
                preview: r.preview.clone(),
                cwd: r.cwd.clone(),
                archived: r.archived,
                updated_ms: r.updated_ms,
                agent_id: live.map(|a| a.terminal_id.clone()),
                status: live
                    .map(|a| {
                        if a.launch_pending {
                            "starting".into()
                        } else {
                            a.agent_status.clone()
                        }
                    })
                    .unwrap_or_else(|| "saved".into()),
                recorded: r.reference.is_some(),
            }
        })
        .collect();
    result.sort_by(|a, b| {
        b.updated_ms
            .cmp(&a.updated_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    result
}

pub(super) fn saved_record<'a>(key: &ConversationKey, book: &'a SessionBook) -> Option<&'a Record> {
    book.catalogs
        .get(&key.host)?
        .iter()
        .find(|r| r.id == key.conversation)
}

/// The pane serving a conversation. An archived record keeps its running agent out of
/// the chat. `launching` is the pane opened for this conversation: a saved thread cannot
/// match it until it reports that thread, and no other session-less pane may stand in.
pub(super) fn conversation_agent<'a>(
    key: &ConversationKey,
    book: &SessionBook,
    agents: &'a [Agent],
    launching: Option<&str>,
) -> Option<&'a Agent> {
    match saved_record(key, book) {
        Some(record) if record.archived => None,
        Some(record) => agents.iter().find(|a| record.matches(a)).or_else(|| {
            let terminal = launching?;
            agents
                .iter()
                .find(|a| a.terminal_id == terminal && a.agent_session.is_none())
        }),
        None => agents.iter().find(|a| a.terminal_id == key.conversation),
    }
}

fn activity_status<'a>(
    agent: &'a Agent,
    paused: bool,
    question: bool,
    runtime: Option<&kite_agent::interaction::RuntimeState>,
) -> &'a str {
    if agent.launch_pending {
        "starting"
    } else if paused {
        "paused"
    } else if question {
        "question"
    } else if runtime
        .is_some_and(|r| r.approval().is_some() || r.phase() == ResponseActivity::Waiting)
    {
        "blocked"
    } else if runtime.is_some_and(|r| r.phase() == ResponseActivity::Failed) {
        "failed"
    } else if runtime.is_some_and(|r| {
        matches!(
            r.phase(),
            ResponseActivity::Sending
                | ResponseActivity::Thinking
                | ResponseActivity::Tool
                | ResponseActivity::Responding
        )
    }) {
        "working"
    } else {
        agent.agent_status.as_str()
    }
}

impl AppCore {
    pub(super) async fn refresh_session_activity(&self, c: &Connected, generation: u64) {
        #[derive(Deserialize)]
        struct Facts {
            facts: Vec<Fact>,
        }
        #[derive(Deserialize)]
        struct Fact {
            session_id: String,
            thread: String,
            controls: String,
            controls_known: bool,
        }
        // One batch covers questions in every session, including archived running
        // sessions and sessions which this phone has never opened.
        let command = composer::helper_command(&format!(
            "session-facts --herdr {}",
            composer::quote(&c.path)
        ));
        let facts =
            match tokio::time::timeout(Duration::from_secs(15), c.ssh.exec(&command, &[])).await {
                Ok(Ok(output))
                    if output.exit_status == Some(0) && output.stdout.len() <= 4 * 1024 * 1024 =>
                {
                    serde_json::from_slice::<Facts>(&output.stdout).ok()
                }
                _ => None,
            };
        let snapshot = self.snapshot();
        let clients = self.chat_clients.lock().await;
        let mut states = BTreeMap::new();
        let mut routed = Vec::new();
        for session in &snapshot.sessions {
            let agent = session
                .agent_id
                .as_ref()
                .and_then(|id| snapshot.agents.iter().find(|a| &a.terminal_id == id));
            let Some(agent) = agent else { continue };
            let fact = facts
                .as_ref()
                .and_then(|facts| facts.facts.iter().find(|f| f.session_id == session.id));
            let controls = fact
                .filter(|f| f.controls_known)
                .map(|f| kite_agent::async_input::history(&f.thread, f.controls.as_bytes()));
            let client = clients
                .get(&AgentKey::new(&c.host.id, agent))
                .filter(|c| !c.is_closed());
            if let (Some(client), Some(controls)) = (client, &controls) {
                client.reconcile_async(controls);
            }
            let runtime = client.map(|c| c.runtime());
            let question = runtime.as_ref().is_some_and(|r| r.question().is_some())
                || controls.as_ref().is_some_and(|h| !h.pending.is_empty());
            let paused = lock(&self.paused_processes)
                .contains_key(&pane_key(&c.host.id, &agent.terminal_id));
            let status = activity_status(agent, paused, question, runtime.as_ref());
            states.insert(session.id.clone(), status.to_owned());
            if let Some(runtime) = runtime.filter(|_| !session.archived) {
                routed.push((ConversationKey::new(&c.host.id, &session.id), runtime));
            }
        }
        drop(clients);
        if self.generation.load(Ordering::SeqCst) == generation {
            self.route_runtime(routed);
            self.update(|s| {
                for session in &mut s.sessions {
                    if let Some(status) = states.get(&session.id) {
                        session.status = status.clone();
                    }
                }
            });
        }
    }

    /// A notification changes nothing on its own, but its conversation may now hold
    /// a request: read it through that conversation's client, whichever is on screen.
    pub(super) async fn route_notice(&self, c: &Connected, key: &ConversationKey) {
        let snapshot = self.snapshot();
        let client = {
            let book = lock(&self.session_book);
            conversation_agent(key, &book, &snapshot.agents, None)
                .map(|a| AgentKey::new(&key.host, a))
        };
        let Some(client) = client else {
            return;
        };
        if key.host != c.host.id {
            return;
        }
        let runtime = self
            .chat_clients
            .lock()
            .await
            .get(&client)
            .filter(|client| !client.is_closed())
            .map(|client| client.runtime());
        if let Some(runtime) = runtime {
            self.route_runtime(vec![(key.clone(), runtime)]);
        }
    }

    pub(super) fn cached_history(
        &self,
        conversation: &ConversationKey,
    ) -> Option<CachedConversation> {
        let key = conversation.cache_key();
        if let Some(value) = lock(&self.session_book).histories.get(&key).cloned() {
            return Some(value);
        }
        let value = {
            lock(&self.store)
                .as_ref()?
                .read_document::<CachedConversation>(&conversation.cache_document())
        };
        match value {
            Ok(Some(value)) => {
                lock(&self.session_book)
                    .histories
                    .insert(key.clone(), value.clone());
                if let Err(error) = self.trim_memory_history(&key) {
                    self.report(&error);
                }
                Some(value)
            }
            Ok(None) => None,
            Err(error) => {
                self.report(&error.into());
                None
            }
        }
    }
    fn trim_memory_history(&self, keep: &str) -> Result<(), AppError> {
        if lock(&self.session_book).histories.len() <= 4 {
            return Ok(());
        }
        self.flush_history_caches(true)?;
        let mut book = lock(&self.session_book);
        while book.histories.len() > 4 {
            let oldest = book
                .histories
                .iter()
                .filter(|(key, _)| key.as_str() != keep)
                .min_by_key(|(_, v)| v.viewed_ms)
                .map(|(key, _)| key.clone());
            if let Some(key) = oldest {
                book.histories.remove(&key);
            } else {
                break;
            }
        }
        Ok(())
    }
    pub(super) fn flush_history_caches(&self, force: bool) -> Result<(), AppError> {
        let keys: Vec<_> = lock(&self.dirty_histories).iter().cloned().collect();
        for key in keys {
            if !force
                && lock(&self.history_cache_written)
                    .get(&key)
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(2))
            {
                continue;
            }
            let cache = lock(&self.session_book).histories.get(&key).cloned();
            if let Some(cache) = cache {
                if let Some(store) = lock(&self.store).as_ref() {
                    store.write_document(&ConversationKey::cache_document_of(&key), &cache)?;
                }
                lock(&self.history_cache_written).insert(key.clone(), Instant::now());
                let book = lock(&self.session_book);
                if book.histories.get(&key).is_some_and(|current| {
                    Arc::ptr_eq(&current.messages, &cache.messages)
                        && current.available == cache.available
                }) {
                    lock(&self.dirty_histories).remove(&key);
                }
            }
        }
        Ok(())
    }
    fn persist_session_book(&self) -> Result<(), AppError> {
        let book = lock(&self.session_book);
        if let Some(store) = lock(&self.store).as_ref() {
            store.write_document("sessions", &*book)?;
        }
        Ok(())
    }
    pub(super) fn restore_session_cache(&self) {
        let Some(host) = self.snapshot().selected_host else {
            return;
        };
        let book = lock(&self.session_book).clone();
        let records = book.catalogs.get(&host).cloned().unwrap_or_default();
        let selected = book
            .selected
            .get(&host)
            .cloned()
            .flatten()
            .filter(|id| records.iter().any(|r| &r.id == id));
        let selected = selected.map(|id| ConversationKey::new(&host, id));
        self.select(selected, |s| s.sessions = views(&records, &s.agents));
    }
    pub(super) fn install_catalog(
        &self,
        host: &str,
        catalog: &Catalog,
    ) -> Result<Catalog, AppError> {
        let mut book = lock(&self.session_book);
        if book
            .revisions
            .get(host)
            .is_some_and(|revision| *revision > catalog.revision)
        {
            return Ok(Catalog {
                revision: *book.revisions.get(host).unwrap_or(&0),
                now_ms: catalog.now_ms,
                sessions: book.catalogs.get(host).cloned().unwrap_or_default(),
            });
        }
        let changed = book.catalogs.get(host) != Some(&catalog.sessions)
            || book.revisions.get(host) != Some(&catalog.revision);
        if changed {
            book.catalogs.insert(host.into(), catalog.sessions.clone());
            book.revisions.insert(host.into(), catalog.revision);
        }
        drop(book);
        if changed {
            self.persist_session_book()?;
        }
        Ok(catalog.clone())
    }
    pub(super) fn session_record(&self, host: &str, id: &str) -> Option<Record> {
        lock(&self.session_book)
            .catalogs
            .get(host)?
            .iter()
            .find(|r| r.id == id)
            .cloned()
    }
    /// The conversation to show once `agents` are known.
    pub(super) fn preferred_conversation(
        &self,
        host: &str,
        agents: &[Agent],
    ) -> Option<ConversationKey> {
        let snapshot = self.snapshot();
        let book = lock(&self.session_book);
        let records = book
            .catalogs
            .get(host)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let selected = snapshot
            .selected_session
            .as_ref()
            .or_else(|| book.selected.get(host).and_then(Option::as_ref));
        if let Some(record) = selected.and_then(|id| records.iter().find(|r| &r.id == id)) {
            return Some(ConversationKey::new(host, &record.id));
        }
        let agent = agents
            .iter()
            .find(|a| !records.iter().any(|r| r.archived && r.matches(a)))?;
        let id = records
            .iter()
            .find(|r| r.matches(agent))
            .map_or(&agent.terminal_id, |r| &r.id);
        Some(ConversationKey::new(host, id))
    }
    /// The conversation an agent's events belong to: its catalog record, unless the
    /// pane's own session is the one still running because that record is too new.
    pub(super) fn agent_conversation(&self, host: &str, agent: &Agent) -> ConversationKey {
        let saved = lock(&self.session_book)
            .catalogs
            .get(host)
            .and_then(|records| records.iter().find(|r| r.matches(agent)))
            .map(|r| ConversationKey::new(host, &r.id));
        let state = lock(&self.state);
        match saved {
            Some(key) if state.live.get(&key).is_some() => key,
            saved => state
                .live
                .bound_to(host, &agent.terminal_id)
                .or(saved)
                .unwrap_or_else(|| ConversationKey::new(host, &agent.terminal_id)),
        }
    }
    pub(super) async fn sync_sessions(
        &self,
        c: &Connected,
        agents: &[Agent],
        generation: u64,
    ) -> Result<(), AppError> {
        let catalog = Repository::new(&c.ssh).observe(agents).await?;
        let catalog = self.install_catalog(&c.host.id, &catalog)?;
        if self.generation.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        let Ok(_selection_guard) = self.operations.try_lock() else {
            return Ok(());
        };
        let current = self.snapshot();
        let mut selected = current
            .selected_agent
            .as_ref()
            .and_then(|id| agents.iter().find(|a| &a.terminal_id == id))
            .and_then(|a| catalog.sessions.iter().find(|r| r.matches(a)))
            .map(|r| r.id.clone());
        if selected.is_none() {
            selected = current
                .selected_session
                .clone()
                .filter(|id| catalog.sessions.iter().any(|r| &r.id == id));
        }
        if selected.is_none() {
            selected = lock(&self.session_book)
                .selected
                .get(&c.host.id)
                .cloned()
                .flatten()
                .filter(|id| catalog.sessions.iter().any(|r| &r.id == id));
        }
        if selected.is_none() && current.selected_agent.is_none() {
            selected = catalog
                .sessions
                .iter()
                .filter(|r| !r.archived)
                .max_by_key(|r| r.updated_ms)
                .map(|r| r.id.clone());
        }
        let next = selected
            .as_ref()
            .or(current
                .selected_agent
                .as_ref()
                .filter(|id| agents.iter().any(|a| &a.terminal_id == *id)))
            .map(|id| ConversationKey::new(&c.host.id, id));
        let mut all_views = views(&catalog.sessions, agents);
        for view in &mut all_views {
            if let Some(previous) = current
                .sessions
                .iter()
                .find(|s| s.id == view.id && s.agent_id == view.agent_id)
                && matches!(previous.status.as_str(), "question" | "paused" | "failed")
            {
                // Retain refined state until this poll's batch reconciliation.
                view.status = previous.status.clone();
            }
        }
        self.select(next, |s| s.sessions = all_views);
        let selected_changed = {
            let mut book = lock(&self.session_book);
            if book.selected.get(&c.host.id) != Some(&selected) {
                book.selected.insert(c.host.id.clone(), selected);
                true
            } else {
                false
            }
        };
        if selected_changed {
            self.persist_session_book()?;
        }
        Ok(())
    }
    pub(super) async fn prepare_session_agent(
        &self,
        c: &Connected,
        agents: &[Agent],
    ) -> Result<Option<Agent>, AppError> {
        let catalog = Repository::new(&c.ssh).observe(agents).await?;
        let catalog = self.install_catalog(&c.host.id, &catalog)?;
        if agents.is_empty() && !catalog.sessions.is_empty() {
            let selected = lock(&self.session_book)
                .selected
                .get(&c.host.id)
                .cloned()
                .flatten();
            let record = selected
                .as_ref()
                .and_then(|id| catalog.sessions.iter().find(|r| &r.id == id && !r.archived))
                .or_else(|| {
                    catalog
                        .sessions
                        .iter()
                        .filter(|r| !r.archived)
                        .max_by_key(|r| r.updated_ms)
                });
            if let Some(record) = record.filter(|r| {
                r.provider == "codex"
                    && r.reference.is_some()
                    && self.snapshot().providers.iter().any(|p| p.id == r.provider)
            }) {
                return Ok(Some(
                    Repository::new(&c.ssh).resume(&record.id, &c.path).await?,
                ));
            }
        }
        Ok(None)
    }
    pub(super) fn cache_conversation(
        &self,
        conversation: &ConversationKey,
        messages: &SharedItems,
        available: bool,
    ) -> Result<bool, AppError> {
        let key = conversation.cache_key();
        let mut book = lock(&self.session_book);
        if book.histories.get(&key).is_some_and(|v| {
            (Arc::ptr_eq(&v.messages, messages) || v.messages.as_ref() == messages.as_ref())
                && v.available == available
        }) {
            return Ok(false);
        }
        if messages.is_empty()
            && book
                .histories
                .get(&key)
                .is_some_and(|c| !c.messages.is_empty())
        {
            return Ok(false);
        }
        book.histories.insert(
            key.clone(),
            CachedConversation {
                messages: messages.clone(),
                available,
                viewed_ms: now_ms(),
            },
        );
        drop(book);
        lock(&self.dirty_histories).insert(key.clone());
        self.flush_history_caches(false)?;
        self.trim_memory_history(&key)?;
        Ok(true)
    }
    pub(super) async fn record_history(
        &self,
        c: &Connected,
        key: &ConversationKey,
    ) -> Result<(), AppError> {
        self.flush_history_caches(false)?;
        if key.host != c.host.id || self.session_record(&key.host, &key.conversation).is_none() {
            return Ok(());
        }
        let Some((conversation, available)) =
            self.read_live(key, |live| (live.conversation.clone(), live.available))
        else {
            return Ok(());
        };
        if self.cache_conversation(key, &conversation, available)?
            && let Some(message) = conversation
                .iter()
                .rev()
                .find(|m| m.tool().is_none() && !m.intermediate && !m.blocks.is_empty())
        {
            let catalog = Repository::new(&c.ssh)
                .note(&key.conversation, &message.display_text(), false)
                .await?;
            self.install_catalog(&c.host.id, &catalog)?;
        }
        Ok(())
    }
    pub(super) async fn refresh_saved_session(
        &self,
        c: &Connected,
        key: &ConversationKey,
        generation: u64,
    ) -> Result<(), AppError> {
        let Some(reference) = self
            .session_record(&key.host, &key.conversation)
            .and_then(|record| record.reference)
        else {
            return Ok(());
        };
        let cached = self.read_live(key, |live| live.history.clone()).flatten();
        let history = history::read(&c.ssh, &reference, cached).await;
        if self.generation.load(Ordering::SeqCst) == generation
            && self.read_live(key, |live| live.agent.is_none()) == Some(true)
        {
            self.present_history(key, &history);
            self.record_history(c, key).await?;
        }
        Ok(())
    }
    pub async fn open_session(self: &Arc<Self>, id: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let host = core
                .snapshot()
                .selected_host
                .ok_or_else(|| AppError::State("host unavailable".into()))?;
            let connection = core.connection().ok();
            if let Some(c) = &connection {
                let catalog = Repository::new(&c.ssh).read().await?;
                core.install_catalog(&host, &catalog)?;
            }
            core.session_record(&host, &id)
                .ok_or_else(|| AppError::State("session unavailable".into()))?;
            lock(&core.session_book)
                .selected
                .insert(host.clone(), Some(id.clone()));
            core.persist_session_book()?;
            core.select(Some(ConversationKey::new(&host, &id)), |_| {});
            drop(_guard);
            if let Some(c) = connection {
                core.refresh_selected(&c, core.generation.load(Ordering::SeqCst), false)
                    .await?;
            }
            Ok(())
        })
        .await
    }
    pub(super) async fn activate_session(&self, c: &Connected, id: &str) -> Result<(), AppError> {
        let agent = Repository::new(&c.ssh).resume(id, &c.path).await?;
        let catalog = Repository::new(&c.ssh).archive(id, false).await?;
        self.install_catalog(&c.host.id, &catalog)?;
        lock(&self.session_book)
            .selected
            .insert(c.host.id.clone(), Some(id.into()));
        self.persist_session_book()?;
        let mut agents = self.fetch_agents(c).await?;
        if !agents.iter().any(|a| a.terminal_id == agent.terminal_id) {
            agents.push(agent.clone());
        }
        if agent.launch_pending {
            lock(&self.agent_starts).insert(
                agent.terminal_id.clone(),
                PendingAgentStart {
                    pane: agent.pane_id.clone(),
                    workspace: agent.workspace_id.clone(),
                    since: Instant::now(),
                },
            );
        }
        let key = ConversationKey::new(&c.host.id, id);
        self.select_state(Some(key.clone()), |state| {
            state.snapshot.agents = agents;
            state.snapshot.failure = None;
            if let Some(live) = state.live.get_mut(&key) {
                // A slow start returns before the pane reports its thread.
                live.launching = agent
                    .agent_session
                    .is_none()
                    .then(|| agent.terminal_id.clone());
            }
        });
        Ok(())
    }
    pub async fn resume_session(self: &Arc<Self>, id: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            core.activate_session(&c, &id).await?;
            let agents = core.snapshot().agents;
            drop(_guard);
            core.sync_sessions(&c, &agents, core.generation.load(Ordering::SeqCst))
                .await?;
            Ok(())
        })
        .await
    }
    pub async fn archive_session(
        self: &Arc<Self>,
        id: String,
        archived: bool,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            let catalog = Repository::new(&c.ssh).archive(&id, archived).await?;
            core.install_catalog(&c.host.id, &catalog)?;
            if archived && core.snapshot().selected_session.as_ref() == Some(&id) {
                let next = catalog
                    .sessions
                    .iter()
                    .filter(|r| !r.archived)
                    .max_by_key(|r| r.updated_ms)
                    .map(|r| r.id.clone());
                lock(&core.session_book)
                    .selected
                    .insert(c.host.id.clone(), next.clone());
                core.select(next.map(|id| ConversationKey::new(&c.host.id, id)), |_| {});
                core.persist_session_book()?;
            }
            let agents = core.snapshot().agents;
            drop(_guard);
            core.sync_sessions(&c, &agents, core.generation.load(Ordering::SeqCst))
                .await?;
            Ok(())
        })
        .await
    }

    pub async fn release_archived_sessions(
        self: &Arc<Self>,
        host_id: String,
    ) -> Result<SessionCleanup, AppError> {
        let core = self.clone();
        dispatch(async move {
            let _operation = core.operations.lock().await;
            let connection = core.connection()?;
            if connection.host.id != host_id {
                return Err(AppError::State(
                    "session cleanup belongs to another host".into(),
                ));
            }
            let output = connection
                .ssh
                .exec(
                    "\"$HOME/.local/bin/kite-node\" sessions release-archived",
                    &[],
                )
                .await?;
            if output.exit_status != Some(0) || output.stdout.len() > 65536 {
                return Err(AppError::State(
                    "archived session release unavailable".into(),
                ));
            }
            let result: SessionCleanup = serde_json::from_slice(&output.stdout)
                .map_err(|error| AppError::State(error.to_string()))?;
            core.refresh_resources(&connection, core.generation.load(Ordering::SeqCst))
                .await;
            Ok(result)
        })
        .await
    }
    pub async fn create_session(
        self: &Arc<Self>,
        provider: String,
        cwd: String,
        title: String,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            let cwd = cwd.trim();
            let title = title.trim();
            if !core.snapshot().providers.iter().any(|p| p.id == provider)
                || title.len() > 512
                || (!cwd.is_empty()
                    && (!cwd.starts_with('/')
                        || cwd.len() > 4096
                        || cwd.chars().any(char::is_control)))
            {
                return Err(AppError::State("invalid session parameters".into()));
            }
            let id = format!("{}{}", new_device_id()?, new_device_id()?);
            let catalog = Repository::new(&c.ssh)
                .create(
                    id.clone(),
                    provider.clone(),
                    title.into(),
                    if cwd.is_empty() {
                        None
                    } else {
                        Some(cwd.into())
                    },
                )
                .await?;
            core.install_catalog(&c.host.id, &catalog)?;
            lock(&core.session_book)
                .selected
                .insert(c.host.id.clone(), Some(id.clone()));
            core.persist_session_book()?;
            core.select(Some(ConversationKey::new(&c.host.id, &id)), |s| {
                s.sessions = views(&catalog.sessions, &s.agents);
                s.failure = None;
            });
            if providers::managed(&provider) {
                core.activate_session(&c, &id).await?;
            }
            let agents = core.snapshot().agents;
            drop(_guard);
            core.sync_sessions(&c, &agents, core.generation.load(Ordering::SeqCst))
                .await?;
            Ok(())
        })
        .await
    }
    pub(super) async fn note_sent_prompt(
        &self,
        c: &Connected,
        agent: &Agent,
        item: &Item,
    ) -> Result<(), AppError> {
        let id = lock(&self.session_book)
            .catalogs
            .get(&c.host.id)
            .and_then(|records| records.iter().find(|r| r.matches(agent)))
            .map(|r| r.id.clone());
        if let Some(id) = id {
            let body = item.display_text();
            let display = if body.trim().is_empty() {
                item.attachments()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                body
            };
            let catalog = Repository::new(&c.ssh).note(&id, &display, true).await?;
            self.install_catalog(&c.host.id, &catalog)?;
            self.record_history(c, &ConversationKey::new(&c.host.id, &id))
                .await?;
        }
        Ok(())
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
    struct Silent;
    impl Observer for Silent {
        fn changed(&self, _: Snapshot) {}
    }
    #[test]
    fn session_attention_survives_turn_completion_and_resolves_from_fresh_controls() {
        let mut agent: Agent = serde_json::from_value(serde_json::json!({
            "terminal_id":"terminal", "agent_status":"done", "workspace_id":"workspace", "tab_id":"tab", "pane_id":"workspace:p1", "revision":1
        })).unwrap();
        let mut runtime = kite_agent::interaction::RuntimeState::default();
        assert_eq!(
            activity_status(&agent, false, true, Some(&runtime)),
            "question"
        );
        assert_eq!(
            activity_status(&agent, false, false, Some(&runtime)),
            "done"
        );
        runtime.begin();
        assert_eq!(
            activity_status(&agent, false, false, Some(&runtime)),
            "working"
        );
        assert_eq!(
            activity_status(&agent, true, true, Some(&runtime)),
            "paused"
        );
        runtime.failed("failure".into());
        assert_eq!(
            activity_status(&agent, false, false, Some(&runtime)),
            "failed"
        );
        agent.launch_pending = true;
        assert_eq!(
            activity_status(&agent, false, true, Some(&runtime)),
            "starting"
        );
        agent.launch_pending = false;
        agent.agent_status = "blocked".into();
        assert_eq!(activity_status(&agent, false, false, None), "blocked");
    }
    #[test]
    fn the_selected_agent_is_derived_from_the_selected_conversation() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        let agent: Agent = serde_json::from_value(serde_json::json!({
            "terminal_id":"terminal", "agent":"codex", "agent_status":"idle", "workspace_id":"w", "tab_id":"t", "pane_id":"p", "revision":1,
            "agent_session":{"source":"herdr:codex","agent":"codex","kind":"id","value":"thread"}
        })).unwrap();
        let mut record = Record::draft("saved".into(), "codex".into(), "topic".into(), None, 1);
        record.reference = agent.agent_session.clone();
        let install = |revision, record: &Record| {
            core.install_catalog(
                "host",
                &Catalog {
                    revision,
                    now_ms: 1,
                    sessions: vec![record.clone()],
                },
            )
            .unwrap();
        };
        install(1, &record);
        core.select(Some(ConversationKey::new("host", "saved")), |s| {
            s.selected_host = Some("host".into());
            s.agents = vec![agent.clone()];
        });
        let selected = core.snapshot();
        assert_eq!(selected.selected_session.as_deref(), Some("saved"));
        assert_eq!(selected.selected_agent.as_deref(), Some("terminal"));
        assert_eq!(selected.conversation_context(), "host/saved");
        record.archived = true;
        install(2, &record);
        core.update(|_| {});
        let archived = core.snapshot();
        assert_eq!(archived.selected_session.as_deref(), Some("saved"));
        assert!(archived.selected_agent.is_none());
        record.archived = false;
        install(3, &record);
        core.update(|s| s.agents.clear());
        assert!(core.snapshot().selected_agent.is_none());
        // A pane without a record yet is selected by its terminal.
        core.select(Some(ConversationKey::new("host", "terminal")), |s| {
            s.agents = vec![agent.clone()];
        });
        let pane = core.snapshot();
        assert!(pane.selected_session.is_none());
        assert_eq!(pane.selected_agent.as_deref(), Some("terminal"));
        assert_eq!(pane.conversation_context(), "host/terminal");
        // A selection never survives on another host.
        core.update(|s| s.selected_host = Some("other".into()));
        let other = core.snapshot();
        assert!(other.selected_agent.is_none() && other.selected_session.is_none());
        assert_eq!(other.conversation_context(), "other/");
    }
    #[test]
    fn stale_poll_results_cannot_undo_an_archive() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        let record = Record::draft("a".into(), "codex".into(), "topic".into(), None, 1);
        let old = Catalog {
            revision: 1,
            now_ms: 1,
            sessions: vec![record.clone()],
        };
        core.install_catalog("host", &old).unwrap();
        let mut archived = record;
        archived.archived = true;
        core.install_catalog(
            "host",
            &Catalog {
                revision: 2,
                now_ms: 2,
                sessions: vec![archived],
            },
        )
        .unwrap();
        let effective = core.install_catalog("host", &old).unwrap();
        assert_eq!(effective.revision, 2);
        assert!(effective.sessions[0].archived);
    }
    #[test]
    fn cached_archived_record_and_provider_survive_round_trip() {
        let mut record = Record::draft("a".into(), "claude".into(), "topic".into(), None, 1);
        record.archived = true;
        let mut book = SessionBook::default();
        book.catalogs.insert("host".into(), vec![record]);
        book.selected.insert("host".into(), Some("a".into()));
        let data = serde_json::to_vec(&book).unwrap();
        let restored: SessionBook = serde_json::from_slice(&data).unwrap();
        let view = views(&restored.catalogs["host"], &[]).pop().unwrap();
        assert!(view.archived && view.agent_id.is_none() && !view.recorded);
        assert_eq!(view.provider, "claude");
        assert_eq!(restored.selected["host"], Some("a".into()));
    }
    #[test]
    fn an_empty_successful_or_failed_read_never_erases_a_nonempty_cache() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        let message = Item::new(
            "old".into(),
            Role::Assistant,
            vec![Block::Text("retained history".into())],
        );
        let key = ConversationKey::new("host", "session");
        core.cache_conversation(&key, &Arc::new(vec![Arc::new(message.clone())]), true)
            .unwrap();
        for available in [true, false] {
            assert!(
                !core
                    .cache_conversation(&key, &Arc::new(Vec::new()), available)
                    .unwrap()
            );
            assert_eq!(
                lock(&core.session_book).histories[&key.cache_key()].messages,
                Arc::new(vec![Arc::new(message.clone())])
            );
        }
    }
    #[tokio::test]
    async fn old_inline_history_migrates_to_a_complete_independent_cache() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let conversation = ConversationKey::new("host", "session");
        let key = conversation.cache_key();
        // Written before typed blocks: every saved record must still open.
        let messages: Vec<serde_json::Value> = (0..10000)
            .map(|i| {
                serde_json::json!({
                    "id":format!("m-{i}"), "role":"Assistant", "text":format!("record {i}"),
                    "intermediate":false, "tool":null
                })
            })
            .collect();
        let record = Record::draft("session".into(), "codex".into(), "saved".into(), None, 1);
        let old = serde_json::json!({"catalogs":{"host":[record]},"selected":{"host":"session"},"revisions":{},"histories":{(key.clone()):{"messages":messages,"available":true,"viewed_ms":1}}});
        store.write_document("sessions", &old).unwrap();
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        core.initialize(
            directory.path().into(),
            "fixture".into(),
            LegacyImport {
                source: "migration".into(),
                host: Some(Host {
                    id: "host".into(),
                    host: "localhost".into(),
                    port: 22,
                    username: "fixture".into(),
                    herdr_path: None,
                    stream_host: None,
                    sunshine_port: 47989,
                }),
                trusted_keys: Vec::new(),
            },
        )
        .await
        .unwrap();
        assert_eq!(core.snapshot().conversation.len(), 10000);
        let stored: CachedConversation = store
            .read_document(&conversation.cache_document())
            .unwrap()
            .unwrap();
        assert_eq!(stored.messages.len(), 10000);
        let metadata: serde_json::Value = store.read_document("sessions").unwrap().unwrap();
        assert!(metadata.get("histories").is_none());
        assert_eq!(core.snapshot().conversation[0].id, "m-0");
        assert_eq!(core.snapshot().conversation[9999].id, "m-9999");
        assert_eq!(core.snapshot().conversation[9999].text(), "record 9999");
        lock(&core.session_book).histories.clear();
        assert_eq!(
            core.cached_history(&conversation).unwrap().messages.len(),
            10000
        );
    }
    #[test]
    fn memory_eviction_keeps_every_saved_conversation_file() {
        let directory = tempfile::tempdir().unwrap();
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        *lock(&core.store) = Some(Store::open(directory.path()).unwrap());
        for i in 0..16 {
            let records = Arc::new(vec![Arc::new(Item::new(
                format!("m-{i}"),
                Role::Assistant,
                vec![Block::Text(format!("conversation {i}"))],
            ))]);
            core.cache_conversation(
                &ConversationKey::new("host", format!("s-{i}")),
                &records,
                true,
            )
            .unwrap();
        }
        core.flush_history_caches(true).unwrap();
        assert!(lock(&core.session_book).histories.len() <= 4);
        for i in 0..16 {
            let cache = core
                .cached_history(&ConversationKey::new("host", format!("s-{i}")))
                .unwrap();
            assert_eq!(cache.messages[0].id, format!("m-{i}"));
            assert!(lock(&core.session_book).histories.len() <= 4);
        }
    }
}
