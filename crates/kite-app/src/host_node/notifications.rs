use super::*;
use kite_agent::notifications::{Fact, Ledger, Page};
use std::{fs::OpenOptions, io::Write, sync::Mutex};
use tokio::task::JoinHandle;

pub(super) struct Journal {
    ledger: Mutex<Ledger>,
    path: PathBuf,
    pub last_scan: AtomicU64,
}
#[derive(Deserialize)]
struct Facts {
    facts: Vec<SourceFact>,
}
#[derive(Deserialize)]
struct SourceFact {
    session_id: String,
    title: String,
    source: String,
    status: String,
    revision: u64,
    thread: String,
    controls: String,
    controls_known: bool,
    lifecycle: bool,
}
pub(super) struct Watch(pub JoinHandle<()>);
impl Drop for Watch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Journal {
    pub fn open(directory: &Path, node: &str) -> Result<Self> {
        let path = directory.join("notifications.json");
        let ledger = match fs::read(&path) {
            Ok(bytes) => {
                if bytes.len() > 4 * 1024 * 1024 {
                    return Err(Error::Protocol("notification journal too large"));
                }
                let value: Ledger = serde_json::from_slice(&bytes)?;
                if !value.valid(node) {
                    return Err(Error::Protocol(
                        "invalid notification journal identity/version",
                    ));
                }
                value
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ledger::new(node.into()),
            Err(error) => return Err(error.into()),
        };
        Ok(Self {
            ledger: Mutex::new(ledger),
            path,
            last_scan: AtomicU64::new(0),
        })
    }
    pub fn page(&self, after: Option<&str>) -> Result<Page> {
        if after.is_some_and(|after| after.len() > 192) {
            return Err(Error::Protocol("notification cursor too long"));
        }
        Ok(self
            .ledger
            .lock()
            .map_err(|_| Error::Protocol("notification journal locked"))?
            .page(after))
    }
    pub fn event(&self, id: &str) -> Result<Option<kite_agent::notifications::Event>> {
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Error::Protocol("invalid notification identity"));
        }
        Ok(self
            .ledger
            .lock()
            .map_err(|_| Error::Protocol("notification journal locked"))?
            .event(id))
    }
    fn ingest(&self, facts: &[Fact], now: i64) -> Result<()> {
        let mut guard = self
            .ledger
            .lock()
            .map_err(|_| Error::Protocol("notification journal locked"))?;
        let mut next = guard.clone();
        if next.observe(facts, now) {
            let bytes = serde_json::to_vec(&next)?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(Error::Protocol("notification journal too large"));
            }
            let temporary = self.path.with_extension("pending");
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(temporary, &self.path)?;
            if let Some(parent) = self.path.parent() {
                fs::File::open(parent)?.sync_all()?;
            }
            *guard = next;
        }
        self.last_scan.store(now.max(0) as u64, Ordering::Relaxed);
        Ok(())
    }
}
pub(super) fn start(node: &Arc<Node>) -> Watch {
    let node = Arc::downgrade(node);
    Watch(tokio::spawn(async move {
        loop {
            let Some(node) = node.upgrade() else { return };
            let mut command = Command::new(&node.config.python);
            command
                .arg(&node.config.backend)
                .args(["notification-facts", "--herdr"])
                .arg(&node.config.herdr)
                .envs(&node.config.environment);
            let result = async {
                let bytes = bounded_output(command).await?;
                let source: Facts = serde_json::from_slice(&bytes)?;
                if source.facts.len() > 1024 {
                    return Err(Error::Protocol("notification fact capacity exceeded"));
                }
                let facts: Vec<_> = source
                    .facts
                    .into_iter()
                    .map(|source| {
                        let questions = if source.controls_known {
                            kite_agent::async_input::history(
                                &source.thread,
                                source.controls.as_bytes(),
                            )
                            .pending
                            .into_iter()
                            .map(|pending| pending.prompt.id)
                            .collect()
                        } else {
                            Vec::new()
                        };
                        Fact {
                            session_id: source.session_id,
                            title: source.title,
                            source: source.source,
                            status: source.status,
                            revision: source.revision,
                            questions,
                            questions_known: source.controls_known,
                            lifecycle: source.lifecycle,
                            terminal: kite_agent::notifications::terminal_from_controls(
                                source.controls.as_bytes(),
                            ),
                        }
                    })
                    .collect();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis()
                    .min(i64::MAX as u128) as i64;
                node.notification_journal()?.ingest(&facts, now)
            }
            .await;
            if result.is_err() {
                log::debug!("notification fact scan unavailable");
            }
            drop(node);
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_journal_survives_restart_and_uses_private_permissions() {
        let temporary = tempfile::tempdir().unwrap();
        let journal = Journal::open(temporary.path(), "node").unwrap();
        let fact = |status: &str, revision| Fact {
            session_id: "session".into(),
            title: "name".into(),
            source: "source".into(),
            status: status.into(),
            revision,
            questions: Vec::new(),
            questions_known: true,
            lifecycle: false,
            terminal: None,
        };
        journal.ingest(&[fact("working", 1)], 1).unwrap();
        journal.ingest(&[fact("idle", 2)], 2).unwrap();
        let restored = Journal::open(temporary.path(), "node").unwrap();
        assert_eq!(restored.page(Some("node:0")).unwrap().events.len(), 1);
        assert_eq!(
            fs::metadata(temporary.path().join("notifications.json"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert!(Journal::open(temporary.path(), "other-node").is_err());
    }
}
