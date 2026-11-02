//! Real AppCore approval commands over local SSH, with managed agents absent from Herdr.
use super::*;
use russh::keys::PrivateKey;
use russh::server::{self, Auth, Msg, Session as ServerSession, VerifiedPublicKey};
use russh::{Channel, ChannelId};
use serde_json::{Value, json};
use tokio::net::TcpListener;

const THREAD: &str = "12345678-1234-1234-1234-123456789abc";
struct Secrets;
impl SecretStorage for Secrets {
    fn read(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(None)
    }
    fn write(&self, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
}
struct ObserverSink;
impl Observer for ObserverSink {
    fn changed(&self, _: Snapshot) {}
}
struct Peer {
    agent: Value,
    catalog: Arc<Mutex<Value>>,
    opened: Arc<Mutex<Value>>,
    seen: Arc<Mutex<Vec<Value>>>,
    channels: BTreeMap<ChannelId, (String, Vec<u8>)>,
}
fn send(session: &mut ServerSession, channel: ChannelId, value: Value) -> Result<(), russh::Error> {
    session.data(channel, format!("{value}\n").into_bytes())
}
fn finish(
    session: &mut ServerSession,
    channel: ChannelId,
    value: Value,
) -> Result<(), russh::Error> {
    send(session, channel, value)?;
    session.exit_status_request(channel, 0)?;
    session.eof(channel)?;
    session.close(channel)
}
impl server::Handler for Peer {
    type Error = russh::Error;
    async fn auth_publickey(
        &mut self,
        _: &str,
        _: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
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
        let command = String::from_utf8(data.to_vec()).unwrap();
        session.channel_success(channel)?;
        if command.ends_with("api-version") {
            return finish(
                session,
                channel,
                json!({"api":1,"backendProtocol":5,"harnessApi":2}),
            );
        }
        if command.ends_with("session-facts --herdr 'fixture-herdr'") {
            return finish(session, channel, json!({"facts":[]}));
        }
        if command.contains("session-open") {
            let agent = lock(&self.opened).clone();
            return finish(session, channel, json!({"agent":agent}));
        }
        if command.ends_with("catalog-read") {
            let catalog = lock(&self.catalog).clone();
            return finish(session, channel, catalog);
        }
        if command.contains("harness-history") {
            session.data(channel, b"kite-history-pending-v1\n".to_vec())?;
            session.exit_status_request(channel, 0)?;
            session.eof(channel)?;
            return session.close(channel);
        }
        if command.ends_with("harness-scan") {
            let managed = matches!(self.agent["agent"].as_str(), Some("ply" | "dsh"));
            return finish(
                session,
                channel,
                json!({"providers":[],"agents":if managed {vec![self.agent.clone()]} else {vec![]}}),
            );
        }
        assert!(
            command.contains("herdr-api") || command.contains("proxy --pane"),
            "unexpected command {command}"
        );
        self.channels.insert(channel, (command, vec![]));
        Ok(())
    }
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        let (command, buffer) = self.channels.get_mut(&channel).unwrap();
        buffer.extend_from_slice(data);
        while let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<_> = buffer.drain(..=end).collect();
            let request: Value = serde_json::from_slice(&line).unwrap();
            lock(&self.seen).push(request.clone());
            if command.contains("herdr-api") {
                assert_eq!(request["method"], "agent.list");
                let herdr = self.agent["agent"] == "codex";
                return finish(
                    session,
                    channel,
                    json!({"id":request["id"],"result":{"type":"agent_list","agents":if herdr {vec![self.agent.clone()]} else {vec![]}}}),
                );
            }
            if request.get("method").is_none() {
                send(
                    session,
                    channel,
                    json!({"method":"serverRequest/resolved","params":{"threadId":THREAD,"requestId":request["id"]}}),
                )?;
                continue;
            }
            let result = match request["method"].as_str().unwrap() {
                "initialize" => json!({"serverInfo":{"name":"kite-harness"}}),
                "initialized" => continue,
                "model/list" => {
                    json!({"data":[{"model":"fixture","displayName":"Fixture","description":"","hidden":false,"isDefault":true,"defaultReasoningEffort":"high","supportedReasoningEfforts":[{"reasoningEffort":"high","description":"High"}],"inputModalities":["text"]}],"nextCursor":null})
                }
                "thread/loaded/list" => json!({"data":[THREAD]}),
                "config/read" => {
                    json!({"config":{"model":"fixture","model_reasoning_effort":"high","session_settings":[{"id":"approval_mode","name":"Tool approval","currentValue":"ask","options":[{"value":"ask","name":"Ask"},{"value":"risk","name":"Risk"},{"value":"bypass","name":"Allow"}]}]}})
                }
                "thread/resume" => {
                    send(
                        session,
                        channel,
                        json!({"id":"permission-1","method":"item/commandExecution/requestApproval","params":{"threadId":THREAD,"turnId":"turn-1","toolName":"bash","command":"df -h\nfree -h","cwd":"/owned","availableDecisions":["accept","decline"]}}),
                    )?;
                    json!({"thread":{"id":THREAD,"status":{"type":"active"}}})
                }
                "turn/start" => json!({"turn":{"id":"turn-2","status":"inProgress"}}),
                method => panic!("unexpected native request {method}"),
            };
            send(
                session,
                channel,
                json!({"id":request["id"],"result":result}),
            )?;
        }
        Ok(())
    }
}

struct Fixture {
    core: Arc<AppCore>,
    client: Arc<ChatClient>,
    agent: Agent,
    catalog: Arc<Mutex<Value>>,
    opened: Arc<Mutex<Value>>,
    connection: Arc<Connected>,
    seen: Arc<Mutex<Vec<Value>>>,
    ssh: Arc<Session>,
    listener: JoinHandle<()>,
    _directory: tempfile::TempDir,
}
impl Fixture {
    async fn start(provider: &str) -> Self {
        let key = kite_ssh::generate_ed25519("local-fixture").unwrap();
        let config = Arc::new(server::Config {
            keys: vec![PrivateKey::from_openssh(&key.private_key_openssh).unwrap()],
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let agent = json!({"terminal_id":format!("kite-harness-{THREAD}"),"pane_id":format!("kite-harness-{THREAD}"),"workspace_id":"owned","tab_id":"owned","agent":provider,"agent_status":"blocked","revision":1,"interactive_ready":true,"agent_session":{"source":"kite:harness","agent":provider,"kind":"id","value":THREAD}});
        let seen = Arc::new(Mutex::new(vec![]));
        let catalog = Arc::new(Mutex::new(json!({"revision":1,"now_ms":1,"sessions":[]})));
        let shared_catalog = catalog.clone();
        let opened = Arc::new(Mutex::new(Value::Null));
        let shared_opened = opened.clone();
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let records = seen.clone();
        let native_agent = agent.clone();
        let listener = tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let handler = Peer {
                    agent: native_agent.clone(),
                    catalog: shared_catalog.clone(),
                    opened: shared_opened.clone(),
                    seen: records.clone(),
                    channels: BTreeMap::new(),
                };
                let config = config.clone();
                tokio::spawn(async move {
                    if let Ok(session) = server::run_stream(config, socket, handler).await {
                        let _ = session.await;
                    }
                });
            }
        });
        let ssh = Arc::new(
            Session::connect(
                &Target {
                    host: "127.0.0.1".into(),
                    port,
                    username: "fixture".into(),
                },
                &key.private_key_openssh,
                Some(&key.public_key_openssh),
            )
            .await
            .unwrap(),
        );
        let agent: Agent = serde_json::from_value(agent).unwrap();
        let client = Arc::new(
            ChatClient::connect(&ssh, &agent.pane_id, THREAD)
                .await
                .unwrap(),
        );
        let core = AppCore::new(Arc::new(Secrets), Arc::new(ObserverSink));
        let directory = tempfile::tempdir().unwrap();
        *lock(&core.store) = Some(Store::open(directory.path()).unwrap());
        let connection = Arc::new(Connected {
            generation: 0,
            host: Host {
                id: "owned-host".into(),
                host: "127.0.0.1".into(),
                port,
                username: "fixture".into(),
                herdr_path: None,
                stream_host: None,
                sunshine_port: 47989,
            },
            ssh: ssh.clone(),
            api: Arc::new(Api::new(ssh.clone(), "fixture-herdr")),
            version: "fixture".into(),
            path: "fixture-herdr".into(),
            capability: String::new(),
        });
        core.chat_clients
            .lock()
            .await
            .insert(AgentKey::new(&connection.host.id, &agent), client.clone());
        *lock(&core.connected) = Some(connection.clone());
        core.select(
            Some(ConversationKey::new("owned-host", &agent.terminal_id)),
            |state| {
                state.selected_host = Some("owned-host".into());
                state.agents = vec![agent.clone()];
            },
        );
        core.update_live(
            &ConversationKey::new("owned-host", &agent.terminal_id),
            |live| {
                live.approval = client.runtime().approval();
                live.composer.selection = client.initial.clone();
            },
        );
        Self {
            core,
            client,
            agent,
            catalog,
            opened,
            connection,
            seen,
            ssh,
            listener,
            _directory: directory,
        }
    }
    async fn stop(self) {
        self.ssh.close().await.unwrap();
        self.listener.abort();
    }
}

#[tokio::test]
async fn approval_reaches_managed_owner_when_herdr_has_no_agent() {
    for provider in ["ply", "dsh", "codex"] {
        for choice in [0, 1] {
            let fixture = Fixture::start(provider).await;
            let request = fixture.client.runtime().approval().unwrap().id;
            assert!(
                fixture
                    .core
                    .respond_approval(
                        fixture.agent.terminal_id.clone(),
                        "rpc-stale".into(),
                        choice
                    )
                    .await
                    .is_err()
            );
            fixture
                .core
                .respond_approval(fixture.agent.terminal_id.clone(), request.clone(), choice)
                .await
                .unwrap();
            fixture
                .client
                .request("config/read", json!({}))
                .await
                .unwrap(); // ordered barrier after the response
            let replies: Vec<_> = lock(&fixture.seen)
                .iter()
                .filter(|request| request.get("method").is_none())
                .cloned()
                .collect();
            assert_eq!(replies.len(), 1);
            assert_eq!(replies[0]["id"], "permission-1");
            assert_eq!(
                replies[0]["result"]["decision"],
                if choice == 0 { "accept" } else { "decline" }
            );
            assert!(
                fixture
                    .core
                    .respond_approval(fixture.agent.terminal_id.clone(), request, choice)
                    .await
                    .is_err()
            );
            fixture.stop().await;
        }
    }
}

#[tokio::test]
async fn permission_choice_survives_model_changes_and_reaches_next_native_turn() {
    let fixture = Fixture::start("ply").await;
    let id = fixture.agent.terminal_id.clone();
    assert!(
        fixture
            .core
            .set_chat_setting(id.clone(), "approval_mode".into(), "invented".into())
            .await
            .is_err()
    );
    fixture
        .core
        .set_chat_setting(id.clone(), "approval_mode".into(), "risk".into())
        .await
        .unwrap();
    fixture
        .core
        .set_chat_model(id, "fixture".into(), "high".into())
        .await
        .unwrap();
    assert!(
        lock(&fixture.seen)
            .iter()
            .all(|request| request["method"] != "turn/start")
    );
    let selection = fixture.core.snapshot().composer.selection;
    assert_eq!(selection.settings["approval_mode"], "risk");
    fixture
        .client
        .send(&[Block::Text("owned test".into())], &selection, &[])
        .await
        .unwrap();
    let requests = lock(&fixture.seen).clone();
    let turn = requests
        .iter()
        .find(|request| request["method"] == "turn/start")
        .unwrap();
    assert_eq!(turn["params"]["sessionSettings"]["approval_mode"], "risk");
    let stored: BTreeMap<String, Selection> = lock(&fixture.core.store)
        .as_ref()
        .unwrap()
        .read_document("chat-settings")
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.values().next().unwrap().settings["approval_mode"],
        "risk"
    );
    fixture.stop().await;
}

#[tokio::test]
async fn approval_of_an_unselected_conversation_waits_and_shows_after_switching_back() {
    let fixture = Fixture::start("ply").await;
    let core = &fixture.core;
    let request = fixture.client.runtime().approval().unwrap().id;
    let waiting = ConversationKey::new("owned-host", "waiting");
    let other = ConversationKey::new("owned-host", "other");
    let mut bound =
        kite_agent::sessions::Record::draft("waiting".into(), "ply".into(), String::new(), None, 1);
    bound.reference = fixture.agent.agent_session.clone();
    let idle =
        kite_agent::sessions::Record::draft("other".into(), "ply".into(), String::new(), None, 1);
    let catalog = kite_agent::sessions::Catalog {
        revision: 1,
        now_ms: 1,
        sessions: vec![bound.clone(), idle.clone()],
    };
    *lock(&fixture.catalog) = serde_json::to_value(&catalog).unwrap();
    core.install_catalog("owned-host", &catalog).unwrap();
    core.select(Some(other.clone()), |s| {
        s.sessions = sessions::views(&catalog.sessions, &s.agents);
    });
    assert!(core.read_live(&waiting, |_| ()).is_none());
    assert!(core.snapshot().approval.is_none());

    // The request arrives while another conversation is on screen.
    core.refresh_session_activity(&fixture.connection, 0).await;
    let on_screen = core.snapshot();
    assert_eq!(on_screen.conversation_context(), "owned-host/other");
    assert!(on_screen.approval.is_none());
    assert_eq!(
        core.read_live(&waiting, |live| live
            .approval
            .as_ref()
            .map(|a| a.id.clone())),
        Some(Some(request.clone()))
    );

    // Switching back shows it at once, before any refresh reads it again.
    core.select(Some(waiting.clone()), |_| {});
    let back = core.snapshot();
    assert_eq!(back.conversation_context(), "owned-host/waiting");
    assert_eq!(
        back.selected_agent.as_ref(),
        Some(&fixture.agent.terminal_id)
    );
    assert_eq!(back.approval.as_ref().unwrap().id, request);

    // A real open keeps it, and it is answered through the owning conversation.
    core.select(Some(other.clone()), |_| {});
    assert!(core.snapshot().approval.is_none());
    core.open_session("waiting".into()).await.unwrap();
    assert_eq!(core.snapshot().approval.as_ref().unwrap().id, request);
    core.respond_approval(fixture.agent.terminal_id.clone(), request, 0)
        .await
        .unwrap();
    assert!(core.snapshot().approval.is_none());
    fixture.stop().await;
}

#[tokio::test]
async fn a_notice_routes_a_waiting_request_to_its_own_conversation() {
    let fixture = Fixture::start("dsh").await;
    let core = &fixture.core;
    let request = fixture.client.runtime().approval().unwrap().id;
    let waiting = ConversationKey::new("owned-host", "waiting");
    let mut bound =
        kite_agent::sessions::Record::draft("waiting".into(), "dsh".into(), String::new(), None, 1);
    bound.reference = fixture.agent.agent_session.clone();
    core.install_catalog(
        "owned-host",
        &kite_agent::sessions::Catalog {
            revision: 1,
            now_ms: 1,
            sessions: vec![bound],
        },
    )
    .unwrap();
    core.select(
        Some(ConversationKey::new("owned-host", "elsewhere")),
        |_| {},
    );
    core.route_notice(&fixture.connection, &waiting).await;
    assert!(core.snapshot().approval.is_none());
    assert_eq!(
        core.read_live(&waiting, |live| live
            .approval
            .as_ref()
            .map(|a| a.id.clone())),
        Some(Some(request))
    );
    // A notice about an unknown conversation changes nothing.
    core.route_notice(
        &fixture.connection,
        &ConversationKey::new("owned-host", "gone"),
    )
    .await;
    assert!(
        core.read_live(&ConversationKey::new("owned-host", "gone"), |_| ())
            .is_none()
    );
    fixture.stop().await;
}

#[tokio::test]
async fn a_slow_resume_binds_the_returned_pane_until_it_reports_the_same_thread() {
    let fixture = Fixture::start("ply").await;
    let core = &fixture.core;
    let key = ConversationKey::new("owned-host", "resumed");
    // Another thread than the fixture's own running pane.
    let reference = kite_herdr::api::AgentSession {
        source: "kite:harness".into(),
        agent: "ply".into(),
        kind: "id".into(),
        value: "87654321-4321-4321-4321-cba987654321".into(),
    };
    let mut record =
        kite_agent::sessions::Record::draft("resumed".into(), "ply".into(), String::new(), None, 1);
    record.reference = Some(reference.clone());
    let catalog = kite_agent::sessions::Catalog {
        revision: 1,
        now_ms: 1,
        sessions: vec![record],
    };
    *lock(&fixture.catalog) = serde_json::to_value(&catalog).unwrap();
    // The bounded wait in session-open ended before the pane reported its thread.
    let launching = json!({"terminal_id":"slow","pane_id":"slow-pane","workspace_id":"owned","tab_id":"owned","agent":"ply","agent_status":"working","revision":1,"launch_pending":true});
    *lock(&fixture.opened) = launching.clone();
    core.activate_session(&fixture.connection, "resumed")
        .await
        .unwrap();
    let opened = core.snapshot();
    assert_eq!(opened.selected_session.as_deref(), Some("resumed"));
    assert_eq!(opened.selected_agent.as_deref(), Some("slow"));

    // The catalog sync that follows does not undo it, and its history is not a failure.
    let mut pane: Agent = serde_json::from_value(launching).unwrap();
    core.sync_sessions(&fixture.connection, std::slice::from_ref(&pane), 0)
        .await
        .unwrap();
    assert_eq!(core.snapshot().selected_agent.as_deref(), Some("slow"));
    core.refresh_selected(&fixture.connection, 0, false)
        .await
        .unwrap();
    let starting = core.snapshot();
    assert_eq!(starting.selected_agent.as_deref(), Some("slow"));
    assert!(starting.history_status.waiting && !starting.history_status.failed);

    core.update_live(&key, |live| {
        live.response_activity = ResponseActivity::Sending;
        live.agent_output = "starting up".into();
    });
    pane.launch_pending = false;
    pane.agent_session = Some(reference);
    core.update(|s| s.agents = vec![pane]);
    let bound = core.snapshot();
    assert_eq!(bound.selected_agent.as_deref(), Some("slow"));
    assert_eq!(bound.response_activity, ResponseActivity::Sending);
    assert_eq!(bound.agent_output, "starting up");
    fixture.stop().await;
}
