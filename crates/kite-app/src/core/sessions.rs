use super::*;
use kite_agent::sessions::{self as domain, Catalog, Record, Repository};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionCleanup {
    pub reclaimed: u32,
    pub retained: u32,
    pub failed: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct CachedConversation {
    messages: SharedMessages,
    available: bool,
    viewed_ms: i64,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct SessionBook {
    catalogs: BTreeMap<String, Vec<Record>>,
    selected: BTreeMap<String, Option<String>>,
    revisions: BTreeMap<String, u64>,
    #[serde(default)]
    sorts: BTreeMap<String, SessionSort>,
    #[serde(default, skip_serializing)]
    pub(super) histories: BTreeMap<String, CachedConversation>,
}
pub(super) fn history_document(key: &str) -> String {
    // A namespaced 256-bit key encoded as letters fits Store's existing path contract.
    fingerprint(&format!("conversation-cache/{key}"))
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
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}
fn views(records: &[Record], agents: &[Agent], sort: SessionSort) -> Vec<ConversationSession> {
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
                created_ms: r.created_ms,
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
    sort_views(&mut result, sort);
    result
}

fn sort_views(result: &mut [ConversationSession], sort: SessionSort) {
    result.sort_by(|a, b| {
        (match sort {
            SessionSort::Newest => b.created_ms.cmp(&a.created_ms),
            SessionSort::Oldest => a.created_ms.cmp(&b.created_ms),
            SessionSort::RecentActivity => b.updated_ms.cmp(&a.updated_ms),
            SessionSort::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
        })
        .then_with(|| b.created_ms.cmp(&a.created_ms))
        .then_with(|| a.id.cmp(&b.id))
    });
}

/// Use the order the user acted on, before the remote mutation changes metadata.
fn next_session(
    previous: &[ConversationSession],
    removed: &str,
    records: &[Record],
    sort: SessionSort,
) -> Option<String> {
    let available = |id: &str| {
        records
            .iter()
            .any(|r| r.id == id && !r.archived && r.id != removed)
    };
    if let Some(at) = previous.iter().position(|s| s.id == removed)
        && let Some(next) = previous[at + 1..]
            .iter()
            .chain(previous[..at].iter().rev())
            .find(|s| available(&s.id))
    {
        return Some(next.id.clone());
    }
    views(records, &[], sort)
        .into_iter()
        .find(|s| available(&s.id))
        .map(|s| s.id)
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
    pub async fn set_session_sort(self: &Arc<Self>, sort: SessionSort) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let host = core
                .snapshot()
                .selected_host
                .ok_or_else(|| AppError::State("host unavailable".into()))?;
            lock(&core.session_book).sorts.insert(host.clone(), sort);
            core.persist_session_book()?;
            core.update(|s| {
                s.session_sort = sort;
                sort_views(&mut s.sessions, sort);
            });
            Ok(())
        })
        .await
    }

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
            let client = clients.get(&chat_key(c, agent)).filter(|c| !c.is_closed());
            if let (Some(client), Some(controls)) = (client, &controls) {
                client.reconcile_async(controls);
            }
            let runtime = client.map(|c| c.runtime());
            let question = runtime.as_ref().is_some_and(|r| r.question().is_some())
                || controls.as_ref().is_some_and(|h| !h.pending.is_empty());
            let paused = lock(&self.paused_processes)
                .contains_key(&format!("{}/{}", c.host.id, agent.terminal_id));
            let status = activity_status(agent, paused, question, runtime.as_ref());
            states.insert(session.id.clone(), status.to_owned());
        }
        drop(clients);
        if self.generation.load(Ordering::SeqCst) == generation {
            self.update(|s| {
                for session in &mut s.sessions {
                    if let Some(status) = states.get(&session.id) {
                        session.status = status.clone();
                    }
                }
            });
        }
    }

    fn cached_history(&self, host: &str, id: &str) -> Option<CachedConversation> {
        let key = domain::cache_key(host, id);
        if let Some(value) = lock(&self.session_book).histories.get(&key).cloned() {
            return Some(value);
        }
        let value = {
            lock(&self.store)
                .as_ref()?
                .read_document::<CachedConversation>(&history_document(&key))
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
            let book = lock(&self.session_book);
            let cache = book.histories.get(&key).cloned();
            if let Some(cache) = cache {
                if let Some(store) = lock(&self.store).as_ref() {
                    store.write_document(&history_document(&key), &cache)?;
                }
                lock(&self.history_cache_written).insert(key.clone(), Instant::now());
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
        let cache = selected
            .as_ref()
            .and_then(|id| self.cached_history(&host, id));
        self.update(|s| {
            s.session_sort = book.sorts.get(&host).copied().unwrap_or_default();
            s.sessions = views(&records, &s.agents, s.session_sort);
            s.selected_session = selected;
            if s.selected_agent.is_none() {
                s.replace_shared_conversation(
                    cache
                        .as_ref()
                        .map(|c| c.messages.as_ref().clone())
                        .unwrap_or_default(),
                    cache.is_some_and(|c| c.available),
                );
            }
        });
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
        let removed: Vec<_> = book
            .catalogs
            .get(host)
            .into_iter()
            .flatten()
            .filter(|record| !catalog.sessions.iter().any(|r| r.id == record.id))
            .map(|record| domain::cache_key(host, &record.id))
            .collect();
        // Keep the book lease through removal so a stale history writer cannot recreate the file.
        for key in removed {
            book.histories.remove(&key);
            lock(&self.dirty_histories).remove(&key);
            lock(&self.history_cache_written).remove(&key);
            if let Some(store) = lock(&self.store).as_ref() {
                store.remove_document(&history_document(&key))?;
            }
        }
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
    pub(super) fn preferred_agent(&self, host: &str, agents: &[Agent]) -> Option<String> {
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
            return if record.archived {
                None
            } else {
                agents
                    .iter()
                    .find(|a| record.matches(a))
                    .map(|a| a.terminal_id.clone())
            };
        }
        agents
            .iter()
            .find(|a| !records.iter().any(|r| r.archived && r.matches(a)))
            .map(|a| a.terminal_id.clone())
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
            .selected_session
            .clone()
            .filter(|id| catalog.sessions.iter().any(|r| &r.id == id));
        if selected.is_none() {
            selected = current
                .selected_agent
                .as_ref()
                .and_then(|id| agents.iter().find(|a| &a.terminal_id == id))
                .and_then(|a| catalog.sessions.iter().find(|r| r.matches(a)))
                .map(|r| r.id.clone());
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
            selected = views(&catalog.sessions, agents, current.session_sort)
                .into_iter()
                .find(|r| !r.archived)
                .map(|r| r.id);
        }
        let record = selected
            .as_ref()
            .and_then(|id| catalog.sessions.iter().find(|r| &r.id == id));
        let active = record
            .filter(|r| !r.archived)
            .and_then(|r| agents.iter().find(|a| r.matches(a)))
            .map(|a| a.terminal_id.clone())
            .or_else(|| {
                if selected.is_none() {
                    current
                        .selected_agent
                        .clone()
                        .filter(|id| agents.iter().any(|a| &a.terminal_id == id))
                } else {
                    None
                }
            });
        let session_changed = current.selected_session != selected;
        let changed = session_changed || current.selected_agent != active;
        if session_changed {
            self.reset_history_view();
        }
        let cache = selected
            .as_ref()
            .and_then(|id| self.cached_history(&c.host.id, id));
        let mut all_views = views(&catalog.sessions, agents, current.session_sort);
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
        self.update(|s| {
            s.sessions = all_views;
            s.selected_session = selected.clone();
            s.selected_agent = active;
            if changed {
                if session_changed {
                    s.history_status = HistoryStatus::default();
                    s.replace_shared_conversation(
                        cache
                            .as_ref()
                            .map(|c| c.messages.as_ref().clone())
                            .unwrap_or_default(),
                        cache.is_some_and(|c| c.available),
                    );
                }
                s.composer = Composer::default();
                s.approval = None;
                s.question = None;
                s.response_activity = ResponseActivity::Idle;
                s.agent_output.clear();
                s.agent_pause = AgentPause::Running;
            }
        });
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
        if session_changed {
            *lock(&self.history) = None;
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
            let ordered = views(&catalog.sessions, agents, self.snapshot().session_sort);
            let record = selected
                .as_ref()
                .and_then(|id| catalog.sessions.iter().find(|r| &r.id == id && !r.archived))
                .or_else(|| {
                    ordered
                        .iter()
                        .find(|r| !r.archived)
                        .and_then(|view| catalog.sessions.iter().find(|r| r.id == view.id))
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
    fn cache_conversation(
        &self,
        host: &str,
        id: &str,
        messages: &SharedMessages,
        available: bool,
    ) -> Result<bool, AppError> {
        let key = domain::cache_key(host, id);
        let mut book = lock(&self.session_book);
        if book
            .catalogs
            .get(host)
            .is_some_and(|records| !records.iter().any(|r| r.id == id))
        {
            return Ok(false);
        }
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
    pub(super) async fn record_selected_history(&self, c: &Connected) -> Result<(), AppError> {
        self.flush_history_caches(false)?;
        let snapshot = self.snapshot();
        if snapshot.selected_host.as_deref() != Some(&c.host.id) {
            return Ok(());
        }
        let Some(id) = snapshot.selected_session else {
            return Ok(());
        };
        if self.cache_conversation(
            &c.host.id,
            &id,
            &snapshot.conversation,
            snapshot.conversation_available,
        )? && let Some(message) = snapshot
            .conversation
            .iter()
            .rev()
            .find(|m| m.tool.is_none() && !m.intermediate && !m.text.is_empty())
        {
            let (text, _) = composer::split_attachments(&message.text);
            let catalog = Repository::new(&c.ssh).note(&id, &text, false).await?;
            self.install_catalog(&c.host.id, &catalog)?;
        }
        Ok(())
    }
    pub(super) async fn refresh_saved_session(
        &self,
        c: &Connected,
        generation: u64,
    ) -> Result<(), AppError> {
        let snapshot = self.snapshot();
        let Some(id) = snapshot.selected_session else {
            return Ok(());
        };
        let Some(record) = self.session_record(&c.host.id, &id) else {
            return Ok(());
        };
        let Some(reference) = record.reference else {
            return Ok(());
        };
        let cached = lock(&self.history).clone();
        let history = history::read(&c.ssh, &reference, cached).await;
        if self.generation.load(Ordering::SeqCst) == generation
            && self.snapshot().selected_session.as_ref() == Some(&id)
            && self.snapshot().selected_agent.is_none()
        {
            self.present_history(&history, history.messages.clone());
            self.record_selected_history(c).await?;
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
            let record = core
                .session_record(&host, &id)
                .ok_or_else(|| AppError::State("session unavailable".into()))?;
            let agents = core.snapshot().agents;
            let active = if record.archived {
                None
            } else {
                agents
                    .iter()
                    .find(|a| record.matches(a))
                    .map(|a| a.terminal_id.clone())
            };
            let cache = core.cached_history(&host, &id);
            lock(&core.session_book)
                .selected
                .insert(host.clone(), Some(id.clone()));
            core.persist_session_book()?;
            core.reset_history_view();
            core.update(|s| {
                s.selected_session = Some(id);
                s.history_status = HistoryStatus::default();
                s.selected_agent = active;
                s.replace_shared_conversation(
                    cache
                        .as_ref()
                        .map(|c| c.messages.as_ref().clone())
                        .unwrap_or_default(),
                    cache.is_some_and(|c| c.available),
                );
                s.composer = Composer::default();
                s.approval = None;
                s.question = None;
                s.response_activity = ResponseActivity::Idle;
                s.agent_output.clear();
                s.agent_pause = AgentPause::Running;
            });
            *lock(&core.history) = None;
            drop(_guard);
            if let Some(c) = connection {
                if let Some(agent) = core.snapshot().selected_agent {
                    core.refresh_agent(&c, &agent, core.generation.load(Ordering::SeqCst))
                        .await;
                } else {
                    core.refresh_saved_session(&c, core.generation.load(Ordering::SeqCst))
                        .await?;
                }
            }
            Ok(())
        })
        .await
    }
    async fn activate_session(&self, c: &Connected, id: &str) -> Result<(), AppError> {
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
        self.update(|s| {
            s.agents = agents;
            s.selected_session = Some(id.into());
            s.selected_agent = Some(agent.terminal_id);
            s.composer = Composer::default();
            s.approval = None;
            s.question = None;
            s.response_activity = ResponseActivity::Idle;
            s.agent_pause = AgentPause::Running;
            s.failure = None;
        });
        *lock(&self.history) = None;
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
            let before = core.snapshot();
            let catalog = Repository::new(&c.ssh).archive(&id, archived).await?;
            core.apply_session_archive(&c.host.id, &id, archived, &catalog, &before)?;
            let agents = core.snapshot().agents;
            drop(_guard);
            core.sync_sessions(&c, &agents, core.generation.load(Ordering::SeqCst))
                .await?;
            Ok(())
        })
        .await
    }

    fn apply_session_archive(
        &self,
        host: &str,
        id: &str,
        archived: bool,
        catalog: &Catalog,
        before: &Snapshot,
    ) -> Result<(), AppError> {
        let catalog = self.install_catalog(host, catalog)?;
        let selected = archived && before.selected_session.as_deref() == Some(id);
        let next = if selected {
            next_session(&before.sessions, id, &catalog.sessions, before.session_sort)
        } else {
            before.selected_session.clone()
        };
        let cache = if selected {
            next.as_ref().and_then(|id| self.cached_history(host, id))
        } else {
            None
        };
        if selected {
            self.reset_history_view();
            lock(&self.session_book)
                .selected
                .insert(host.to_owned(), next.clone());
            self.persist_session_book()?;
            *lock(&self.history) = None;
        }
        self.update(|s| {
            s.sessions = views(&catalog.sessions, &s.agents, s.session_sort);
            if selected {
                s.selected_session = next;
                s.selected_agent = None;
                s.replace_shared_conversation(
                    cache
                        .as_ref()
                        .map(|c| c.messages.as_ref().clone())
                        .unwrap_or_default(),
                    cache.is_some_and(|c| c.available),
                );
                s.history_status = HistoryStatus::default();
                s.composer = Composer::default();
                s.approval = None;
                s.question = None;
                s.response_activity = ResponseActivity::Idle;
                s.agent_output.clear();
                s.agent_pause = AgentPause::Running;
            }
        });
        Ok(())
    }

    pub async fn delete_session(
        self: &Arc<Self>,
        host_id: String,
        id: String,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let connection = core.connection()?;
            let record = core.session_record(&connection.host.id, &id);
            if connection.host.id != host_id {
                return Err(AppError::State(
                    "session deletion belongs to another host".into(),
                ));
            }
            let catalog = Repository::new(&connection.ssh)
                .delete(&id, &connection.path)
                .await?;
            let agents: Vec<_> = core
                .snapshot()
                .agents
                .into_iter()
                .filter(|a| {
                    !record.as_ref().is_some_and(|r| {
                        r.matches(a) || (!r.pane_id.is_empty() && r.pane_id == a.pane_id)
                    })
                })
                .collect();
            if let Some(record) = &record {
                let key = format!(
                    "{}/{}",
                    connection.host.id,
                    record
                        .reference
                        .as_ref()
                        .map(history::key)
                        .unwrap_or_else(|| fingerprint(&record.terminal_id))
                );
                core.chat_clients.lock().await.remove(&key);
                lock(&core.chat_failures).remove(&key);
                lock(&core.uploads).remove(&key);
                lock(&core.skill_drafts).remove(&key);
                lock(&core.paused_processes)
                    .remove(&format!("{}/{}", connection.host.id, record.terminal_id));
                core.persist_pauses()?;
            }
            core.apply_session_deletion(&connection.host.id, &id, &catalog, agents)?;
            core.refresh_resources(&connection, core.generation.load(Ordering::SeqCst))
                .await;
            Ok(())
        })
        .await
    }

    fn apply_session_deletion(
        &self,
        host: &str,
        id: &str,
        catalog: &Catalog,
        agents: Vec<Agent>,
    ) -> Result<(), AppError> {
        let selected = self.snapshot().selected_session.as_deref() == Some(id);
        let before = self.snapshot();
        if selected {
            self.reset_history_view();
        }
        let catalog = self.install_catalog(host, catalog)?;
        let next = if selected {
            next_session(&before.sessions, id, &catalog.sessions, before.session_sort)
        } else {
            self.snapshot().selected_session
        };
        lock(&self.session_book)
            .selected
            .insert(host.into(), next.clone());
        let cache = selected
            .then(|| next.as_ref().and_then(|id| self.cached_history(host, id)))
            .flatten();
        self.update(|snapshot| {
            snapshot.sessions = views(&catalog.sessions, &agents, snapshot.session_sort);
            snapshot.agents = agents;
            if selected {
                snapshot.selected_session = next;
                snapshot.selected_agent = None;
                snapshot.replace_shared_conversation(
                    cache
                        .as_ref()
                        .map(|c| c.messages.as_ref().clone())
                        .unwrap_or_default(),
                    cache.is_some_and(|c| c.available),
                );
                snapshot.history_status = HistoryStatus::default();
                snapshot.composer = Composer::default();
                snapshot.approval = None;
                snapshot.question = None;
                snapshot.response_activity = ResponseActivity::Idle;
                snapshot.agent_output.clear();
                snapshot.agent_pause = AgentPause::Running;
                snapshot.failure = None;
            }
        });
        if selected {
            *lock(&self.history) = None;
        }
        self.persist_session_book()?;
        Ok(())
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
            core.reset_history_view();
            core.update(|s| {
                s.sessions = views(&catalog.sessions, &s.agents, s.session_sort);
                s.selected_session = Some(id.clone());
                s.selected_agent = None;
                s.replace_conversation(Vec::new(), false);
                s.conversation_available = false;
                s.history_status = HistoryStatus::default();
                s.composer = Composer::default();
                s.approval = None;
                s.question = None;
                s.response_activity = ResponseActivity::Idle;
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
        text: &str,
    ) -> Result<(), AppError> {
        let id = lock(&self.session_book)
            .catalogs
            .get(&c.host.id)
            .and_then(|records| records.iter().find(|r| r.matches(agent)))
            .map(|r| r.id.clone());
        if let Some(id) = id {
            let (body, files) = composer::split_attachments(text);
            let display = if body.trim().is_empty() {
                files
                    .into_iter()
                    .map(|f| f.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                body
            };
            let catalog = Repository::new(&c.ssh).note(&id, &display, true).await?;
            self.install_catalog(&c.host.id, &catalog)?;
            self.record_selected_history(c).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_order_does_not_move_on_activity_or_archive_and_ties_are_deterministic() {
        let mut records: Vec<_> = [("b", 10), ("c", 20), ("a", 10)]
            .into_iter()
            .map(|(id, created)| {
                Record::draft(id.into(), "codex".into(), id.to_uppercase(), None, created)
            })
            .collect();
        let ids = |sort| {
            views(&records, &[], sort)
                .into_iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(SessionSort::Newest), ["c", "a", "b"]);
        assert_eq!(ids(SessionSort::Oldest), ["a", "b", "c"]);
        assert_eq!(ids(SessionSort::Title), ["a", "b", "c"]);
        records[0].updated_ms = 9999;
        records[0].archived = true;
        records.reverse();
        let ids = |sort| {
            views(&records, &[], sort)
                .into_iter()
                .map(|s| s.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(SessionSort::Newest), ["c", "a", "b"]);
        assert_eq!(ids(SessionSort::RecentActivity), ["b", "c", "a"]);
    }

    #[test]
    fn removing_the_selection_uses_its_previous_neighbor_even_after_remote_reordering() {
        let mut records: Vec<_> = [("a", 30), ("b", 20), ("c", 10)]
            .into_iter()
            .map(|(id, created)| Record::draft(id.into(), "codex".into(), id.into(), None, created))
            .collect();
        let before = views(&records, &[], SessionSort::RecentActivity);
        records[1].archived = true;
        records[0].updated_ms = 1000;
        records[2].updated_ms = 999;
        assert_eq!(
            next_session(&before, "b", &records, SessionSort::RecentActivity).as_deref(),
            Some("c")
        );
        records.retain(|r| r.id != "c");
        assert_eq!(
            next_session(&before, "c", &records, SessionSort::Newest).as_deref(),
            Some("a")
        );
        records[0].archived = true;
        assert!(next_session(&before, "b", &records, SessionSort::Newest).is_none());
    }
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
    fn archiving_publishes_the_new_list_and_neighbor_cache_without_disturbing_other_selection() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        let mut records: Vec<_> = [("a", 30), ("b", 20), ("c", 10)]
            .into_iter()
            .map(|(id, created)| Record::draft(id.into(), "codex".into(), id.into(), None, created))
            .collect();
        core.update(|s| {
            s.selected_host = Some("host".into());
            s.selected_session = Some("b".into());
            s.sessions = views(&records, &[], SessionSort::Newest);
        });
        let before = core.snapshot();
        let messages = Arc::new(vec![Arc::new(Message {
            id: "cached-next".into(),
            role: Role::Assistant,
            text: "Next conversation".into(),
            intermediate: false,
            tool: None,
            question_replies: Vec::new(),
        })]);
        core.cache_conversation("host", "c", &messages, true)
            .unwrap();
        records[1].archived = true;
        records[0].updated_ms = 9999;
        let catalog = Catalog {
            revision: 1,
            now_ms: 1,
            sessions: records.clone(),
        };
        core.apply_session_archive("host", "b", true, &catalog, &before)
            .unwrap();
        assert_eq!(core.snapshot().selected_session.as_deref(), Some("c"));
        assert_eq!(core.snapshot().conversation[0].text, "Next conversation");
        assert!(
            core.snapshot()
                .sessions
                .iter()
                .find(|s| s.id == "b")
                .unwrap()
                .archived
        );
        records[0].archived = true;
        let before = core.snapshot();
        core.apply_session_archive(
            "host",
            "a",
            true,
            &Catalog {
                revision: 2,
                now_ms: 2,
                sessions: records,
            },
            &before,
        )
        .unwrap();
        assert_eq!(core.snapshot().selected_session.as_deref(), Some("c"));
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
    fn deleting_the_selected_session_clears_its_cache_and_selects_the_next_active_record() {
        let directory = tempfile::tempdir().unwrap();
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        *lock(&core.store) = Some(Store::open(directory.path()).unwrap());
        let removed = Record::draft("removed".into(), "codex".into(), "removed".into(), None, 1);
        let next = Record::draft("next".into(), "codex".into(), "next".into(), None, 2);
        let mut archived = Record::draft(
            "archived".into(),
            "codex".into(),
            "archived".into(),
            None,
            3,
        );
        archived.archived = true;
        let previous = Catalog {
            revision: 1,
            now_ms: 1,
            sessions: vec![removed, next.clone(), archived.clone()],
        };
        core.install_catalog("host", &previous).unwrap();
        core.update(|s| {
            s.selected_host = Some("host".into());
            s.selected_session = Some("removed".into());
            s.selected_agent = Some("old-agent".into());
        });
        let messages = Arc::new(vec![Arc::new(Message {
            id: "old".into(),
            role: Role::Assistant,
            text: "old conversation".into(),
            intermediate: false,
            tool: None,
            question_replies: Vec::new(),
        })]);
        core.cache_conversation("host", "removed", &messages, true)
            .unwrap();
        core.flush_history_caches(true).unwrap();
        let key = domain::cache_key("host", "removed");
        let latest = Catalog {
            revision: 2,
            now_ms: 2,
            sessions: vec![next, archived],
        };
        core.apply_session_deletion("host", "removed", &latest, vec![])
            .unwrap();
        let snapshot = core.snapshot();
        assert_eq!(snapshot.selected_session.as_deref(), Some("next"));
        assert!(snapshot.selected_agent.is_none() && snapshot.conversation.is_empty());
        assert!(!lock(&core.session_book).histories.contains_key(&key));
        assert!(!lock(&core.dirty_histories).contains(&key));
        let store = lock(&core.store);
        assert!(
            store
                .as_ref()
                .unwrap()
                .read_document::<CachedConversation>(&history_document(&key))
                .unwrap()
                .is_none()
        );
        drop(store);
        assert!(
            !core
                .cache_conversation("host", "removed", &messages, true)
                .unwrap()
        );
        core.flush_history_caches(true).unwrap();
        assert!(core.cached_history("host", "removed").is_none());
        assert_eq!(
            core.install_catalog("host", &previous)
                .unwrap()
                .sessions
                .len(),
            2
        );
        let stored: SessionBook = lock(&core.store)
            .as_ref()
            .unwrap()
            .read_document("sessions")
            .unwrap()
            .unwrap();
        assert_eq!(stored.selected["host"].as_deref(), Some("next"));
    }
    #[test]
    fn deleting_a_background_or_last_session_keeps_selection_consistent() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        let selected = Record::draft("selected".into(), "codex".into(), "".into(), None, 1);
        let background = Record::draft("background".into(), "codex".into(), "".into(), None, 2);
        core.install_catalog(
            "host",
            &Catalog {
                revision: 1,
                now_ms: 1,
                sessions: vec![selected.clone(), background],
            },
        )
        .unwrap();
        core.update(|s| {
            s.selected_host = Some("host".into());
            s.selected_session = Some("selected".into());
            s.selected_agent = Some("active".into());
        });
        core.apply_session_deletion(
            "host",
            "background",
            &Catalog {
                revision: 2,
                now_ms: 2,
                sessions: vec![selected],
            },
            vec![],
        )
        .unwrap();
        assert_eq!(
            core.snapshot().selected_session.as_deref(),
            Some("selected")
        );
        assert_eq!(core.snapshot().selected_agent.as_deref(), Some("active"));
        core.apply_session_deletion(
            "host",
            "selected",
            &Catalog {
                revision: 3,
                now_ms: 3,
                sessions: vec![],
            },
            vec![],
        )
        .unwrap();
        assert!(core.snapshot().selected_session.is_none());
        assert!(core.snapshot().selected_agent.is_none());
        assert!(core.snapshot().sessions.is_empty());
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
        let view = views(&restored.catalogs["host"], &[], SessionSort::default())
            .pop()
            .unwrap();
        assert!(view.archived && view.agent_id.is_none() && !view.recorded);
        assert_eq!(view.provider, "claude");
        assert_eq!(restored.selected["host"], Some("a".into()));
    }
    #[test]
    fn an_empty_successful_or_failed_read_never_erases_a_nonempty_cache() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        let message = Message {
            id: "old".into(),
            role: Role::Assistant,
            text: "retained history".into(),
            intermediate: false,
            tool: None,
            question_replies: Vec::new(),
        };
        core.cache_conversation(
            "host",
            "session",
            &Arc::new(vec![Arc::new(message.clone())]),
            true,
        )
        .unwrap();
        for available in [true, false] {
            assert!(
                !core
                    .cache_conversation("host", "session", &Arc::new(Vec::new()), available)
                    .unwrap()
            );
            assert_eq!(
                lock(&core.session_book).histories[&domain::cache_key("host", "session")].messages,
                Arc::new(vec![Arc::new(message.clone())])
            );
        }
    }
    #[tokio::test]
    async fn old_inline_history_migrates_to_a_complete_independent_cache() {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let key = domain::cache_key("host", "session");
        let messages: Vec<Message> = (0..10000)
            .map(|i| Message {
                id: format!("m-{i}"),
                role: Role::Assistant,
                text: format!("record {i}"),
                intermediate: false,
                tool: None,
                question_replies: Vec::new(),
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
            .read_document(&history_document(&key))
            .unwrap()
            .unwrap();
        assert_eq!(stored.messages.len(), 10000);
        let metadata: serde_json::Value = store.read_document("sessions").unwrap().unwrap();
        assert!(metadata.get("histories").is_none());
        assert_eq!(core.snapshot().conversation[0].id, "m-0");
        assert_eq!(core.snapshot().conversation[9999].id, "m-9999");
        lock(&core.session_book).histories.clear();
        assert_eq!(
            core.cached_history("host", "session")
                .unwrap()
                .messages
                .len(),
            10000
        );
    }
    #[test]
    fn memory_eviction_keeps_every_saved_conversation_file() {
        let directory = tempfile::tempdir().unwrap();
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        *lock(&core.store) = Some(Store::open(directory.path()).unwrap());
        for i in 0..16 {
            let records = Arc::new(vec![Arc::new(Message {
                id: format!("m-{i}"),
                role: Role::Assistant,
                text: format!("conversation {i}"),
                intermediate: false,
                tool: None,
                question_replies: Vec::new(),
            })]);
            core.cache_conversation("host", &format!("s-{i}"), &records, true)
                .unwrap();
        }
        core.flush_history_caches(true).unwrap();
        assert!(lock(&core.session_book).histories.len() <= 4);
        for i in 0..16 {
            let cache = core.cached_history("host", &format!("s-{i}")).unwrap();
            assert_eq!(cache.messages[0].id, format!("m-{i}"));
            assert!(lock(&core.session_book).histories.len() <= 4);
        }
    }
}
