//! Private push subscriptions/outbox. Transport failure never interrupts chat or scanning.
use super::*;
use std::{fs::OpenOptions, io::Write, sync::Mutex};

#[derive(Clone, Deserialize, Serialize)]
struct Device {
    host_id: String,
    token: String,
    project_id: String,
    after: String,
    updated_ms: i64,
    retry_at: i64,
    failures: u32,
}
#[derive(Clone, Deserialize, Serialize)]
struct State {
    version: u32,
    devices: BTreeMap<String, Device>,
}
pub(super) struct Push {
    state: Mutex<State>,
    path: PathBuf,
}
impl Push {
    pub fn open(directory: &Path) -> Result<Self> {
        let path = directory.join("push-devices.json");
        let state = match fs::read(&path) {
            Ok(bytes) if bytes.len() <= 256 * 1024 => {
                let state: State = serde_json::from_slice(&bytes)?;
                if state.version != 1 || state.devices.len() > 32 {
                    return Err(Error::Config("invalid push registry"));
                }
                state
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => State {
                version: 1,
                devices: BTreeMap::new(),
            },
            _ => return Err(Error::Config("push registry unavailable or too large")),
        };
        Ok(Self {
            state: Mutex::new(state),
            path,
        })
    }
    fn save(&self, next: &State) -> Result<()> {
        let bytes = serde_json::to_vec(next)?;
        let path = self.path.with_extension("pending");
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(path, &self.path)?;
        if let Some(parent) = self.path.parent() {
            fs::File::open(parent)?.sync_all()?
        }
        Ok(())
    }
    pub fn register(&self, params: &Value, cursor: String, configured: bool) -> Result<Value> {
        let id = params["device_id"].as_str().unwrap_or_default();
        let host = params["host_id"].as_str().unwrap_or_default();
        let valid = |s: &str, max: usize| {
            !s.is_empty()
                && s.len() <= max
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        };
        if !valid(id, 128) || !valid(host, 128) {
            return Err(Error::Protocol("invalid push device/host identity"));
        }
        let mut guard = self
            .state
            .lock()
            .map_err(|_| Error::Protocol("push registry locked"))?;
        let mut next = guard.clone();
        let key = format!("{id}/{host}");
        if let Some(token) = params["token"].as_str() {
            let project = params["project_id"].as_str().unwrap_or_default();
            if token.len() < 8
                || token.len() > 4096
                || token.bytes().any(|b| b <= 32)
                || !valid(project, 63)
            {
                return Err(Error::Protocol("invalid push registration"));
            }
            if !next.devices.contains_key(&key) && next.devices.len() >= 32 {
                return Err(Error::Config("push device capacity exceeded"));
            }
            let after = next
                .devices
                .get(&key)
                .map(|d| d.after.clone())
                .unwrap_or(cursor);
            next.devices.insert(
                key,
                Device {
                    host_id: host.into(),
                    token: token.into(),
                    project_id: project.into(),
                    after,
                    updated_ms: now(),
                    retry_at: 0,
                    failures: 0,
                },
            );
        } else {
            next.devices.remove(&key);
        }
        self.save(&next)?;
        *guard = next;
        Ok(json!({"configured":configured,"registered":params["token"].is_string()}))
    }
    pub fn status(&self, configured: bool) -> Value {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        json!({"configured":configured,"devices":state.devices.len(),"retrying":state.devices.values().filter(|d|d.failures>0).count()})
    }
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

pub(super) fn start(node: &Arc<Node>) -> notifications::Watch {
    let weak = Arc::downgrade(node);
    notifications::Watch(tokio::spawn(async move {
        loop {
            let Some(node) = weak.upgrade() else { return };
            if let (Some(account), Some(push)) = (&node.config.push_service_account, &node.push) {
                let devices = push
                    .state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .devices
                    .clone();
                for (key, device) in devices {
                    if device.retry_at > now() {
                        continue;
                    }
                    let Ok(page) = node
                        .notification_journal()
                        .and_then(|journal| journal.page(Some(&device.after)))
                    else {
                        continue;
                    };
                    let events: Vec<_> = page.events.iter().take(4).cloned().collect();
                    if events.is_empty() {
                        if page.cursor != device.after {
                            let mut guard = push.state.lock().unwrap_or_else(|e| e.into_inner());
                            let mut next = guard.clone();
                            if let Some(current) = next.devices.get_mut(&key).filter(|current| {
                                current.token == device.token && current.after == device.after
                            }) {
                                current.after = page.cursor;
                                if push.save(&next).is_ok() {
                                    *guard = next;
                                }
                            }
                        }
                        continue;
                    }
                    let job = json!({"subscription":device,"events":events});
                    let result = transport(&node, account, &job).await;
                    let mut guard = push.state.lock().unwrap_or_else(|e| e.into_inner());
                    // A token rotation or removal while the transport was running wins.
                    if guard.devices.get(&key).is_none_or(|current| {
                        current.token != device.token || current.after != device.after
                    }) {
                        continue;
                    }
                    let mut next = guard.clone();
                    let current = next.devices.get_mut(&key).expect("checked registration");
                    let results = result.as_ref().ok().and_then(|r| r["results"].as_array());
                    let mut success = 0;
                    let mut removed = false;
                    if let Some(results) = results {
                        for (event, row) in events.iter().zip(results.iter()) {
                            if row["id"] != event.id {
                                break;
                            }
                            match row["status"].as_str() {
                                Some("sent") => success += 1,
                                Some("unregistered") => {
                                    removed = true;
                                    break;
                                }
                                _ => break,
                            }
                        }
                    }
                    if removed {
                        next.devices.remove(&key);
                    } else {
                        if success > 0 {
                            current.after =
                                format!("{}:{}", page.node_id, events[success - 1].sequence);
                        }
                        if success == events.len() {
                            current.failures = 0;
                            current.retry_at = 0;
                        } else {
                            current.failures = current.failures.saturating_add(1);
                            current.retry_at = now() + 30_000 * (1i64 << current.failures.min(6));
                        }
                    }
                    if push.save(&next).is_ok() {
                        *guard = next;
                    }
                }
            }
            drop(node);
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }))
}
async fn transport(node: &Node, account: &Path, job: &Value) -> Result<Value> {
    let mut command = Command::new(&node.config.python);
    command
        .arg(node.config.backend.with_file_name("push-v1.py"))
        .arg(account)
        .arg(
            node.config
                .socket
                .parent()
                .ok_or(Error::Config("missing node directory"))?
                .join("push-oauth.json"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn()?;
    let mut input = child
        .stdin
        .take()
        .ok_or(Error::Protocol("push input unavailable"))?;
    input.write_all(&serde_json::to_vec(job)?).await?;
    input.shutdown().await?;
    drop(input);
    let mut output = child
        .stdout
        .take()
        .ok_or(Error::Protocol("push output unavailable"))?;
    let mut bytes = vec![];
    tokio::time::timeout(Duration::from_secs(45), async {
        (&mut output)
            .take(16 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await?;
        if bytes.len() > 16 * 1024 {
            return Err(Error::Protocol("push output too large"));
        }
        if !child.wait().await?.success() {
            return Err(Error::Protocol("push transport failed"));
        }
        Ok(())
    })
    .await
    .map_err(|_| Error::Protocol("push transport timed out"))??;
    Ok(serde_json::from_slice(&bytes)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_registration_rotates_tokens_without_replaying_old_events_and_can_be_revoked() {
        let root = tempfile::tempdir().unwrap();
        let push = Push::open(root.path()).unwrap();
        let mut registration = json!({"device_id":"phone","host_id":"host","token":"test-token","project_id":"kite-test"});
        push.register(&registration, "node:7".into(), false)
            .unwrap();
        registration["token"] = json!("rotated-token");
        push.register(&registration, "node:20".into(), true)
            .unwrap();
        let recovered = Push::open(root.path()).unwrap();
        assert_eq!(
            recovered.state.lock().unwrap().devices["phone/host"].after,
            "node:7"
        );
        assert_eq!(
            fs::metadata(root.path().join("push-devices.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        registration["token"] = Value::Null;
        assert_eq!(
            push.register(&registration, "node:21".into(), true)
                .unwrap()["registered"],
            false
        );
        assert_eq!(push.status(true)["devices"], 0);
    }
}
