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
    sync::Semaphore,
};

mod bridge;
mod mcp;
mod wire;
pub type Result<T> = std::result::Result<T, Error>;
pub const API: u64 = 1;
const LIMIT: usize = 16 * 1024;

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
    pub services: Vec<Service>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}
impl Config {
    fn validate(&self) -> Result<()> {
        if self.node_id.is_empty()
            || self.machine.is_empty()
            || !self.socket.is_absolute()
            || !self.backend.is_absolute()
            || !self.herdr.is_absolute()
            || !self.python.is_absolute()
            || !self.systemctl.is_absolute()
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
    permits: Arc<Semaphore>,
}

pub fn run(mut args: Vec<String>) -> Result<i32> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()?;
    let result = runtime.block_on(async {
        let socket = leading_option(&mut args, "--socket")?
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("KITE_NODE_SOCKET").map(PathBuf::from))
            .unwrap_or(default_path(".local/state/kite/node/host.sock")?);
        let command = args.first().map(String::as_str).unwrap_or("status");
        match command {
            "version" | "--version" => {
                println!(
                    "{}",
                    json!({"version":version(),"api":API,"backendProtocol":5})
                );
                Ok(0)
            }
            "serve" => {
                let config = if args.len() == 3 && args[1] == "--config" {
                    PathBuf::from(&args[2])
                } else if args.len() == 1 {
                    default_path(".config/kite/node.json")?
                } else {
                    return Err(Error::Config("serve accepts only --config PATH"));
                };
                let data: Config = serde_json::from_slice(&fs::read(config)?)?;
                data.validate()?;
                serve(data).await?;
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
            "mcp" => {
                mcp::serve(&socket).await?;
                Ok(0)
            }
            "status" | "api-version" | "sessions" | "ensure" => {
                let method = if command == "api-version" {
                    "status"
                } else {
                    command
                };
                let mut request = NodeRequest::new(method);
                if command == "ensure" {
                    request.params =
                        json!({"service":args.get(1).map(String::as_str).unwrap_or("all")});
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
    let mut bytes = vec![];
    let n = input
        .take((LIMIT + 1) as u64)
        .read_until(b'\n', &mut bytes)
        .await?;
    if n == 0 || n > LIMIT || bytes.last() != Some(&b'\n') {
        return Err(Error::Protocol("invalid request size or boundary"));
    }
    Ok(serde_json::from_slice(&bytes)?)
}
pub async fn rpc(socket: &Path, request: NodeRequest) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(30), async {
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
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(directory.join("node.lock"))?;
    lock.try_lock()
        .map_err(|_| Error::Config("node already running"))?;
    if let Ok(meta) = fs::symlink_metadata(&config.socket) {
        if !meta.file_type().is_socket() {
            return Err(Error::Config("socket path is not a socket"));
        }
        fs::remove_file(&config.socket)?;
    }
    if let Some(parent) = config.backend.parent() {
        fs::create_dir_all(parent)?;
    }
    let pending = config.backend.with_extension("pending");
    fs::write(&pending, kite_agent::composer::BACKEND_SOURCE)?;
    fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))?;
    fs::rename(pending, &config.backend)?;
    let listener = UnixListener::bind(&config.socket)?;
    fs::set_permissions(&config.socket, fs::Permissions::from_mode(0o600))?;
    let node = Arc::new(Node {
        config,
        started: Instant::now(),
        requests: AtomicU64::new(0),
        backend_calls: AtomicU64::new(0),
        permits: Arc::new(Semaphore::new(64)),
    });
    loop {
        let (stream, _) = listener.accept().await?;
        let permit = match node.permits.clone().try_acquire_owned() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let node = node.clone();
        tokio::spawn(async move {
            let _permit = permit;
            if let Err(error) = node.connection(stream).await {
                eprintln!("node request: {error}");
            }
        });
    }
}

impl Node {
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
                    bridge::server(read, write, command).await
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
        Ok(command)
    }
    async fn control(&self, request: &NodeRequest) -> Result<Value> {
        match request.method.as_str() {
            "status" => {
                let mut services = vec![];
                for service in &self.config.services {
                    services.push(self.service(service).await);
                }
                let health = if services.iter().all(|service| service["active"] == true) {
                    "ready"
                } else {
                    "degraded"
                };
                Ok(
                    json!({"api":API,"backendProtocol":5,"version":version(),"health":health,"nodeId":self.config.node_id,"machine":self.config.machine,"pid":std::process::id(),"uptimeSeconds":self.started.elapsed().as_secs(),"requests":self.requests.load(Ordering::Relaxed),"backendCalls":self.backend_calls.load(Ordering::Relaxed),"services":services}),
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
                for service in self
                    .config
                    .services
                    .iter()
                    .filter(|s| name == "all" || s.name == name)
                {
                    if self.service(service).await["active"] == true {
                        continue;
                    }
                    let mut c = Command::new(&self.config.systemctl);
                    c.args(["--user", "start", &service.unit]);
                    bounded_output(c).await?;
                }
                Ok(json!({"requested":name}))
            }
            _ => Err(Error::Protocol("unknown node method")),
        }
    }
    async fn service(&self, service: &Service) -> Value {
        let mut c = Command::new(&self.config.systemctl);
        c.args([
            "--user",
            "show",
            &service.unit,
            "--property=ActiveState",
            "--property=MainPID",
            "--property=FragmentPath",
        ]);
        match bounded_output(c).await {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes);
                let field = |key: &str| {
                    text.lines()
                        .find_map(|l| l.strip_prefix(key))
                        .unwrap_or("")
                        .to_owned()
                };
                json!({"name":service.name,"unit":service.unit,"active":field("ActiveState=")=="active","pid":field("MainPID=").parse::<u32>().unwrap_or(0),"ownership":if service.external{"external"}else{"kite"}})
            }
            Err(_) => {
                json!({"name":service.name,"unit":service.unit,"active":false,"pid":0,"ownership":if service.external{"external"}else{"kite"}})
            }
        }
    }
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
async fn bounded_output(mut command: Command) -> Result<Vec<u8>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    tokio::time::timeout(Duration::from_secs(15), async {
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
    fn config(root: &Path) -> Config {
        Config {
            node_id: "controlled-node".into(),
            machine: "fixture".into(),
            socket: root.join("node/host.sock"),
            backend: root.join("adapter.py"),
            herdr: PathBuf::from("/bin/sh"),
            python: PathBuf::from("/usr/bin/python3"),
            systemctl: PathBuf::from("/usr/bin/false"),
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
            b"#!/usr/bin/python3\nimport sys\nfrom pathlib import Path\nactive = 'Herdr.service' in sys.argv or Path(__file__).with_name('recovered').exists()\nprint('ActiveState=' + ('active' if active else 'inactive'))\nprint('MainPID=' + ('123' if active else '0'))\n",
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
        fs::write(root.path().join("recovered"), b"").unwrap();
        let status = rpc(&path, NodeRequest::new("status")).await.unwrap();
        assert_eq!(status["health"], "ready");
        assert_eq!(status["services"][1]["active"], true);
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
        let second = tokio::spawn(serve(c));
        for _ in 0..100 {
            if rpc(&path, NodeRequest::new("status")).await.is_ok() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(
            rpc(&path, NodeRequest::new("status")).await.unwrap()["api"],
            API
        );
        second.abort();
        let _ = second.await;
    }
}
