//! Reclaim inactive provider owners independently of connected phone clients.
use super::*;
use tokio::task::JoinHandle;

const IDLE_TIMEOUT_MS: u64 = 60 * 60 * 1000;
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

fn reason(fact: &Fact, now: u64) -> Option<&'static str> {
    if !fact.idle || !fact.persisted {
        return None;
    }
    if fact.archived {
        return Some("archived");
    }
    if fact.last_activity_ms > 0 && now.checked_sub(fact.last_activity_ms)? >= IDLE_TIMEOUT_MS {
        return Some("idle_timeout");
    }
    None
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

async fn reap(node: &Node, fact: &Fact, reason: &str) -> Result<bool> {
    let mut request = serde_json::to_value(fact)?;
    request["reason"] = reason.into();
    let input = serde_json::to_vec(&request)?;
    let mut command = Command::new(&node.config.python);
    command
        .arg(&node.config.backend)
        .args(["lifecycle-reap", "--herdr"])
        .arg(&node.config.herdr)
        .envs(&node.config.environment)
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

async fn scan(node: &Node) -> Result<()> {
    let mut command = Command::new(&node.config.python);
    command
        .arg(&node.config.backend)
        .args(["lifecycle-facts", "--herdr"])
        .arg(&node.config.herdr)
        .envs(&node.config.environment);
    let bytes = bounded_output(command).await?;
    let facts: Facts = serde_json::from_slice(&bytes)?;
    if facts.facts.len() > 1024 {
        return Err(Error::Protocol("lifecycle fact capacity exceeded"));
    }
    for fact in facts.facts {
        if node.draining() {
            return Ok(());
        }
        let Some(reason) = reason(&fact, now_ms()) else {
            continue;
        };
        match reap(node, &fact, reason).await {
            Ok(true) => {
                node.lifecycle_reclaimed.fetch_add(1, Ordering::Relaxed);
                log::info!("reclaimed inactive session {} ({reason})", fact.session_id);
            }
            Ok(false) => {}
            Err(_) => log::debug!("session lifecycle revalidation unavailable"),
        }
    }
    node.lifecycle_last_scan.store(now_ms(), Ordering::Relaxed);
    Ok(())
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
            drop(node);
            tokio::time::sleep(SCAN_INTERVAL).await;
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
    fn idle_session_expires_after_one_hour_without_polling_extending_its_life() {
        let fact = fact();
        for now in [1001, 60_000, IDLE_TIMEOUT_MS + 999] {
            assert_eq!(reason(&fact, now), None);
        }
        assert_eq!(reason(&fact, IDLE_TIMEOUT_MS + 1000), Some("idle_timeout"));
        let mut active_again = fact;
        active_again.last_activity_ms = IDLE_TIMEOUT_MS;
        assert_eq!(reason(&active_again, IDLE_TIMEOUT_MS + 1000), None);
    }

    #[test]
    fn archived_sessions_are_reclaimed_only_after_work_and_history_are_settled() {
        let mut fact = fact();
        fact.archived = true;
        fact.idle = false;
        assert_eq!(reason(&fact, 10 * IDLE_TIMEOUT_MS), None);
        fact.idle = true;
        fact.persisted = false;
        assert_eq!(reason(&fact, 10 * IDLE_TIMEOUT_MS), None);
        fact.persisted = true;
        assert_eq!(reason(&fact, 1001), Some("archived"));
    }

    #[test]
    fn missing_activity_and_a_clock_rollback_do_not_expire_sessions() {
        let mut fact = fact();
        assert_eq!(reason(&fact, 999), None);
        fact.last_activity_ms = 0;
        assert_eq!(reason(&fact, 10 * IDLE_TIMEOUT_MS), None);
    }

    #[tokio::test]
    async fn host_scan_reaps_expired_and_archived_owners_but_keeps_live_work() {
        let root = tempfile::tempdir().unwrap();
        let interpreter = root.path().join("interpreter");
        fs::write(
            &interpreter,
            "#!/usr/bin/python3\nimport json,sys\nfrom pathlib import Path\nr=Path(__file__).parent\nif sys.argv[2]=='lifecycle-facts':print((r/'facts.json').read_text())\nelif sys.argv[2]=='lifecycle-reap':\n p=json.load(sys.stdin)\n with (r/'attempts').open('a') as f:f.write(json.dumps(p)+'\\n')\n print(json.dumps({'reclaimed':p['session_id']!='changed'}))\nelse:raise AssertionError(sys.argv)\n",
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
        let mut recent = expired.clone();
        recent.session_id = "recent".into();
        recent.last_activity_ms = now_ms();
        let mut changed = expired.clone();
        changed.session_id = "changed".into();
        fs::write(
            root.path().join("facts.json"),
            serde_json::to_vec(&json!({"facts":[expired,archived,active,recent,changed]})).unwrap(),
        )
        .unwrap();
        let mut config = super::super::tests::config(root.path());
        config.python = interpreter;
        let node = Node {
            config,
            started: Instant::now(),
            requests: AtomicU64::new(0),
            backend_calls: AtomicU64::new(0),
            lifecycle_last_scan: AtomicU64::new(0),
            lifecycle_reclaimed: AtomicU64::new(0),
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
            upgrade: None,
            observation_gate: RwLock::new(()),
        };
        scan(&node).await.unwrap();
        let attempts: Vec<Value> = fs::read_to_string(root.path().join("attempts"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(attempts.len(), 3);
        assert_eq!(attempts[0]["reason"], "idle_timeout");
        assert_eq!(attempts[1]["reason"], "archived");
        assert_eq!(node.lifecycle_reclaimed.load(Ordering::Relaxed), 2);
        assert!(node.lifecycle_last_scan.load(Ordering::Relaxed) > 0);
    }
}
