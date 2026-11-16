//! Owned Android end-to-end host. Requires an explicit QA public key and loopback
//! port; all data is synthetic and the Herdr peer refuses work-starting methods.
use russh::{
    Channel, ChannelId,
    keys::{PrivateKey, PublicKey},
    server::{self, Auth, Msg, Session as ServerSession, VerifiedPublicKey},
};
use serde_json::json;
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::PathBuf,
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{io::AsyncWriteExt, net::TcpListener, process::Command};

struct Peer {
    root: PathBuf,
    authorized: PublicKey,
    commands: HashMap<ChannelId, (String, Vec<u8>)>,
}
impl server::Handler for Peer {
    type Error = russh::Error;
    async fn auth_publickey(
        &mut self,
        _: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
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
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }
    async fn exec_request(
        &mut self,
        ch: ChannelId,
        data: &[u8],
        s: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        self.commands
            .insert(ch, (String::from_utf8(data.to_vec()).unwrap(), vec![]));
        s.channel_success(ch)
    }
    async fn data(
        &mut self,
        ch: ChannelId,
        data: &[u8],
        s: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        self.commands
            .get_mut(&ch)
            .unwrap()
            .1
            .extend_from_slice(data);
        let ready = self.commands.get(&ch).is_some_and(|(command, input)| {
            command.contains("herdr-api") && input.contains(&b'\n')
        });
        if ready {
            self.channel_eof(ch, s).await?;
        }
        Ok(())
    }
    async fn channel_eof(
        &mut self,
        ch: ChannelId,
        s: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        let Some((cmd, input)) = self.commands.remove(&ch) else {
            return Ok(());
        };
        let mut child = Command::new("/bin/sh")
            .args(if cmd == "/bin/sh -s" {
                vec!["-s"]
            } else {
                vec!["-c", &cmd]
            })
            .env("HOME", &self.root)
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("KITE_NODE_SOCKET")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&input).await.unwrap();
        stdin.shutdown().await.unwrap();
        drop(stdin);
        let out = child.wait_with_output().await.unwrap();
        if !out.stdout.is_empty() {
            s.data(ch, out.stdout)?;
        }
        if !out.stderr.is_empty() {
            s.extended_data(ch, 1, out.stderr)?;
        }
        s.exit_status_request(ch, out.status.code().unwrap_or(1) as u32)?;
        s.eof(ch)?;
        s.close(ch)
    }
}
#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(
        args.len(),
        6,
        "ROOT NODE_BINARY AUTHORIZED_PUBLIC_KEY PORT A_OR_B"
    );
    let root = PathBuf::from(&args[1]);
    assert!(root.to_string_lossy().contains("kite-native-history-qa"));
    let node = fs::canonicalize(&args[2]).unwrap();
    let authorized = PublicKey::from_openssh(fs::read_to_string(&args[3]).unwrap().trim()).unwrap();
    let port: u16 = args[4].parse().unwrap();
    assert!((54000..55000).contains(&port));
    let label = &args[5];
    assert!(matches!(label.as_str(), "a" | "b"));
    fs::create_dir_all(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    for path in [".local/bin", ".config/kite", ".local/share/kite", ".codex"] {
        fs::create_dir_all(root.join(path)).unwrap();
    }
    let link = root.join(".local/bin/kite-node");
    if link.exists() {
        fs::remove_file(&link).unwrap();
    }
    symlink(&node, &link).unwrap();
    let bun = String::from_utf8(
        std::process::Command::new("/bin/sh")
            .args(["-c", "command -v bun"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();
    let launcher = root.join("owned-bun");
    fs::write(&launcher,format!("#!/bin/sh\nexport HOME={}\nunset CODEX_HOME CLAUDE_CONFIG_DIR DISPLAY XAUTHORITY\nexport KITE_ENABLE_DSH=0 KITE_ENABLE_PLY=0\nif test \"$2\" = catalog-read; then sleep 0.4; fi\nexec {} \"$@\"\n",kite_agent::composer::quote(root.to_str().unwrap()),kite_agent::composer::quote(&bun))).unwrap();
    fs::set_permissions(&launcher, fs::Permissions::from_mode(0o700)).unwrap();
    let herdr = root.join("owned-herdr");
    fs::write(&herdr,r#"#!/usr/bin/python3
import json,sys
if '--check' in sys.argv:print('herdr-api-bridge-v1');sys.exit(0)
if '--version' in sys.argv:print('herdr owned-history-fixture');sys.exit(0)
req=json.loads(sys.stdin.readline());method=req.get('method')
if method=='ping':result={'type':'pong','version':'owned-history-fixture'}
elif method=='agent.list':result={'agents':[]}
elif method=='workspace.list':result={'workspaces':[]}
else:print(json.dumps({'id':req.get('id'),'error':{'code':'owned-read-only','message':method}}));sys.exit(0)
print(json.dumps({'id':req.get('id'),'result':result}))
"#).unwrap();
    fs::set_permissions(&herdr, fs::Permissions::from_mode(0o700)).unwrap();
    let mut sessions = vec![];
    for index in 0..2 {
        let id = format!("{}{index}", label.repeat(31));
        let thread = format!("{label}0000000-0000-0000-0000-{index:012}");
        sessions.push(json!({"id":id,"provider":"codex","title":format!("Owned native {label} {index}"),"preview":"","automatic_title":false,"cwd":root,"archived":true,"created_ms":1,"updated_ms":1,"terminal_id":"","pane_id":"","workspace_id":"","agent_name":"","reference":{"source":"herdr:codex","agent":"codex","kind":"id","value":thread}}));
    }
    fs::write(
        root.join(".local/share/kite/sessions.json"),
        json!({"version":1,"revision":1,"sessions":sessions}).to_string(),
    )
    .unwrap();
    let seed = r#"import{Database}from'bun:sqlite';const db=new Database(process.argv[1]);const sessions=JSON.parse(process.argv[2]);
db.exec('CREATE TABLE IF NOT EXISTS thread_items(thread_id TEXT,item_id TEXT,item_type TEXT,item_json TEXT,rollout_ordinal INTEGER,updated_at_ordinal INTEGER,created_at_ms INTEGER);CREATE TABLE IF NOT EXISTS thread_turns(thread_id TEXT,turn_id TEXT,status TEXT,error_json TEXT,rollout_ordinal INTEGER,rollout_end_ordinal INTEGER);DELETE FROM thread_items;DELETE FROM thread_turns;');
const insert=db.query('INSERT INTO thread_items VALUES(?,?,?,?,?,?,?)');for(const session of sessions){const t=session.reference.value;let n=1;const add=(v)=>{insert.run(t,v.id,v.type,JSON.stringify(v),n,n,n);n++;};add({id:'question',type:'userMessage',content:[{type:'text',text:'Owned native question'}]});for(let i=0;i<450;i++)add({id:'tool-'+i,type:'commandExecution',command:'printf owned',cwd:'/owned',status:'completed',aggregatedOutput:'owned output '.repeat(600),exitCode:0});add({id:'answer',type:'agentMessage',phase:'final_answer',text:'NATIVE_HISTORY_FINAL_'+session.id});db.query('INSERT INTO thread_turns VALUES(?,?,?,?,?,?)').run(t,'turn','completed',null,1,n);}db.close();"#;
    let status = Command::new(&bun)
        .args(["-e", seed])
        .arg(root.join(".codex/thread_history_1.sqlite"))
        .arg(serde_json::to_string(&sessions).unwrap())
        .status()
        .await
        .unwrap();
    assert!(status.success());
    let config = root.join("node.json");
    fs::write(&config,json!({"node_id":format!("owned-native-{label}"),"machine":format!("Owned native {label}"),"socket":root.join(".local/state/kite/node/host.sock"),"backend":root.join("host.js"),"herdr":herdr,"bun":launcher,"systemctl":"/usr/bin/false","services":[],"environment":{}}).to_string()).unwrap();
    let mut process = Command::new(&node)
        .args(["serve", "--config"])
        .arg(&config)
        .env("HOME", &root)
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    for _ in 0..200 {
        if root.join(".local/state/kite/node/host.sock").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let key_file = root.join("owned-ssh-host-key");
    let key = if key_file.exists() {
        kite_ssh::describe_private_key(&fs::read_to_string(&key_file).unwrap()).unwrap()
    } else {
        let key = kite_ssh::generate_ed25519("owned-native-history-host").unwrap();
        fs::write(&key_file, &key.private_key_openssh).unwrap();
        fs::set_permissions(&key_file, fs::Permissions::from_mode(0o600)).unwrap();
        key
    };
    fs::write(root.join("host.json"),json!({"host":"10.0.2.2","port":port,"username":"owned","herdr":herdr,"fingerprint":key.fingerprint,"label":label,"sessions":sessions.iter().map(|s|s["id"].clone()).collect::<Vec<_>>()} ).to_string()).unwrap();
    let cfg = Arc::new(server::Config {
        keys: vec![PrivateKey::from_openssh(key.private_key_openssh).unwrap()],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
    println!("owned native history host {label} ready on {port}");
    loop {
        tokio::select! {
            _=process.wait()=>panic!("owned Node exited"),
            connection=listener.accept()=>{let(socket,_)=connection.unwrap();let peer=Peer{root:root.clone(),authorized:authorized.clone(),commands:HashMap::new()};let cfg=cfg.clone();tokio::spawn(async move{if let Ok(session)=server::run_stream(cfg,socket,peer).await{let _=session.await;}});}
        }
    }
}
