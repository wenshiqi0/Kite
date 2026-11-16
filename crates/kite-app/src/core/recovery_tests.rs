//! Local SSH recovery acceptance. The peer rejects every work-starting command.
use super::*;
use russh::keys::PrivateKey;
use russh::server::{self, Auth, Msg, Session as ServerSession, VerifiedPublicKey};
use russh::{Channel, ChannelId};
use serde_json::{Value, json};
use tokio::net::TcpListener;

struct Secrets {
    key: String,
}
impl SecretStorage for Secrets {
    fn read(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(Some(self.key.as_bytes().to_vec()))
    }
    fn write(&self, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}
struct Silent;
impl Observer for Silent {
    fn changed(&self, _: Snapshot) {}
}
async fn until(mut check: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(8), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("recovery condition timed out");
}
fn finish(
    session: &mut ServerSession,
    channel: ChannelId,
    bytes: Vec<u8>,
) -> Result<(), russh::Error> {
    if !bytes.is_empty() {
        session.data(channel, bytes)?;
    }
    session.exit_status_request(channel, 0)?;
    session.eof(channel)?;
    session.close(channel)
}
struct Peer {
    catalog: Value,
    seen: Arc<Mutex<Vec<String>>>,
    pending: BTreeMap<ChannelId, Vec<u8>>,
    reject_auth: bool,
    blocked_catalog: Arc<AtomicBool>,
}
impl server::Handler for Peer {
    type Error = russh::Error;
    async fn auth_publickey(
        &mut self,
        _: &str,
        _: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        Ok(if self.reject_auth {
            Auth::Reject {
                proceed_with_methods: None,
                partial_success: false,
            }
        } else {
            Auth::Accept
        })
    }
    async fn channel_open_session(
        &mut self,
        _: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).to_string();
        lock(&self.seen).push(command.clone());
        assert!(
            ![
                "session-open",
                "catalog-write",
                "agent.start",
                "workspace.create",
                "turn/start"
            ]
            .iter()
            .any(|s| command.contains(s)),
            "recovery replayed work: {command}"
        );
        session.channel_success(channel)?;
        if command.contains("catalog-read") && self.blocked_catalog.load(Ordering::SeqCst) {
            return Ok(());
        }
        if command.contains("herdr-api") {
            self.pending.insert(channel, vec![]);
            return Ok(());
        }
        let output = if command.ends_with("api-version") {
            json!({"api":1,"backendProtocol":5,"harnessApi":2}).to_string()
        } else if command == "/bin/sh -s" {
            "path=/owned/herdr\ncapability=herdr-api-bridge-v1\nversion=fixture\n".into()
        } else if command.contains("catalog-read") {
            self.catalog.to_string()
        } else if command.ends_with("harness-scan") {
            json!({"providers":[],"agents":[]}).to_string()
        } else if command.contains("session-facts") {
            json!({"facts":[]}).to_string()
        } else if command.contains("notifications") {
            json!({"version":1,"node_id":"fixture","cursor":"","events":[]}).to_string()
        } else {
            "{}".into()
        };
        finish(session, channel, output.into_bytes())
    }
    async fn data(
        &mut self,
        channel: ChannelId,
        bytes: &[u8],
        session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        let Some(buffer) = self.pending.get_mut(&channel) else {
            return Ok(());
        };
        buffer.extend_from_slice(bytes);
        if !buffer.contains(&b'\n') {
            return Ok(());
        }
        let request: Value = serde_json::from_slice(buffer).unwrap();
        let method = request["method"].as_str().unwrap();
        lock(&self.seen).push(method.into());
        let result = match method {
            "ping" => json!({"type":"pong","version":"fixture"}),
            "agent.list" => json!({"type":"agent_list","agents":[]}),
            method => panic!("recovery sent unexpected RPC {method}"),
        };
        finish(
            session,
            channel,
            format!("{}\n", json!({"id":request["id"],"result":result})).into_bytes(),
        )
    }
}
pub(super) struct Fixture {
    pub(super) core: Arc<AppCore>,
    directory: tempfile::TempDir,
    secrets: Arc<Secrets>,
    listener: JoinHandle<()>,
    seen: Arc<Mutex<Vec<String>>>,
    connections: Arc<AtomicU64>,
    blocked_catalog: Arc<AtomicBool>,
}
impl Fixture {
    pub(super) async fn new(reject_auth: bool) -> Self {
        let key = kite_ssh::generate_ed25519("owned-recovery").unwrap();
        let secrets = Arc::new(Secrets {
            key: key.private_key_openssh.clone(),
        });
        let config = Arc::new(server::Config {
            keys: vec![PrivateKey::from_openssh(&key.private_key_openssh).unwrap()],
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let record = kite_agent::sessions::Record::draft(
            "a".repeat(32),
            "codex".into(),
            "Original conversation".into(),
            None,
            1,
        );
        let catalog = json!({"revision":1,"now_ms":1,"sessions":[record]});
        let seen = Arc::new(Mutex::new(vec![]));
        let connections = Arc::new(AtomicU64::new(0));
        let seen_server = seen.clone();
        let accepts = connections.clone();
        let server_catalog = catalog.clone();
        let blocked_catalog = Arc::new(AtomicBool::new(false));
        let server_block = blocked_catalog.clone();
        let listener = tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                accepts.fetch_add(1, Ordering::SeqCst);
                let peer = Peer {
                    catalog: server_catalog.clone(),
                    seen: seen_server.clone(),
                    pending: BTreeMap::new(),
                    reject_auth,
                    blocked_catalog: server_block.clone(),
                };
                let config = config.clone();
                tokio::spawn(async move {
                    if let Ok(session) = server::run_stream(config, socket, peer).await {
                        let _ = session.await;
                    }
                });
            }
        });
        let directory = tempfile::tempdir().unwrap();
        let core = AppCore::new(secrets.clone(), Arc::new(Silent));
        core.initialize(
            directory.path().into(),
            "fixture".into(),
            LegacyImport {
                source: "fixture".into(),
                host: Some(Host {
                    id: "host".into(),
                    host: "127.0.0.1".into(),
                    port,
                    username: "fixture".into(),
                    herdr_path: Some("/owned/herdr".into()),
                    stream_host: None,
                    sunshine_port: 47989,
                }),
                trusted_keys: vec![(format!("127.0.0.1:{port}"), key.public_key_openssh)],
            },
        )
        .await
        .unwrap();
        core.install_catalog("host", &serde_json::from_value(catalog).unwrap())
            .unwrap();
        let cache = ConversationKey::new("host", "a".repeat(32)).cache_key();
        lock(&core.store).as_ref().unwrap().write_document(&ConversationKey::cache_document_of(&cache), &json!({"messages":[{"id":"retained","role":"Assistant","text":"Cached original conversation","intermediate":false,"tool":null,"question_replies":[]}],"available":true,"viewed_ms":1})).unwrap();
        core.open_session("a".repeat(32)).await.unwrap();
        core.set_connection_intent(Some("host".into())).unwrap();
        Self {
            core,
            directory,
            secrets,
            listener,
            seen,
            connections,
            blocked_catalog,
        }
    }
    pub(super) async fn stop(&self) {
        self.core.disconnect().await.unwrap();
        self.listener.abort();
    }
}

#[tokio::test]
async fn switching_saved_conversations_does_not_wait_for_an_unresponsive_catalog() {
    let f = Fixture::new(false).await;
    f.core.restore_connection().await.unwrap();
    until(|| f.core.snapshot().connection == ConnectionPhase::Connected).await;
    f.blocked_catalog.store(true, Ordering::SeqCst);
    let records = (0..8)
        .map(|index| {
            kite_agent::sessions::Record::draft(
                format!("view-{index}"),
                "codex".into(),
                format!("Owned view {index}"),
                None,
                1,
            )
        })
        .collect();
    f.core
        .install_catalog(
            "host",
            &kite_agent::sessions::Catalog {
                revision: 5,
                now_ms: 1,
                sessions: records,
            },
        )
        .unwrap();
    for index in 0..8 {
        let id = format!("view-{index}");
        tokio::time::timeout(Duration::from_millis(500), f.core.open_session(id.clone()))
            .await
            .expect("view selection waited for a remote catalog/history request")
            .unwrap();
        assert_eq!(
            f.core.snapshot().selected_session.as_deref(),
            Some(id.as_str())
        );
    }
    f.blocked_catalog.store(false, Ordering::SeqCst);
    f.stop().await;
}

#[tokio::test]
async fn a_lost_transport_recovers_the_same_conversation_without_replaying_work() {
    let f = Fixture::new(false).await;
    f.core.restore_connection().await.unwrap();
    until(|| f.core.snapshot().connection == ConnectionPhase::Connected).await;
    let generation = f.core.generation.load(Ordering::SeqCst);
    let old = f.core.connection().unwrap();
    old.ssh.close().await.unwrap();
    f.core.network_changed(true);
    until(|| {
        f.core.generation.load(Ordering::SeqCst) > generation
            && f.core.snapshot().connection == ConnectionPhase::Connected
    })
    .await;
    let snapshot = f.core.snapshot();
    assert_eq!(snapshot.selected_session, Some("a".repeat(32)));
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(
        snapshot.conversation[0].text(),
        "Cached original conversation"
    );
    assert!(f.connections.load(Ordering::SeqCst) >= 2);
    assert!(
        lock(&f.seen)
            .iter()
            .all(|s| !s.contains("session-open") && !s.contains("turn/start"))
    );
    // Process teardown preserves the requested host; a new Core restores its saved session cache.
    f.core.disconnect_inner(false).await.unwrap();
    let next = AppCore::new(f.secrets.clone(), Arc::new(Silent));
    next.initialize(
        f.directory.path().into(),
        "recreated".into(),
        LegacyImport {
            source: "fixture".into(),
            host: None,
            trusted_keys: vec![],
        },
    )
    .await
    .unwrap();
    next.restore_connection().await.unwrap();
    until(|| next.snapshot().connection == ConnectionPhase::Connected).await;
    assert_eq!(next.snapshot().selected_session, Some("a".repeat(32)));
    assert_eq!(
        next.snapshot().conversation[0].text(),
        "Cached original conversation"
    );
    next.disconnect().await.unwrap();
    f.stop().await;
}

#[tokio::test]
async fn switching_hosts_cancels_the_previous_hosts_pending_retry() {
    let f = Fixture::new(false).await;
    f.core.network_changed(false);
    f.core
        .schedule_recovery("host", f.core.generation.load(Ordering::SeqCst));
    let mut other = f.core.host("host").unwrap();
    other.id = "other".into();
    f.core.save_host(other).await.unwrap();
    f.core.connect("other".into()).await.unwrap();
    until(|| f.core.snapshot().selected_host.as_deref() == Some("other")).await;
    f.core.network_changed(true);
    until(|| f.core.snapshot().connection == ConnectionPhase::Connected).await;
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(f.core.snapshot().selected_host.as_deref(), Some("other"));
    assert_eq!(lock(&f.core.desired_connection).as_deref(), Some("other"));
    f.stop().await;
}

#[tokio::test]
async fn an_invalid_connection_intent_cannot_block_chat_initialization_or_start_a_connection() {
    let f = Fixture::new(false).await;
    let file = f.directory.path().join("connection-intent.json");
    std::fs::write(&file, b"corrupt intent").unwrap();
    let next = AppCore::new(f.secrets.clone(), Arc::new(Silent));
    next.initialize(
        f.directory.path().into(),
        "recreated".into(),
        LegacyImport {
            source: "fixture".into(),
            host: None,
            trusted_keys: vec![],
        },
    )
    .await
    .unwrap();
    next.restore_connection().await.unwrap();
    assert!(next.snapshot().initialized);
    assert_eq!(next.snapshot().connection, ConnectionPhase::Disconnected);
    assert_eq!(std::fs::read(&file).unwrap(), b"corrupt intent");
    assert_eq!(f.connections.load(Ordering::SeqCst), 0);
    f.stop().await;
}

#[tokio::test]
async fn offline_retries_wait_and_manual_disconnect_cancels_pending_and_persisted_recovery() {
    let f = Fixture::new(false).await;
    f.core.network_changed(false);
    let generation = f.core.generation.load(Ordering::SeqCst);
    f.core.schedule_recovery("host", generation);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(f.connections.load(Ordering::SeqCst), 0);
    f.core.network_changed(true);
    until(|| f.core.snapshot().connection == ConnectionPhase::Connected).await;
    f.core.network_changed(false);
    f.core
        .schedule_recovery("host", f.core.generation.load(Ordering::SeqCst));
    f.core.disconnect().await.unwrap();
    let accepted = f.connections.load(Ordering::SeqCst);
    f.core.network_changed(true);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(f.connections.load(Ordering::SeqCst), accepted);
    assert_eq!(f.core.snapshot().connection, ConnectionPhase::Disconnected);
    let next = AppCore::new(f.secrets.clone(), Arc::new(Silent));
    next.initialize(
        f.directory.path().into(),
        "recreated".into(),
        LegacyImport {
            source: "fixture".into(),
            host: None,
            trusted_keys: vec![],
        },
    )
    .await
    .unwrap();
    next.restore_connection().await.unwrap();
    assert_eq!(next.snapshot().connection, ConnectionPhase::Disconnected);
    f.stop().await;
}

#[tokio::test]
async fn a_recreated_process_restores_only_the_requested_connection_and_auth_failure_stops_retrying()
 {
    let f = Fixture::new(true).await;
    let next = AppCore::new(f.secrets.clone(), Arc::new(Silent));
    next.initialize(
        f.directory.path().into(),
        "recreated".into(),
        LegacyImport {
            source: "fixture".into(),
            host: None,
            trusted_keys: vec![],
        },
    )
    .await
    .unwrap();
    next.restore_connection().await.unwrap();
    until(|| next.snapshot().connection == ConnectionPhase::Failed).await;
    assert_eq!(
        next.snapshot().failure.unwrap().code,
        FailureCode::Authentication
    );
    assert!(lock(&next.desired_connection).is_none());
    let accepted = f.connections.load(Ordering::SeqCst);
    next.network_changed(true);
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(f.connections.load(Ordering::SeqCst), accepted);
    next.disconnect().await.unwrap();
    f.stop().await;
}
