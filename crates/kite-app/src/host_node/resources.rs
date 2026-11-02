//! Host sampling, persistent startup reservations and the configurable agent budget.
use super::*;
use crate::resources::{GIB, HostResources, RawResources, ResourceTracker, STARTUP_RESERVATION};
use std::{fs::OpenOptions, io::Write, os::unix::fs::MetadataExt};
use tokio::task::JoinHandle;

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
    let helper = &node.config.backend;
    let mut command = Command::new(&node.config.bun);
    command
        .arg(helper)
        .arg("resources")
        .arg(action)
        .args(["--herdr"])
        .arg(&node.config.herdr)
        .envs(&node.config.environment)
        .env("KITE_HOST_BACKEND_PATH", &node.config.backend);
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
                    Some(json!({"limit_bytes":state.limit,"high_bytes":state.limit})),
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
        if raw.uncontained_owners > 0 {
            state.configured = false;
            state.problem = Some("managed agent processes remain outside the memory budget".into());
        }
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
        let herdr = pane.split_once(":p").is_some_and(|(prefix, suffix)| {
            !prefix.is_empty()
                && !suffix.is_empty()
                && prefix
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                && suffix.bytes().all(|b| b.is_ascii_digit())
        });
        let harness = pane.strip_prefix("kite-harness-").is_some_and(|value| {
            value.len() == 36
                && value.bytes().enumerate().all(|(i, b)| {
                    if [8, 13, 18, 23].contains(&i) {
                        b == b'-'
                    } else {
                        b.is_ascii_hexdigit()
                    }
                })
        });
        if (!herdr && !harness) || pane.len() > 96 {
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
                json!({"allowed":true,"limit_bytes":state.limit,"high_bytes":state.limit,"reserved_bytes":existing.bytes}),
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
            json!({"allowed":true,"limit_bytes":state.limit,"high_bytes":state.limit,"reserved_bytes":bytes}),
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
            Some(json!({"limit_bytes":limit,"high_bytes":limit})),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path) -> Node {
        let interpreter = root.join("resource-interpreter");
        fs::write(&interpreter, "#!/usr/bin/python3\nimport json,sys,time\nfrom pathlib import Path\nr=Path(__file__).parent\na=sys.argv[3]\np=r/'sample.json'\nv=json.loads(p.read_text())\nif a=='configure':\n q=json.load(sys.stdin);v['agent_effective_limit_bytes']=q['limit_bytes'];p.write_text(json.dumps(v));print('{}')\nelif a=='orphans':print('{}')\nelif a=='sample':v['sampled_at_ms']=int(time.time()*1000);print(json.dumps(v))\nelse:raise AssertionError(a)\n").unwrap();
        fs::set_permissions(&interpreter, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join("sample.json"),serde_json::to_vec(&json!({
            "sampled_at_ms":1000,"cpu_total_ticks":100,"cpu_idle_ticks":50,"cpu_cores":4,"load_average":[0.0,0.0,0.0],
            "memory_total_bytes":20*GIB,"memory_available_bytes":10*GIB,"swap_total_bytes":GIB,"swap_used_bytes":0,
            "memory_pressure_some":0.0,"memory_pressure_full":0.0,"agent_current_bytes":8*GIB,"agent_reclaimable_bytes":0,
            "agent_reserved_bytes":0,"agent_effective_limit_bytes":10*GIB,"agent_cpu_usage_usec":0,
            "agent_process_count":2,"agent_oom_kills":0,"enforced":true,"uncontained_owners":0,"processes":[]
        })).unwrap()).unwrap();
        let mut config = super::super::tests::config(root);
        config.bun = interpreter;
        config.source_path = Some(root.join("config.json"));
        fs::write(
            config.source_path.as_ref().unwrap(),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        Node {
            config,
            started: Instant::now(),
            requests: AtomicU64::new(0),
            backend_calls: AtomicU64::new(0),
            lifecycle_last_scan: AtomicU64::new(0),
            lifecycle_reclaimed: AtomicU64::new(0),
            lifecycle_scan: Mutex::new(()),
            lifecycle_wake: tokio::sync::Notify::new(),
            resources: Resources::open(root, 10 * GIB).unwrap(),
            permits: Arc::new(Semaphore::new(4)),
            notifications: None,
            push: None,
            notification_problem: None,
            push_problem: None,
            services: Mutex::new(None),
            upgrade: None,
            observation_gate: RwLock::new(()),
        }
    }

    #[tokio::test]
    async fn concurrent_starts_share_persisted_reservations_without_overcommit() {
        let root = tempfile::tempdir().unwrap();
        let node = fixture(root.path());
        let mut a = std::process::Command::new("/bin/sleep")
            .arg("20")
            .spawn()
            .unwrap();
        let mut b = std::process::Command::new("/bin/sleep")
            .arg("20")
            .spawn()
            .unwrap();
        let (first, second) = tokio::join!(
            node.resources.admit(&node, "w1:p1", a.id()),
            node.resources.admit(&node, "w1:p2", b.id())
        );
        let first = first.unwrap();
        let second = second.unwrap();
        assert_eq!(
            [first["allowed"] == true, second["allowed"] == true]
                .into_iter()
                .filter(|v| *v)
                .count(),
            1
        );
        let restored = Resources::open(root.path(), 10 * GIB).unwrap();
        assert_eq!(restored.state.lock().await.reservations.len(), 1);
        let successful = if first["allowed"] == true {
            a.id()
        } else {
            b.id()
        };
        node.resources.ready(successful).await.unwrap();
        assert!(node.resources.state.lock().await.reservations.is_empty());
        a.kill().unwrap();
        b.kill().unwrap();
        a.wait().unwrap();
        b.wait().unwrap();
    }

    #[tokio::test]
    async fn limit_changes_are_persistent_and_unsafe_shrinks_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let node = fixture(root.path());
        let before = fs::read(root.path().join("config.json")).unwrap();
        assert!(node.resources.set_limit(&node, 7 * GIB).await.is_err());
        assert_eq!(fs::read(root.path().join("config.json")).unwrap(), before);
        let profile = node.resources.set_limit(&node, 12 * GIB).await.unwrap();
        assert!(profile.limit_enforced);
        assert_eq!(profile.agent_memory_limit_bytes, 12 * GIB);
        let saved: Value =
            serde_json::from_slice(&fs::read(root.path().join("config.json")).unwrap()).unwrap();
        assert_eq!(saved["agent_memory_limit_bytes"], 12 * GIB);
    }

    #[test]
    fn corrupt_reservations_fail_closed_and_preserve_the_original_file() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("resource-reservations.json");
        fs::write(&path, b"not-json").unwrap();
        assert!(Resources::open(root.path(), 10 * GIB).is_err());
        assert_eq!(fs::read(path).unwrap(), b"not-json");
    }
}
