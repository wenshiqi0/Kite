//! Real AppCore approval and interruption commands over local SSH, including managed owners absent from Herdr.
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
    seen: Arc<Mutex<Vec<Value>>>,
    channels: BTreeMap<ChannelId, (String, Vec<u8>)>,
    active: bool,
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
                "thread/turns/list" => {
                    json!({"data":if self.active { vec![json!({"id":"turn-1","status":"inProgress"})] } else { vec![] },"nextCursor":null})
                }
                "turn/interrupt" => {
                    assert_eq!(
                        request["params"],
                        json!({"threadId":THREAD,"turnId":"turn-1"})
                    );
                    assert!(self.active);
                    self.active = false;
                    send(
                        session,
                        channel,
                        json!({"method":"turn/completed","params":{"threadId":THREAD,"turn":{"id":"turn-1","status":"interrupted"}}}),
                    )?;
                    json!({})
                }
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
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let records = seen.clone();
        let native_agent = agent.clone();
        let listener = tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let handler = Peer {
                    agent: native_agent.clone(),
                    seen: records.clone(),
                    channels: BTreeMap::new(),
                    active: true,
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
            .insert(chat_key(&connection, &agent), client.clone());
        *lock(&core.connected) = Some(connection);
        core.update(|state| {
            state.selected_agent = Some(agent.terminal_id.clone());
            state.agents = vec![agent.clone()];
            state.approval = client.runtime().approval();
            state.composer.selection = client.initial.clone();
        });
        Self {
            core,
            client,
            agent,
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
async fn interruption_reaches_each_managed_owner_and_rejects_stale_or_paused_controls() {
    for provider in ["codex", "dsh", "ply"] {
        let fixture = Fixture::start(provider).await;
        let id = fixture.agent.terminal_id.clone();
        fixture.core.update(|s| {
            s.composer.ready = true;
            s.providers = vec![providers::Provider {
                id: provider.into(),
                managed_chat: true,
                models: true,
                attachments: true,
                skills: false,
                usage: false,
                pause: false,
                terminal: false,
                release: true,
            }];
        });
        assert!(
            fixture
                .core
                .interrupt_agent("stale-agent".into())
                .await
                .is_err()
        );
        fixture.core.update(|s| s.agent_pause = AgentPause::Paused);
        assert!(fixture.core.interrupt_agent(id.clone()).await.is_err());
        fixture.core.update(|s| s.agent_pause = AgentPause::Running);
        fixture.core.interrupt_agent(id.clone()).await.unwrap();
        assert_eq!(fixture.client.runtime().phase(), ResponseActivity::Idle);
        assert!(fixture.client.runtime().approval().is_none());
        // Tapping a now-stale stop action must not send another cancellation or a new turn.
        fixture.core.interrupt_agent(id).await.unwrap();
        let seen = lock(&fixture.seen).clone();
        assert_eq!(
            seen.iter()
                .filter(|r| r["method"] == "turn/interrupt")
                .count(),
            1
        );
        assert!(seen.iter().all(|r| r["method"] != "turn/start"));
        fixture.stop().await;
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
        .send("owned test", &[], &selection, &[])
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
