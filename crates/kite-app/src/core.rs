//! Process-owned domain state, command execution and cancellation.
use crate::AppError;
use kite_agent::{
    Approval, Message, Role,
    composer::{self, Client as ChatClient, Composer, Selection, Upload},
    fingerprint, history, process,
};
use kite_herdr::{
    api::{Agent, Api},
    check_api_bridge,
};
use kite_ssh::{Session, SshError, Target};
use kite_store::{Host, PairedServer, Store};
use kite_stream::{
    MediaSink, StreamConfig,
    control::{Client, Identity, new_device_id, new_pin},
    native::{self, NativeSession},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    runtime::Runtime,
    sync::{Mutex as AsyncMutex, watch},
    task::JoinHandle,
};
mod sessions;
use crate::{QuestionAnswer, QuestionPrompt, ResponseActivity};
use sessions::SessionBook;

#[derive(Clone, Debug)]
pub struct ConversationSession {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub preview: String,
    pub cwd: Option<String>,
    pub archived: bool,
    pub updated_ms: i64,
    pub agent_id: Option<String>,
    pub status: String,
    pub recorded: bool,
}

pub trait SecretStorage: Send + Sync {
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String>;
    fn write(&self, name: &str, value: &[u8]) -> Result<(), String>;
}
pub trait Observer: Send + Sync {
    fn changed(&self, snapshot: Snapshot);
}

#[derive(Clone, Debug)]
pub struct DeviceKey {
    pub public_key: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug)]
pub struct HostKeyPrompt {
    pub host_id: String,
    pub address: String,
    pub fingerprint: String,
    pub public_key: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionPhase {
    Disconnected,
    Connecting,
    AwaitingTrust,
    Connected,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionStep {
    Idle,
    Ssh,
    HerdrDiscovery,
    HerdrApi,
    Agent,
    Ready,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopPhase {
    Idle,
    Pairing,
    Starting,
    Streaming,
    Stopping,
    Failed,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentPause {
    Running,
    Paused,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureCode {
    Connection,
    Authentication,
    HostKeyChanged,
    HerdrMissing,
    HerdrUnsupported,
    HerdrApi,
    Storage,
    InvalidKey,
    InvalidConfiguration,
    Stream,
    AgentStart,
}
#[derive(Clone, Debug)]
pub struct Failure {
    pub code: FailureCode,
    pub details: String,
    pub username: String,
    pub trusted: String,
    pub presented: String,
}
#[derive(Clone, Debug)]
pub struct Desktop {
    pub phase: DesktopPhase,
    pub pin: Option<String>,
    pub pairing_name: Option<String>,
    pub stage: i32,
    pub received: u64,
    pub rendered: u64,
    pub takeover: bool,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub fps: u32,
}
impl Default for Desktop {
    fn default() -> Self {
        Self {
            phase: DesktopPhase::Idle,
            pin: None,
            pairing_name: None,
            stage: 0,
            received: 0,
            rendered: 0,
            takeover: false,
            width: 0,
            height: 0,
            source_width: 0,
            source_height: 0,
            fps: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub revision: u64,
    pub initialized: bool,
    pub key: Option<DeviceKey>,
    pub hosts: Vec<Host>,
    pub selected_host: Option<String>,
    pub connection: ConnectionPhase,
    pub connection_step: ConnectionStep,
    pub pending_key: Option<HostKeyPrompt>,
    pub herdr_version: Option<String>,
    pub herdr_path: Option<String>,
    pub capability: Option<String>,
    pub host_key_fingerprint: Option<String>,
    pub agents: Vec<Agent>,
    pub selected_agent: Option<String>,
    pub sessions: Vec<ConversationSession>,
    pub selected_session: Option<String>,
    pub agent_output: String,
    pub conversation: Vec<Message>,
    pub conversation_available: bool,
    pub approval: Option<Approval>,
    pub question: Option<QuestionPrompt>,
    pub response_activity: ResponseActivity,
    pub agent_pause: AgentPause,
    pub composer: Composer,
    pub desktop: Desktop,
    pub failure: Option<Failure>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            initialized: false,
            key: None,
            hosts: Vec::new(),
            selected_host: None,
            connection: ConnectionPhase::Disconnected,
            connection_step: ConnectionStep::Idle,
            pending_key: None,
            herdr_version: None,
            herdr_path: None,
            capability: None,
            host_key_fingerprint: None,
            agents: Vec::new(),
            selected_agent: None,
            sessions: Vec::new(),
            selected_session: None,
            agent_output: String::new(),
            conversation: Vec::new(),
            conversation_available: false,
            approval: None,
            question: None,
            response_activity: ResponseActivity::Idle,
            agent_pause: AgentPause::Running,
            composer: Composer::default(),
            desktop: Desktop::default(),
            failure: None,
        }
    }
}

pub struct LegacyImport {
    pub source: String,
    pub host: Option<Host>,
    pub trusted_keys: Vec<(String, String)>,
}
struct Connected {
    host: Host,
    ssh: Arc<Session>,
    api: Arc<Api>,
    version: String,
    path: String,
    capability: String,
}
struct DesktopJob {
    cancel: watch::Sender<bool>,
    task: JoinHandle<()>,
}
struct PendingPair {
    client: Client,
    pin: String,
    generation: u64,
}
struct PendingAgentStart {
    pane: String,
    workspace: String,
    since: Instant,
}
struct PendingMessage {
    key: String,
    message: Message,
    previous: BTreeSet<String>,
}

pub struct AppCore {
    secrets: Arc<dyn SecretStorage>,
    observer: Arc<dyn Observer>,
    state: Mutex<Snapshot>,
    store: Mutex<Option<Store>>,
    operations: AsyncMutex<()>,
    connected: Mutex<Option<Arc<Connected>>>,
    connect_job: Mutex<Option<JoinHandle<()>>>,
    poll_job: Mutex<Option<JoinHandle<()>>>,
    desktop_job: AsyncMutex<Option<DesktopJob>>,
    pending_pair: Mutex<Option<PendingPair>>,
    media: Mutex<Option<Arc<dyn MediaSink>>>,
    generation: AtomicU64,
    desktop_generation: AtomicU64,
    history: Mutex<Option<history::History>>,
    pending_messages: Mutex<Vec<PendingMessage>>,
    paused_processes: Mutex<BTreeMap<String, process::Token>>,
    consumed_approvals: Mutex<BTreeSet<String>>,
    agent_starts: Mutex<BTreeMap<String, PendingAgentStart>>,
    chat_clients: AsyncMutex<BTreeMap<String, Arc<ChatClient>>>,
    chat_failures: Mutex<BTreeSet<String>>,
    uploads: Mutex<BTreeMap<String, Vec<Upload>>>,
    session_book: Mutex<SessionBook>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
async fn dispatch<T: Send + 'static>(
    future: impl Future<Output = Result<T, AppError>> + Send + 'static,
) -> Result<T, AppError> {
    static RUNTIME: OnceLock<Result<Runtime, std::io::Error>> = OnceLock::new();
    let runtime = RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4)
                .thread_name("kite-core")
                .enable_all()
                .build()
        })
        .as_ref()
        .map_err(|_| AppError::State("runtime initialization failed".into()))?;
    runtime
        .spawn(future)
        .await
        .map_err(|_| AppError::State("core command interrupted".into()))?
}

impl AppCore {
    pub fn new(secrets: Arc<dyn SecretStorage>, observer: Arc<dyn Observer>) -> Arc<Self> {
        Arc::new(Self {
            secrets,
            observer,
            state: Mutex::new(Snapshot::default()),
            store: Mutex::new(None),
            operations: AsyncMutex::new(()),
            connected: Mutex::new(None),
            connect_job: Mutex::new(None),
            poll_job: Mutex::new(None),
            desktop_job: AsyncMutex::new(None),
            pending_pair: Mutex::new(None),
            media: Mutex::new(None),
            generation: AtomicU64::new(0),
            desktop_generation: AtomicU64::new(0),
            history: Mutex::new(None),
            pending_messages: Mutex::new(Vec::new()),
            paused_processes: Mutex::new(BTreeMap::new()),
            consumed_approvals: Mutex::new(BTreeSet::new()),
            agent_starts: Mutex::new(BTreeMap::new()),
            chat_clients: AsyncMutex::new(BTreeMap::new()),
            chat_failures: Mutex::new(BTreeSet::new()),
            uploads: Mutex::new(BTreeMap::new()),
            session_book: Mutex::new(SessionBook::default()),
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        lock(&self.state).clone()
    }
    fn update(&self, f: impl FnOnce(&mut Snapshot)) {
        let snapshot = {
            let mut s = lock(&self.state);
            f(&mut s);
            s.revision = s.revision.saturating_add(1);
            s.clone()
        };
        self.observer.changed(snapshot);
    }
    fn report(&self, error: &AppError) {
        self.update(|s| s.failure = Some(describe(error)));
    }
    fn sync_store(&self) {
        let data = lock(&self.store).as_ref().map(|s| s.data().clone());
        if let Some(d) = data {
            self.update(|s| {
                s.hosts = d.hosts;
                s.selected_host = d.selected_host;
            });
        }
    }
    fn host(&self, id: &str) -> Result<Host, AppError> {
        lock(&self.store)
            .as_ref()
            .and_then(|s| s.data().hosts.iter().find(|h| h.id == id).cloned())
            .ok_or_else(|| AppError::State("unknown host".into()))
    }
    fn connection(&self) -> Result<Arc<Connected>, AppError> {
        lock(&self.connected)
            .as_ref()
            .filter(|c| !c.ssh.is_closed())
            .cloned()
            .ok_or_else(|| AppError::State("not connected".into()))
    }

    pub async fn initialize(
        self: &Arc<Self>,
        directory: PathBuf,
        device_name: String,
        legacy: LegacyImport,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            if core.snapshot().initialized {
                return Ok(());
            }
            let result = (|| {
                let mut store = Store::open(&directory)?;
                *lock(&core.paused_processes) =
                    store.read_document("paused-processes")?.unwrap_or_default();
                *lock(&core.session_book) = store.read_document("sessions")?.unwrap_or_default();
                if !store.data().completed_imports.contains(&legacy.source) {
                    for (_, key) in &legacy.trusted_keys {
                        kite_ssh::fingerprint_of(key)?;
                    }
                }
                store.import_legacy(&legacy.source, legacy.host, &legacy.trusted_keys)?;
                let key = match core.secrets.read("ssh-ed25519").map_err(AppError::State)? {
                    Some(bytes) => {
                        kite_ssh::describe_private_key(std::str::from_utf8(&bytes).map_err(
                            |_| AppError::State("invalid stored SSH key encoding".into()),
                        )?)?
                    }
                    None => {
                        let key = kite_ssh::generate_ed25519(&device_name)?;
                        core.secrets
                            .write("ssh-ed25519", key.private_key_openssh.as_bytes())
                            .map_err(AppError::State)?;
                        key
                    }
                };
                if store.data().device_id.is_empty() {
                    let mut next = store.data().clone();
                    next.device_id = new_device_id()?;
                    store.commit(next)?;
                }
                *lock(&core.store) = Some(store);
                core.sync_store();
                core.restore_session_cache();
                core.update(|s| {
                    s.initialized = true;
                    s.key = Some(DeviceKey {
                        public_key: key.public_key_openssh,
                        fingerprint: key.fingerprint,
                    });
                    s.failure = None;
                });
                Ok(())
            })();
            if let Err(e) = &result {
                core.report(e);
            }
            result
        })
        .await
    }

    pub async fn save_host(self: &Arc<Self>, mut host: Host) -> Result<String, AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            host.host = host.host.trim().into();
            host.username = host.username.trim().into();
            host.herdr_path = host
                .herdr_path
                .map(|p| p.trim().to_owned())
                .filter(|p| !p.is_empty());
            host.stream_host = host
                .stream_host
                .map(|p| p.trim().to_owned())
                .filter(|p| !p.is_empty());
            if host.host.is_empty()
                || host.username.is_empty()
                || host.port == 0
                || !(6..=65514).contains(&host.sunshine_port)
                || host.host.contains(['\r', '\n', '\0'])
            {
                return Err(AppError::State("invalid host configuration".into()));
            }
            if host.id.is_empty() {
                host.id = format!("host-{}", new_device_id()?);
            }
            let id = host.id.clone();
            {
                let mut store = lock(&core.store);
                let store = store
                    .as_mut()
                    .ok_or_else(|| AppError::State("core is not initialized".into()))?;
                let mut next = store.data().clone();
                if let Some(old) = next.hosts.iter_mut().find(|h| h.id == id) {
                    *old = host;
                } else {
                    next.hosts.push(host);
                }
                next.selected_host = Some(id.clone());
                store.commit(next)?;
            }
            core.sync_store();
            Ok(id)
        })
        .await
    }

    pub async fn connect(self: &Arc<Self>, host_id: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let host = core.host(&host_id)?;
            core.generation.fetch_add(1, Ordering::SeqCst);
            core.update(|s| {
                s.connection = ConnectionPhase::Connecting;
                s.connection_step = ConnectionStep::Ssh;
                s.selected_host = Some(host.id.clone());
                s.failure = None;
                s.pending_key = None;
            });
            core.disconnect_inner(false).await?;
            let generation = core.generation.fetch_add(1, Ordering::SeqCst) + 1;
            core.update(|s| {
                s.connection = ConnectionPhase::Connecting;
                s.connection_step = ConnectionStep::Ssh;
                s.selected_host = Some(host.id.clone());
                s.failure = None;
                s.pending_key = None;
            });
            let task_core = core.clone();
            let task = tokio::spawn(async move {
                task_core.connect_flow(host, generation).await;
            });
            *lock(&core.connect_job) = Some(task);
            Ok(())
        })
        .await
    }

    async fn connect_flow(self: Arc<Self>, host: Host, generation: u64) {
        let result = self.open_connection(&host, generation).await;
        if self.generation.load(Ordering::SeqCst) != generation {
            if let Ok(c) = result {
                let _ = c.ssh.close().await;
            }
            return;
        }
        match result {
            Ok(connection) => {
                let connection = Arc::new(connection);
                *lock(&self.connected) = Some(connection.clone());
                self.update(|s| {
                    s.connection_step = ConnectionStep::Agent;
                    s.pending_key = None;
                    s.failure = None;
                    s.herdr_version = Some(connection.version.clone());
                    s.herdr_path = Some(connection.path.clone());
                    s.capability = Some(connection.capability.clone());
                    s.host_key_fingerprint = Some(connection.ssh.host_key_fingerprint());
                });
                let prepared = self.prepare_agent(&connection).await;
                let preferred = prepared
                    .as_ref()
                    .ok()
                    .and_then(|agents| self.preferred_agent(&connection.host.id, agents));
                if self.generation.load(Ordering::SeqCst) != generation {
                    return;
                }
                self.update(|s| {
                    s.connection = ConnectionPhase::Connected;
                    if let Ok(agents) = &prepared {
                        s.agents = agents.clone();
                        s.selected_agent = preferred.clone();
                        s.agent_pause = AgentPause::Running;
                        s.connection_step = if agents.first().is_some_and(|a| a.launch_pending) {
                            ConnectionStep::Agent
                        } else {
                            ConnectionStep::Ready
                        };
                    }
                });
                if let Err(e) = prepared {
                    self.update(|s| {
                        s.failure = Some(Failure {
                            code: FailureCode::AgentStart,
                            details: e.to_string(),
                            username: connection.host.username.clone(),
                            trusted: String::new(),
                            presented: String::new(),
                        })
                    });
                }
                let core = self.clone();
                let task = tokio::spawn(async move {
                    core.poll_connection(connection, generation).await;
                });
                *lock(&self.poll_job) = Some(task);
            }
            Err(AppError::Ssh(SshError::UnknownHostKey {
                fingerprint,
                public_key,
            })) => self.update(|s| {
                s.connection = ConnectionPhase::AwaitingTrust;
                s.pending_key = Some(HostKeyPrompt {
                    host_id: host.id.clone(),
                    address: host.address(),
                    fingerprint,
                    public_key,
                });
            }),
            Err(error) => {
                self.report(&error);
                self.update(|s| s.connection = ConnectionPhase::Failed);
            }
        }
    }
    async fn open_connection(&self, host: &Host, generation: u64) -> Result<Connected, AppError> {
        let bytes = self
            .secrets
            .read("ssh-ed25519")
            .map_err(AppError::State)?
            .ok_or_else(|| AppError::State("device key is unavailable".into()))?;
        let private = std::str::from_utf8(&bytes)
            .map_err(|_| AppError::State("invalid device key encoding".into()))?;
        let trusted = lock(&self.store)
            .as_ref()
            .and_then(|s| s.data().trusted_keys.get(&host.address()).cloned());
        let ssh = Arc::new(
            Session::connect(
                &Target {
                    host: host.host.clone(),
                    port: host.port,
                    username: host.username.clone(),
                },
                private,
                trusted.as_deref(),
            )
            .await?,
        );
        if self.generation.load(Ordering::SeqCst) == generation {
            self.update(|s| s.connection_step = ConnectionStep::HerdrDiscovery);
        }
        let checked = check_api_bridge(&ssh, host.herdr_path.as_deref()).await?;
        if self.generation.load(Ordering::SeqCst) == generation {
            self.update(|s| s.connection_step = ConnectionStep::HerdrApi);
        }
        let api = Arc::new(Api::new(ssh.clone(), &checked.herdr_path));
        let version = api.ping().await?;
        // Desktop and SSH remain usable if the richer chat backend needs setup.
        let _ = composer::install(&ssh).await;
        Ok(Connected {
            host: host.clone(),
            ssh,
            api,
            version,
            path: checked.herdr_path,
            capability: checked.capability,
        })
    }
    async fn prepare_agent(&self, c: &Connected) -> Result<Vec<Agent>, AppError> {
        let mut agents = c.api.agents().await?;
        let repository = kite_agent::sessions::Repository::new(&c.ssh);
        let catalog = repository.observe(&agents).await?;
        self.install_catalog(&c.host.id, &catalog)?;
        if agents.is_empty() {
            let agent = if catalog.sessions.is_empty() {
                let id = format!("{}{}", new_device_id()?, new_device_id()?);
                let catalog = repository
                    .create(id.clone(), "codex".into(), String::new(), None)
                    .await?;
                self.install_catalog(&c.host.id, &catalog)?;
                Some(repository.resume(&id, &c.path).await?)
            } else {
                self.prepare_session_agent(c, &agents).await?
            };
            if let Some(agent) = agent {
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
                agents = c.api.agents().await?;
                if !agents.iter().any(|a| a.terminal_id == agent.terminal_id) {
                    agents.push(agent);
                }
            }
        }
        agents.sort_by_key(|a| {
            (
                a.name.as_deref() != Some("kite"),
                a.agent_status != "blocked",
            )
        });
        Ok(agents)
    }

    pub async fn ensure_agent(self: &Arc<Self>) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            core.update(|s| {
                s.connection_step = ConnectionStep::Agent;
                s.failure = None;
            });
            match core.prepare_agent(&c).await {
                Ok(agents) => {
                    let preferred = core.preferred_agent(&c.host.id, &agents);
                    core.update(|s| {
                        s.selected_agent = preferred;
                        s.agents = agents;
                        s.agent_pause = AgentPause::Running;
                        s.connection_step = if s.agents.first().is_some_and(|a| a.launch_pending) {
                            ConnectionStep::Agent
                        } else {
                            ConnectionStep::Ready
                        };
                    });
                }
                Err(error) => {
                    core.update(|s| {
                        s.failure = Some(Failure {
                            code: FailureCode::AgentStart,
                            details: error.to_string(),
                            username: c.host.username.clone(),
                            trusted: String::new(),
                            presented: String::new(),
                        })
                    });
                    return Err(error);
                }
            }
            Ok(())
        })
        .await
    }

    async fn poll_connection(self: Arc<Self>, connection: Arc<Connected>, generation: u64) {
        loop {
            if self.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            if connection.ssh.is_closed() {
                self.report(&AppError::State("SSH connection closed".into()));
                self.update(|s| s.connection = ConnectionPhase::Failed);
                if let Some(job) = self.desktop_job.lock().await.as_ref() {
                    let _ = job.cancel.send(true);
                }
                return;
            }
            match connection.api.agents().await {
                Ok(mut agents) => {
                    let mut failed_starts = Vec::new();
                    {
                        let mut starts = lock(&self.agent_starts);
                        starts.retain(|id, pending| {
                            if let Some(agent) = agents.iter().find(|a| &a.terminal_id == id) {
                                if agent.interactive_ready {
                                    return false;
                                }
                                if agent.launch_pending
                                    || pending.since.elapsed() < Duration::from_secs(35)
                                {
                                    return true;
                                }
                            }
                            if pending.since.elapsed() < Duration::from_secs(5) {
                                return true;
                            }
                            failed_starts.push((pending.workspace.clone(), pending.pane.clone()));
                            false
                        });
                    }
                    for (workspace, pane) in failed_starts {
                        let output = connection.api.read_pane(&pane).await.unwrap_or_default();
                        if self.generation.load(Ordering::SeqCst) == generation {
                            self.update(|s| s.failure = Some(Failure {
                                code: FailureCode::AgentStart,
                                details: format!("Agent did not become ready in {workspace} ({pane}). Check installation and sign-in in this terminal.\n{output}"),
                                username: connection.host.username.clone(), trusted: String::new(), presented: String::new(),
                            }));
                        }
                    }
                    agents.sort_by_key(|a| {
                        (
                            a.name.as_deref() != Some("kite"),
                            a.agent_status != "blocked",
                        )
                    });
                    if self.generation.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    if self.snapshot().connection_step == ConnectionStep::Agent
                        && agents.iter().any(|a| !a.launch_pending)
                    {
                        self.update(|s| s.connection_step = ConnectionStep::Ready);
                    }
                    if self.snapshot().agents != agents {
                        let selection_guard = self.operations.try_lock().ok();
                        let preferred = self.preferred_agent(&connection.host.id, &agents);
                        self.update(|s| {
                            if selection_guard.is_some()
                                && !agents
                                    .iter()
                                    .any(|a| Some(&a.terminal_id) == s.selected_agent.as_ref())
                            {
                                s.selected_agent = preferred;
                                s.conversation.clear();
                                s.agent_output.clear();
                                s.approval = None;
                                s.conversation_available = false;
                                s.agent_pause = AgentPause::Unknown;
                            }
                            s.agents = agents;
                        });
                    }
                }
                Err(e) => {
                    if self.generation.load(Ordering::SeqCst) == generation {
                        self.report(&e.into());
                    }
                }
            }
            let agents = self.snapshot().agents;
            if let Err(error) = self.sync_sessions(&connection, &agents, generation).await {
                self.report(&error);
            }
            if let Some(selected) = self.snapshot().selected_agent {
                self.refresh_agent(&connection, &selected, generation).await;
                if let Err(error) = self.record_selected_history(&connection).await {
                    self.report(&error);
                }
            } else if let Err(error) = self.refresh_saved_session(&connection, generation).await {
                self.report(&error);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn refresh_agent(&self, c: &Connected, selected: &str, generation: u64) {
        let Some(agent) = self
            .snapshot()
            .agents
            .iter()
            .find(|a| a.terminal_id == selected)
            .cloned()
        else {
            return;
        };
        if agent.agent.as_deref() == Some("codex")
            && agent.agent_session.is_none()
            && !agent.launch_pending
        {
            let _ = c
                .ssh
                .exec(
                    &composer::helper_command(&format!(
                        "bind --pane {} --herdr {}",
                        composer::quote(&agent.pane_id),
                        composer::quote(&c.path)
                    )),
                    &[],
                )
                .await;
        }
        self.refresh_composer(c, &agent, generation).await;
        let client = self
            .chat_clients
            .lock()
            .await
            .get(&chat_key(c, &agent))
            .cloned();
        let runtime = client.as_ref().map(|c| c.runtime());
        let question = runtime.as_ref().and_then(|r| r.question());
        let activity = runtime.as_ref().map(|r| r.phase()).unwrap_or_else(|| {
            if agent.agent_status == "working" {
                ResponseActivity::Thinking
            } else if agent.agent_status == "blocked" {
                ResponseActivity::Waiting
            } else {
                ResponseActivity::Idle
            }
        });
        let Ok(text) = c.api.read_agent(&agent.pane_id).await else {
            return;
        };
        let cached = lock(&self.history).clone();
        let history = if let Some(reference) = &agent.agent_session {
            history::read(&c.ssh, reference, cached).await
        } else {
            history::History::default()
        };
        let provider = agent
            .agent_session
            .as_ref()
            .map(|s| s.agent.as_str())
            .or(agent.agent.as_deref())
            .unwrap_or_default();
        let mut approval =
            kite_agent::terminal_choices(agent.agent_status == "blocked", provider, &text);
        if let Some(a) = &mut approval {
            a.id = format!("{}-{}-{}", source_key(&agent), agent.state_change_seq, a.id);
        }
        if let Some(rpc) = runtime.as_ref().and_then(|r| r.approval()) {
            approval = Some(rpc);
        }
        if question.is_some() {
            approval = None;
        }
        let base = format!("{}/{}", c.host.id, selected);
        if approval
            .as_ref()
            .is_some_and(|a| lock(&self.consumed_approvals).contains(&format!("{base}/{}", a.id)))
        {
            approval = None;
        }
        // Reconciliation must not remove a recovery token during a pause/resume.
        let (pause, observed_token) = if let Ok(_guard) = self.operations.try_lock() {
            let token = lock(&self.paused_processes).get(&base).cloned();
            let pause = if let Some(token) = &token {
                match process::paused(&c.ssh, token).await {
                    Ok(true) => AgentPause::Paused,
                    Ok(false) | Err(process::Error::Identity) => {
                        lock(&self.paused_processes).remove(&base);
                        if let Err(e) = self.persist_pauses() {
                            self.report(&e);
                        }
                        AgentPause::Running
                    }
                    Err(_) => AgentPause::Unknown,
                }
            } else {
                AgentPause::Running
            };
            let current = lock(&self.paused_processes).get(&base).cloned();
            (pause, current)
        } else {
            (
                self.snapshot().agent_pause,
                lock(&self.paused_processes).get(&base).cloned(),
            )
        };
        let key = format!("{base}/{}", source_key(&agent));
        let mut messages = history.messages.clone();
        {
            let mut pending = lock(&self.pending_messages);
            let mut matched = BTreeSet::new();
            pending.retain(|p| {
                if p.key != key {
                    return true;
                }
                if let Some(found) = messages.iter().find(|m| {
                    m.role == Role::User
                        && m.text == p.message.text
                        && !p.previous.contains(&m.id)
                        && !matched.contains(&m.id)
                }) {
                    matched.insert(found.id.clone());
                    false
                } else {
                    true
                }
            });
            messages.extend(
                pending
                    .iter()
                    .filter(|p| p.key == key)
                    .map(|p| p.message.clone()),
            );
        }
        if self.generation.load(Ordering::SeqCst) != generation
            || self.snapshot().selected_agent.as_deref() != Some(selected)
            || self
                .snapshot()
                .agents
                .iter()
                .find(|a| a.terminal_id == selected)
                .is_none_or(|a| source_key(a) != source_key(&agent))
        {
            return;
        }
        let available = history.available;
        *lock(&self.history) = Some(history);
        self.update(|s| {
            if s.selected_agent.as_deref() == Some(selected) {
                s.agent_output = text;
                s.conversation = messages;
                s.conversation_available = available;
                s.approval = approval;
                s.question = question;
                s.response_activity = activity;
                if lock(&self.paused_processes).get(&base) == observed_token.as_ref() {
                    s.agent_pause = pause;
                }
            }
        });
    }

    async fn refresh_composer(&self, c: &Connected, agent: &Agent, generation: u64) {
        let Some(reference) = agent
            .agent_session
            .as_ref()
            .filter(|r| r.agent == "codex" && r.kind == "id")
        else {
            return;
        };
        let key = chat_key(c, agent);
        let mut clients = self.chat_clients.lock().await;
        if clients.get(&key).is_some_and(|c| c.is_closed()) {
            clients.remove(&key);
        }
        if !clients.contains_key(&key) && !lock(&self.chat_failures).contains(&key) {
            self.update(|s| {
                if s.selected_agent.as_deref() == Some(&agent.terminal_id) {
                    s.composer.loading = true;
                }
            });
            match ChatClient::connect(&c.ssh, &agent.pane_id, &reference.value).await {
                Ok(client) => {
                    clients.insert(key.clone(), Arc::new(client));
                }
                Err(error) => {
                    lock(&self.chat_failures).insert(key.clone());
                    if self.generation.load(Ordering::SeqCst) == generation {
                        self.update(|s| {
                            if s.selected_agent.as_deref() == Some(&agent.terminal_id) {
                                s.composer = Composer {
                                    details: error.to_string(),
                                    ..Composer::default()
                                };
                            }
                        });
                    }
                    return;
                }
            }
        }
        let Some(client) = clients.get(&key).cloned() else {
            return;
        };
        drop(clients);
        let _ = client.observe_thread().await;
        let stored = lock(&self.store)
            .as_ref()
            .and_then(|s| {
                s.read_document::<BTreeMap<String, Selection>>("chat-settings")
                    .ok()
                    .flatten()
            })
            .and_then(|m| m.get(&key).cloned());
        let selected =
            composer::selection(&client.models, stored.as_ref().unwrap_or(&client.initial))
                .unwrap_or_else(|_| client.initial.clone());
        let uploads = lock(&self.uploads).get(&key).cloned().unwrap_or_default();
        if self.generation.load(Ordering::SeqCst) == generation {
            self.update(|s| {
                if s.selected_agent.as_deref() == Some(&agent.terminal_id) {
                    s.composer = Composer {
                        ready: true,
                        models: client.models.clone(),
                        selection: selected,
                        uploads,
                        ..Composer::default()
                    };
                }
            });
        }
    }

    pub async fn reload_composer(self: &Arc<Self>) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let c = core.connection()?;
            lock(&core.chat_failures).clear();
            if let Some(id) = core.snapshot().selected_agent {
                core.refresh_agent(&c, &id, core.generation.load(Ordering::SeqCst))
                    .await;
            }
            Ok(())
        })
        .await
    }

    pub async fn set_chat_model(
        self: &Arc<Self>,
        id: String,
        model: String,
        effort: String,
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
            let key = chat_key(&c, &agent);
            let client = core
                .chat_clients
                .lock()
                .await
                .get(&key)
                .cloned()
                .ok_or_else(|| AppError::State("chat backend unavailable".into()))?;
            let candidate = client
                .models
                .iter()
                .find(|m| m.model == model)
                .ok_or_else(|| AppError::State("model unavailable".into()))?;
            if !candidate
                .supported_reasoning_efforts
                .iter()
                .any(|e| e.reasoning_effort == effort)
            {
                return Err(AppError::State("reasoning effort unavailable".into()));
            }
            let selected = Selection { model, effort };
            {
                let store = lock(&core.store);
                let store = store
                    .as_ref()
                    .ok_or_else(|| AppError::State("store unavailable".into()))?;
                let mut settings: BTreeMap<String, Selection> =
                    store.read_document("chat-settings")?.unwrap_or_default();
                settings.insert(key, selected.clone());
                store.write_document("chat-settings", &settings)?;
            }
            core.update(|s| {
                if s.selected_agent.as_deref() == Some(&id) {
                    s.composer.selection = selected;
                }
            });
            Ok(())
        })
        .await
    }

    pub async fn upload_attachment(
        self: &Arc<Self>,
        id: String,
        local_path: String,
        name: String,
        mime: String,
        retry_id: Option<String>,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let c = core.connection()?;
            let generation = core.generation.load(Ordering::SeqCst);
            let agent = core
                .snapshot()
                .agents
                .into_iter()
                .find(|a| a.terminal_id == id)
                .ok_or_else(|| AppError::State("agent unavailable".into()))?;
            let key = chat_key(&c, &agent);
            if !core.chat_clients.lock().await.contains_key(&key) {
                return Err(AppError::State("chat backend unavailable".into()));
            }
            let upload_id = format!("{}{}", new_device_id()?, new_device_id()?);
            let size = tokio::fs::metadata(&local_path)
                .await
                .map_err(|e| AppError::State(e.to_string()))?
                .len();
            if size == 0 || size > composer::MAX_FILE {
                return Err(AppError::State(
                    "attachment must be 1 byte to 25 MiB".into(),
                ));
            }
            {
                let mut all = lock(&core.uploads);
                let files = all.entry(key.clone()).or_default();
                if let Some(retry) = retry_id {
                    let position = files
                        .iter()
                        .position(|u| u.attachment.id == retry && u.failed)
                        .ok_or_else(|| AppError::State("upload retry expired".into()))?;
                    files.remove(position);
                }
                if files.len() >= 5 {
                    return Err(AppError::State(
                        "maximum five attachments per message".into(),
                    ));
                }
                files.push(Upload {
                    attachment: composer::Attachment {
                        id: upload_id.clone(),
                        name: name.clone(),
                        mime: mime.clone(),
                        size,
                        path: String::new(),
                    },
                    sent: 0,
                    failed: false,
                    local_path: local_path.clone(),
                });
            }
            core.publish_uploads(&id, &key);
            let last = std::sync::Mutex::new(Instant::now());
            let result = composer::upload(&c.ssh, &local_path, &upload_id, &name, &mime, |sent| {
                if sent != size && lock(&last).elapsed() < Duration::from_millis(200) {
                    return;
                }
                *lock(&last) = Instant::now();
                if let Some(upload) = lock(&core.uploads)
                    .get_mut(&key)
                    .and_then(|u| u.iter_mut().find(|u| u.attachment.id == upload_id))
                {
                    upload.sent = sent;
                }
                core.publish_uploads(&id, &key);
            })
            .await;
            if core.generation.load(Ordering::SeqCst) != generation {
                if let Some(upload) = lock(&core.uploads)
                    .get_mut(&key)
                    .and_then(|u| u.iter_mut().find(|u| u.attachment.id == upload_id))
                {
                    upload.failed = true;
                }
                return Err(AppError::State("host changed during upload".into()));
            }
            if let Some(upload) = lock(&core.uploads)
                .get_mut(&key)
                .and_then(|u| u.iter_mut().find(|u| u.attachment.id == upload_id))
            {
                match &result {
                    Ok(file) => {
                        upload.attachment = file.clone();
                        upload.sent = file.size;
                    }
                    Err(_) => {
                        upload.failed = true;
                    }
                }
            }
            core.publish_uploads(&id, &key);
            result.map(|_| ()).map_err(Into::into)
        })
        .await
    }
    fn publish_uploads(&self, id: &str, key: &str) {
        let uploads = lock(&self.uploads).get(key).cloned().unwrap_or_default();
        self.update(|s| {
            if s.selected_agent.as_deref() == Some(id)
                && s.selected_host
                    .as_ref()
                    .zip(s.agents.iter().find(|a| a.terminal_id == id))
                    .is_some_and(|(h, a)| format!("{h}/{}", source_key(a)) == key)
            {
                s.composer.uploads = uploads;
            }
        });
    }
    pub async fn remove_attachment(
        self: &Arc<Self>,
        id: String,
        upload_id: String,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let c = core.connection()?;
            let agent = core
                .snapshot()
                .agents
                .into_iter()
                .find(|a| a.terminal_id == id)
                .ok_or_else(|| AppError::State("agent unavailable".into()))?;
            let key = chat_key(&c, &agent);
            if let Some(files) = lock(&core.uploads).get_mut(&key) {
                files.retain(|u| u.attachment.id != upload_id);
            }
            core.publish_uploads(&id, &key);
            Ok(())
        })
        .await
    }

    pub async fn trust_host(
        self: &Arc<Self>,
        host_id: String,
        fingerprint: String,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let pending = core
                .snapshot()
                .pending_key
                .ok_or_else(|| AppError::State("host confirmation expired".into()))?;
            if pending.host_id != host_id || pending.fingerprint != fingerprint {
                return Err(AppError::State("host confirmation changed".into()));
            }
            {
                let mut store = lock(&core.store);
                let store = store
                    .as_mut()
                    .ok_or_else(|| AppError::State("not initialized".into()))?;
                let mut next = store.data().clone();
                next.trusted_keys
                    .insert(pending.address, pending.public_key);
                store.commit(next)?;
            }
            core.connect(host_id).await
        })
        .await
    }
    pub async fn forget_host(self: &Arc<Self>, id: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let host = core.host(&id)?;
            core.disconnect_inner(true).await?;
            {
                let mut store = lock(&core.store);
                let store = store
                    .as_mut()
                    .ok_or_else(|| AppError::State("not initialized".into()))?;
                let mut next = store.data().clone();
                next.trusted_keys.remove(&host.address());
                next.paired_servers.remove(&id);
                store.commit(next)?;
            }
            Ok(())
        })
        .await
    }
    pub async fn disconnect(self: &Arc<Self>) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            core.disconnect_inner(true).await
        })
        .await
    }
    async fn disconnect_inner(&self, notify: bool) -> Result<(), AppError> {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.stop_desktop_inner().await;
        let connect = lock(&self.connect_job).take();
        if let Some(task) = connect {
            task.abort();
            let _ = task.await;
        }
        let poll = lock(&self.poll_job).take();
        if let Some(task) = poll {
            task.abort();
            let _ = task.await;
        }
        let old = lock(&self.connected).take();
        if let Some(old) = old {
            let _ = old.ssh.close().await;
        }
        self.update(|s| {
            if notify {
                s.connection = ConnectionPhase::Disconnected;
            }
            s.pending_key = None;
            s.connection_step = ConnectionStep::Idle;
            s.herdr_version = None;
            s.herdr_path = None;
            s.capability = None;
            s.host_key_fingerprint = None;
            s.agents.clear();
            s.selected_agent = None;
            s.agent_output.clear();
            s.conversation.clear();
            s.conversation_available = false;
            s.approval = None;
            s.question = None;
            s.response_activity = ResponseActivity::Idle;
            s.agent_pause = AgentPause::Running;
        });
        *lock(&self.history) = None;
        lock(&self.pending_messages).clear();
        lock(&self.consumed_approvals).clear();
        lock(&self.agent_starts).clear();
        self.chat_clients.lock().await.clear();
        lock(&self.chat_failures).clear();
        for files in lock(&self.uploads).values_mut() {
            for file in files {
                if file.attachment.path.is_empty() {
                    file.failed = true;
                }
            }
        }
        self.update(|s| s.composer = Composer::default());
        self.restore_session_cache();
        Ok(())
    }

    pub async fn select_agent(self: &Arc<Self>, id: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            if !core.snapshot().agents.iter().any(|a| a.terminal_id == id) {
                return Err(AppError::State("agent unavailable".into()));
            }
            core.update(|s| {
                s.selected_agent = Some(id);
                s.composer = Composer::default();
                s.agent_output.clear();
                s.conversation.clear();
                s.conversation_available = false;
                s.approval = None;
                s.question = None;
                s.response_activity = ResponseActivity::Idle;
                s.agent_pause = AgentPause::Unknown;
            });
            *lock(&core.history) = None;
            if let Ok(c) = core.connection()
                && let Some(selected) = core.snapshot().selected_agent
            {
                core.refresh_agent(&c, &selected, core.generation.load(Ordering::SeqCst))
                    .await;
            }
            Ok(())
        })
        .await
    }
    pub async fn prompt_agent(self: &Arc<Self>, id: String, text: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            let generation = core.generation.load(Ordering::SeqCst);
            if lock(&core.paused_processes).contains_key(&format!("{}/{}", c.host.id, id)) {
                return Err(AppError::State(
                    "resume the tracked agent process before submitting input".into(),
                ));
            }
            if (text.trim().is_empty() && core.snapshot().composer.uploads.is_empty())
                || text.len() > 65536
            {
                return Err(AppError::State("invalid prompt length".into()));
            }
            let agent = core
                .snapshot()
                .agents
                .into_iter()
                .find(|a| a.terminal_id == id)
                .ok_or_else(|| AppError::State("agent unavailable".into()))?;
            if core.snapshot().selected_agent.as_deref() == Some(&id)
                && core.snapshot().agent_pause != AgentPause::Running
            {
                return Err(AppError::State(
                    "resume the agent before submitting a prompt".into(),
                ));
            }
            let previous: BTreeSet<String> = core
                .snapshot()
                .conversation
                .iter()
                .map(|m| m.id.clone())
                .collect();
            let key = chat_key(&c, &agent);
            let files = lock(&core.uploads).get(&key).cloned().unwrap_or_default();
            if files
                .iter()
                .any(|u| u.failed || u.attachment.path.is_empty())
            {
                return Err(AppError::State(
                    "wait for all attachments to upload or remove failed files".into(),
                ));
            }
            let attachments: Vec<_> = files.iter().map(|u| u.attachment.clone()).collect();
            if core
                .snapshot()
                .question
                .as_ref()
                .is_some_and(|q| q.blocking)
            {
                return Err(AppError::State("answer the pending model question".into()));
            }
            core.update(|s| {
                if s.selected_agent.as_ref() == Some(&id) {
                    s.response_activity = ResponseActivity::Sending;
                }
            });
            let result = if agent.agent.as_deref() == Some("codex") {
                let client = core
                    .chat_clients
                    .lock()
                    .await
                    .get(&key)
                    .cloned()
                    .ok_or_else(|| AppError::State("Codex chat backend requires setup".into()))?;
                let selected = lock(&core.store)
                    .as_ref()
                    .and_then(|s| {
                        s.read_document::<BTreeMap<String, Selection>>("chat-settings")
                            .ok()
                            .flatten()
                    })
                    .and_then(|m| m.get(&key).cloned())
                    .unwrap_or_else(|| client.initial.clone());
                client
                    .send(&text, &attachments, &selected)
                    .await
                    .map(|_| ())
                    .map_err(AppError::from)
            } else {
                if !files.is_empty() {
                    return Err(AppError::State(
                        "attachments require the Codex backend".into(),
                    ));
                }
                c.api
                    .prompt_agent(&agent.pane_id, &text)
                    .await
                    .map_err(AppError::from)
            };
            let activity = core
                .chat_clients
                .lock()
                .await
                .get(&key)
                .map(|c| c.runtime().phase())
                .unwrap_or(if result.is_ok() {
                    ResponseActivity::Thinking
                } else {
                    ResponseActivity::Failed
                });
            core.update(|s| {
                if s.selected_agent.as_ref() == Some(&id) {
                    s.response_activity = activity;
                }
            });
            let text = composer::compose_text(&text, &attachments)?;
            if core.generation.load(Ordering::SeqCst) != generation {
                return Err(AppError::State(
                    "host changed during prompt submission".into(),
                ));
            }
            if let Err(e) = &result {
                core.report(e);
            }
            if result.is_ok() {
                if let Err(error) = core.note_sent_prompt(&c, &agent, &text).await {
                    core.report(&error);
                }
                if let Some(staged) = lock(&core.uploads).get_mut(&key) {
                    staged.retain(|u| {
                        !files
                            .iter()
                            .any(|sent| sent.attachment.id == u.attachment.id)
                    });
                }
                core.publish_uploads(&id, &key);
                let message = Message {
                    id: format!("local-{}", new_device_id()?),
                    role: Role::User,
                    intermediate: false,
                    text,
                    tool: None,
                };
                let mut pending = lock(&core.pending_messages);
                if pending.len() >= 32 {
                    pending.remove(0);
                }
                pending.push(PendingMessage {
                    key: format!("{}/{}/{}", c.host.id, id, source_key(&agent)),
                    message: message.clone(),
                    previous,
                });
                drop(pending);
                core.update(|s| {
                    if s.selected_agent.as_deref() == Some(&id) {
                        s.conversation.push(message);
                    }
                });
            }
            result
        })
        .await
    }

    pub async fn send_agent_key(self: &Arc<Self>, id: String, key: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            if !matches!(
                key.as_str(),
                "Up" | "Down" | "Left" | "Right" | "Enter" | "Escape" | "Tab"
            ) {
                return Err(AppError::State("unsupported terminal key".into()));
            }
            let c = core.connection()?;
            if lock(&core.paused_processes).contains_key(&format!("{}/{}", c.host.id, id)) {
                return Err(AppError::State(
                    "resume the tracked agent process before submitting input".into(),
                ));
            }
            if core.snapshot().selected_agent.as_deref() != Some(&id) {
                return Err(AppError::State("selected agent changed".into()));
            }
            let agent = c
                .api
                .agents()
                .await?
                .into_iter()
                .find(|a| a.terminal_id == id)
                .ok_or_else(|| AppError::State("agent unavailable".into()))?;
            c.api.send_keys(&agent.pane_id, &[key]).await?;
            Ok(())
        })
        .await
    }

    pub async fn respond_approval(
        self: &Arc<Self>,
        id: String,
        request: String,
        choice: u32,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            if lock(&core.paused_processes).contains_key(&format!("{}/{}", c.host.id, id)) {
                return Err(AppError::State(
                    "resume the tracked agent process before submitting input".into(),
                ));
            }
            let agent = c
                .api
                .agents()
                .await?
                .into_iter()
                .find(|a| a.terminal_id == id)
                .ok_or_else(|| AppError::State("agent unavailable".into()))?;
            if request.starts_with("rpc-") {
                let client = core
                    .chat_clients
                    .lock()
                    .await
                    .get(&chat_key(&c, &agent))
                    .cloned()
                    .ok_or_else(|| AppError::State("approval connection expired".into()))?;
                client.approve(&request, choice).await?;
                core.update(|s| {
                    if s.selected_agent.as_ref() == Some(&id) {
                        s.approval = None;
                        s.response_activity = client.runtime().phase();
                    }
                });
                return Ok(());
            }
            let text = c.api.read_agent(&agent.pane_id).await?;
            let provider = agent
                .agent_session
                .as_ref()
                .map(|s| s.agent.as_str())
                .or(agent.agent.as_deref())
                .unwrap_or_default();
            let approval =
                kite_agent::terminal_choices(agent.agent_status == "blocked", provider, &text)
                    .ok_or_else(|| AppError::State("approval is no longer available".into()))?;
            let keys = approval_action(&agent, &approval, &request, choice)?;
            let token = format!("{}/{}/{}", c.host.id, id, request);
            if !lock(&core.consumed_approvals).insert(token) {
                return Err(AppError::State("approval was already submitted".into()));
            }
            core.update(|s| {
                if s.selected_agent.as_deref() == Some(&id) {
                    s.approval = None;
                }
            });
            let result = c
                .api
                .send_keys(&agent.pane_id, &keys)
                .await
                .map_err(AppError::from);
            if let Err(e) = &result {
                core.report(e);
            }
            result
        })
        .await
    }

    pub async fn answer_question(
        self: &Arc<Self>,
        id: String,
        request: String,
        answers: Vec<QuestionAnswer>,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            let snapshot = core.snapshot();
            if snapshot.selected_agent.as_ref() != Some(&id)
                || snapshot.question.as_ref().is_none_or(|q| q.id != request)
            {
                return Err(AppError::State("question selection changed".into()));
            }
            if lock(&core.paused_processes).contains_key(&format!("{}/{}", c.host.id, id)) {
                return Err(AppError::State("resume the agent before answering".into()));
            }
            let agent = snapshot
                .agents
                .iter()
                .find(|a| a.terminal_id == id)
                .ok_or_else(|| AppError::State("agent unavailable".into()))?;
            let client = core
                .chat_clients
                .lock()
                .await
                .get(&chat_key(&c, agent))
                .cloned()
                .ok_or_else(|| AppError::State("question connection unavailable".into()))?;
            client.answer(&request, &answers).await?;
            let runtime = client.runtime();
            core.update(|s| {
                if s.selected_agent.as_ref() == Some(&id) {
                    s.question = runtime.question();
                    s.response_activity = runtime.phase();
                }
            });
            Ok(())
        })
        .await
    }

    pub async fn set_agent_paused(
        self: &Arc<Self>,
        id: String,
        paused: bool,
    ) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            let key = format!("{}/{}", c.host.id, id);
            let result = async {
                if paused {
                    let agent = c
                        .api
                        .agents()
                        .await?
                        .into_iter()
                        .find(|a| a.terminal_id == id)
                        .ok_or_else(|| AppError::State("agent unavailable".into()))?;
                    let token = process::prepare(&c.api, &c.ssh, &agent).await?;
                    lock(&core.paused_processes).insert(key.clone(), token.clone());
                    core.persist_pauses()?;
                    core.update(|s| {
                        if s.selected_agent.as_deref() == Some(&id) {
                            s.agent_pause = AgentPause::Unknown;
                        }
                    });
                    process::signal(&c.ssh, &token, true).await?;
                } else {
                    let token = lock(&core.paused_processes)
                        .get(&key)
                        .cloned()
                        .ok_or_else(|| AppError::State("no tracked paused process".into()))?;
                    process::signal(&c.ssh, &token, false).await?;
                    lock(&core.paused_processes).remove(&key);
                    core.persist_pauses()?;
                }
                core.update(|s| {
                    if s.selected_agent.as_deref() == Some(&id) {
                        s.agent_pause = if paused {
                            AgentPause::Paused
                        } else {
                            AgentPause::Running
                        };
                    }
                });
                Ok::<_, AppError>(())
            }
            .await;
            if let Err(e) = &result {
                core.report(e);
            }
            result
        })
        .await
    }

    fn persist_pauses(&self) -> Result<(), AppError> {
        let records = lock(&self.paused_processes).clone();
        lock(&self.store)
            .as_ref()
            .ok_or_else(|| AppError::State("not initialized".into()))?
            .write_document("paused-processes", &records)?;
        Ok(())
    }
    pub async fn clear_failure(self: &Arc<Self>) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            core.update(|s| s.failure = None);
            Ok(())
        })
        .await
    }

    fn identity(&self) -> Result<Arc<Identity>, AppError> {
        match self
            .secrets
            .read("sunshine-identity")
            .map_err(AppError::State)?
        {
            Some(bytes) => Ok(Arc::new(Identity::from_secret(&bytes)?)),
            None => {
                let id = Identity::generate()?;
                self.secrets
                    .write("sunshine-identity", &id.to_secret()?)
                    .map_err(AppError::State)?;
                Ok(Arc::new(id))
            }
        }
    }
    fn client(&self, c: &Connected) -> Result<Client, AppError> {
        let data = lock(&self.store)
            .as_ref()
            .ok_or_else(|| AppError::State("not initialized".into()))?
            .data()
            .clone();
        Ok(Client::new(
            c.ssh.clone(),
            c.host
                .stream_host
                .clone()
                .unwrap_or_else(|| c.host.host.clone()),
            c.host.sunshine_port,
            data.device_id,
            self.identity()?,
            data.paired_servers
                .get(&c.host.id)
                .map(|p| p.certificate_pem.as_str()),
        )?)
    }

    pub async fn start_desktop(self: &Arc<Self>, sink: Arc<dyn MediaSink>) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            core.stop_desktop_inner().await;
            let c = core.connection()?;
            let generation = core.desktop_generation.fetch_add(1, Ordering::SeqCst) + 1;
            let (tx, rx) = watch::channel(false);
            *lock(&core.media) = Some(sink.clone());
            core.update(|s| {
                s.desktop = Desktop {
                    phase: DesktopPhase::Starting,
                    ..Desktop::default()
                };
                s.failure = None;
            });
            let job_core = core.clone();
            let task = tokio::spawn(async move {
                job_core.desktop_flow(c, sink, generation, rx).await;
            });
            *core.desktop_job.lock().await = Some(DesktopJob { cancel: tx, task });
            Ok(())
        })
        .await
    }
    async fn desktop_flow(
        self: Arc<Self>,
        c: Arc<Connected>,
        sink: Arc<dyn MediaSink>,
        generation: u64,
        mut cancel: watch::Receiver<bool>,
    ) {
        let work = async {
            let mut client = self.client(&c)?;
            let info = client.info().await?;
            let source = client.desktop_size().await?;
            let config = StreamConfig::for_desktop(source, |format| sink.supports(format))?;
            self.desktop_update(generation, |s| {
                s.desktop.source_width = source.width;
                s.desktop.source_height = source.height;
                s.desktop.width = config.width;
                s.desktop.height = config.height;
                s.desktop.fps = config.fps;
            });
            if !info.paired {
                let pin = new_pin()?;
                *lock(&self.pending_pair) = Some(PendingPair {
                    client: client.clone(),
                    pin: pin.clone(),
                    generation,
                });
                self.desktop_update(generation, |s| {
                    s.desktop.phase = DesktopPhase::Pairing;
                    s.desktop.pin = Some(pin.clone());
                    s.desktop.pairing_name = Some(client.pairing_name().to_owned());
                });
                client = client.pair(&pin).await?;
                {
                    let mut store = lock(&self.store);
                    let store = store
                        .as_mut()
                        .ok_or_else(|| AppError::State("not initialized".into()))?;
                    let mut next = store.data().clone();
                    next.paired_servers.insert(
                        c.host.id.clone(),
                        PairedServer {
                            certificate_pem: client.certificate_pem()?,
                        },
                    );
                    store.commit(next)?;
                }
            }
            *lock(&self.pending_pair) = None;
            self.desktop_update(generation, |s| {
                s.desktop.phase = DesktopPhase::Starting;
                s.desktop.pin = None;
            });
            Ok::<_, AppError>((client.launch_desktop(config).await?, config))
        };
        let launch = tokio::select! {result=work=>result,_=cancel.changed()=>return};
        let (launch, config) = match launch {
            Ok(v) => v,
            Err(e) => {
                self.desktop_error(generation, &e);
                return;
            }
        };
        if *cancel.borrow() {
            return;
        }
        let mut starting =
            tokio::task::spawn_blocking(move || NativeSession::connect(launch, config, sink));
        let session = loop {
            tokio::select! {
                result=&mut starting=>break result,
                _=tokio::time::sleep(Duration::from_millis(50))=>{if *cancel.borrow(){native::interrupt_pending();}}
            }
        };
        let session = match session {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                if !*cancel.borrow() {
                    self.desktop_error(generation, &e.into());
                }
                return;
            }
            Err(_) => {
                self.desktop_error(
                    generation,
                    &AppError::State("stream worker interrupted".into()),
                );
                return;
            }
        };
        let began = std::time::Instant::now();
        let mut published = std::time::Instant::now() - Duration::from_secs(2);
        loop {
            if *cancel.borrow() {
                break;
            }
            let stats = session.stats();
            if stats.terminated || stats.decoder_error != 0 {
                self.desktop_error(
                    generation,
                    &AppError::Stream(kite_stream::StreamError::Native {
                        stage: stats.stage,
                        code: if stats.decoder_error != 0 {
                            stats.decoder_error
                        } else {
                            stats.error
                        },
                    }),
                );
                break;
            }
            if began.elapsed() > Duration::from_secs(15) && stats.rendered == 0 {
                self.desktop_error(
                    generation,
                    &AppError::State("decoder did not render a video frame".into()),
                );
                break;
            }
            if published.elapsed() >= Duration::from_secs(2)
                || (stats.rendered > 0 && self.snapshot().desktop.phase != DesktopPhase::Streaming)
            {
                self.desktop_update(generation, |s| {
                    s.desktop.stage = stats.stage;
                    s.desktop.received = stats.received;
                    s.desktop.rendered = stats.rendered;
                    if stats.rendered > 0 {
                        s.desktop.phase = DesktopPhase::Streaming;
                    }
                });
                published = std::time::Instant::now();
            }
            tokio::select! {_=cancel.changed()=>{},_=tokio::time::sleep(Duration::from_millis(500))=>{}}
        }
        let _ = tokio::task::spawn_blocking(move || session.stop()).await;
    }
    fn desktop_update(&self, generation: u64, f: impl FnOnce(&mut Snapshot)) {
        if self.desktop_generation.load(Ordering::SeqCst) == generation {
            self.update(f);
        }
    }
    fn desktop_error(&self, generation: u64, error: &AppError) {
        self.desktop_update(generation, |s| {
            s.desktop.phase = DesktopPhase::Failed;
            s.desktop.pin = None;
            s.desktop.takeover = false;
            s.failure = Some(describe(error));
        });
    }
    pub async fn approve_pairing(self: &Arc<Self>, pin: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let client = {
                let pair = lock(&core.pending_pair);
                let p = pair
                    .as_ref()
                    .ok_or_else(|| AppError::State("no pairing request".into()))?;
                if p.pin != pin || p.generation != core.desktop_generation.load(Ordering::SeqCst) {
                    return Err(AppError::State("pairing request expired".into()));
                }
                p.client.clone()
            };
            let result = client
                .approve_pin_from_saved_credentials(&pin)
                .await
                .map_err(AppError::from);
            if let Err(e) = &result {
                core.report(e);
            }
            result
        })
        .await
    }
    pub async fn set_takeover(self: &Arc<Self>, enabled: bool) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            if !enabled {
                if let Some(media) = lock(&core.media).clone() {
                    media.set_takeover(false);
                }
                core.update(|s| s.desktop.takeover = false);
                return Ok(());
            }
            if core.snapshot().desktop.phase != DesktopPhase::Streaming {
                return Err(AppError::State("desktop is not ready".into()));
            }
            let media = lock(&core.media)
                .clone()
                .ok_or_else(|| AppError::State("no media target".into()))?;
            media.set_takeover(enabled);
            core.update(|s| s.desktop.takeover = enabled);
            Ok(())
        })
        .await
    }
    pub async fn stop_desktop(self: &Arc<Self>) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            core.stop_desktop_inner().await;
            Ok(())
        })
        .await
    }
    async fn stop_desktop_inner(&self) {
        self.desktop_generation.fetch_add(1, Ordering::SeqCst);
        *lock(&self.pending_pair) = None;
        if let Some(media) = lock(&self.media).take() {
            media.set_takeover(false);
        }
        let job = self.desktop_job.lock().await.take();
        if let Some(job) = job {
            self.update(|s| {
                s.desktop.phase = DesktopPhase::Stopping;
                s.desktop.takeover = false;
            });
            let _ = job.cancel.send(true);
            let _ = job.task.await;
        }
        self.update(|s| s.desktop = Desktop::default());
    }
}

fn describe(error: &AppError) -> Failure {
    let mut f = Failure {
        code: FailureCode::Connection,
        details: error.to_string().chars().take(2048).collect(),
        username: String::new(),
        trusted: String::new(),
        presented: String::new(),
    };
    match error {
        AppError::Ssh(SshError::HostKeyChanged { trusted, presented }) => {
            f.code = FailureCode::HostKeyChanged;
            f.trusted = trusted.clone();
            f.presented = presented.clone();
        }
        AppError::Ssh(SshError::AuthRejected(user)) => {
            f.code = FailureCode::Authentication;
            f.username = user.clone();
        }
        AppError::Ssh(SshError::InvalidKey(_)) => f.code = FailureCode::InvalidKey,
        AppError::Herdr(kite_herdr::CheckError::NotFound) => f.code = FailureCode::HerdrMissing,
        AppError::Herdr(kite_herdr::CheckError::Unsupported { .. }) => {
            f.code = FailureCode::HerdrUnsupported
        }
        AppError::Store(_) => f.code = FailureCode::Storage,
        AppError::Api(_) | AppError::AgentProcess(_) => f.code = FailureCode::HerdrApi,
        AppError::Stream(_) => f.code = FailureCode::Stream,
        AppError::State(_) => f.code = FailureCode::InvalidConfiguration,
        _ => {}
    }
    f
}

fn chat_key(c: &Connected, agent: &Agent) -> String {
    format!("{}/{}", c.host.id, source_key(agent))
}

fn source_key(agent: &Agent) -> String {
    agent
        .agent_session
        .as_ref()
        .map(history::key)
        .unwrap_or_else(|| fingerprint(&agent.terminal_id))
}

fn approval_id(agent: &Agent, raw: &str) -> String {
    format!("{}-{}-{}", source_key(agent), agent.state_change_seq, raw)
}

fn approval_action(
    agent: &Agent,
    approval: &Approval,
    request: &str,
    choice: u32,
) -> Result<Vec<String>, AppError> {
    if request != approval_id(agent, &approval.id) {
        return Err(AppError::State(
            "approval changed; review the current request".into(),
        ));
    }
    approval
        .choices
        .get(choice as usize)
        .map(|c| c.keys.clone())
        .ok_or_else(|| AppError::State("invalid approval choice".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tokio::io::AsyncReadExt;
    struct Secrets {
        values: Mutex<BTreeMap<String, Vec<u8>>>,
        writes: AtomicU64,
    }
    impl SecretStorage for Secrets {
        fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(lock(&self.values).get(name).cloned())
        }
        fn write(&self, name: &str, value: &[u8]) -> Result<(), String> {
            lock(&self.values).insert(name.into(), value.to_vec());
            self.writes.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }
    }
    struct NoObserver;
    impl Observer for NoObserver {
        fn changed(&self, _: Snapshot) {}
    }
    #[test]
    fn approval_cannot_cross_session_state_or_question() {
        let mut agent = Agent {
            launch_pending: false,
            interactive_ready: true,
            terminal_id: "t1".into(),
            name: None,
            agent: Some("claude".into()),
            title: None,
            agent_status: "blocked".into(),
            workspace_id: "w1".into(),
            tab_id: "w1:t1".into(),
            pane_id: "w1:p1".into(),
            cwd: None,
            revision: 1,
            state_change_seq: 10,
            agent_session: Some(kite_herdr::api::AgentSession {
                source: "herdr:claude".into(),
                agent: "claude".into(),
                kind: "id".into(),
                value: "session-a".into(),
            }),
        };
        let a = kite_agent::terminal_choices(true, "claude", "Proceed?\n❯ 1. Yes\n 2. No").unwrap();
        let request = approval_id(&agent, &a.id);
        assert_eq!(approval_action(&agent, &a, &request, 0).unwrap(), ["Enter"]);
        assert!(approval_action(&agent, &a, &request, 9).is_err());
        agent.state_change_seq += 1;
        assert!(approval_action(&agent, &a, &request, 0).is_err());
        agent.state_change_seq -= 1;
        agent.agent_session.as_mut().unwrap().value = "session-b".into();
        assert!(approval_action(&agent, &a, &request, 0).is_err());
        agent.agent_session.as_mut().unwrap().value = "session-a".into();
        let b = kite_agent::terminal_choices(
            true,
            "claude",
            "Different permission. Proceed?\n❯ 1. Yes\n 2. No",
        )
        .unwrap();
        assert!(approval_action(&agent, &b, &request, 0).is_err());
    }

    #[tokio::test]
    async fn takeover_does_not_pause_agents_and_release_is_idempotent() {
        use std::sync::atomic::AtomicBool;
        struct Sink(AtomicBool);
        impl MediaSink for Sink {
            fn supports(&self, _: kite_stream::VideoFormat) -> bool {
                true
            }
            fn configure(&self, _: kite_stream::VideoFormat) -> bool {
                true
            }
            fn submit(&self, _: &[u8], _: kite_stream::FrameInfo) -> bool {
                true
            }
            fn set_input(&self, _: Option<Arc<dyn kite_stream::InputSender>>) {}
            fn rendered_frames(&self) -> u64 {
                1
            }
            fn failure_code(&self) -> i32 {
                0
            }
            fn set_takeover(&self, v: bool) {
                self.0.store(v, Ordering::SeqCst);
            }
            fn stop(&self) {}
        }
        let (secrets, _) = fixture();
        let core = AppCore::new(secrets, Arc::new(NoObserver));
        core.set_takeover(false).await.unwrap();
        let sink = Arc::new(Sink(AtomicBool::new(false)));
        *lock(&core.media) = Some(sink.clone());
        core.update(|s| s.desktop.phase = DesktopPhase::Streaming);
        core.set_takeover(true).await.unwrap();
        assert!(sink.0.load(Ordering::SeqCst));
        assert!(lock(&core.paused_processes).is_empty());
        core.set_takeover(false).await.unwrap();
        core.set_takeover(false).await.unwrap();
        assert!(!sink.0.load(Ordering::SeqCst));
    }
    fn fixture() -> (Arc<Secrets>, kite_ssh::KeyPair) {
        let key = kite_ssh::generate_ed25519("fixture").unwrap();
        let secrets = Arc::new(Secrets {
            values: Mutex::new(BTreeMap::from([(
                "ssh-ed25519".into(),
                key.private_key_openssh.as_bytes().to_vec(),
            )])),
            writes: AtomicU64::new(0),
        });
        (secrets, key)
    }
    fn legacy() -> LegacyImport {
        LegacyImport {
            source: "android-m1".into(),
            host: None,
            trusted_keys: Vec::new(),
        }
    }

    #[tokio::test]
    async fn initialization_reuses_the_native_private_key_across_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let (secrets, key) = fixture();
        let core = AppCore::new(secrets.clone(), Arc::new(NoObserver));
        core.initialize(dir.path().into(), "new-name".into(), legacy())
            .await
            .unwrap();
        assert_eq!(core.snapshot().key.unwrap().fingerprint, key.fingerprint);
        let next = AppCore::new(secrets.clone(), Arc::new(NoObserver));
        next.initialize(dir.path().into(), "another-name".into(), legacy())
            .await
            .unwrap();
        assert_eq!(
            next.snapshot().key.unwrap().public_key,
            key.public_key_openssh
        );
        assert_eq!(secrets.writes.load(Ordering::Relaxed), 0);
    }
    #[tokio::test]
    async fn a_corrupt_native_secret_is_not_silently_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let (secrets, _) = fixture();
        lock(&secrets.values).insert("ssh-ed25519".into(), b"corrupt".to_vec());
        let core = AppCore::new(secrets.clone(), Arc::new(NoObserver));
        assert!(
            core.initialize(dir.path().into(), "new-name".into(), legacy())
                .await
                .is_err()
        );
        assert!(!core.snapshot().initialized);
        assert_eq!(secrets.writes.load(Ordering::Relaxed), 0);
    }
    #[tokio::test]
    async fn disconnect_cancels_a_pending_handshake_without_late_state_updates() {
        let dir = tempfile::tempdir().unwrap();
        let (secrets, _) = fixture();
        let core = AppCore::new(secrets, Arc::new(NoObserver));
        core.initialize(dir.path().into(), "fixture".into(), legacy())
            .await
            .unwrap();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let id = core
            .save_host(Host {
                id: "waiting".into(),
                host: "127.0.0.1".into(),
                port,
                username: "kite".into(),
                herdr_path: None,
                stream_host: None,
                sunshine_port: 47989,
            })
            .await
            .unwrap();
        core.connect(id).await.unwrap();
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
            .await
            .unwrap()
            .unwrap();
        core.disconnect().await.unwrap();
        let mut data = Vec::new();
        tokio::time::timeout(Duration::from_secs(2), socket.read_to_end(&mut data))
            .await
            .unwrap()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(core.snapshot().connection, ConnectionPhase::Disconnected);
    }
}
