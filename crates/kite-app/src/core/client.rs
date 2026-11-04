//! One client, independent host workspaces, and a single selected media surface.
use super::*;
use crate::{ClipboardPlatform, RemoteClipboard};

struct WorkspaceObserver {
    client: std::sync::Weak<ClientCore>,
    host: Option<String>,
}
impl Observer for WorkspaceObserver {
    fn changed(&self, snapshot: Snapshot) {
        if let Some(client) = self.client.upgrade() {
            client.accept(self.host.clone(), snapshot);
        }
    }
}

#[derive(Default)]
struct ClientState {
    revision: u64,
    notification_revision: u64,
    selected: Option<String>,
    base: Snapshot,
    workspaces: BTreeMap<String, Snapshot>,
}

/// Shared persistence is serialized once; transport, polling, retries and conversation
/// state belong to each host. Changing selection never replaces a host's runtime.
pub struct ClientCore {
    base: Arc<AppCore>,
    observer: Arc<dyn Observer>,
    state: Mutex<ClientState>,
    runtimes: Mutex<BTreeMap<String, Arc<AppCore>>>,
    operations: AsyncMutex<()>,
    self_weak: std::sync::Weak<Self>,
}

impl ClientCore {
    pub fn new(secrets: Arc<dyn SecretStorage>, observer: Arc<dyn Observer>) -> Arc<Self> {
        Arc::new_cyclic(|weak| Self {
            base: AppCore::new(
                secrets,
                Arc::new(WorkspaceObserver {
                    client: weak.clone(),
                    host: None,
                }),
            ),
            observer,
            state: Mutex::new(ClientState::default()),
            runtimes: Mutex::new(BTreeMap::new()),
            operations: AsyncMutex::new(()),
            self_weak: weak.clone(),
        })
    }

    fn accept(&self, host: Option<String>, snapshot: Snapshot) {
        {
            let mut state = lock(&self.state);
            let previous = host
                .as_ref()
                .and_then(|id| state.workspaces.get(id))
                .unwrap_or(&state.base);
            if host.is_some()
                && snapshot.revision <= previous.revision
                && state.workspaces.contains_key(host.as_ref().unwrap())
            {
                return;
            }
            if snapshot.notification_revision != previous.notification_revision {
                state.notification_revision = state.notification_revision.wrapping_add(1);
            }
            if let Some(host) = host {
                state.workspaces.insert(host, snapshot);
            } else {
                if state.selected.is_none() {
                    state.selected = snapshot.selected_host.clone();
                }
                state.base = snapshot;
            }
            state.revision = state.revision.saturating_add(1);
        }
        self.observer.changed(self.snapshot());
    }

    fn publish(&self) {
        lock(&self.state).revision += 1;
        self.observer.changed(self.snapshot());
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut snapshot = {
            let state = lock(&self.state);
            let mut snapshot = state
                .selected
                .as_ref()
                .and_then(|id| state.workspaces.get(id))
                .unwrap_or(&state.base)
                .clone();
            snapshot.selected_host = state.selected.clone();
            snapshot.revision = state.revision;
            snapshot.notification_revision = state.notification_revision;
            snapshot.host_connections = state
                .workspaces
                .iter()
                .map(|(id, s)| HostConnection {
                    host_id: id.clone(),
                    connection: s.connection,
                    connection_step: s.connection_step,
                    failure: s.failure.clone(),
                    resources: s.host_resources.clone(),
                })
                .collect();
            snapshot
        };
        if let Some(store) = lock(&self.base.store).as_ref() {
            snapshot.hosts = store.data().hosts.clone();
        }
        snapshot
    }

    fn runtime(&self, host: &str) -> Result<Arc<AppCore>, AppError> {
        self.base.host(host)?;
        let mut runtimes = lock(&self.runtimes);
        Ok(runtimes
            .entry(host.to_owned())
            .or_insert_with(|| {
                let core = AppCore::workspace(
                    &self.base,
                    host.to_owned(),
                    Arc::new(WorkspaceObserver {
                        client: self.self_weak.clone(),
                        host: Some(host.to_owned()),
                    }),
                );
                core.network_changed(self.base.network_available.load(Ordering::SeqCst));
                core
            })
            .clone())
    }

    fn selected(&self) -> Result<Arc<AppCore>, AppError> {
        let host = lock(&self.state)
            .selected
            .clone()
            .ok_or_else(|| AppError::State("host unavailable".into()))?;
        self.runtime(&host)
    }

    pub async fn initialize(
        self: &Arc<Self>,
        directory: PathBuf,
        device: String,
        legacy: LegacyImport,
    ) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        self.base.initialize(directory, device, legacy).await?;
        let connections = {
            let store = lock(&self.base.store);
            let store = store
                .as_ref()
                .ok_or_else(|| AppError::State("core is not initialized".into()))?;
            match store.read_document::<recovery::Connections>("connections") {
                Ok(Some(connections)) => connections,
                Ok(None) => recovery::Connections {
                    hosts: lock(&self.base.desired_connection)
                        .iter()
                        .cloned()
                        .collect(),
                },
                Err(error) => {
                    log::warn!("connection intents unavailable: {error}");
                    recovery::Connections::default()
                }
            }
        };
        *lock(&self.base.connection_intents) = connections
            .hosts
            .into_iter()
            .filter(|id| self.base.host(id).is_ok())
            .collect();
        let selected = lock(&self.state).selected.clone();
        if let Some(host) = selected {
            self.runtime(&host)?;
        }
        Ok(())
    }

    async fn select_inner(&self, host: &str) -> Result<(), AppError> {
        let current = lock(&self.state).selected.clone();
        if current.as_deref() == Some(host) {
            return Ok(());
        }
        self.base.host(host)?;
        if let Some(old) = current.and_then(|id| lock(&self.runtimes).get(&id).cloned()) {
            old.stop_desktop().await?;
        }
        {
            let mut store = lock(&self.base.store);
            let store = store
                .as_mut()
                .ok_or_else(|| AppError::State("core is not initialized".into()))?;
            let mut next = store.data().clone();
            next.selected_host = Some(host.to_owned());
            store.commit(next)?;
        }
        self.runtime(host)?;
        lock(&self.state).selected = Some(host.to_owned());
        self.publish();
        Ok(())
    }

    pub async fn select_host(&self, host: String) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        self.select_inner(&host).await
    }

    pub async fn connect(&self, host: String) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        self.select_inner(&host).await?;
        let runtime = self.runtime(&host)?;
        if !matches!(
            runtime.snapshot().connection,
            ConnectionPhase::Disconnected | ConnectionPhase::Failed
        ) {
            return Ok(());
        }
        runtime.connect(host).await
    }

    pub async fn restore_connection(&self) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        let hosts = lock(&self.base.connection_intents).clone();
        for host in hosts {
            let runtime = self.runtime(&host)?;
            if matches!(
                runtime.snapshot().connection,
                ConnectionPhase::Disconnected | ConnectionPhase::Failed
            ) {
                *lock(&runtime.desired_connection) = Some(host);
                runtime.restore_connection().await?;
            }
        }
        Ok(())
    }

    pub fn network_changed(&self, available: bool) {
        self.base.network_changed(available);
        for runtime in lock(&self.runtimes).values() {
            runtime.network_changed(available);
        }
    }

    pub async fn save_host(&self, host: Host) -> Result<String, AppError> {
        let _guard = self.operations.lock().await;
        let runtime = if host.id.is_empty() || self.base.host(&host.id).is_err() {
            self.base.clone()
        } else {
            self.runtime(&host.id)?
        };
        let id = runtime.save_host(host).await?;
        if lock(&self.state).selected.is_none() {
            lock(&self.state).selected = Some(id.clone());
        }
        self.publish();
        Ok(id)
    }

    pub async fn disconnect(&self) -> Result<(), AppError> {
        let host = lock(&self.state)
            .selected
            .clone()
            .ok_or_else(|| AppError::State("host unavailable".into()))?;
        self.disconnect_host(host).await
    }

    pub async fn disconnect_host(&self, host: String) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        self.runtime(&host)?.disconnect().await
    }

    pub async fn disconnect_all(&self) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        let mut hosts = lock(&self.base.connection_intents).clone();
        hosts.extend(lock(&self.runtimes).keys().cloned());
        for host in hosts {
            self.runtime(&host)?.disconnect().await?;
        }
        Ok(())
    }

    pub async fn trust_host(&self, host: String, fingerprint: String) -> Result<(), AppError> {
        self.runtime(&host)?.trust_host(host, fingerprint).await
    }

    pub async fn forget_host(&self, host: String) -> Result<(), AppError> {
        self.runtime(&host)?.forget_host(host).await
    }

    pub fn notifications(&self) -> Vec<AgentNotice> {
        self.base.notifications()
    }
    pub fn notification_host(&self, id: &str) -> Result<String, AppError> {
        self.base.notification_host(id)
    }
    pub async fn load_notification(&self, id: String) -> Result<AgentNotice, AppError> {
        self.runtime(&self.notification_host(&id)?)?
            .load_notification(id)
            .await
    }
    pub async fn receive_push(
        &self,
        id: String,
        kind: crate::AgentNoticeKind,
        session: String,
        created_ms: i64,
    ) -> Result<AgentNotice, AppError> {
        self.base.receive_push(id, kind, session, created_ms).await
    }
    pub async fn acknowledge_notification(
        &self,
        id: String,
        read: bool,
        alerted: bool,
    ) -> Result<(), AppError> {
        self.base.acknowledge_notification(id, read, alerted).await
    }
    pub async fn register_push(
        &self,
        device: String,
        token: Option<String>,
        project: String,
    ) -> Result<bool, AppError> {
        let runtimes: Vec<_> = lock(&self.runtimes)
            .values()
            .filter(|core| core.snapshot().connection == ConnectionPhase::Connected)
            .cloned()
            .collect();
        let mut configured = !runtimes.is_empty();
        let mut failure = None;
        for core in runtimes {
            match core
                .register_push(device.clone(), token.clone(), project.clone())
                .await
            {
                Ok(ready) => configured &= ready,
                Err(error) => {
                    configured = false;
                    failure = Some(error);
                }
            }
        }
        if let Some(error) = failure {
            Err(error)
        } else {
            Ok(configured)
        }
    }
    pub fn conversation_since(&self, context: &str, revision: u64) -> ConversationDelta {
        self.selected()
            .unwrap_or_else(|_| self.base.clone())
            .conversation_since(context, revision)
    }

    pub async fn open_session(&self, host: String, id: String) -> Result<(), AppError> {
        self.runtime(&host)?.open_session(id).await
    }
    pub async fn resume_session(&self, host: String, id: String) -> Result<(), AppError> {
        self.runtime(&host)?.resume_session(id).await
    }
    pub async fn archive_session(
        &self,
        host: String,
        id: String,
        archived: bool,
    ) -> Result<(), AppError> {
        self.runtime(&host)?.archive_session(id, archived).await
    }
    pub async fn create_session(
        &self,
        host: String,
        provider: String,
        cwd: String,
        title: String,
    ) -> Result<(), AppError> {
        self.runtime(&host)?
            .create_session(provider, cwd, title)
            .await
    }
    pub async fn delete_session(&self, host: String, id: String) -> Result<(), AppError> {
        self.runtime(&host)?.delete_session(host, id).await
    }
    pub async fn release_archived_sessions(
        &self,
        host: String,
    ) -> Result<SessionCleanup, AppError> {
        self.runtime(&host)?.release_archived_sessions(host).await
    }
    pub async fn set_agent_memory_limit(&self, host: String, bytes: u64) -> Result<(), AppError> {
        self.runtime(&host)?
            .set_agent_memory_limit(host, bytes)
            .await
    }
    pub async fn set_session_sort(&self, host: String, sort: SessionSort) -> Result<(), AppError> {
        self.runtime(&host)?.set_session_sort(sort).await
    }
}

impl Drop for ClientCore {
    fn drop(&mut self) {
        // Jobs retain their workspace while alive. Cancel them when the client owner leaves.
        for core in lock(&self.runtimes).values() {
            core.generation.fetch_add(1, Ordering::SeqCst);
            for job in [
                &core.connect_job,
                &core.recovery_job,
                &core.poll_job,
                &core.resource_job,
                &core.history_sync_job,
                &core.notification_job,
            ] {
                if let Some(job) = lock(job).take() {
                    job.abort();
                }
            }
            if let Ok(job) = core.desktop_job.try_lock()
                && let Some(job) = job.as_ref()
            {
                let _ = job.cancel.send(true);
            }
        }
    }
}

impl ClientCore {
    pub async fn ensure_agent(&self, host: String) -> Result<(), AppError> {
        self.runtime(&host)?.ensure_agent().await
    }
    pub async fn reload_composer(&self) -> Result<(), AppError> {
        self.selected()?.reload_composer().await
    }
    pub async fn refresh_account_usage(&self) -> Result<(), AppError> {
        self.selected()?.refresh_account_usage().await
    }
    pub async fn set_chat_model(
        &self,
        host: String,
        id: String,
        model: String,
        effort: String,
    ) -> Result<(), AppError> {
        self.runtime(&host)?.set_chat_model(id, model, effort).await
    }
    pub async fn upload_attachment(
        &self,
        host: String,
        id: String,
        local_path: String,
        name: String,
        mime: String,
        retry_id: Option<String>,
    ) -> Result<(), AppError> {
        self.runtime(&host)?
            .upload_attachment(id, local_path, name, mime, retry_id)
            .await
    }
    pub async fn remove_attachment(
        &self,
        host: String,
        id: String,
        upload_id: String,
    ) -> Result<(), AppError> {
        self.runtime(&host)?.remove_attachment(id, upload_id).await
    }
    pub async fn select_agent(&self, host: String, id: String) -> Result<(), AppError> {
        self.runtime(&host)?.select_agent(id).await
    }
    pub async fn prompt_agent(
        &self,
        host: String,
        id: String,
        text: String,
    ) -> Result<(), AppError> {
        self.runtime(&host)?.prompt_agent(id, text).await
    }
    pub async fn interrupt_agent(&self, host: String, id: String) -> Result<(), AppError> {
        self.runtime(&host)?.interrupt_agent(id).await
    }
    pub async fn send_agent_key(
        &self,
        host: String,
        id: String,
        key: String,
    ) -> Result<(), AppError> {
        self.runtime(&host)?.send_agent_key(id, key).await
    }
    pub async fn respond_approval(
        &self,
        host: String,
        id: String,
        request: String,
        choice: u32,
    ) -> Result<(), AppError> {
        self.runtime(&host)?
            .respond_approval(id, request, choice)
            .await
    }
    pub async fn answer_question(
        &self,
        host: String,
        id: String,
        request: String,
        answers: Vec<QuestionAnswer>,
    ) -> Result<(), AppError> {
        self.runtime(&host)?
            .answer_question(id, request, answers)
            .await
    }
    pub async fn set_agent_paused(
        &self,
        host: String,
        id: String,
        paused: bool,
    ) -> Result<(), AppError> {
        self.runtime(&host)?.set_agent_paused(id, paused).await
    }
    pub async fn clear_failure(&self) -> Result<(), AppError> {
        self.selected()?.clear_failure().await
    }
    fn require_selected(&self, host: &str) -> Result<(), AppError> {
        if lock(&self.state).selected.as_deref() != Some(host) {
            return Err(AppError::State("workspace changed".into()));
        }
        Ok(())
    }
    pub async fn start_desktop(
        &self,
        host: String,
        sink: Arc<dyn MediaSink>,
    ) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        self.require_selected(&host)?;
        self.runtime(&host)?.start_desktop(sink).await
    }
    pub async fn approve_pairing(&self, host: String, pin: String) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        self.require_selected(&host)?;
        self.runtime(&host)?.approve_pairing(pin).await
    }
    pub async fn set_takeover(&self, host: String, enabled: bool) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        if enabled {
            self.require_selected(&host)?;
        }
        self.runtime(&host)?.set_takeover(enabled).await
    }
    pub async fn stop_desktop(&self, host: String) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        self.runtime(&host)?.stop_desktop().await
    }
    pub async fn read_clipboard(&self, host: String) -> Result<RemoteClipboard, AppError> {
        self.selected()?.read_clipboard(host).await
    }
    pub async fn write_clipboard(
        &self,
        host: String,
        text: String,
    ) -> Result<ClipboardPlatform, AppError> {
        self.selected()?.write_clipboard(host, text).await
    }
    pub async fn synchronize_clipboard(
        &self,
        host: String,
        local: Option<String>,
    ) -> Result<Option<String>, AppError> {
        self.selected()?.synchronize_clipboard(host, local).await
    }
    pub async fn set_chat_setting(
        &self,
        host: String,
        id: String,
        setting: String,
        value: String,
    ) -> Result<(), AppError> {
        self.runtime(&host)?
            .set_chat_setting(id, setting, value)
            .await
    }
    pub async fn retry_history(&self) -> Result<(), AppError> {
        self.selected()?.retry_history().await
    }
    pub async fn chat_attachment(&self, context: String, id: String) -> Result<Vec<u8>, AppError> {
        self.selected()?.chat_attachment(context, id).await
    }
    pub async fn chat_image(&self, context: String, id: String) -> Result<Vec<u8>, AppError> {
        self.selected()?.chat_image(context, id).await
    }
    pub async fn refresh_host_resources(&self, host: String) -> Result<(), AppError> {
        self.runtime(&host)?.refresh_host_resources().await
    }
    pub async fn refresh_skills(&self, id: String) -> Result<(), AppError> {
        self.selected()?.refresh_skills(id).await
    }
    pub async fn select_skill(
        &self,
        host: String,
        id: String,
        path: String,
        selected: bool,
    ) -> Result<(), AppError> {
        self.runtime(&host)?.select_skill(id, path, selected).await
    }
}

#[cfg(test)]
mod tests {
    use super::super::recovery_tests::Fixture;
    use super::*;

    async fn until(mut condition: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(12), async {
            while !condition() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("multi-host condition timed out");
    }

    #[tokio::test]
    async fn hosts_stay_connected_when_switching_and_recover_independently() {
        let first = Fixture::new(false).await;
        let second = Fixture::new(false).await;
        let directory = tempfile::tempdir().unwrap();
        let client = ClientCore::new(first.core.secrets.clone(), first.core.observer.clone());
        let mut other = second.core.host("host").unwrap();
        other.id = "other".into();
        let keys = [&first, &second]
            .into_iter()
            .flat_map(|f| {
                lock(&f.core.store)
                    .as_ref()
                    .unwrap()
                    .data()
                    .trusted_keys
                    .clone()
            })
            .collect();
        client
            .initialize(
                directory.path().into(),
                "owned-multi-host".into(),
                LegacyImport {
                    source: "multi-host".into(),
                    host: Some(first.core.host("host").unwrap()),
                    trusted_keys: keys,
                },
            )
            .await
            .unwrap();
        client.save_host(other.clone()).await.unwrap();
        client.connect("host".into()).await.unwrap();
        until(|| client.snapshot().connection == ConnectionPhase::Connected).await;
        let original = client.runtime("host").unwrap().connection().unwrap();
        client.connect(other.id.clone()).await.unwrap();
        until(|| client.snapshot().connection == ConnectionPhase::Connected).await;
        let background = client.runtime("other").unwrap().connection().unwrap();
        client.select_host("host".into()).await.unwrap();
        client.connect(other.id.clone()).await.unwrap();
        assert!(Arc::ptr_eq(
            &original,
            &client.runtime("host").unwrap().connection().unwrap()
        ));
        assert!(Arc::ptr_eq(
            &background,
            &client.runtime("other").unwrap().connection().unwrap()
        ));
        assert_eq!(
            client
                .snapshot()
                .host_connections
                .iter()
                .filter(|h| h.connection == ConnectionPhase::Connected)
                .count(),
            2
        );

        // A slow third handshake must neither restart on selection nor block established hosts.
        let pending_listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let mut pending = other.clone();
        pending.id = "pending".into();
        pending.port = pending_listener.local_addr().unwrap().port();
        client.save_host(pending).await.unwrap();
        client.connect("pending".into()).await.unwrap();
        let generation = client
            .runtime("pending")
            .unwrap()
            .generation
            .load(Ordering::SeqCst);
        client.connect("pending".into()).await.unwrap();
        assert_eq!(
            client
                .runtime("pending")
                .unwrap()
                .generation
                .load(Ordering::SeqCst),
            generation
        );
        client.disconnect_host("pending".into()).await.unwrap();
        client.select_host("other".into()).await.unwrap();

        original.ssh.close().await.unwrap();
        client.runtime("host").unwrap().network_changed(true);
        until(|| {
            client
                .runtime("host")
                .unwrap()
                .connection()
                .is_ok_and(|c| !Arc::ptr_eq(&original, &c))
                && client.runtime("host").unwrap().snapshot().connection
                    == ConnectionPhase::Connected
        })
        .await;
        assert_eq!(client.snapshot().selected_host.as_deref(), Some("other"));
        assert!(Arc::ptr_eq(
            &background,
            &client.runtime("other").unwrap().connection().unwrap()
        ));

        client.disconnect_host("host".into()).await.unwrap();
        assert_eq!(
            client.runtime("other").unwrap().snapshot().connection,
            ConnectionPhase::Connected
        );
        let persisted: recovery::Connections = lock(&client.base.store)
            .as_ref()
            .unwrap()
            .read_document("connections")
            .unwrap()
            .unwrap();
        assert_eq!(persisted.hosts, BTreeSet::from(["other".into()]));

        // Reopening the client restores every requested connection without taking selection.
        client.select_host("host".into()).await.unwrap();
        drop(client);
        let reopened = ClientCore::new(first.core.secrets.clone(), first.core.observer.clone());
        reopened
            .initialize(
                directory.path().into(),
                "owned-multi-host".into(),
                LegacyImport {
                    source: "multi-host".into(),
                    host: None,
                    trusted_keys: vec![],
                },
            )
            .await
            .unwrap();
        reopened.restore_connection().await.unwrap();
        until(|| {
            reopened.runtime("other").unwrap().snapshot().connection == ConnectionPhase::Connected
        })
        .await;
        assert_eq!(reopened.snapshot().selected_host.as_deref(), Some("host"));
        assert_eq!(
            reopened.snapshot().connection,
            ConnectionPhase::Disconnected
        );
        reopened.disconnect_all().await.unwrap();
        assert!(lock(&reopened.base.connection_intents).is_empty());
        first.stop().await;
        second.stop().await;
    }

    #[tokio::test]
    async fn workspace_selection_sort_and_bound_actions_do_not_leak_between_hosts() {
        let first = Fixture::new(false).await;
        let directory = tempfile::tempdir().unwrap();
        let client = ClientCore::new(first.core.secrets.clone(), first.core.observer.clone());
        let host = first.core.host("host").unwrap();
        client
            .initialize(
                directory.path().into(),
                "owned-multi-host".into(),
                LegacyImport {
                    source: "multi-host".into(),
                    host: Some(host.clone()),
                    trusted_keys: vec![],
                },
            )
            .await
            .unwrap();
        client
            .save_host(Host {
                id: "other".into(),
                ..host
            })
            .await
            .unwrap();
        let records: Vec<_> = ["first", "second"]
            .into_iter()
            .enumerate()
            .map(|(i, id)| {
                kite_agent::sessions::Record::draft(
                    id.into(),
                    "codex".into(),
                    id.into(),
                    None,
                    i as i64,
                )
            })
            .collect();
        for host in ["host", "other"] {
            let runtime = client.runtime(host).unwrap();
            runtime
                .install_catalog(
                    host,
                    &kite_agent::sessions::Catalog {
                        revision: 1,
                        now_ms: 1,
                        sessions: records.clone(),
                    },
                )
                .unwrap();
            runtime.restore_session_cache();
        }
        client
            .open_session("host".into(), "first".into())
            .await
            .unwrap();
        client
            .set_session_sort("host".into(), SessionSort::Oldest)
            .await
            .unwrap();
        client.select_host("other".into()).await.unwrap();
        client
            .open_session("other".into(), "second".into())
            .await
            .unwrap();
        // An action captured before switching keeps the original host, including identical IDs.
        client
            .open_session("host".into(), "first".into())
            .await
            .unwrap();
        assert_eq!(client.snapshot().selected_host.as_deref(), Some("other"));
        assert_eq!(
            client.snapshot().selected_session.as_deref(),
            Some("second")
        );
        assert_eq!(client.snapshot().session_sort, SessionSort::Newest);
        client.select_host("host".into()).await.unwrap();
        assert_eq!(client.snapshot().selected_session.as_deref(), Some("first"));
        assert_eq!(client.snapshot().session_sort, SessionSort::Oldest);
        assert_eq!(client.snapshot().sessions[0].id, "first");
        assert!(
            client
                .open_session("missing-host".into(), "first".into())
                .await
                .is_err()
        );
        drop(client);
        let restored = ClientCore::new(first.core.secrets.clone(), first.core.observer.clone());
        restored
            .initialize(
                directory.path().into(),
                "owned-multi-host".into(),
                LegacyImport {
                    source: "multi-host".into(),
                    host: None,
                    trusted_keys: vec![],
                },
            )
            .await
            .unwrap();
        assert_eq!(
            restored.snapshot().selected_session.as_deref(),
            Some("first")
        );
        assert_eq!(restored.snapshot().session_sort, SessionSort::Oldest);
        restored.select_host("other".into()).await.unwrap();
        assert_eq!(
            restored.snapshot().selected_session.as_deref(),
            Some("second")
        );
        assert_eq!(restored.snapshot().session_sort, SessionSort::Newest);
        first.stop().await;
    }
}
