//! Codex app-server v2 over an authenticated SSH exec channel. One host owner per pane.
use crate::interaction::{Answer, RuntimeState};
use kite_ssh::{ByteStream, Session};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, WriteHalf},
    sync::{Mutex as AsyncMutex, oneshot},
    task::JoinHandle,
};

const DEADLINE: Duration = Duration::from_secs(25);
const MAX_LINE: usize = 2 * 1024 * 1024;
pub const MAX_FILE: u64 = 25 * 1024 * 1024;
const HELPER: &str = "$HOME/.local/lib/kite/backend-v2.py";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Ssh(#[from] kite_ssh::SshError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("Codex backend: {0}")]
    Remote(String),
    #[error(
        "Codex request timed out; delivery may have occurred, inspect the conversation before resubmitting"
    )]
    Timeout,
    #[error("invalid composer input: {0}")]
    Invalid(&'static str),
    #[error("session catalog revision conflict")]
    Conflict,
}
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
pub fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
pub fn helper_command(args: &str) -> String {
    format!("python3 \"{HELPER}\" {args}")
}

/// The versioned helper contains no keys or user-specific configuration.
pub async fn install(ssh: &Session) -> Result<(), Error> {
    let install = r#"import os,pathlib,sys,tempfile
os.umask(0o077)
p=pathlib.Path.home()/'.local/lib/kite'
p.mkdir(parents=True,exist_ok=True,mode=0o700)
s=sys.stdin.buffer.read(65537)
assert 0<len(s)<=65536
with tempfile.NamedTemporaryFile(dir=p,delete=False) as f:
    f.write(s);f.flush();os.fsync(f.fileno());t=pathlib.Path(f.name)
t.chmod(0o700);t.replace(p/'backend-v2.py')
"#;
    let output = ssh
        .exec(
            &format!("python3 -c {}", quote(install)),
            include_bytes!("../../../backend/kite_codex.py"),
        )
        .await?;
    if output.exit_status != Some(0) {
        return Err(Error::Remote("backend helper installation failed".into()));
    }
    Ok(())
}

pub async fn launch_args(ssh: &Session, pane: &str, herdr: &str) -> Result<Vec<String>, Error> {
    let out = ssh
        .exec(
            &helper_command(&format!(
                "ensure --pane {} --herdr {}",
                quote(pane),
                quote(herdr)
            )),
            &[],
        )
        .await?;
    if out.exit_status != Some(0) {
        return Err(Error::Remote(
            String::from_utf8_lossy(&out.stderr)
                .chars()
                .take(1024)
                .collect(),
        ));
    }
    let v: Value = serde_json::from_slice(&out.stdout)?;
    let socket = v["socket"]
        .as_str()
        .filter(|s| s.starts_with('/') && !s.chars().any(char::is_control))
        .ok_or(Error::Invalid("backend socket"))?;
    Ok(vec![
        "--remote".into(),
        format!("unix://{socket}"),
        "-c".into(),
        "check_for_update_on_startup=false".into(),
    ])
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Effort {
    pub reasoning_effort: String,
    pub description: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub model: String,
    pub display_name: String,
    pub description: String,
    pub hidden: bool,
    pub is_default: bool,
    pub default_reasoning_effort: String,
    pub supported_reasoning_efforts: Vec<Effort>,
    pub input_modalities: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Selection {
    pub model: String,
    pub effort: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub path: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upload {
    pub attachment: Attachment,
    pub sent: u64,
    pub failed: bool,
    pub local_path: String,
}
#[derive(Clone, Debug, Default)]
pub struct Composer {
    pub ready: bool,
    pub loading: bool,
    pub details: String,
    pub models: Vec<Model>,
    pub selection: Selection,
    pub uploads: Vec<Upload>,
}

type Replies = Arc<Mutex<BTreeMap<String, oneshot::Sender<Result<Value, String>>>>>;
pub struct Client {
    writer: Arc<AsyncMutex<WriteHalf<ByteStream>>>,
    replies: Replies,
    sequence: AtomicU64,
    closed: Arc<AtomicBool>,
    reader: JoinHandle<()>,
    runtime: Arc<Mutex<RuntimeState>>,
    subscribed: AtomicBool,
    pub thread: String,
    pub models: Vec<Model>,
    pub initial: Selection,
}
impl Drop for Client {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl Client {
    pub async fn connect(ssh: &Session, pane: &str, thread: &str) -> Result<Self, Error> {
        install(ssh).await?;
        if thread.len() != 36 || !thread.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') {
            return Err(Error::Invalid("thread ID"));
        }
        let stream = ssh
            .open_command(&helper_command(&format!("proxy --pane {}", quote(pane))))
            .await?;
        let (reader, writer) = tokio::io::split(stream);
        let writer = Arc::new(AsyncMutex::new(writer));
        let replies: Replies = Arc::new(Mutex::new(BTreeMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let runtime = Arc::new(Mutex::new(RuntimeState::default()));
        let (rx_replies, rx_closed, rx_runtime, rx_thread) = (
            replies.clone(),
            closed.clone(),
            runtime.clone(),
            thread.to_owned(),
        );
        let reader = tokio::spawn(async move {
            let mut reader = BufReader::new(reader);
            while let Ok(value) = read_json(&mut reader).await {
                lock(&rx_runtime).ingest(&value, &rx_thread);
                if value.get("method").is_none()
                    && let Some(id) = value["id"].as_str()
                    && let Some(reply) = lock(&rx_replies).remove(id)
                {
                    let result = value
                        .get("result")
                        .cloned()
                        .ok_or_else(|| value["error"].to_string());
                    let _ = reply.send(result);
                }
            }
            rx_closed.store(true, Ordering::SeqCst);
            let mut state = lock(&rx_runtime);
            if !matches!(
                state.phase(),
                crate::interaction::Activity::Idle | crate::interaction::Activity::Failed
            ) {
                state.disconnected();
            }
            drop(state);
            for (_, reply) in std::mem::take(&mut *lock(&rx_replies)) {
                let _ = reply.send(Err("backend channel closed".into()));
            }
        });
        let mut client = Self {
            writer,
            replies,
            sequence: AtomicU64::new(1),
            closed,
            reader,
            runtime,
            subscribed: AtomicBool::new(false),
            thread: thread.into(),
            models: vec![],
            initial: Selection::default(),
        };
        client
            .request(
                "initialize",
                json!({"clientInfo":{"name":"kite","title":"Kite","version":"0.2.0"},"capabilities":{"experimentalApi":true}}),
            )
            .await?;
        client.write(json!({"method":"initialized"})).await?;
        let mut cursor: Option<String> = None;
        for _ in 0..8 {
            let page = client
                .request(
                    "model/list",
                    json!({"limit":100,"cursor":cursor,"includeHidden":false}),
                )
                .await?;
            let models: Vec<Model> = serde_json::from_value(page["data"].clone())?;
            client
                .models
                .extend(models.into_iter().filter(|m| !m.hidden));
            cursor = page["nextCursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        // The attached herdr TUI already owns this thread, including before its first
        // persisted turn. Never resume it through a second owner or require a rollout.
        let loaded = client.request("thread/loaded/list", json!({})).await?;
        if !loaded["data"]
            .as_array()
            .is_some_and(|ids| ids.iter().any(|id| id == thread))
        {
            return Err(Error::Invalid("pane thread is not loaded in its owner"));
        }
        let configuration = client
            .request("config/read", json!({"includeLayers":false}))
            .await?;
        client.initial = Selection {
            model: configuration["config"]["model"]
                .as_str()
                .unwrap_or_default()
                .into(),
            effort: configuration["config"]["model_reasoning_effort"]
                .as_str()
                .unwrap_or_default()
                .into(),
        };
        client.initial = selection(&client.models, &client.initial)?;
        client.observe_thread().await?;
        Ok(client)
    }
    pub fn runtime(&self) -> RuntimeState {
        lock(&self.runtime).clone()
    }
    pub fn is_observing(&self) -> bool {
        self.subscribed.load(Ordering::SeqCst)
    }
    pub async fn observe_thread(&self) -> Result<(), Error> {
        if self.subscribed.load(Ordering::SeqCst) {
            return Ok(());
        }
        match self
            .request(
                "thread/resume",
                json!({"threadId":self.thread,"excludeTurns":true}),
            )
            .await
        {
            Ok(value) => {
                if value["thread"]["id"] != self.thread {
                    return Err(Error::Invalid("resumed thread mismatch"));
                }
                self.subscribed.store(true, Ordering::SeqCst);
                if value["thread"].get("status").is_some() {
                    lock(&self.runtime).ingest(&json!({"method":"thread/status/changed","params":{"threadId":self.thread,"status":value["thread"]["status"]}}),&self.thread);
                }
                Ok(())
            }
            Err(Error::Remote(details)) if details.contains("no rollout found for thread id") => {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
    pub async fn answer(&self, id: &str, answers: &[Answer]) -> Result<(), Error> {
        let reply = lock(&self.runtime).question_reply(id, answers)?;
        self.write(reply).await?;
        lock(&self.runtime).answered(id);
        Ok(())
    }
    pub async fn approve(&self, id: &str, choice: u32) -> Result<(), Error> {
        let reply = lock(&self.runtime).approval_reply(id, choice)?;
        self.write(reply).await?;
        lock(&self.runtime).answered(id);
        Ok(())
    }
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }
    async fn write(&self, value: Value) -> Result<(), Error> {
        let mut bytes = serde_json::to_vec(&value)?;
        if bytes.len() > MAX_LINE {
            return Err(Error::Invalid("request too large"));
        }
        bytes.push(b'\n');
        tokio::time::timeout(DEADLINE, async {
            let mut w = self.writer.lock().await;
            w.write_all(&bytes).await?;
            w.flush().await
        })
        .await
        .map_err(|_| Error::Timeout)??;
        Ok(())
    }
    pub async fn request(&self, method: &str, params: Value) -> Result<Value, Error> {
        if self.is_closed() {
            return Err(Error::Remote("backend disconnected".into()));
        }
        let id = format!("kite-{}", self.sequence.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = oneshot::channel();
        lock(&self.replies).insert(id.clone(), tx);
        if let Err(e) = self
            .write(json!({"id":id,"method":method,"params":params}))
            .await
        {
            lock(&self.replies).remove(&id);
            return Err(e);
        }
        let response = tokio::time::timeout(DEADLINE, rx).await;
        lock(&self.replies).remove(&id);
        response
            .map_err(|_| Error::Timeout)?
            .map_err(|_| Error::Remote("backend disconnected".into()))?
            .map_err(Error::Remote)
    }
    pub async fn send(
        &self,
        text: &str,
        attachments: &[Attachment],
        selected: &Selection,
    ) -> Result<String, Error> {
        let selected = selection(&self.models, selected)?;
        let model = self
            .models
            .iter()
            .find(|m| m.model == selected.model)
            .ok_or(Error::Invalid("model unavailable"))?;
        let text = compose_text(text, attachments)?;
        let mut input = vec![json!({"type":"text","text":text})];
        for a in attachments {
            if a.mime.starts_with("image/") {
                if !model.input_modalities.iter().any(|v| v == "image") {
                    return Err(Error::Invalid("selected model does not accept images"));
                }
                input.push(json!({"type":"localImage","path":a.path}));
            } else if a.mime.starts_with("audio/")
                && model.input_modalities.iter().any(|v| v == "audio")
            {
                input.push(json!({"type":"localAudio","path":a.path}));
            }
        }
        if lock(&self.runtime).question().is_some_and(|q| q.blocking) {
            return Err(Error::Invalid("answer the pending model question"));
        }
        lock(&self.runtime).begin();
        if let Err(error)=self.request("turn/start", json!({"threadId":self.thread,"input":input,"model":selected.model,"effort":selected.effort})).await {
            lock(&self.runtime).failed(error.to_string());return Err(error);
        }
        lock(&self.runtime).accepted();
        for _ in 0..3 {
            let _ = self.observe_thread().await;
            if self.subscribed.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(text)
    }
}

pub fn selection(models: &[Model], selected: &Selection) -> Result<Selection, Error> {
    let model = models
        .iter()
        .find(|m| m.model == selected.model)
        .or_else(|| models.iter().find(|m| m.is_default))
        .ok_or(Error::Invalid("no available models"))?;
    let effort = if model
        .supported_reasoning_efforts
        .iter()
        .any(|e| e.reasoning_effort == selected.effort)
    {
        &selected.effort
    } else {
        &model.default_reasoning_effort
    };
    Ok(Selection {
        model: model.model.clone(),
        effort: effort.clone(),
    })
}

pub fn compose_text(text: &str, files: &[Attachment]) -> Result<String, Error> {
    if files.is_empty() {
        return Ok(text.into());
    }
    Ok(format!(
        "{text}\n\n<kite_attachments>\n{}\n</kite_attachments>",
        serde_json::to_string(files)?
    ))
}
pub fn split_attachments(text: &str) -> (String, Vec<Attachment>) {
    if let Some((body, tail)) = text.rsplit_once("\n\n<kite_attachments>\n")
        && let Some(json) = tail.strip_suffix("\n</kite_attachments>")
        && let Ok(files) = serde_json::from_str::<Vec<Attachment>>(json)
        && files.len() <= 5
    {
        return (body.into(), files);
    }
    (text.into(), vec![])
}

async fn read_json(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Value, Error> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return Err(Error::Remote("incomplete backend response".into()));
        }
        let end = buffer.iter().position(|b| *b == b'\n').map(|p| p + 1);
        let count = end.unwrap_or(buffer.len());
        if bytes.len() + count > MAX_LINE {
            return Err(Error::Invalid("backend response too large"));
        }
        bytes.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(serde_json::from_slice(&bytes)?);
        }
    }
}

pub async fn upload(
    ssh: &Session,
    local: &str,
    id: &str,
    name: &str,
    mime: &str,
    progress: impl Fn(u64),
) -> Result<Attachment, Error> {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '/' | '\\'))
        .take(80)
        .collect();
    if name.is_empty()
        || matches!(name.as_str(), "." | "..")
        || name.len() > 180
        || mime.len() > 128
    {
        return Err(Error::Invalid("attachment metadata"));
    }
    let mut file = tokio::fs::File::open(Path::new(local)).await?;
    let meta = file.metadata().await?;
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_FILE {
        return Err(Error::Invalid("attachment size; maximum 25 MiB"));
    }
    let size = meta.len();
    let mut checksum = openssl::sha::Sha256::new();
    let mut buf = vec![0u8; 65536];
    let mut read = 0u64;
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        checksum.update(&buf[..n]);
        read += n as u64;
        if read > size {
            return Err(Error::Invalid("attachment changed"));
        }
    }
    if read != size {
        return Err(Error::Invalid("attachment changed"));
    }
    let digest = hex::encode(checksum.finish());
    use tokio::io::AsyncSeekExt;
    file.rewind().await?;
    let command = helper_command(&format!(
        "upload --id {} --name {} --size {size} --sha256 {}",
        quote(id),
        quote(&name),
        quote(&digest)
    ));
    tokio::time::timeout(Duration::from_secs(120), async {
        let mut stream = ssh.open_command(&command).await?;
        let mut sent = 0u64;
        while sent < size {
            let n = file.read(&mut buf).await?;
            if n == 0 {
                return Err(Error::Invalid("attachment changed"));
            }
            let n = n.min((size - sent) as usize);
            stream.write_all(&buf[..n]).await?;
            sent += n as u64;
            progress(sent);
        }
        stream.flush().await?;
        let reply = read_json(&mut BufReader::new(stream)).await?;
        if reply["sha256"] != digest || reply["size"] != size {
            return Err(Error::Invalid("upload receipt mismatch"));
        }
        let path = reply["path"]
            .as_str()
            .filter(|p| p.starts_with('/'))
            .ok_or(Error::Invalid("upload path"))?;
        Ok(Attachment {
            id: id.into(),
            name,
            mime: mime.into(),
            size,
            path: path.into(),
        })
    })
    .await
    .map_err(|_| Error::Timeout)?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn rpc_frames_are_bounded_and_require_complete_json() {
        assert!(
            read_json(&mut BufReader::new(&b"{\"id\":1}"[..]))
                .await
                .is_err()
        );
        assert!(
            read_json(&mut BufReader::new(&vec![b'x'; MAX_LINE + 1][..]))
                .await
                .is_err()
        );
        let mut r = BufReader::new(&b"{\"id\":1}\n{\"id\":2}\n"[..]);
        assert_eq!(read_json(&mut r).await.unwrap()["id"], 1);
        assert_eq!(read_json(&mut r).await.unwrap()["id"], 2);
    }
    #[test]
    fn attachments_survive_history_without_leaking_transport_markup_into_chat() {
        let files = vec![Attachment {
            id: "a".into(),
            name: "photo.png".into(),
            mime: "image/png".into(),
            size: 3,
            path: "/home/user/.local/share/kite/uploads/a/photo.png".into(),
        }];
        let text = compose_text("Look at this", &files).unwrap();
        assert_eq!(split_attachments(&text), ("Look at this".into(), files));
        assert!(split_attachments("<kite_attachments>broken").1.is_empty());
    }
}
