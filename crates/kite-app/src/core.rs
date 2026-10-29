//! Process-owned domain state, command execution and cancellation.
use crate::AppError;
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
    future::Future,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    runtime::Runtime,
    sync::{Mutex as AsyncMutex, watch},
    task::JoinHandle,
};

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
pub enum DesktopPhase {
    Idle,
    Pairing,
    Starting,
    Streaming,
    Stopping,
    Failed,
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
    pub pending_key: Option<HostKeyPrompt>,
    pub herdr_version: Option<String>,
    pub herdr_path: Option<String>,
    pub capability: Option<String>,
    pub host_key_fingerprint: Option<String>,
    pub agents: Vec<Agent>,
    pub selected_agent: Option<String>,
    pub agent_output: String,
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
            pending_key: None,
            herdr_version: None,
            herdr_path: None,
            capability: None,
            host_key_fingerprint: None,
            agents: Vec::new(),
            selected_agent: None,
            agent_output: String::new(),
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
                s.selected_host = Some(host.id.clone());
                s.failure = None;
                s.pending_key = None;
            });
            core.disconnect_inner(false).await?;
            let generation = core.generation.fetch_add(1, Ordering::SeqCst) + 1;
            core.update(|s| {
                s.connection = ConnectionPhase::Connecting;
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
        let result = self.open_connection(&host).await;
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
                    s.connection = ConnectionPhase::Connected;
                    s.pending_key = None;
                    s.failure = None;
                    s.herdr_version = Some(connection.version.clone());
                    s.herdr_path = Some(connection.path.clone());
                    s.capability = Some(connection.capability.clone());
                    s.host_key_fingerprint = Some(connection.ssh.host_key_fingerprint());
                });
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
    async fn open_connection(&self, host: &Host) -> Result<Connected, AppError> {
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
        let checked = check_api_bridge(&ssh, host.herdr_path.as_deref()).await?;
        let api = Arc::new(Api::new(ssh.clone(), &checked.herdr_path));
        let version = api.ping().await?;
        Ok(Connected {
            host: host.clone(),
            ssh,
            api,
            version,
            path: checked.herdr_path,
            capability: checked.capability,
        })
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
                    agents.sort_by_key(|a| if a.agent_status == "blocked" { 0 } else { 1 });
                    if self.generation.load(Ordering::SeqCst) != generation {
                        return;
                    }
                    if self.snapshot().agents != agents {
                        self.update(|s| s.agents = agents);
                    }
                }
                Err(e) => {
                    if self.generation.load(Ordering::SeqCst) == generation {
                        self.report(&e.into());
                    }
                }
            }
            if let Some(selected) = self.snapshot().selected_agent
                && let Ok(text) = connection.api.read_agent(&selected).await
                && self.generation.load(Ordering::SeqCst) == generation
            {
                self.update(|s| {
                    if s.selected_agent.as_deref() == Some(&selected) {
                        s.agent_output = text;
                    }
                });
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
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
            s.herdr_version = None;
            s.herdr_path = None;
            s.capability = None;
            s.host_key_fingerprint = None;
            s.agents.clear();
            s.selected_agent = None;
            s.agent_output.clear();
        });
        Ok(())
    }

    pub async fn select_agent(self: &Arc<Self>, id: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            core.update(|s| {
                s.selected_agent = Some(id);
                s.agent_output.clear();
            });
            Ok(())
        })
        .await
    }
    pub async fn prompt_agent(self: &Arc<Self>, id: String, text: String) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let c = core.connection()?;
            let result = c.api.prompt_agent(&id, &text).await.map_err(AppError::from);
            if let Err(e) = &result {
                core.report(e);
            }
            result
        })
        .await
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
        AppError::Api(_) => f.code = FailureCode::HerdrApi,
        AppError::Stream(_) => f.code = FailureCode::Stream,
        AppError::State(_) => f.code = FailureCode::InvalidConfiguration,
        _ => {}
    }
    f
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
