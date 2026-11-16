//! The actual client reader crosses authenticated SSH, the Node allowlist,
//! binary bridge frames and the bundled TypeScript projection. No model turn.
#![cfg(target_os = "linux")]
use super::*;
use kite_agent::{Role, composer, history};
use kite_herdr::api::AgentSession;
use kite_ssh::{Session as Ssh, Target};
use russh::{
    Channel, ChannelId,
    keys::{PrivateKey, PublicKey},
    server::{self, Auth, Msg, Session as ServerSession, VerifiedPublicKey},
};
use std::collections::HashMap;
use tokio::{io::AsyncWriteExt, net::TcpListener, process::Command};

struct Remote {
    authorized: PublicKey,
    root: PathBuf,
    legacy: bool,
    commands: HashMap<ChannelId, (String, Vec<u8>)>,
}
impl server::Handler for Remote {
    type Error = russh::Error;
    async fn auth_publickey(
        &mut self,
        _: &str,
        key: VerifiedPublicKey<'_>,
    ) -> std::result::Result<Auth, Self::Error> {
        Ok(if key.public_key.key_data() == self.authorized.key_data() {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }
    async fn channel_open_session(
        &mut self,
        _: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut ServerSession,
    ) -> std::result::Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        command: &[u8],
        session: &mut ServerSession,
    ) -> std::result::Result<(), Self::Error> {
        self.commands.insert(
            channel,
            (String::from_utf8(command.to_vec()).unwrap(), vec![]),
        );
        session.channel_success(channel)
    }
    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _: &mut ServerSession,
    ) -> std::result::Result<(), Self::Error> {
        self.commands
            .get_mut(&channel)
            .unwrap()
            .1
            .extend_from_slice(data);
        Ok(())
    }
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut ServerSession,
    ) -> std::result::Result<(), Self::Error> {
        let (command, input) = self.commands.remove(&channel).unwrap();
        let mut child = Command::new("/bin/sh")
            .args(if command == "/bin/sh -s" {
                vec!["-s"]
            } else {
                vec!["-c", &command]
            })
            .env("HOME", &self.root)
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env("KITE_TEST_LEGACY_HOST", if self.legacy { "1" } else { "0" })
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&input).await.unwrap();
        stdin.shutdown().await.unwrap();
        drop(stdin);
        let output = child.wait_with_output().await.unwrap();
        if !output.stdout.is_empty() {
            session.data(channel, output.stdout)?;
        }
        if !output.stderr.is_empty() {
            session.extended_data(channel, 1, output.stderr)?;
        }
        session.exit_status_request(channel, output.status.code().unwrap_or(1) as u32)?;
        session.eof(channel)?;
        session.close(channel)
    }
}
struct Fixture {
    root: tempfile::TempDir,
    ssh: Ssh,
    node: tokio::task::JoinHandle<Result<()>>,
    remote: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.node.abort();
        self.remote.abort();
    }
}
impl Fixture {
    async fn seed_native(&self, text: &str, id: &str, ordinal: u32) {
        let directory = self.root.path().join(".codex");
        fs::create_dir_all(&directory).unwrap();
        let script = r#"import {Database} from 'bun:sqlite';
const db=new Database(process.argv[1]);
db.exec('CREATE TABLE IF NOT EXISTS thread_items(thread_id TEXT,item_id TEXT,item_type TEXT,item_json TEXT,rollout_ordinal INTEGER,updated_at_ordinal INTEGER,created_at_ms INTEGER); CREATE TABLE IF NOT EXISTS thread_turns(thread_id TEXT,turn_id TEXT,status TEXT,error_json TEXT,rollout_ordinal INTEGER,rollout_end_ordinal INTEGER)');
const item=JSON.parse(process.argv[2]);const ordinal=Number(process.argv[3]);
db.query('INSERT INTO thread_items VALUES(?,?,?,?,?,?,?)').run('12345678-1234-1234-1234-123456789abc',item.id,item.type,JSON.stringify(item),ordinal,ordinal,ordinal);db.close();"#;
        let output = Command::new(std::env::var("KITE_BUN").unwrap_or_else(|_| "bun".into()))
            .args(["-e", script])
            .arg(directory.join("thread_history_1.sqlite"))
            .arg(
                json!({"id": id, "type": "agentMessage", "text": text, "phase": "final_answer"})
                    .to_string(),
            )
            .arg(ordinal.to_string())
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    async fn new(legacy: bool) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::Builder::new()
            .prefix("kite-history SSH ")
            .tempdir()
            .unwrap();
        let mut config = super::tests::config(root.path());
        config.bun = root.path().join("owned-bun");
        config.backend = root.path().join("host.js");
        let bun = std::env::var("KITE_BUN").unwrap_or_else(|_| "bun".into());
        fs::write(
            &config.bun,
            format!(
                "#!/bin/sh\nexport HOME={}\nunset CODEX_HOME CLAUDE_CONFIG_DIR\nexec {} \"$@\"\n",
                composer::quote(root.path().to_str().unwrap()),
                composer::quote(&bun)
            ),
        )
        .unwrap();
        fs::set_permissions(&config.bun, fs::Permissions::from_mode(0o700)).unwrap();
        let bin = root.path().join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        // A transport-only CLI peer: the Rust Node still validates and executes
        // every backend action against the real bundled host adapter.
        let helper = bin.join("kite-node");
        fs::write(&helper, format!(r#"#!/usr/bin/python3
import json,os,socket,struct,sys
args=sys.argv[1:]
legacy=os.environ.get('KITE_TEST_LEGACY_HOST')=='1'
if legacy and '--provider' in args:
    sys.stderr.write('node protocol: backend option denied\n');sys.exit(1)
method='status' if args==['api-version'] else 'backend'
s=socket.socket(socket.AF_UNIX);s.connect({socket})
s.sendall((json.dumps({{'method':method,'args':[] if method=='status' else args[1:],'params':{{}}}})+'\n').encode())
f=s.makefile('rb',buffering=0)
reply=json.loads(f.readline())
if not reply.get('ok'):
    sys.stderr.write(reply.get('error','bridge rejected'));sys.exit(1)
if method=='status':
    result=reply['result']
    if legacy:
        result.pop('historyProjectionApi',None)
        result.pop('historyNativeApi',None)
    print(json.dumps(result));sys.exit(0)
s.sendall(b'\x04\0\0\0\0')
def read(n):
    data=b''
    while len(data)<n:
        block=f.read(n-len(data))
        if not block: raise EOFError('incomplete bridge frame')
        data+=block
    return data
while True:
    kind,length=struct.unpack('>BI',read(5));data=read(length)
    if kind==1: sys.stdout.buffer.write(data);sys.stdout.buffer.flush()
    elif kind==2: sys.stderr.buffer.write(data);sys.stderr.buffer.flush()
    elif kind==3: sys.exit(struct.unpack('>i',data)[0])
"#, socket=serde_json::to_string(&config.socket).unwrap())).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let socket = config.socket.clone();
        let node = tokio::spawn(serve(config));
        for _ in 0..200 {
            if socket.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(socket.exists());
        let client = kite_ssh::generate_ed25519("history-reinstall-test").unwrap();
        let authorized = PublicKey::from_openssh(&client.public_key_openssh).unwrap();
        let host = kite_ssh::generate_ed25519("owned-ssh-host").unwrap();
        let trusted = host.public_key_openssh;
        let server_config = Arc::new(server::Config {
            keys: vec![PrivateKey::from_openssh(host.private_key_openssh).unwrap()],
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let target = Target {
            host: "127.0.0.1".into(),
            port: listener.local_addr().unwrap().port(),
            username: "owned".into(),
        };
        let home = root.path().to_path_buf();
        let remote = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let handler = Remote {
                authorized,
                root: home,
                legacy,
                commands: HashMap::new(),
            };
            let session = server::run_stream(server_config, socket, handler)
                .await
                .unwrap();
            let _ = session.await;
        });
        let ssh = Ssh::connect(&target, &client.private_key_openssh, Some(&trusted))
            .await
            .unwrap();
        Self {
            root,
            ssh,
            node,
            remote,
        }
    }
    fn history(&self, provider: &str, records: &[Value]) -> (AgentSession, PathBuf) {
        let path = if provider == "codex" {
            self.root
                .path()
                .join(".codex/sessions/2026/11/11/rollout-owned-owned-thread.jsonl")
        } else {
            self.root
                .path()
                .join(".claude/projects/owned/owned-thread.jsonl")
        };
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            &path,
            records
                .iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>(),
        )
        .unwrap();
        (
            AgentSession {
                source: format!("herdr:{provider}"),
                agent: provider.into(),
                kind: "id".into(),
                value: "owned-thread".into(),
            },
            path,
        )
    }
}

#[tokio::test]
async fn native_paginated_history_with_no_rollout_reaches_the_real_client_and_updates_after_append()
{
    let fixture = Fixture::new(false).await;
    fixture
        .seed_native("Existing paginated substantive answer", "native-first", 1)
        .await;
    assert!(!fixture.root.path().join(".codex/sessions").exists());
    let reference = AgentSession {
        source: "herdr:codex".into(),
        agent: "codex".into(),
        kind: "id".into(),
        value: "12345678-1234-1234-1234-123456789abc".into(),
    };
    let first = history::read(&fixture.ssh, &reference, None).await;
    assert!(first.available, "{}", first.problem);
    assert_eq!(
        first.items.last().unwrap().text(),
        "Existing paginated substantive answer"
    );
    let unchanged = history::read(&fixture.ssh, &reference, Some(first.clone())).await;
    assert!(Arc::ptr_eq(&first, &unchanged));
    fixture
        .seed_native("New paginated substantive answer", "native-second", 2)
        .await;
    let second = history::read(&fixture.ssh, &reference, Some(first)).await;
    assert!(second.available, "{}", second.problem);
    assert_eq!(second.items.len(), 2);
    assert_eq!(
        second.items.last().unwrap().text(),
        "New paginated substantive answer"
    );
    let reinstalled = history::read(&fixture.ssh, &reference, None).await;
    assert_eq!(second.items, reinstalled.items);
    fixture.ssh.close().await.unwrap();
}
fn codex_message(id: &str, text: &str) -> Value {
    json!({"type":"response_item","payload":{"id":id,"type":"message","role":"assistant","phase":"final_answer","content":[{"type":"output_text","text":text}]}})
}

#[tokio::test]
async fn no_cache_history_and_new_substantive_replies_cross_ssh_and_the_real_node_bridge() {
    for legacy in [true, false] {
        let fixture = Fixture::new(legacy).await;
        let records = vec![
            json!({"type":"response_item","payload":{"id":"progress","type":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"Thinking and using tools"}]}}),
            json!({"type":"response_item","payload":{"type":"function_call","call_id":"owned-call","name":"read_file","arguments":"{}"}}),
            json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"owned-call","output":"owned tool result"}}),
            codex_message("final", "Substantive existing answer"),
        ];
        let (reference, path) = fixture.history("codex", &records);
        let fresh = history::read(&fixture.ssh, &reference, None).await;
        assert!(fresh.available, "legacy={legacy}: {}", fresh.problem);
        assert_eq!(
            fresh.items.last().unwrap().text(),
            "Substantive existing answer"
        );
        assert_eq!(fresh.items.last().unwrap().role, Role::Assistant);
        assert!(!fresh.items.last().unwrap().intermediate);
        assert!(
            fresh
                .items
                .iter()
                .find(|item| item.id == "owned-call")
                .unwrap()
                .tool()
                .unwrap()
                .complete
        );
        let unchanged = history::read(&fixture.ssh, &reference, Some(fresh.clone())).await;
        assert!(Arc::ptr_eq(&fresh, &unchanged));
        let mut records = records;
        records.push(codex_message("next-final", "Substantive new answer"));
        fs::write(
            &path,
            records
                .iter()
                .map(|row| format!("{row}\n"))
                .collect::<String>(),
        )
        .unwrap();
        let next = history::read(&fixture.ssh, &reference, Some(fresh)).await;
        assert!(next.available, "{}", next.problem);
        assert_eq!(next.items.last().unwrap().text(), "Substantive new answer");
        let reinstalled = history::read(&fixture.ssh, &reference, None).await;
        assert_eq!(reinstalled.items, next.items);
        fixture.ssh.close().await.unwrap();
    }
}

#[tokio::test]
async fn claude_uses_advertised_projection_and_retains_raw_history_on_older_hosts() {
    for legacy in [true, false] {
        let fixture = Fixture::new(legacy).await;
        let (reference,_)=fixture.history("claude",&[
            json!({"uuid":"user","type":"user","message":{"content":[{"type":"text","text":"owned question"}]}}),
            json!({"uuid":"answer","type":"assistant","message":{"content":[{"type":"text","text":"Substantive Claude answer"}]}}),
        ]);
        let history = history::read(&fixture.ssh, &reference, None).await;
        assert!(history.available, "legacy={legacy}: {}", history.problem);
        assert_eq!(history.items.len(), 2);
        assert_eq!(
            history.items.last().unwrap().text(),
            "Substantive Claude answer"
        );
        assert_eq!(history.page.is_some(), !legacy);
        fixture.ssh.close().await.unwrap();
    }
}
