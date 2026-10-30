use super::*;
use kite_agent::sessions::{self as domain, Catalog, Record, Repository};
use serde::{Deserialize, Serialize};

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
fn views(records: &[Record], agents: &[Agent]) -> Vec<ConversationSession> {
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
                    .map(|a| a.agent_status.clone())
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

impl AppCore {
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
            let cache = lock(&self.session_book).histories.get(&key).cloned();
            if let Some(cache) = cache {
                if let Some(store) = lock(&self.store).as_ref() {
                    store.write_document(&history_document(&key), &cache)?;
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
        let cache = selected
            .as_ref()
            .and_then(|id| self.cached_history(&host, id));
        self.update(|s| {
            s.sessions = views(&records, &s.agents);
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
        let all_views = views(&catalog.sessions, agents);
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
            if let Some(record) = record.filter(|r| r.provider == "codex" && r.reference.is_some())
            {
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
        let mut agents = c.api.agents().await?;
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
            let catalog = Repository::new(&c.ssh).archive(&id, archived).await?;
            core.install_catalog(&c.host.id, &catalog)?;
            if archived && core.snapshot().selected_session.as_ref() == Some(&id) {
                core.reset_history_view();
                let next = catalog
                    .sessions
                    .iter()
                    .filter(|r| !r.archived)
                    .max_by_key(|r| r.updated_ms)
                    .map(|r| r.id.clone());
                lock(&core.session_book)
                    .selected
                    .insert(c.host.id.clone(), next.clone());
                core.update(|s| {
                    s.selected_session = next;
                    s.selected_agent = None;
                    s.replace_conversation(Vec::new(), false);
                    s.conversation_available = false;
                    s.history_status = HistoryStatus::default();
                    s.composer = Composer::default();
                    s.approval = None;
                    s.question = None;
                    s.response_activity = ResponseActivity::Idle;
                });
                core.persist_session_book()?;
                *lock(&core.history) = None;
            }
            let agents = core.snapshot().agents;
            drop(_guard);
            core.sync_sessions(&c, &agents, core.generation.load(Ordering::SeqCst))
                .await?;
            Ok(())
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
            if !matches!(provider.as_str(), "codex" | "claude")
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
                s.sessions = views(&catalog.sessions, &s.agents);
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
            if provider == "codex" {
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
        let message = Message {
            id: "old".into(),
            role: Role::Assistant,
            text: "retained history".into(),
            intermediate: false,
            tool: None,
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
