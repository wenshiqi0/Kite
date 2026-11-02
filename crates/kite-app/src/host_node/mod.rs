//! Desktop host application runtime. Native panels consume its structured CLI only.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{FileTypeExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    process::Command,
    sync::{Mutex, RwLock, Semaphore},
    task::JoinSet,
};

mod bridge;
mod desktop;
mod docs;
mod harness;
mod lifecycle;
mod mcp;
mod notifications;
mod push;
mod resources;
mod upgrade;
mod wire;
pub type Result<T> = std::result::Result<T, Error>;
pub const API: u64 = 1;
const LIMIT: usize = 16 * 1024;
const SERVICE_TIMEOUT: Duration = Duration::from_secs(4);
const STATUS_TIMEOUT: Duration = Duration::from_secs(5);
const SERVICE_CACHE_TTL: Duration = Duration::from_secs(3);

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("node configuration: {0}")]
    Config(&'static str),
    #[error("node protocol: {0}")]
    Protocol(&'static str),
    #[error("Kite Node unavailable: {0}")]
    Unavailable(String),
    #[error("node request failed: {0}")]
    Remote(String),
}
pub fn version() -> &'static str {
    include_str!("../../../../android/version.properties")
        .lines()
        .find_map(|l| l.strip_prefix("versionName="))
        .unwrap_or("unknown")
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Service {
    pub name: String,
    pub unit: String,
    #[serde(default)]
    pub external: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub node_id: String,
    pub machine: String,
    pub socket: PathBuf,
    pub backend: PathBuf,
    pub herdr: PathBuf,
    pub python: PathBuf,
    pub systemctl: PathBuf,
    #[serde(default = "crate::resources::default_agent_memory_limit")]
    pub agent_memory_limit_bytes: u64,
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
    #[serde(default)]
    pub push_service_account: Option<PathBuf>,
    pub services: Vec<Service>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}
impl Config {
    fn validate(&self) -> Result<()> {
        if self.agent_memory_limit_bytes < 256 * 1024 * 1024
            || self.agent_memory_limit_bytes > 1024 * crate::resources::GIB
        {
            return Err(Error::Config("invalid agent memory limit"));
        }
        if self.node_id.is_empty()
            || self.machine.is_empty()
            || !self.socket.is_absolute()
            || !self.backend.is_absolute()
            || !self.herdr.is_absolute()
            || !self.python.is_absolute()
            || !self.systemctl.is_absolute()
            || self
                .push_service_account
                .as_ref()
                .is_some_and(|path| !path.is_absolute())
        {
            return Err(Error::Config("absolute paths and node identity required"));
        }
        if self.services.len() > 8
            || self.services.iter().any(|s| {
                s.name.is_empty()
                    || s.unit.starts_with('-')
                    || !s.unit.ends_with(".service")
                    || !s
                        .unit
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"@_.-".contains(&b))
            })
        {
            return Err(Error::Config("invalid service unit"));
        }
        if self.environment.keys().any(|k| {
            ![
                "DISPLAY",
                "XAUTHORITY",
                "XDG_RUNTIME_DIR",
                "DBUS_SESSION_BUS_ADDRESS",
                "PATH",
                "KITE_ENABLE_DSH",
                "KITE_DSH_COMMAND",
                "KITE_DSH_PROVIDER",
                "KITE_DSH_MODEL",
                "KITE_DSH_HEALTH_URL",
                "KITE_ENABLE_PLY",
                "KITE_PLY_COMMAND",
                "KITE_PLY_PROVIDER",
                "KITE_PLY_MODEL",
                "KITE_PLY_HEALTH_URL",
                "DEEPSEEK_API_KEY",
                "MIMO_API_KEY",
                "DSH_HOME",
            ]
            .contains(&k.as_str())
        }) {
            return Err(Error::Config("unsupported environment key"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeRequest {
    pub method: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub params: Value,
}
impl NodeRequest {
    fn new(method: &str) -> Self {
        Self {
            method: method.into(),
            args: vec![],
            params: json!({}),
        }
    }
}
struct Node {
    config: Config,
    started: Instant,
    requests: AtomicU64,
    backend_calls: AtomicU64,
    lifecycle_last_scan: AtomicU64,
    lifecycle_reclaimed: AtomicU64,
    lifecycle_scan: Mutex<()>,
    lifecycle_wake: tokio::sync::Notify,
    resources: resources::Resources,
    permits: Arc<Semaphore>,
    notifications: Option<notifications::Journal>,
    push: Option<push::Push>,
    notification_problem: Option<String>,
    push_problem: Option<String>,
    services: Mutex<Option<(Instant, Vec<Value>)>>,
    upgrade: Option<upgrade::Handover>,
    observation_gate: RwLock<()>,
}
struct NodeLock(fs::File);
impl Drop for NodeLock {
    fn drop(&mut self) {
        // A concurrently spawning child may briefly inherit the open file
        // description before exec closes it. Release ownership explicitly.
        let _ = self.0.unlock();
    }
}

pub fn run(mut args: Vec<String>) -> Result<i32> {
    let socket_option = leading_option(&mut args, "--socket")?.map(PathBuf::from);
    match args.first().map(String::as_str) {
        Some("docs") => return docs::cli(&args[1..]),
        Some("--skill") => return docs::cli(&["skill".into()]),
        Some("desktop") if args.get(1).is_some_and(|s| s == "--help" || s == "-h") => {
            return docs::cli(&["desktop".into()]);
        }
        Some("--help" | "-h") => {
            println!(
                "Kite host CLI\n  kite docs [TOPIC ...]    Concise agent guides (offline)\n  kite desktop ACTION     Configured Linux X11 desktop\n  kite status | sessions [release-archived] | notifications | ensure all\n  kite mcp                Standard stdio MCP\n  --socket PATH           Select the private Node listener"
            );
            return Ok(0);
        }
        _ => {}
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let socket = socket_option
            .or_else(|| std::env::var_os("KITE_NODE_SOCKET").map(PathBuf::from))
            .unwrap_or(default_path(".local/state/kite/node/host.sock")?);
        let command = args.first().map(String::as_str).unwrap_or("status");
        match command {
            "version" | "--version" => {
                println!(
                    "{}",
                    json!({"version":version(),"api":API,"backendProtocol":5,"harnessApi":2,"liveUpgrade":1,"generationLaunch":1,"docsApi":1,"desktopApi":1,"resourceApi":1})
                );
                Ok(0)
            }
            "serve" => {
                let activated = args.iter().any(|s| s == "--activated");
                let replacement = args.iter().any(|s| s == "--handover");
                let bootstrap = args.iter().any(|s| s == "--generation-bootstrap");
                args.retain(|s| s != "--activated" && s != "--handover" && s != "--generation-bootstrap");
                let config = if args.len() == 3 && args[1] == "--config" {
                    PathBuf::from(&args[2])
                } else if args.len() == 1 {
                    default_path(".config/kite/node.json")?
                } else {
                    return Err(Error::Config("serve accepts only --config PATH"));
                };
                if replacement && !activated {
                    return Err(Error::Config("handover requires an activated listener"));
                }
                if bootstrap {
                    if !replacement { return Err(Error::Config("generation bootstrap requires handover")); }
                    upgrade::prepare_replacement().await?;
                }
                let mut data: Config = serde_json::from_slice(&fs::read(&config)?)?;
                data.source_path = Some(fs::canonicalize(&config)?);
                data.validate()?;
                if activated {
                    serve_runtime(data, Some((config, replacement))).await?;
                } else {
                    serve(data).await?;
                }
                Ok(0)
            }
            "sessions" if args.get(1).map(String::as_str) == Some("release-archived") => {
                if args.len() != 2 { return Err(Error::Config("sessions release-archived accepts no arguments")); }
                println!("{}", rpc(&socket, NodeRequest::new("sessions-release-archived")).await?);
                Ok(0)
            }
            "resources" => {
                if args.len()!=1 { return Err(Error::Config("resources accepts no arguments")); }
                println!("{}",rpc(&socket,NodeRequest::new("resources")).await?);
                Ok(0)
            }
            "resource-limit" => {
                if args.len()!=3 || args[1]!="--bytes" { return Err(Error::Config("resource-limit requires --bytes N")); }
                let limit=args[2].parse::<u64>().map_err(|_|Error::Config("invalid memory limit"))?;
                println!("{}",rpc(&socket,NodeRequest{method:"resource-limit".into(),args:vec![],params:json!({"limit_bytes":limit})}).await?);
                Ok(0)
            }
            "resource-admit" => {
                if args.len()!=5 || args[1]!="--pane" || args[3]!="--pid" { return Err(Error::Config("resource-admit requires pane and pid")); }
                let pid=args[4].parse::<u32>().map_err(|_|Error::Config("invalid resource pid"))?;
                println!("{}",rpc(&socket,NodeRequest{method:"resource-admit".into(),args:vec![],params:json!({"pane":args[2],"pid":pid})}).await?);
                Ok(0)
            }
            "resource-ready" => {
                if args.len()!=3 || args[1]!="--pid" { return Err(Error::Config("resource-ready requires --pid N")); }
                let pid=args[2].parse::<u32>().map_err(|_|Error::Config("invalid resource pid"))?;
                println!("{}",rpc(&socket,NodeRequest{method:"resource-ready".into(),args:vec![],params:json!({"pid":pid})}).await?);
                Ok(0)
            }
            "spawn-generation" => {
                if args.len() != 3 || args[1] != "--config" {
                    return Err(Error::Config("spawn-generation requires --config PATH"));
                }
                // This short-lived launcher exits so the new generation is
                // adopted by the user manager before MAINPID is transferred.
                std::process::Command::new(std::env::current_exe()?)
                    .args(["serve", "--config", &args[2], "--activated", "--handover", "--generation-bootstrap"])
                    .stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit())
                    .spawn()?;
                Ok(0)
            }
            "upgrade" => {
                if args.len() != 3 || args[1] != "--binary" {
                    return Err(Error::Config("upgrade requires --binary PATH"));
                }
                let mut request = NodeRequest::new("upgrade");
                request.params = json!({"binary": args[2]});
                println!("{}", rpc(&socket, request).await?);
                Ok(0)
            }
            "backend" => {
                bridge::client(
                    &socket,
                    NodeRequest {
                        method: "backend".into(),
                        args: args[1..].to_vec(),
                        params: json!({}),
                    },
                )
                .await
            }
            "herdr-api" => {
                bridge::client(
                    &socket,
                    NodeRequest {
                        method: "herdr-api".into(),
                        args: args[1..].to_vec(),
                        params: json!({}),
                    },
                )
                .await
            }
            "push-register" => {
                if args.len() != 1 {
                    return Err(Error::Config("push registration uses stdin only"));
                }
                let mut request = NodeRequest::new("push-register");
                request.params = tokio::time::timeout(
                    Duration::from_secs(10),
                    read_json(&mut BufReader::new(tokio::io::stdin())),
                )
                .await
                .map_err(|_| Error::Protocol("push registration input timed out"))??;
                println!("{}", rpc(&socket, request).await?);
                Ok(0)
            }
            "mcp" => {
                mcp::serve(&socket).await?;
                Ok(0)
            }
            "desktop" => {
                let cli=desktop::cli(&args[1..])?;
                let mut request=NodeRequest::new("desktop");request.params=serde_json::to_value(&cli.request)?;
                let mut value=rpc(&socket,request).await?;
                if let Some(output)=cli.output && let Some(bytes)=desktop::image(&socket,&value)? {
                    let output=std::env::current_dir()?.join(output);
                    if let Some(parent)=output.parent(){ fs::create_dir_all(parent)?; }
                    fs::write(&output,bytes)?;fs::set_permissions(&output,fs::Permissions::from_mode(0o600))?;
                    value["path"]=json!(output);
                }
                println!("{value}");Ok(0)
            }
            "status" | "api-version" | "sessions" | "ensure" | "notifications"
            | "notification-get" => {
                let method = if command == "api-version" {
                    "status"
                } else {
                    command
                };
                let mut request = NodeRequest::new(method);
                if command == "status" && args.get(1).map(String::as_str) == Some("--refresh") {
                    request.params = json!({"refresh": true});
                }
                if command == "ensure" {
                    request.params =
                        json!({"service":args.get(1).map(String::as_str).unwrap_or("all")});
                }
                if command == "notifications" {
                    if args.len() == 3 && args[1] == "--after" {
                        request.params = json!({"after":args[2]});
                    } else if args.len() != 1 {
                        return Err(Error::Config("notifications accepts only --after CURSOR"));
                    }
                }
                if command == "notification-get" {
                    if args.len() != 2 {
                        return Err(Error::Config("notification-get requires ID"));
                    }
                    request.params = json!({"id":args[1]});
                }
                println!("{}", rpc(&socket, request).await?);
                Ok(0)
            }
            _ => Err(Error::Config("unknown command")),
        }
    });
    runtime.shutdown_background();
    result
}
fn leading_option(args: &mut Vec<String>, name: &str) -> Result<Option<String>> {
    if args.first().map(String::as_str) != Some(name) {
        return Ok(None);
    }
    if args.len() < 3 {
        return Err(Error::Config("global option requires a value and command"));
    }
    args.remove(0);
    Ok(Some(args.remove(0)))
}
fn default_path(suffix: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var_os("HOME").ok_or(Error::Config("HOME missing"))?).join(suffix))
}

pub async fn json_line<W: AsyncWrite + Unpin, T: Serialize>(out: &mut W, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    out.write_all(&bytes).await?;
    out.flush().await?;
    Ok(())
}
pub async fn read_json<R: AsyncBufRead + Unpin>(input: &mut R) -> Result<Value> {
    read_json_bounded(input, LIMIT).await
}
pub async fn read_json_bounded<R: AsyncBufRead + Unpin>(
    input: &mut R,
    limit: usize,
) -> Result<Value> {
    let mut bytes = vec![];
    let n = input
        .take((limit + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .await?;
    if n == 0 || n > limit || bytes.last() != Some(&b'\n') {
        return Err(Error::Protocol("invalid request size or boundary"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}
pub async fn rpc(socket: &Path, request: NodeRequest) -> Result<Value> {
    let timeout = if request.method == "upgrade" {
        Duration::from_secs(60)
    } else if request.method == "status" {
        STATUS_TIMEOUT
    } else {
        Duration::from_secs(30)
    };
    tokio::time::timeout(timeout, async {
        let mut stream = UnixStream::connect(socket)
            .await
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        json_line(&mut stream, &request).await?;
        // Control responses are bounded independently from the streaming backend.
        let mut bytes = vec![];
        BufReader::new(stream)
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(Error::Protocol("control response too large"));
        }
        let response: Value = serde_json::from_slice(&bytes)?;
        if response["ok"] != true {
            return Err(Error::Remote(
                response["error"]
                    .as_str()
                    .unwrap_or("request rejected")
                    .into(),
            ));
        }
        Ok(response["result"].clone())
    })
    .await
    .map_err(|_| Error::Protocol("node request timed out"))?
}

async fn serve(config: Config) -> Result<()> {
    serve_runtime(config, None).await
}
async fn serve_runtime(mut config: Config, activated: Option<(PathBuf, bool)>) -> Result<()> {
    let directory = config
        .socket
        .parent()
        .ok_or(Error::Config("socket has no parent"))?;
    let existed = directory.exists();
    fs::create_dir_all(directory)?;
    if fs::symlink_metadata(directory)?.file_type().is_symlink() {
        return Err(Error::Config("socket directory is a symlink"));
    }
    if existed && fs::metadata(directory)?.permissions().mode() & 0o077 != 0 {
        return Err(Error::Config("socket directory must already be private"));
    }
    if !existed {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    let _lock = if activated.is_none() {
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(directory.join("node.lock"))?;
        file.try_lock()
            .map_err(|_| Error::Config("node already running"))?;
        Some(NodeLock(file))
    } else {
        None
    };
    if activated.is_none()
        && let Ok(meta) = fs::symlink_metadata(&config.socket)
    {
        if !meta.file_type().is_socket() {
            return Err(Error::Config("socket path is not a socket"));
        }
        fs::remove_file(&config.socket)?;
    }
    if activated.is_some() {
        let source = format!(
            "{}{}{}{}{}",
            String::from_utf8_lossy(kite_agent::composer::BACKEND_SOURCE),
            include_str!("../../../../backend/kite_push.py"),
            desktop::SOURCE,
            harness::SOURCES
                .iter()
                .map(|(_, source)| *source)
                .collect::<String>(),
            String::from_utf8_lossy(resources::SOURCE),
        );
        let generation = config
            .backend
            .parent()
            .ok_or(Error::Config("missing adapter directory"))?
            .join("adapters")
            .join(kite_agent::fingerprint(&source));
        config.backend = generation.join("backend-v5.py");
    }
    if let Some(parent) = config.backend.parent() {
        fs::create_dir_all(parent)?;
    }
    let pending = config.backend.with_extension("pending");
    fs::write(&pending, kite_agent::composer::BACKEND_SOURCE)?;
    fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))?;
    fs::rename(pending, &config.backend)?;
    for (name, source) in harness::SOURCES {
        let target = config.backend.with_file_name(name);
        let pending = target.with_extension("pending");
        fs::write(&pending, source)?;
        fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))?;
        fs::rename(pending, target)?;
    }
    let push_backend = config.backend.with_file_name("push-v1.py");
    let pending = push_backend.with_extension("pending");
    fs::write(&pending, include_str!("../../../../backend/kite_push.py"))?;
    fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))?;
    fs::rename(pending, push_backend)?;
    let desktop_backend = config.backend.with_file_name("desktop-v1.py");
    let pending = desktop_backend.with_extension("pending");
    fs::write(&pending, desktop::SOURCE)?;
    fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))?;
    fs::rename(pending, desktop_backend)?;
    let resource_backend = config.backend.with_file_name("kite_resources.py");
    let pending = resource_backend.with_extension("pending");
    fs::write(&pending, resources::SOURCE)?;
    fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))?;
    fs::rename(pending, resource_backend)?;
    let listener = if activated.is_some() {
        upgrade::inherited_listener(&config.socket)?
    } else {
        let listener = UnixListener::bind(&config.socket)?;
        fs::set_permissions(&config.socket, fs::Permissions::from_mode(0o600))?;
        listener
    };
    let handover = activated
        .as_ref()
        .map(|(path, _)| upgrade::Handover::new(path, &listener))
        .transpose()?;
    let (journal, notification_problem) =
        match notifications::Journal::open(directory, &config.node_id) {
            Ok(journal) => (Some(journal), None),
            Err(error) => (None, Some(error.to_string())),
        };
    let (push, push_problem) = match push::Push::open(directory) {
        Ok(push) => (Some(push), None),
        Err(error) => (None, Some(error.to_string())),
    };
    let resources = resources::Resources::open(directory, config.agent_memory_limit_bytes)?;
    let node = Arc::new(Node {
        config,
        started: Instant::now(),
        requests: AtomicU64::new(0),
        backend_calls: AtomicU64::new(0),
        lifecycle_last_scan: AtomicU64::new(0),
        lifecycle_reclaimed: AtomicU64::new(0),
        lifecycle_scan: Mutex::new(()),
        lifecycle_wake: tokio::sync::Notify::new(),
        resources,
        permits: Arc::new(Semaphore::new(64)),
        notifications: journal,
        push,
        notification_problem,
        push_problem,
        services: Mutex::new(None),
        upgrade: handover,
        observation_gate: RwLock::new(()),
    });
    if let Some((_, replacement)) = activated {
        if replacement {
            upgrade::activate_replacement(&node).await?;
        } else {
            upgrade::notify(&node.config, std::process::id()).await?;
        }
    }
    let _notification_watch = notifications::start(&node);
    let _lifecycle_watch = lifecycle::start(&node);
    let _resource_watch = resources::start(&node);
    let _push_watch = push::start(&node);
    let mut connections = JoinSet::new();
    loop {
        let accepted = tokio::select! {
            biased;
            _ = async {
                if let Some(upgrade) = &node.upgrade { upgrade.drain.notified().await }
                else { std::future::pending::<()>().await }
            } => break,
            _ = connections.join_next(), if !connections.is_empty() => continue,
            accepted = listener.accept() => accepted?,
        };
        let (stream, _) = accepted;
        let permit = match node.permits.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let node = node.clone();
        connections.spawn(async move {
            let _permit = permit;
            if let Err(error) = node.connection(stream).await {
                eprintln!("node request: {error}");
            }
        });
    }
    drop(listener);
    drop(_notification_watch);
    drop(_push_watch);
    // Existing bridges and in-flight operations finish in their own generation.
    while connections.join_next().await.is_some() {}
    Ok(())
}

impl Node {
    fn draining(&self) -> bool {
        self.upgrade
            .as_ref()
            .is_some_and(|u| u.draining.load(Ordering::Acquire))
    }
    fn notification_journal(&self) -> Result<&notifications::Journal> {
        self.notifications.as_ref().ok_or(Error::Protocol(
            "notification journal unavailable; existing file preserved",
        ))
    }
    fn push_registry(&self) -> Result<&push::Push> {
        self.push.as_ref().ok_or(Error::Protocol(
            "push registry unavailable; existing file preserved",
        ))
    }
    async fn connection(&self, stream: UnixStream) -> Result<()> {
        let (read, mut write) = stream.into_split();
        let mut read = BufReader::new(read);
        let value = tokio::time::timeout(Duration::from_secs(10), read_json(&mut read))
            .await
            .map_err(|_| Error::Protocol("request header timed out"))??;
        let request: NodeRequest = serde_json::from_value(value)?;
        self.requests.fetch_add(1, Ordering::Relaxed);
        if request.method == "backend" || request.method == "herdr-api" {
            let command = self.command(&request);
            match command {
                Ok(command) => {
                    self.backend_calls.fetch_add(1, Ordering::Relaxed);
                    json_line(&mut write, &json!({"ok":true})).await?;
                    let result = bridge::server(read, write, command).await;
                    if result.is_ok()
                        && request.args.first().is_some_and(|action| {
                            action == "catalog-write" || action == "session-facts"
                        })
                    {
                        self.lifecycle_wake.notify_one();
                    }
                    result
                }
                Err(error) => {
                    json_line(&mut write, &json!({"ok":false,"error":error.to_string()})).await
                }
            }
        } else {
            let result = self.control(&request).await;
            let response = match result {
                Ok(v) => json!({"ok":true,"result":v}),
                Err(e) => json!({"ok":false,"error":e.to_string()}),
            };
            json_line(&mut write, &response).await
        }
    }
    fn command(&self, request: &NodeRequest) -> Result<Command> {
        validate_args(&self.config, request)?;
        let mut command = if request.method == "herdr-api" {
            let mut c = Command::new(&self.config.herdr);
            c.arg("remote-api-bridge");
            c
        } else {
            let mut c = Command::new(&self.config.python);
            c.arg(&self.config.backend).args(&request.args);
            c
        };
        command.envs(&self.config.environment);
        command
            .env("KITE_AGENT_RESOURCE_CONTROL", "1")
            .env("KITE_CODEX_BACKEND_PATH", &self.config.backend);
        Ok(command)
    }
    async fn control(&self, request: &NodeRequest) -> Result<Value> {
        let _registration = if matches!(
            request.method.as_str(),
            "push-register"
                | "resource-admit"
                | "resource-ready"
                | "resource-limit"
                | "sessions-release-archived"
        ) {
            let gate = self.observation_gate.read().await;
            if self.draining() {
                return Box::pin(rpc(&self.config.socket, request.clone())).await;
            }
            Some(gate)
        } else {
            None
        };
        match request.method.as_str() {
            "sessions-release-archived" => Ok(serde_json::to_value(lifecycle::scan(self).await?)?),
            "resources" => Ok(serde_json::to_value(self.resources.profile(self).await)?),
            "resource-limit" => Ok(serde_json::to_value(
                self.resources
                    .set_limit(
                        self,
                        request.params["limit_bytes"]
                            .as_u64()
                            .ok_or(Error::Protocol("memory limit required"))?,
                    )
                    .await?,
            )?),
            "resource-admit" => {
                self.resources
                    .admit(
                        self,
                        request.params["pane"].as_str().unwrap_or_default(),
                        u32::try_from(request.params["pid"].as_u64().unwrap_or(0))
                            .map_err(|_| Error::Protocol("invalid resource pid"))?,
                    )
                    .await
            }
            "resource-ready" => {
                self.resources
                    .ready(
                        u32::try_from(request.params["pid"].as_u64().unwrap_or(0))
                            .map_err(|_| Error::Protocol("invalid resource pid"))?,
                    )
                    .await
            }
            "desktop" => {
                desktop::execute(&self.config, desktop::parse(request.params.clone())?).await
            }
            "upgrade" => upgrade::handover(self, request).await,
            "push-register" => self.push_registry()?.register(
                &request.params,
                self.notification_journal()?.page(None)?.cursor,
                self.config.push_service_account.is_some(),
            ),
            "notification-get" => Ok(serde_json::to_value(
                self.notification_journal()?
                    .event(request.params["id"].as_str().unwrap_or_default())?,
            )?),
            "notifications" => Ok(serde_json::to_value(
                self.notification_journal()?
                    .page(request.params["after"].as_str())?,
            )?),
            "status" => {
                let services = self.services(request.params["refresh"] == true).await;
                let health = if services.iter().all(|service| service["active"] == true) {
                    "ready"
                } else {
                    "degraded"
                };
                Ok(
                    json!({"api":API,"backendProtocol":5,"harnessApi":2,"version":version(),"health":health,"nodeId":self.config.node_id,"machine":self.config.machine,"pid":std::process::id(),"uptimeSeconds":self.started.elapsed().as_secs(),"requests":self.requests.load(Ordering::Relaxed),"backendCalls":self.backend_calls.load(Ordering::Relaxed),"liveUpgrade":self.upgrade.is_some(),"resourceApi":1,"sessionLifecycle":{"archivedOnly":true,"scanIntervalSeconds":30,"lastScanMs":self.lifecycle_last_scan.load(Ordering::Relaxed),"reclaimed":self.lifecycle_reclaimed.load(Ordering::Relaxed)},"notificationApi":1,"notificationLastScanMs":self.notifications.as_ref().map(|journal|journal.last_scan.load(Ordering::Relaxed)),"notificationProblem":self.notification_problem,"push":self.push.as_ref().map(|push|push.status(self.config.push_service_account.is_some())),"pushProblem":self.push_problem,"services":services}),
                )
            }
            "sessions" => {
                let mut c = Command::new(&self.config.python);
                c.arg(&self.config.backend)
                    .arg("catalog-read")
                    .envs(&self.config.environment);
                let out = bounded_output(c).await?;
                Ok(serde_json::from_slice(&out)?)
            }
            "ensure" => {
                let name = request.params["service"].as_str().unwrap_or("all");
                if name != "all" && !self.config.services.iter().any(|s| s.name == name) {
                    return Err(Error::Config("unknown service"));
                }
                let services = self.services(true).await;
                let result = tokio::time::timeout(Duration::from_secs(20), async {
                    for (service, state) in self
                        .config
                        .services
                        .iter()
                        .zip(&services)
                        .filter(|(s, _)| name == "all" || s.name == name)
                    {
                        if state["active"] == true {
                            continue;
                        }
                        let mut c = Command::new(&self.config.systemctl);
                        c.args(["--user", "start", &service.unit]);
                        bounded_output(c).await?;
                    }
                    Ok::<(), Error>(())
                })
                .await;
                // A start may change service state even if the caller sees a timeout.
                *self.services.lock().await = None;
                result.map_err(|_| Error::Protocol("service startup timed out"))??;
                Ok(json!({"requested":name}))
            }
            _ => Err(Error::Protocol("unknown node method")),
        }
    }
    async fn services(&self, refresh: bool) -> Vec<Value> {
        // Hold this lock across the bounded probe so concurrent panels/MCP clients
        // share one inspection instead of spawning duplicate systemctl processes.
        let requested_at = Instant::now();
        let mut cache = self.services.lock().await;
        if let Some((at, services)) = &*cache
            && (*at >= requested_at || (!refresh && at.elapsed() < SERVICE_CACHE_TTL))
        {
            return services.clone();
        }
        let services = self.inspect_services().await;
        *cache = Some((Instant::now(), services.clone()));
        services
    }

    async fn inspect_services(&self) -> Vec<Value> {
        if self.config.services.is_empty() {
            return vec![];
        }
        let mut c = Command::new(&self.config.systemctl);
        c.args(["--user", "show"]);
        c.args(self.config.services.iter().map(|service| &service.unit));
        c.args([
            "--property=Id",
            "--property=Names",
            "--property=ActiveState",
            "--property=MainPID",
        ]);
        let result = bounded_output_with_timeout(c, SERVICE_TIMEOUT).await;
        match result {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                self.config
                    .services
                    .iter()
                    .map(|service| {
                        let block = text.split("\n\n").find(|block| {
                            block.lines().any(|line| {
                                line.strip_prefix("Id=") == Some(service.unit.as_str())
                                    || line.strip_prefix("Names=").is_some_and(|names| {
                                        names.split_whitespace().any(|name| name == service.unit)
                                    })
                            })
                        });
                        match block {
                            Some(block) => {
                                let field = |key: &str| {
                                    block
                                        .lines()
                                        .find_map(|l| l.strip_prefix(key))
                                        .unwrap_or("")
                                };
                                let mut value = service_state(service);
                                value["active"] = json!(field("ActiveState=") == "active");
                                value["pid"] = json!(field("MainPID=").parse::<u32>().unwrap_or(0));
                                if field("ActiveState=").is_empty() {
                                    value["error"] = json!("service state missing");
                                }
                                value
                            }
                            None => {
                                let mut value = service_state(service);
                                value["error"] = json!("service not returned by systemctl");
                                value
                            }
                        }
                    })
                    .collect()
            }
            Err(error) => self
                .config
                .services
                .iter()
                .map(|service| {
                    let mut value = service_state(service);
                    value["error"] = json!(error.to_string());
                    value
                })
                .collect(),
        }
    }
}
fn service_state(service: &Service) -> Value {
    json!({"name":service.name,"unit":service.unit,"active":false,"pid":0,"ownership":if service.external{"external"}else{"kite"}})
}
fn validate_args(config: &Config, request: &NodeRequest) -> Result<()> {
    if request.args.len() > 32
        || request.args.iter().map(String::len).sum::<usize>() > 8192
        || request.args.iter().any(|s| s.contains('\0'))
    {
        return Err(Error::Protocol("invalid argument size"));
    }
    if request.method == "backend" {
        if !matches!(
            request.args.first().map(String::as_str),
            Some(
                "ensure"
                    | "proxy"
                    | "upgrade"
                    | "upload"
                    | "bind"
                    | "identity"
                    | "tui"
                    | "history"
                    | "catalog-read"
                    | "catalog-write"
                    | "session-open"
                    | "session-facts"
                    | "image-read"
                    | "attachment-read"
                    | "harness-scan"
                    | "harness-history"
            )
        ) {
            return Err(Error::Protocol("backend action denied"));
        }
        let mut i = 1;
        while i < request.args.len() {
            let flag = request.args[i].as_str();
            if flag == "--compressed" {
                i += 1;
                continue;
            }
            if !matches!(
                flag,
                "--pane"
                    | "--herdr"
                    | "--thread"
                    | "--file"
                    | "--before"
                    | "--after"
                    | "--id"
                    | "--name"
                    | "--size"
                    | "--sha256"
            ) || i + 1 >= request.args.len()
            {
                return Err(Error::Protocol("backend option denied"));
            }
            if flag == "--herdr"
                && fs::canonicalize(&request.args[i + 1])? != fs::canonicalize(&config.herdr)?
            {
                return Err(Error::Protocol("unconfigured herdr binary"));
            }
            i += 2;
        }
    } else if request.method == "herdr-api" {
        if request.args.len() != 2
            || request.args[0] != "--herdr"
            || fs::canonicalize(&request.args[1])? != fs::canonicalize(&config.herdr)?
        {
            return Err(Error::Protocol("unconfigured herdr binary"));
        }
    } else {
        return Err(Error::Protocol("invalid bridge method"));
    }
    Ok(())
}
async fn bounded_output(command: Command) -> Result<Vec<u8>> {
    bounded_output_with_timeout(command, Duration::from_secs(15)).await
}
async fn bounded_output_with_timeout(mut command: Command, timeout: Duration) -> Result<Vec<u8>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    tokio::time::timeout(timeout, async {
        let mut child = command.spawn()?;
        let mut bytes = vec![];
        child
            .stdout
            .take()
            .ok_or(Error::Protocol("missing stdout"))?
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err(Error::Protocol("process output too large"));
        }
        let status = child.wait().await?;
        if !status.success() {
            return Err(Error::Remote(format!("process exit {:?}", status.code())));
        }
        Ok(bytes)
    })
    .await
    .map_err(|_| Error::Protocol("process timed out"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backend_argument_values_cannot_become_global_socket_options() {
        let mut args = vec![
            "backend".into(),
            "upload".into(),
            "--name".into(),
            "--socket".into(),
        ];
        assert!(leading_option(&mut args, "--socket").unwrap().is_none());
        assert_eq!(args[3], "--socket");
        let mut args = vec![
            "--socket".into(),
            "/private/node.sock".into(),
            "status".into(),
        ];
        assert_eq!(
            leading_option(&mut args, "--socket").unwrap().as_deref(),
            Some("/private/node.sock")
        );
        assert_eq!(args, ["status"]);
    }
    #[test]
    fn ply_configuration_accepts_its_opt_in_environment_without_opening_other_keys() {
        let root = tempfile::tempdir().unwrap();
        let mut value = config(root.path());
        for (key, content) in [
            ("KITE_ENABLE_PLY", "1"),
            (
                "KITE_PLY_COMMAND",
                r#"["/usr/bin/ply","mcp","--host-chat"]"#,
            ),
            ("KITE_PLY_PROVIDER", "xiaomi-token-plan-cn"),
            ("KITE_PLY_MODEL", "mimo-v2.6-pro"),
            ("KITE_PLY_HEALTH_URL", "https://example.test/v1/models"),
        ] {
            value.environment.insert(key.into(), content.into());
        }
        value.validate().unwrap();
        value
            .environment
            .insert("NODE_OPTIONS".into(), "unrelated".into());
        assert!(matches!(
            value.validate(),
            Err(Error::Config("unsupported environment key"))
        ));
    }

    pub(super) fn config(root: &Path) -> Config {
        Config {
            node_id: "controlled-node".into(),
            machine: "fixture".into(),
            socket: root.join("node/host.sock"),
            backend: root.join("adapter.py"),
            herdr: PathBuf::from("/bin/sh"),
            python: PathBuf::from("/usr/bin/python3"),
            systemctl: PathBuf::from("/usr/bin/false"),
            agent_memory_limit_bytes: crate::resources::DEFAULT_AGENT_MEMORY_LIMIT,
            source_path: None,
            push_service_account: None,
            services: vec![],
            environment: BTreeMap::new(),
        }
    }
    #[test]
    fn untrusted_bridge_requests_cannot_choose_executables_or_options() {
        let root = tempfile::tempdir().unwrap();
        let c = config(root.path());
        for args in [
            vec!["shell"],
            vec!["history", "--exec", "bad"],
            vec!["proxy", "--herdr", "/bin/ls"],
        ] {
            let request = NodeRequest {
                method: "backend".into(),
                args: args.into_iter().map(String::from).collect(),
                params: json!({}),
            };
            assert!(validate_args(&c, &request).is_err());
        }
        let request = NodeRequest {
            method: "backend".into(),
            args: vec![
                "history".into(),
                "--file".into(),
                "/a path/with 'quotes'".into(),
                "--compressed".into(),
            ],
            params: json!({}),
        };
        validate_args(&c, &request).unwrap();
    }
    #[test]
    fn session_facts_and_image_downloads_are_allowed_through_the_host_bridge() {
        let root = tempfile::tempdir().unwrap();
        let config = config(root.path());
        for args in [
            vec!["session-facts".into(), "--herdr".into(), "/bin/sh".into()],
            vec!["image-read".into(), "--id".into(), "a".repeat(64)],
            vec!["attachment-read".into(), "--id".into(), "b".repeat(32)],
        ] {
            validate_args(
                &config,
                &NodeRequest {
                    method: "backend".into(),
                    args,
                    params: json!({}),
                },
            )
            .unwrap();
        }
    }

    #[tokio::test]
    async fn image_download_crosses_the_real_node_socket_as_complete_binary_frames() {
        let root = tempfile::tempdir().unwrap();
        let c = config(root.path());
        let path = c.socket.clone();
        let backend = c.backend.clone();
        let task = tokio::spawn(serve(c));
        for _ in 0..100 {
            if path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // Only this owned adapter fixture is replaced; no production node is used.
        fs::write(
            &backend,
            "import sys\nsys.stdout.buffer.write(b'\\x89PNG\\r\\n\\x1a\\n' + bytes(6*1024*1024))\n",
        )
        .unwrap();
        let stream = UnixStream::connect(&path).await.unwrap();
        let (read, mut write) = stream.into_split();
        let request = NodeRequest {
            method: "backend".into(),
            args: vec!["image-read".into(), "--id".into(), "a".repeat(64)],
            params: json!({}),
        };
        json_line(&mut write, &request).await.unwrap();
        let mut read = tokio::io::BufReader::new(read);
        assert_eq!(read_json(&mut read).await.unwrap()["ok"], true);
        wire::write(&mut write, wire::EOF, &[]).await.unwrap();
        let mut bytes = Vec::new();
        loop {
            let (kind, data) = wire::read(&mut read).await.unwrap().unwrap();
            match kind {
                wire::OUTPUT => bytes.extend_from_slice(&data),
                wire::EXIT => {
                    assert_eq!(data, 0_i32.to_be_bytes());
                    break;
                }
                _ => panic!("unexpected image frame"),
            }
        }
        assert_eq!(bytes.len(), 6 * 1024 * 1024 + 8);
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert!(bytes[8..].iter().all(|b| *b == 0));
        task.abort();
        let _ = task.await;
    }

    #[test]
    fn service_units_and_environment_are_declarative() {
        let root = tempfile::tempdir().unwrap();
        let mut c = config(root.path());
        c.services.push(Service {
            name: "bad".into(),
            unit: "--host=remote.service".into(),
            external: true,
        });
        assert!(c.validate().is_err());
        c.services.clear();
        c.environment
            .insert("LD_PRELOAD".into(), "untrusted".into());
        assert!(c.validate().is_err());
    }
    #[tokio::test]
    async fn status_health_tracks_a_stopped_service_and_its_recovery() {
        let root = tempfile::tempdir().unwrap();
        let mut c = config(root.path());
        c.services = ["Herdr", "Sunshine"]
            .into_iter()
            .map(|name| Service {
                name: name.into(),
                unit: format!("{name}.service"),
                external: true,
            })
            .collect();
        c.systemctl = root.path().join("systemctl");
        fs::write(
            &c.systemctl,
            b"#!/usr/bin/python3\nimport sys\nfrom pathlib import Path\nroot = Path(__file__).parent\nwith (root/'calls').open('a') as calls: calls.write('probe\\n')\nfor unit in sys.argv[3:]:\n if unit.startswith('--'): continue\n active = unit == 'Herdr.service' or (root/'recovered').exists()\n print('Id=' + unit)\n print('ActiveState=' + ('active' if active else 'inactive'))\n print('MainPID=' + ('123' if active else '0'))\n print()\n",
        )
        .unwrap();
        fs::set_permissions(&c.systemctl, fs::Permissions::from_mode(0o700)).unwrap();
        let path = c.socket.clone();
        let task = tokio::spawn(serve(c));
        for _ in 0..100 {
            if path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let status = rpc(&path, NodeRequest::new("status")).await.unwrap();
        assert_eq!(status["health"], "degraded");
        assert_eq!(status["services"][0]["active"], true);
        assert_eq!(status["services"][1]["active"], false);
        let (one, two) = tokio::join!(
            rpc(&path, NodeRequest::new("status")),
            rpc(&path, NodeRequest::new("status"))
        );
        assert_eq!(one.unwrap()["health"], "degraded");
        assert_eq!(two.unwrap()["health"], "degraded");
        assert_eq!(
            fs::read_to_string(root.path().join("calls"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        fs::write(root.path().join("recovered"), b"").unwrap();
        let mut refresh = NodeRequest::new("status");
        refresh.params = json!({"refresh": true});
        let status = rpc(&path, refresh).await.unwrap();
        assert_eq!(status["health"], "ready");
        assert_eq!(status["services"][1]["active"], true);
        task.abort();
        let _ = task.await;
    }
    #[tokio::test]
    async fn slow_service_inspection_returns_unknown_before_the_status_deadline() {
        let root = tempfile::tempdir().unwrap();
        let mut c = config(root.path());
        c.services = vec![Service {
            name: "Slow".into(),
            unit: "slow.service".into(),
            external: true,
        }];
        c.systemctl = root.path().join("systemctl");
        fs::write(
            &c.systemctl,
            "#!/usr/bin/python3\nimport time\ntime.sleep(10)\n",
        )
        .unwrap();
        fs::set_permissions(&c.systemctl, fs::Permissions::from_mode(0o700)).unwrap();
        let path = c.socket.clone();
        let task = tokio::spawn(serve(c));
        for _ in 0..100 {
            if path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let start = Instant::now();
        let mut refresh = NodeRequest::new("status");
        refresh.params = json!({"refresh": true});
        let (one, two) = tokio::join!(rpc(&path, refresh.clone()), rpc(&path, refresh));
        let status = one.unwrap();
        assert_eq!(two.unwrap()["health"], "degraded");
        assert!(start.elapsed() < STATUS_TIMEOUT);
        assert_eq!(status["health"], "degraded");
        assert!(
            status["services"][0]["error"]
                .as_str()
                .unwrap()
                .contains("timed out")
        );
        task.abort();
        let _ = task.await;
    }

    #[tokio::test]
    async fn service_start_invalidates_the_cache_and_resolves_unit_aliases() {
        let root = tempfile::tempdir().unwrap();
        let mut c = config(root.path());
        c.services = vec![Service {
            name: "Alias".into(),
            unit: "alias.service".into(),
            external: true,
        }];
        c.systemctl = root.path().join("systemctl");
        fs::write(&c.systemctl, "#!/usr/bin/python3\nimport sys\nfrom pathlib import Path\nstate=Path(__file__).with_name('active')\nif sys.argv[2]=='start': state.touch()\nelse: print('Id=real.service\\nNames=real.service alias.service\\nActiveState=' + ('active' if state.exists() else 'inactive') + '\\nMainPID=123\\n')\n").unwrap();
        fs::set_permissions(&c.systemctl, fs::Permissions::from_mode(0o700)).unwrap();
        let path = c.socket.clone();
        let task = tokio::spawn(serve(c));
        for _ in 0..100 {
            if path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let before = rpc(&path, NodeRequest::new("status")).await.unwrap();
        assert_eq!(before["health"], "degraded");
        assert!(before["services"][0].get("error").is_none());
        rpc(&path, NodeRequest::new("ensure")).await.unwrap();
        let after = rpc(&path, NodeRequest::new("status")).await.unwrap();
        assert_eq!(after["health"], "ready");
        task.abort();
        let _ = task.await;
    }
    #[tokio::test]
    async fn private_socket_has_one_owner_and_recovers_after_listener_exit() {
        let root = tempfile::tempdir().unwrap();
        let c = config(root.path());
        let path = c.socket.clone();
        let first = tokio::spawn(serve(c.clone()));
        for _ in 0..100 {
            if path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let status = rpc(&path, NodeRequest::new("status")).await.unwrap();
        assert_eq!(status["nodeId"], "controlled-node");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(matches!(
            serve(c.clone()).await,
            Err(Error::Config("node already running"))
        ));
        assert_eq!(
            rpc(&path, NodeRequest::new("status")).await.unwrap()["api"],
            API
        );
        first.abort();
        let _ = first.await;
        let mut second = tokio::spawn(serve(c));
        let recovered = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if second.is_finished() {
                    panic!(
                        "replacement listener exited before readiness: {:?}",
                        (&mut second).await
                    );
                }
                if let Ok(status) = rpc(&path, NodeRequest::new("status")).await {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("replacement listener did not become ready");
        assert_eq!(recovered["api"], API);
        second.abort();
        let _ = second.await;
    }
}
