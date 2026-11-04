//! Release archived owners and sync Codex archives independently of connected clients.
use super::*;
use tokio::task::JoinHandle;

const SCAN_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Fact {
    session_id: String,
    pane: String,
    owner: Value,
    reference: Value,
    archived: bool,
    idle: bool,
    persisted: bool,
    activity: String,
    last_activity_ms: u64,
    tui_pid: Option<u32>,
}

#[derive(Deserialize)]
struct Facts {
    facts: Vec<Fact>,
}

fn reclaimable(fact: &Fact) -> bool {
    fact.archived && fact.idle && fact.persisted
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn backend_command(node: &Node, action: &str) -> Result<Command> {
    let config = node.config.current()?;
    let mut command = Command::new(&node.config.bun);
    command
        .arg(&node.config.backend)
        .args([action, "--herdr"])
        .arg(&config.herdr)
        .envs(&config.environment)
        .env("KITE_HOST_BACKEND_PATH", &node.config.backend);
    Ok(command)
}

async fn reap(node: &Node, fact: &Fact) -> Result<bool> {
    let mut request = serde_json::to_value(fact)?;
    request["reason"] = "archived".into();
    let input = serde_json::to_vec(&request)?;
    let mut command = backend_command(node, "lifecycle-reap")?;
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut child = command.spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or(Error::Protocol("missing lifecycle stdin"))?;
        stdin.write_all(&input).await?;
        drop(stdin);
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .ok_or(Error::Protocol("missing lifecycle stdout"))?
            .take(65537)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > 65536 || !child.wait().await?.success() {
            return Err(Error::Protocol("lifecycle reap failed"));
        }
        let result: Value = serde_json::from_slice(&bytes)?;
        Ok(result["reclaimed"] == true)
    })
    .await
    .map_err(|_| Error::Protocol("lifecycle reap timed out"))?
}

pub(super) async fn scan(node: &Node) -> Result<crate::SessionCleanup> {
    let _scan = tokio::time::timeout(Duration::from_secs(3), node.lifecycle_scan.lock())
        .await
        .map_err(|_| Error::Protocol("session cleanup already in progress"))?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    let bytes = bounded_output(backend_command(node, "lifecycle-facts")?).await?;
    let facts: Facts = serde_json::from_slice(&bytes)?;
    if facts.facts.len() > 1024 {
        return Err(Error::Protocol("lifecycle fact capacity exceeded"));
    }
    let mut result = crate::SessionCleanup::default();
    for fact in facts.facts.into_iter().filter(|fact| fact.archived) {
        if node.draining() {
            return Err(Error::Protocol("session cleanup generation is draining"));
        }
        if !reclaimable(&fact) {
            result.retained += 1;
            continue;
        }
        if tokio::time::Instant::now() >= deadline {
            result.retained += 1;
            continue;
        }
        match tokio::time::timeout_at(deadline, reap(node, &fact)).await {
            Ok(Ok(true)) => {
                result.reclaimed += 1;
                node.lifecycle_reclaimed.fetch_add(1, Ordering::Relaxed);
                log::info!("released archived session {}", fact.session_id);
            }
            Ok(Ok(false)) => result.retained += 1,
            Ok(Err(_)) | Err(_) => {
                result.failed += 1;
                log::debug!("session lifecycle revalidation unavailable");
            }
        }
    }
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if !remaining.is_zero() && !node.draining() {
        let sync =
            bounded_output_with_timeout(backend_command(node, "lifecycle-archive")?, remaining)
                .await
                .and_then(|bytes| Ok(serde_json::from_slice::<Value>(&bytes)?));
        match sync {
            Ok(value) => {
                if let Some(count) = value["archived"].as_u64().filter(|&n| n > 0) {
                    log::info!("synced {count} Codex archives");
                }
                if let Some(count) = value["failed"].as_u64().filter(|&n| n > 0) {
                    log::warn!("failed to sync {count} Codex archives; will retry");
                }
            }
            Err(error) => log::debug!("Codex archive sync unavailable: {error}"),
        }
    }
    node.lifecycle_last_scan.store(now_ms(), Ordering::Relaxed);
    Ok(result)
}

pub(super) struct Watch(JoinHandle<()>);
impl Drop for Watch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

pub(super) fn start(node: &Arc<Node>) -> Watch {
    let node = Arc::downgrade(node);
    Watch(tokio::spawn(async move {
        // Give the notification observer the first opportunity to record a
        // completed turn before its owner is reclaimed.
        tokio::time::sleep(Duration::from_secs(5)).await;
        loop {
            let Some(node) = node.upgrade() else { return };
            let observation = node.observation_gate.read().await;
            if node.draining() {
                return;
            }
            if scan(&node).await.is_err() {
                log::debug!("session lifecycle facts unavailable");
            }
            drop(observation);
            tokio::select! {
                _ = tokio::time::sleep(SCAN_INTERVAL) => {},
                _ = node.lifecycle_wake.notified() => {},
            }
            drop(node);
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact() -> Fact {
        Fact {
            session_id: "session".into(),
            pane: "w1:p1".into(),
            owner: json!({"pid": 42, "start_ticks": "7"}),
            reference: json!({"value": "original-thread"}),
            archived: false,
            idle: true,
            persisted: true,
            activity: "history-version".into(),
            last_activity_ms: 1000,
            tui_pid: Some(43),
        }
    }

    #[test]
    fn unarchived_sessions_never_expire() {
        let mut fact = fact();
        for activity in [0, 1, u64::MAX] {
            fact.last_activity_ms = activity;
            assert!(!reclaimable(&fact));
        }
    }

    #[test]
    fn archived_sessions_require_finished_work_and_persisted_history() {
        let mut fact = fact();
        fact.archived = true;
        fact.idle = false;
        assert!(!reclaimable(&fact));
        fact.idle = true;
        fact.persisted = false;
        assert!(!reclaimable(&fact));
        fact.persisted = true;
        assert!(reclaimable(&fact));
    }

    #[tokio::test]
    async fn client_command_releases_only_archived_owners_and_reports_retained_work() {
        let root = tempfile::tempdir().unwrap();
        let interpreter = root.path().join("interpreter");
        fs::write(
            &interpreter,
            "#!/usr/bin/python3\nimport json,sys,os\nfrom pathlib import Path\nr=Path(__file__).parent\nassert os.environ['KITE_HOST_BACKEND_PATH']==sys.argv[1]\nif sys.argv[2]=='lifecycle-facts':print((r/'facts.json').read_text())\nelif sys.argv[2]=='lifecycle-reap':\n p=json.load(sys.stdin)\n with (r/'attempts').open('a') as f:f.write(json.dumps(p)+'\\n')\n print(json.dumps({'reclaimed':p['session_id']!='changed'}))\nelif sys.argv[2]=='lifecycle-archive':\n assert len((r/'attempts').read_text().splitlines())==2\n (r/'archive-synced').touch()\n print(json.dumps({'archived':1,'failed':0}))\nelse:raise AssertionError(sys.argv)\n",
        )
        .unwrap();
        fs::set_permissions(&interpreter, fs::Permissions::from_mode(0o700)).unwrap();
        let mut expired = fact();
        expired.session_id = "expired".into();
        let mut archived = fact();
        archived.session_id = "archived".into();
        archived.archived = true;
        archived.last_activity_ms = now_ms();
        let mut active = expired.clone();
        active.session_id = "active".into();
        active.idle = false;
        active.archived = true;
        let mut recent = expired.clone();
        recent.session_id = "recent".into();
        recent.last_activity_ms = now_ms();
        let mut changed = expired.clone();
        changed.session_id = "changed".into();
        changed.archived = true;
        fs::write(
            root.path().join("facts.json"),
            serde_json::to_vec(&json!({"facts":[expired,archived,active,recent,changed]})).unwrap(),
        )
        .unwrap();
        let mut config = super::super::tests::config(root.path());
        config.bun = interpreter;
        let node = Node {
            config,
            started: Instant::now(),
            requests: AtomicU64::new(0),
            backend_calls: AtomicU64::new(0),
            lifecycle_last_scan: AtomicU64::new(0),
            lifecycle_reclaimed: AtomicU64::new(0),
            lifecycle_scan: Mutex::new(()),
            lifecycle_wake: tokio::sync::Notify::new(),
            resources: super::super::resources::Resources::open(
                root.path(),
                crate::resources::DEFAULT_AGENT_MEMORY_LIMIT,
            )
            .unwrap(),
            permits: Arc::new(Semaphore::new(1)),
            notifications: None,
            push: None,
            notification_problem: None,
            push_problem: None,
            services: Mutex::new(None),
            integration_health: Mutex::new(None),
            upgrade: None,
            observation_gate: RwLock::new(()),
        };
        let result = node
            .control(&NodeRequest::new("sessions-release-archived"))
            .await
            .unwrap();
        assert_eq!(result, json!({"reclaimed":1,"retained":2,"failed":0}));
        let attempts: Vec<Value> = fs::read_to_string(root.path().join("attempts"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(attempts.len(), 2);
        assert!(
            attempts
                .iter()
                .all(|attempt| attempt["reason"] == "archived")
        );
        assert_eq!(attempts[0]["session_id"], "archived");
        assert!(root.path().join("archive-synced").exists());
        assert_eq!(node.lifecycle_reclaimed.load(Ordering::Relaxed), 1);
        assert!(node.lifecycle_last_scan.load(Ordering::Relaxed) > 0);
    }
}
