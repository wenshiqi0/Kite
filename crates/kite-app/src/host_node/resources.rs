//! Host sampling, persistent startup reservations and the configurable agent budget.
use super::*;
use crate::resources::{GIB, HostResources, RawResources, ResourceTracker, STARTUP_RESERVATION};
use std::{fs::OpenOptions, io::Write, os::unix::fs::MetadataExt};
use tokio::task::JoinHandle;

pub(super) const SOURCE: &[u8] = include_bytes!("../../../../backend/kite_resources.py");

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Reservation {
    pid: u32,
    started: String,
    pane: String,
    bytes: u64,
    expires_ms: u64,
}

struct State {
    limit: u64,
    configured: bool,
    configured_ms: u64,
    tracker: ResourceTracker,
    profile: HostResources,
    reservations: Vec<Reservation>,
    problem: Option<String>,
}

pub(super) struct Resources {
    state: Mutex<State>,
    reservation_file: PathBuf,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn identity(pid: u32) -> Option<String> {
    let directory = PathBuf::from(format!("/proc/{pid}"));
    if fs::metadata(&directory).ok()?.uid() != fs::metadata("/proc/self").ok()?.uid() {
        return None;
    }
    let stat = fs::read_to_string(directory.join("stat")).ok()?;
    stat.rsplit_once(") ")?
        .1
        .split_whitespace()
        .nth(19)
        .map(str::to_owned)
}

fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let pending = path.with_extension(format!("resource-{}.pending", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&pending)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(pending, path)?;
    if let Some(parent) = path.parent() {
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

async fn platform(node: &Node, action: &str, input: Option<Value>) -> Result<Vec<u8>> {
    let helper = node.config.backend.with_file_name("kite_resources.py");
    let mut command = Command::new(&node.config.python);
    command
        .arg(helper)
        .arg(action)
        .args(["--herdr"])
        .arg(&node.config.herdr)
        .envs(&node.config.environment)
        .env("KITE_CODEX_BACKEND_PATH", &node.config.backend);
    if let Some(input) = input {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        tokio::time::timeout(Duration::from_secs(30), async {
            let mut child = command.spawn()?;
            let mut stdin = child
                .stdin
                .take()
                .ok_or(Error::Protocol("missing resource input"))?;
            stdin.write_all(&serde_json::to_vec(&input)?).await?;
            drop(stdin);
            let output = child.wait_with_output().await?;
            if output.stdout.len() > 1024 * 1024 || output.stderr.len() > 4096 {
                return Err(Error::Protocol("resource output too large"));
            }
            if !output.status.success() {
                return Err(Error::Remote(
                    String::from_utf8_lossy(&output.stderr)
                        .chars()
                        .take(400)
                        .collect(),
                ));
            }
            Ok(output.stdout)
        })
        .await
        .map_err(|_| Error::Protocol("resource configuration timed out"))?
    } else {
        bounded_output(command).await
    }
}

impl Resources {
    pub fn open(directory: &Path, limit: u64) -> Result<Self> {
        let reservation_file = directory.join("resource-reservations.json");
        let reservations = match fs::read(&reservation_file) {
            Ok(bytes) if bytes.len() <= 65536 => {
                serde_json::from_slice::<Vec<Reservation>>(&bytes)?
            }
            Ok(_) => return Err(Error::Protocol("resource reservation file too large")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        if reservations.len() > 128 {
            return Err(Error::Protocol("resource reservation capacity exceeded"));
        }
        Ok(Self {
            reservation_file,
            state: Mutex::new(State {
                limit,
                configured: false,
                configured_ms: 0,
                tracker: ResourceTracker::default(),
                profile: HostResources::default(),
                reservations,
                problem: None,
            }),
        })
    }

    fn save(&self, state: &State) -> Result<()> {
        atomic(
            &self.reservation_file,
            &serde_json::to_vec(&state.reservations)?,
        )
    }

    async fn refresh(&self, node: &Node, state: &mut State) -> Result<()> {
        if node.config.source_path.is_none() {
            return Err(Error::Config(
                "resource control requires a persistent host configuration",
            ));
        }
        let now = now_ms();
        let before = state.reservations.len();
        state.reservations.retain(|item| {
            item.expires_ms > now && identity(item.pid).as_deref() == Some(&item.started)
        });
        if before != state.reservations.len() {
            self.save(state)?;
        }
        if !state.configured || now.saturating_sub(state.configured_ms) >= 30_000 {
            let configure = async {
                platform(node, "orphans", None).await?;
                platform(
                    node,
                    "configure",
                    Some(json!({"limit_bytes":state.limit,"high_bytes":state.limit / 10 * 9})),
                )
                .await?;
                Ok::<(), Error>(())
            }
            .await;
            match configure {
                Ok(()) => {
                    state.configured = true;
                    state.configured_ms = now;
                    state.problem = None;
                }
                Err(error) => {
                    state.configured = false;
                    state.problem = Some(error.to_string());
                }
            }
        }
        let bytes = platform(node, "sample", None).await?;
        let raw: RawResources = serde_json::from_slice(&bytes)?;
        let reserved = state.reservations.iter().map(|item| item.bytes).sum();
        state.profile = state
            .tracker
            .observe(raw, state.limit, reserved, state.configured);
        state.profile.problem = state.problem.clone();
        Ok(())
    }

    pub async fn profile(&self, node: &Node) -> HostResources {
        let mut state = self.state.lock().await;
        if now_ms().saturating_sub(state.profile.sampled_at_ms) > 5000
            && let Err(error) = self.refresh(node, &mut state).await
        {
            state.profile.available = false;
            state.profile.can_start_agents = false;
            state.profile.problem = Some(error.to_string());
        }
        state.profile.clone()
    }

    pub async fn admit(&self, node: &Node, pane: &str, pid: u32) -> Result<Value> {
        let (prefix, suffix) = pane
            .split_once(":p")
            .ok_or(Error::Protocol("invalid resource pane"))?;
        if prefix.is_empty()
            || suffix.is_empty()
            || !prefix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || !suffix.bytes().all(|b| b.is_ascii_digit())
            || pane.len() > 96
        {
            return Err(Error::Protocol("invalid resource pane"));
        }
        let started = identity(pid).ok_or(Error::Protocol("invalid resource process identity"))?;
        let mut state = self.state.lock().await;
        self.refresh(node, &mut state).await?;
        if let Some(existing) = state
            .reservations
            .iter()
            .find(|item| item.pid == pid && item.started == started)
        {
            return Ok(
                json!({"allowed":true,"limit_bytes":state.limit,"high_bytes":state.limit / 10 * 9,"reserved_bytes":existing.bytes}),
            );
        }
        if !state.profile.can_start_agents || state.reservations.len() >= 128 {
            return Ok(json!({"allowed":false,"reason":state.profile.reason}));
        }
        let bytes = STARTUP_RESERVATION.min(state.profile.memory_total_bytes / 4);
        state.reservations.push(Reservation {
            pid,
            started,
            pane: pane.into(),
            bytes,
            expires_ms: now_ms().saturating_add(120_000),
        });
        self.save(&state)?;
        Ok(
            json!({"allowed":true,"limit_bytes":state.limit,"high_bytes":state.limit / 10 * 9,"reserved_bytes":bytes}),
        )
    }

    pub async fn ready(&self, pid: u32) -> Result<Value> {
        let mut state = self.state.lock().await;
        state.reservations.retain(|item| item.pid != pid);
        self.save(&state)?;
        Ok(json!({"released":true}))
    }

    pub async fn set_limit(&self, node: &Node, limit: u64) -> Result<HostResources> {
        let source = node
            .config
            .source_path
            .as_ref()
            .ok_or(Error::Config("resource configuration is not persistent"))?;
        let mut state = self.state.lock().await;
        self.refresh(node, &mut state).await?;
        if limit < GIB / 4
            || limit
                > state
                    .profile
                    .memory_total_bytes
                    .saturating_sub(512 * 1024 * 1024)
        {
            return Err(Error::Protocol("invalid agent memory budget"));
        }
        let committed = state.profile.agent_memory_used_bytes.saturating_add(
            state
                .reservations
                .iter()
                .map(|item| item.bytes)
                .sum::<u64>(),
        );
        if limit < committed.saturating_add(64 * 1024 * 1024) {
            return Err(Error::Protocol(
                "agent budget is below current use and reservations",
            ));
        }
        let mut configuration: Value = serde_json::from_slice(&fs::read(source)?)?;
        configuration["agent_memory_limit_bytes"] = limit.into();
        platform(
            node,
            "configure",
            Some(json!({"limit_bytes":limit,"high_bytes":limit / 10 * 9})),
        )
        .await?;
        state.limit = limit;
        atomic(source, &serde_json::to_vec_pretty(&configuration)?)?;
        state.configured = true;
        state.configured_ms = now_ms();
        self.refresh(node, &mut state).await?;
        Ok(state.profile.clone())
    }
}

pub(super) struct Watch(JoinHandle<()>);
impl Drop for Watch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) fn start(node: &Arc<Node>) -> Watch {
    let weak = Arc::downgrade(node);
    Watch(tokio::spawn(async move {
        loop {
            let Some(node) = weak.upgrade() else { return };
            let gate = node.observation_gate.read().await;
            if node.draining() {
                return;
            }
            {
                let mut state = node.resources.state.lock().await;
                if let Err(error) = node.resources.refresh(&node, &mut state).await {
                    state.profile.available = false;
                    state.profile.can_start_agents = false;
                    state.profile.problem = Some(error.to_string());
                }
            }
            drop(gate);
            drop(node);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }))
}
