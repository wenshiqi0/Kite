//! One client, independent host workspaces, and a single selected media surface.
use super::*;
use crate::resources::PLACEMENT_SAMPLE_MAX_AGE;
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

fn recommended_session_host(
    workspaces: &BTreeMap<String, Snapshot>,
    now: Instant,
) -> Option<String> {
    workspaces
        .iter()
        .filter(|(_, state)| {
            state.connection == ConnectionPhase::Connected
                && !state.providers.is_empty()
                && state
                    .resources_observed_at
                    .is_some_and(|at| now.saturating_duration_since(at) <= PLACEMENT_SAMPLE_MAX_AGE)
        })
        .filter_map(|(host, state)| Some((state.host_resources.as_ref()?.placement_score()?, host)))
        .min()
        .map(|(_, host)| host.clone())
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
            // A canceled workspace can still finish an observer call after removal.
            if host.as_ref().is_some_and(|id| self.base.host(id).is_err()) {
                return;
            }
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
            snapshot.workspace_session_sort = state.base.session_sort;
            snapshot.host_connections = state
                .workspaces
                .iter()
                .map(|(id, s)| HostConnection {
                    host_id: id.clone(),
                    machine: s.machine.clone(),
                    connection: s.connection,
                    connection_step: s.connection_step,
                    failure: s.failure.clone(),
                    resources: s.host_resources.clone(),
                    sessions: s.sessions.clone(),
                    selected_session: s.selected_session.clone(),
                    providers: s.providers.clone(),
                    session_sort: s.session_sort,
                })
                .collect();
            snapshot
        };
        if let Some(store) = lock(&self.base.store).as_ref() {
            snapshot.hosts = store.data().hosts.clone();
        }
        super::subagent_history::associate(&mut snapshot.host_connections, &snapshot.hosts);
        snapshot.workspace_session_order =
            sessions::workspace_order(&snapshot.host_connections, snapshot.workspace_session_sort);
        if let Some(workspace) = snapshot
            .host_connections
            .iter()
            .find(|h| Some(&h.host_id) == snapshot.selected_host.as_ref())
        {
            snapshot.sessions.clone_from(&workspace.sessions);
        }
        snapshot
    }

    fn runtime(&self, host: &str) -> Result<Arc<AppCore>, AppError> {
        // History readers can observe the saved host before base initialization
        // publishes its key and readiness. Do not freeze that partial state into
        // a workspace; conversation_since can serve the base snapshot meanwhile.
        if !self.base.snapshot().initialized {
            return Err(AppError::State("core is not initialized".into()));
        }
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
        let sort = lock(&self.base.store)
            .as_ref()
            .and_then(|store| {
                store
                    .read_document::<SessionSort>("workspace-sort")
                    .ok()
                    .flatten()
            })
            .unwrap_or_default();
        self.base.update(|s| s.session_sort = sort);
        for host in self.base.snapshot().hosts {
            self.runtime(&host.id)?;
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
        self.runtime(&id)?;
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

    /// Remove a saved target. Forgetting only its trust/pairing remains a separate command.
    pub async fn remove_host(&self, host: String) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        self.runtime(&host)?.forget_host(host.clone()).await?;
        let selected = {
            let mut store = lock(&self.base.store);
            let store = store
                .as_mut()
                .ok_or_else(|| AppError::State("core is not initialized".into()))?;
            let mut next = store.data().clone();
            next.hosts.retain(|saved| saved.id != host);
            if next.selected_host.as_deref() == Some(&host) {
                next.selected_host = next.hosts.first().map(|saved| saved.id.clone());
            }
            let selected = next.selected_host.clone();
            store.commit(next)?;
            selected
        };
        let removed = lock(&self.runtimes).remove(&host);
        {
            let mut state = lock(&self.state);
            state.workspaces.remove(&host);
            if state.selected.as_deref() == Some(&host) {
                state.selected = selected;
            }
        }
        drop(removed);
        self.base.sync_store();
        self.publish();
        Ok(())
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
    pub async fn open_workspace_session(&self, host: String, id: String) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        let runtime = self.runtime(&host)?;
        runtime.open_session(id).await?;
        self.select_inner(&host).await
    }
    pub fn recommend_session_host(&self) -> Option<String> {
        let state = lock(&self.state);
        recommended_session_host(&state.workspaces, Instant::now())
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
        let _guard = self.operations.lock().await;
        self.select_inner(&host).await?;
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
    pub async fn set_workspace_sort(&self, sort: SessionSort) -> Result<(), AppError> {
        let _guard = self.operations.lock().await;
        if let Some(store) = lock(&self.base.store).as_ref() {
            store.write_document("workspace-sort", &sort)?;
        }
        self.base.update(|s| s.session_sort = sort);
        for host in self.base.snapshot().hosts {
            self.runtime(&host.id)?.set_session_sort(sort).await?;
        }
        Ok(())
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
                &core.notification_job,
            ] {
                if let Some(job) = lock(job).take() {
                    job.abort();
                }
            }
            drop(lock(&core.state).live.drain());
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
    pub async fn workspace_account_usage(&self) -> Result<Vec<HostAccountUsage>, AppError> {
        use HostAccountUsageStatus::{Offline, Ready, Unavailable};

        let snapshot = self.snapshot();
        let online: BTreeSet<_> = snapshot
            .host_connections
            .iter()
            .filter(|host| host.connection == ConnectionPhase::Connected)
            .map(|host| host.host_id.as_str())
            .collect();
        let mut rows = Vec::with_capacity(snapshot.hosts.len());
        let mut targets = Vec::new();
        {
            let runtimes = lock(&self.runtimes);
            for host in snapshot.hosts {
                let connected = online.contains(host.id.as_str());
                if connected && let Some(runtime) = runtimes.get(&host.id) {
                    targets.push((rows.len(), runtime.clone()));
                }
                rows.push(HostAccountUsage {
                    host_id: host.id,
                    host: host.host,
                    status: if connected { Unavailable } else { Offline },
                    providers: Vec::new(),
                });
            }
        }
        dispatch(async move {
            for batch in targets.chunks(4) {
                let mut jobs = tokio::task::JoinSet::new();
                for (index, runtime) in batch {
                    let (index, runtime) = (*index, runtime.clone());
                    jobs.spawn(async move {
                        let result = runtime.read_host_account_usage().await;
                        (index, result)
                    });
                }
                while let Some(result) = jobs.join_next().await {
                    if let Ok((index, Ok(providers))) = result {
                        rows[index].status = Ready;
                        rows[index].providers = providers;
                    }
                }
            }
            Ok(rows)
        })
        .await
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
    pub async fn inspect_subagent(
        &self,
        host: String,
        parent: String,
        child: String,
    ) -> Result<SubagentInspection, AppError> {
        let snapshot = self.snapshot();
        let target = snapshot
            .host_connections
            .iter()
            .find(|workspace| workspace.host_id == host)
            .and_then(|workspace| workspace.sessions.iter().find(|record| record.id == parent))
            .and_then(|record| {
                record
                    .associated_tasks
                    .iter()
                    .find(|task| task.activity.id == child)
            })
            .and_then(|task| task.session.as_ref());
        if let Some(target) = target {
            return self
                .runtime(&target.host_id)?
                .inspect_associated_session(target.host_id.clone(), target.session_id.clone())
                .await;
        }
        self.runtime(&host)?
            .inspect_subagent(host, parent, child)
            .await
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

    #[test]
    fn placement_recommends_a_connected_host_with_a_fresh_sample_and_a_provider() {
        use crate::resources::{GIB, HostResources, MemoryPressure};

        let now = Instant::now();
        let ready = Snapshot {
            connection: ConnectionPhase::Connected,
            resources_observed_at: Some(now),
            host_resources: Some(HostResources {
                available: true,
                sampled_at_ms: 1,
                cpu_percent: Some(10.0),
                cpu_cores: 8,
                memory_total_bytes: 16 * GIB,
                memory_available_bytes: 8 * GIB,
                agent_memory_limit_bytes: 8 * GIB,
                limit_enforced: true,
                memory_pressure: MemoryPressure::Normal,
                can_start_agents: true,
                ..HostResources::default()
            }),
            providers: vec![providers::Provider {
                id: "codex".into(),
                managed_chat: true,
                models: false,
                attachments: false,
                skills: false,
                usage: false,
                pause: false,
                terminal: false,
                release: false,
            }],
            ..Snapshot::default()
        };
        let mut busy = ready.clone();
        busy.host_resources.as_mut().unwrap().cpu_percent = Some(80.0);
        let workspaces = BTreeMap::from([
            ("busy".into(), busy),
            (
                "a-disconnected".into(),
                Snapshot {
                    connection: ConnectionPhase::Disconnected,
                    ..ready.clone()
                },
            ),
            (
                "b-stale".into(),
                Snapshot {
                    resources_observed_at: Some(now - Duration::from_secs(16)),
                    ..ready.clone()
                },
            ),
            (
                "c-unavailable".into(),
                Snapshot {
                    providers: Vec::new(),
                    ..ready.clone()
                },
            ),
            ("d-ready".into(), ready.clone()),
            ("z-ready".into(), ready),
        ]);
        assert_eq!(
            recommended_session_host(&workspaces, now).as_deref(),
            Some("d-ready")
        );
        assert_eq!(
            recommended_session_host(&workspaces, now + Duration::from_secs(16)),
            None
        );
    }

    #[tokio::test]
    async fn removing_saved_hosts_is_distinct_from_forgetting_trust_and_persists() {
        let fixture = Fixture::new(false).await;
        let directory = tempfile::tempdir().unwrap();
        let client = ClientCore::new(fixture.core.secrets.clone(), fixture.core.observer.clone());
        let host = fixture.core.host("host").unwrap();
        client
            .initialize(
                directory.path().into(),
                "host-removal".into(),
                LegacyImport {
                    source: "host-removal".into(),
                    host: Some(host.clone()),
                    trusted_keys: vec![],
                },
            )
            .await
            .unwrap();
        client
            .save_host(Host {
                id: "other".into(),
                host: "other.invalid".into(),
                ..host
            })
            .await
            .unwrap();
        client.select_host("host".into()).await.unwrap();
        client.forget_host("host".into()).await.unwrap();
        assert_eq!(
            client.snapshot().hosts.len(),
            2,
            "forgetting trust preserves the saved target"
        );
        let original = client.runtime("host").unwrap();
        let other = client.runtime("other").unwrap();
        other.set_connection_intent(Some("other".into())).unwrap();
        client.remove_host("host".into()).await.unwrap();
        let snapshot = client.snapshot();
        assert_eq!(snapshot.hosts.len(), 1);
        assert_eq!(snapshot.selected_host.as_deref(), Some("other"));
        assert!(Arc::ptr_eq(&other, &client.runtime("other").unwrap()));
        assert!(lock(&client.base.connection_intents).contains("other"));
        let mut late = original.snapshot();
        late.revision = u64::MAX;
        client.accept(Some("host".into()), late);
        assert!(
            client
                .snapshot()
                .host_connections
                .iter()
                .all(|workspace| workspace.host_id != "host")
        );
        drop(client);
        let reopened = ClientCore::new(fixture.core.secrets.clone(), fixture.core.observer.clone());
        reopened
            .initialize(
                directory.path().into(),
                "host-removal".into(),
                LegacyImport {
                    source: "host-removal".into(),
                    host: None,
                    trusted_keys: vec![],
                },
            )
            .await
            .unwrap();
        assert_eq!(reopened.snapshot().hosts.len(), 1);
        reopened.remove_host("other".into()).await.unwrap();
        assert!(reopened.snapshot().hosts.is_empty());
        assert!(reopened.snapshot().selected_host.is_none());
        assert!(reopened.snapshot().host_connections.is_empty());
        assert!(lock(&reopened.base.connection_intents).is_empty());
        fixture.stop().await;
    }

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
    async fn reading_history_during_initialization_keeps_workspace_ready() {
        struct StartupReader {
            client: Mutex<std::sync::Weak<ClientCore>>,
            read: AtomicBool,
        }
        impl Observer for StartupReader {
            fn changed(&self, snapshot: Snapshot) {
                // Android requests a conversation delta for every published snapshot,
                // including the saved host published before initialization completes.
                if snapshot.selected_host.is_some()
                    && !snapshot.initialized
                    && !self.read.swap(true, Ordering::SeqCst)
                {
                    let client = lock(&self.client).upgrade().unwrap();
                    client.conversation_since("", 0);
                }
            }
        }

        let fixture = Fixture::new(false).await;
        let directory = tempfile::tempdir().unwrap();
        let reader = Arc::new(StartupReader {
            client: Mutex::new(std::sync::Weak::new()),
            read: AtomicBool::new(false),
        });
        let client = ClientCore::new(fixture.core.secrets.clone(), reader.clone());
        *lock(&reader.client) = Arc::downgrade(&client);
        client
            .initialize(
                directory.path().into(),
                "startup-reader".into(),
                LegacyImport {
                    source: "startup-reader".into(),
                    host: Some(fixture.core.host("host").unwrap()),
                    trusted_keys: vec![],
                },
            )
            .await
            .unwrap();

        assert!(reader.read.load(Ordering::SeqCst));
        let snapshot = client.snapshot();
        assert!(snapshot.initialized);
        assert!(snapshot.key.is_some());
        assert_eq!(snapshot.selected_host.as_deref(), Some("host"));
        assert!(client.runtime("host").unwrap().snapshot().initialized);
        fixture.stop().await;
    }

    #[tokio::test]
    async fn sidebar_orders_offline_sessions_across_hosts_and_opens_the_exact_target() {
        use kite_agent::sessions::{
            Catalog, DelegationPolicy, Record, SessionDelegation, SessionParent,
        };

        let fixture = Fixture::new(false).await;
        let directory = tempfile::tempdir().unwrap();
        let client = ClientCore::new(fixture.core.secrets.clone(), fixture.core.observer.clone());
        let host = fixture.core.host("host").unwrap();
        client
            .initialize(
                directory.path().into(),
                "grouped-workspaces".into(),
                LegacyImport {
                    source: "grouped".into(),
                    host: Some(host.clone()),
                    trusted_keys: vec![],
                },
            )
            .await
            .unwrap();
        client
            .save_host(Host {
                id: "mini".into(),
                ..host.clone()
            })
            .await
            .unwrap();
        for (machine, created_ms) in [("host", 10), ("mini", 20)] {
            let mut records = vec![Record::draft(
                "same-session".into(),
                "codex".into(),
                format!("{machine} build"),
                None,
                created_ms,
            )];
            if machine == "host" {
                let mut task = Record::draft(
                    "disk-check".into(),
                    "antigravity".into(),
                    "Disk check".into(),
                    None,
                    30,
                );
                task.delegation = Some(SessionDelegation {
                    parent: Some(SessionParent {
                        machine: "mini".into(),
                        session_id: "same-session".into(),
                        provider: Some("codex".into()),
                    }),
                    label: None,
                    role: None,
                    policy: DelegationPolicy::Strict,
                    worktree_isolated: None,
                    write_lock: None,
                });
                records.push(task);
            }
            let core = client.runtime(machine).unwrap();
            core.install_catalog(
                machine,
                &Catalog {
                    revision: 1,
                    now_ms: 30,
                    sessions: records,
                },
            )
            .unwrap();
            core.restore_session_cache();
        }
        let snapshot = client.snapshot();
        assert_eq!(snapshot.host_connections.len(), 2);
        assert!(
            snapshot
                .host_connections
                .iter()
                .all(|workspace| workspace.connection == ConnectionPhase::Disconnected)
        );
        let oldest_first = [
            ConversationKey::new("host", "same-session"),
            ConversationKey::new("mini", "same-session"),
            ConversationKey::new("host", "disk-check"),
        ];
        assert_eq!(
            snapshot.workspace_session_order,
            oldest_first.iter().rev().cloned().collect::<Vec<_>>()
        );
        client
            .open_workspace_session("mini".into(), "same-session".into())
            .await
            .unwrap();
        assert_eq!(client.snapshot().selected_host.as_deref(), Some("mini"));
        assert_eq!(client.snapshot().sessions[0].title, "mini build");
        let selected = client.snapshot();
        let task = selected.sessions[0].associated_tasks[0]
            .session
            .as_ref()
            .unwrap();
        assert_eq!((&*task.host_id, &*task.session_id), ("host", "disk-check"));
        assert!(!task.connected);
        assert!(selected.host_connections.iter().any(|workspace| {
            workspace.host_id == task.host_id
                && workspace
                    .sessions
                    .iter()
                    .any(|session| session.id == task.session_id && session.delegated_child)
        }));
        assert!(
            client
                .open_workspace_session("host".into(), "missing-session".into())
                .await
                .is_err()
        );
        assert_eq!(client.snapshot().selected_host.as_deref(), Some("mini"));
        client
            .set_workspace_sort(SessionSort::Oldest)
            .await
            .unwrap();
        assert_eq!(client.snapshot().workspace_session_order, oldest_first);
        client
            .save_host(Host {
                id: "new-machine".into(),
                ..host
            })
            .await
            .unwrap();
        assert!(
            client
                .snapshot()
                .host_connections
                .iter()
                .all(|workspace| workspace.session_sort == SessionSort::Oldest)
        );
        drop(client);
        let reopened = ClientCore::new(fixture.core.secrets.clone(), fixture.core.observer.clone());
        reopened
            .initialize(
                directory.path().into(),
                "grouped-workspaces".into(),
                LegacyImport {
                    source: "grouped".into(),
                    host: None,
                    trusted_keys: vec![],
                },
            )
            .await
            .unwrap();
        assert_eq!(reopened.snapshot().host_connections.len(), 3);
        assert_eq!(
            reopened.snapshot().workspace_session_sort,
            SessionSort::Oldest
        );
        assert_eq!(reopened.snapshot().workspace_session_order, oldest_first);
        assert_eq!(reopened.snapshot().selected_host.as_deref(), Some("mini"));
        assert_eq!(
            reopened.snapshot().selected_session.as_deref(),
            Some("same-session")
        );
        assert_eq!(
            reopened
                .snapshot()
                .host_connections
                .iter()
                .filter(|workspace| !workspace.sessions.is_empty())
                .count(),
            2
        );
        fixture.stop().await;
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
        let selected = client.snapshot().selected_host;
        let quotas = client.workspace_account_usage().await.unwrap();
        assert_eq!(quotas.len(), 2);
        assert!(quotas.iter().all(|row| {
            row.status == HostAccountUsageStatus::Ready
                && row.providers[0].usage.plan.as_deref() == Some("fixture-plan")
        }));
        assert_eq!(client.snapshot().selected_host, selected);
        assert!(client.snapshot().selected_session.is_none());

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
