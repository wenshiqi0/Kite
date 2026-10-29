//! Herdr 0.9.5 NDJSON API, over exec channels of an existing SSH connection.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use kite_ssh::{Session, SshError};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};

const DEADLINE: Duration = Duration::from_secs(15);
const MAX_RESPONSE: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Agent {
    pub terminal_id: String,
    pub name: Option<String>,
    pub agent: Option<String>,
    pub title: Option<String>,
    pub agent_status: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub pane_id: String,
    pub cwd: Option<String>,
    pub revision: u64,
    #[serde(default)]
    pub launch_pending: bool,
    #[serde(default)]
    pub interactive_ready: bool,
    #[serde(default)]
    pub state_change_seq: u64,
    pub agent_session: Option<AgentSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AgentSession {
    pub source: String,
    pub agent: String,
    pub kind: String,
    pub value: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("herdr request timed out")]
    Timeout,
    #[error("herdr returned {code}: {details}")]
    Remote { code: String, details: String },
    #[error("unexpected herdr response: {0}")]
    Protocol(&'static str),
}

pub struct Api {
    session: Arc<Session>,
    command: String,
    sequence: AtomicU64,
}

impl Api {
    pub fn new(session: Arc<Session>, herdr_path: &str) -> Self {
        let quoted = format!("'{}'", herdr_path.replace('\'', "'\\''"));
        Self {
            session,
            command: format!("{quoted} remote-api-bridge"),
            sequence: AtomicU64::new(1),
        }
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, ApiError> {
        let id = format!("kite-{}", self.sequence.fetch_add(1, Ordering::Relaxed));
        let request = json!({"id": id, "method": method, "params": params});
        let mut bytes = serde_json::to_vec(&request)?;
        if bytes.len() as u64 >= MAX_RESPONSE {
            return Err(ApiError::Protocol("request too large"));
        }
        bytes.push(b'\n');
        tokio::time::timeout(DEADLINE, async {
            // The server handles one RPC per API socket. Reuse SSH, not an expired bridge.
            let mut stream = self.session.open_command(&self.command).await?;
            stream.write_all(&bytes).await?;
            stream.flush().await?;
            let result = read_response(&mut stream, &id).await;
            let _ = stream.shutdown().await;
            result
        })
        .await
        .map_err(|_| ApiError::Timeout)?
    }

    pub async fn ping(&self) -> Result<String, ApiError> {
        let value = self.request("ping", json!({})).await?;
        if value["type"] != "pong" {
            return Err(ApiError::Protocol("expected pong"));
        }
        Ok(value["version"]
            .as_str()
            .ok_or(ApiError::Protocol("missing version"))?
            .into())
    }

    pub async fn agents(&self) -> Result<Vec<Agent>, ApiError> {
        let value = self.request("agent.list", json!({})).await?;
        if value["type"] != "agent_list" {
            return Err(ApiError::Protocol("expected agent_list"));
        }
        Ok(serde_json::from_value(value["agents"].clone())?)
    }

    pub async fn create_workspace(
        &self,
        label: &str,
        cwd: Option<&str>,
    ) -> Result<(String, String), ApiError> {
        let value = self
            .request(
                "workspace.create",
                json!({"label":label,"cwd":cwd,"focus":false}),
            )
            .await?;
        let workspace = value["workspace"]["workspace_id"]
            .as_str()
            .ok_or(ApiError::Protocol("missing created workspace"))?;
        let pane = value["root_pane"]["pane_id"]
            .as_str()
            .ok_or(ApiError::Protocol("missing created pane"))?;
        Ok((workspace.into(), pane.into()))
    }

    pub async fn start_agent(
        &self,
        pane: &str,
        name: &str,
        provider: &str,
        args: &[String],
    ) -> Result<Agent, ApiError> {
        let value = self
            .request(
                "agent.start",
                json!({"pane_id":pane,"name":name,"kind":provider,"args":args}),
            )
            .await?;
        if value["type"] != "agent_started" {
            return Err(ApiError::Protocol("expected agent_started"));
        }
        Ok(serde_json::from_value(value["agent"].clone())?)
    }

    pub async fn read_agent(&self, target: &str) -> Result<String, ApiError> {
        let value = self.request("agent.read", json!({"target":target,"source":"visible","format":"text","lines":120,"strip_ansi":true})).await?;
        value["read"]["text"]
            .as_str()
            .map(str::to_owned)
            .ok_or(ApiError::Protocol("missing pane text"))
    }

    pub async fn read_pane(&self, pane: &str) -> Result<String, ApiError> {
        let value = self.request("pane.read", json!({"pane_id":pane,"source":"visible","format":"text","lines":80,"strip_ansi":true})).await?;
        value["read"]["text"]
            .as_str()
            .map(str::to_owned)
            .ok_or(ApiError::Protocol("missing pane text"))
    }

    pub async fn prompt_agent(&self, target: &str, text: &str) -> Result<(), ApiError> {
        self.request("agent.prompt", json!({"target":target,"text":text}))
            .await?;
        Ok(())
    }

    pub async fn send_keys(&self, target: &str, keys: &[String]) -> Result<(), ApiError> {
        self.request("agent.send_keys", json!({"target":target,"keys":keys}))
            .await?;
        Ok(())
    }

    pub async fn process_info(&self, pane_id: &str) -> Result<Value, ApiError> {
        self.request("pane.process_info", json!({"pane_id":pane_id}))
            .await
    }
}

async fn read_response(reader: impl AsyncRead + Unpin, id: &str) -> Result<Value, ApiError> {
    let mut reader = BufReader::new(reader.take(MAX_RESPONSE + 1));
    let mut bytes = Vec::new();
    reader.read_until(b'\n', &mut bytes).await?;
    if bytes.len() as u64 > MAX_RESPONSE || bytes.last() != Some(&b'\n') {
        return Err(ApiError::Protocol("oversized or incomplete response"));
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    if value["id"].as_str() != Some(id) {
        return Err(ApiError::Protocol("response ID mismatch"));
    }
    if let Some(error) = value.get("error") {
        return Err(ApiError::Remote {
            code: error["code"].as_str().unwrap_or("unknown").into(),
            details: error["message"].as_str().unwrap_or_default().into(),
        });
    }
    value
        .get("result")
        .cloned()
        .ok_or(ApiError::Protocol("missing result"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn decodes_real_envelope_shape_and_rejects_wrong_correlation() {
        let bytes = b"{\"id\":\"kite-1\",\"result\":{\"type\":\"pong\",\"version\":\"0.9.5\",\"protocol\":1}}\n";
        assert_eq!(
            read_response(&bytes[..], "kite-1").await.unwrap()["version"],
            "0.9.5"
        );
        assert!(matches!(
            read_response(&bytes[..], "kite-2").await,
            Err(ApiError::Protocol("response ID mismatch"))
        ));
    }

    #[tokio::test]
    async fn remote_errors_are_not_mistaken_for_empty_agent_lists() {
        let bytes = b"{\"id\":\"1\",\"error\":{\"code\":\"not_found\",\"message\":\"no agent\"}}\n";
        assert!(
            matches!(read_response(&bytes[..], "1").await, Err(ApiError::Remote { code, .. }) if code == "not_found")
        );
    }

    #[tokio::test]
    async fn rejects_unterminated_and_oversized_messages() {
        assert!(read_response(&b"{}"[..], "1").await.is_err());
        let bytes = vec![b'x'; (MAX_RESPONSE + 1) as usize];
        assert!(read_response(&bytes[..], "1").await.is_err());
    }
}
