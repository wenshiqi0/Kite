use crate::KiteError;
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug, uniffi::Record)]
pub struct HostRecord {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub herdr_path: Option<String>,
    pub stream_host: Option<String>,
    pub sunshine_port: u16,
}
impl From<HostRecord> for kite_app::SavedHost {
    fn from(h: HostRecord) -> Self {
        Self {
            id: h.id,
            host: h.host,
            port: h.port,
            username: h.username,
            herdr_path: h.herdr_path,
            stream_host: h.stream_host,
            sunshine_port: h.sunshine_port,
        }
    }
}
impl From<kite_app::SavedHost> for HostRecord {
    fn from(h: kite_app::SavedHost) -> Self {
        Self {
            id: h.id,
            host: h.host,
            port: h.port,
            username: h.username,
            herdr_path: h.herdr_path,
            stream_host: h.stream_host,
            sunshine_port: h.sunshine_port,
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct TrustedHostRecord {
    pub address: String,
    pub public_key: String,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct LegacyData {
    pub source: String,
    pub host: Option<HostRecord>,
    pub trusted_keys: Vec<TrustedHostRecord>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct DeviceKeyInfo {
    pub public_key_openssh: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct HostKeyConfirmation {
    pub host_id: String,
    pub address: String,
    pub fingerprint: String,
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    AwaitingTrust,
    Connected,
    Failed,
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum DesktopStatus {
    Idle,
    Pairing,
    Starting,
    Streaming,
    Stopping,
    Failed,
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum FailureKind {
    Connection,
    Authentication,
    HostKeyChanged,
    HerdrMissing,
    HerdrUnsupported,
    HerdrApi,
    Storage,
    InvalidKey,
    InvalidConfiguration,
    Stream,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct FailureInfo {
    pub kind: FailureKind,
    pub details: String,
    pub username: String,
    pub trusted_fingerprint: String,
    pub presented_fingerprint: String,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct AgentInfo {
    pub id: String,
    pub name: Option<String>,
    pub agent: Option<String>,
    pub title: Option<String>,
    pub status: String,
    pub pane_id: String,
    pub cwd: Option<String>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct DesktopInfo {
    pub status: DesktopStatus,
    pub pin: Option<String>,
    pub pairing_name: Option<String>,
    pub stage: i32,
    pub received_frames: u64,
    pub rendered_frames: u64,
    pub takeover: bool,
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub fps: u32,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct CoreSnapshot {
    pub revision: u64,
    pub initialized: bool,
    pub device_key: Option<DeviceKeyInfo>,
    pub hosts: Vec<HostRecord>,
    pub selected_host: Option<String>,
    pub connection: ConnectionStatus,
    pub pending_host_key: Option<HostKeyConfirmation>,
    pub herdr_version: Option<String>,
    pub herdr_path: Option<String>,
    pub capability: Option<String>,
    pub host_key_fingerprint: Option<String>,
    pub agents: Vec<AgentInfo>,
    pub selected_agent: Option<String>,
    pub agent_output: String,
    pub desktop: DesktopInfo,
    pub failure: Option<FailureInfo>,
}

impl From<kite_app::Snapshot> for CoreSnapshot {
    fn from(s: kite_app::Snapshot) -> Self {
        use kite_app::{ConnectionPhase as C, DesktopPhase as D, FailureCode as F};
        Self {
            revision: s.revision,
            initialized: s.initialized,
            device_key: s.key.map(|k| DeviceKeyInfo {
                public_key_openssh: k.public_key,
                fingerprint: k.fingerprint,
            }),
            hosts: s.hosts.into_iter().map(Into::into).collect(),
            selected_host: s.selected_host,
            connection: match s.connection {
                C::Disconnected => ConnectionStatus::Disconnected,
                C::Connecting => ConnectionStatus::Connecting,
                C::AwaitingTrust => ConnectionStatus::AwaitingTrust,
                C::Connected => ConnectionStatus::Connected,
                C::Failed => ConnectionStatus::Failed,
            },
            pending_host_key: s.pending_key.map(|k| HostKeyConfirmation {
                host_id: k.host_id,
                address: k.address,
                fingerprint: k.fingerprint,
            }),
            herdr_version: s.herdr_version,
            herdr_path: s.herdr_path,
            capability: s.capability,
            host_key_fingerprint: s.host_key_fingerprint,
            agents: s
                .agents
                .into_iter()
                .map(|a| AgentInfo {
                    id: a.terminal_id,
                    name: a.name,
                    agent: a.agent,
                    title: a.title,
                    status: a.agent_status,
                    pane_id: a.pane_id,
                    cwd: a.cwd,
                })
                .collect(),
            selected_agent: s.selected_agent,
            agent_output: s.agent_output,
            desktop: DesktopInfo {
                status: match s.desktop.phase {
                    D::Idle => DesktopStatus::Idle,
                    D::Pairing => DesktopStatus::Pairing,
                    D::Starting => DesktopStatus::Starting,
                    D::Streaming => DesktopStatus::Streaming,
                    D::Stopping => DesktopStatus::Stopping,
                    D::Failed => DesktopStatus::Failed,
                },
                pin: s.desktop.pin,
                pairing_name: s.desktop.pairing_name,
                stage: s.desktop.stage,
                received_frames: s.desktop.received,
                rendered_frames: s.desktop.rendered,
                takeover: s.desktop.takeover,
                width: s.desktop.width,
                height: s.desktop.height,
                source_width: s.desktop.source_width,
                source_height: s.desktop.source_height,
                fps: s.desktop.fps,
            },
            failure: s.failure.map(|f| FailureInfo {
                kind: match f.code {
                    F::Connection => FailureKind::Connection,
                    F::Authentication => FailureKind::Authentication,
                    F::HostKeyChanged => FailureKind::HostKeyChanged,
                    F::HerdrMissing => FailureKind::HerdrMissing,
                    F::HerdrUnsupported => FailureKind::HerdrUnsupported,
                    F::HerdrApi => FailureKind::HerdrApi,
                    F::Storage => FailureKind::Storage,
                    F::InvalidKey => FailureKind::InvalidKey,
                    F::InvalidConfiguration => FailureKind::InvalidConfiguration,
                    F::Stream => FailureKind::Stream,
                },
                details: f.details,
                username: f.username,
                trusted_fingerprint: f.trusted,
                presented_fingerprint: f.presented,
            }),
        }
    }
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum SecretStoreError {
    #[error("secret storage: {details}")]
    Failure { details: String },
}
#[uniffi::export(callback_interface)]
pub trait SecretStore: Send + Sync {
    fn read(&self, name: String) -> Result<Option<Vec<u8>>, SecretStoreError>;
    fn write(&self, name: String, value: Vec<u8>) -> Result<(), SecretStoreError>;
}
#[uniffi::export(callback_interface)]
pub trait CoreObserver: Send + Sync {
    fn state_changed(&self, snapshot: CoreSnapshot);
}
struct Secrets(Box<dyn SecretStore>);
impl kite_app::SecretStorage for Secrets {
    fn read(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        self.0.read(name.to_owned()).map_err(|e| e.to_string())
    }
    fn write(&self, name: &str, value: &[u8]) -> Result<(), String> {
        self.0
            .write(name.to_owned(), value.to_vec())
            .map_err(|e| e.to_string())
    }
}
struct Observer(Box<dyn CoreObserver>);
impl kite_app::Observer for Observer {
    fn changed(&self, snapshot: kite_app::Snapshot) {
        self.0.state_changed(snapshot.into());
    }
}

#[derive(uniffi::Object)]
pub struct KiteCore {
    inner: Arc<kite_app::AppCore>,
}
#[uniffi::export]
impl KiteCore {
    #[uniffi::constructor]
    pub fn new(
        secret_store: Box<dyn SecretStore>,
        observer: Box<dyn CoreObserver>,
    ) -> Result<Arc<Self>, KiteError> {
        Ok(Arc::new(Self {
            inner: kite_app::AppCore::new(
                Arc::new(Secrets(secret_store)),
                Arc::new(Observer(observer)),
            ),
        }))
    }
    pub async fn initialize(
        &self,
        data_directory: String,
        device_name: String,
        legacy: LegacyData,
    ) -> Result<(), KiteError> {
        self.inner
            .initialize(
                PathBuf::from(data_directory),
                device_name,
                kite_app::LegacyImport {
                    source: legacy.source,
                    host: legacy.host.map(Into::into),
                    trusted_keys: legacy
                        .trusted_keys
                        .into_iter()
                        .map(|k| (k.address, k.public_key))
                        .collect(),
                },
            )
            .await
            .map_err(Into::into)
    }
    pub fn snapshot(&self) -> Result<CoreSnapshot, KiteError> {
        Ok(self.inner.snapshot().into())
    }
    pub async fn save_host(&self, host: HostRecord) -> Result<String, KiteError> {
        self.inner.save_host(host.into()).await.map_err(Into::into)
    }
    pub async fn connect(&self, host_id: String) -> Result<(), KiteError> {
        self.inner.connect(host_id).await.map_err(Into::into)
    }
    pub async fn trust_host(&self, host_id: String, fingerprint: String) -> Result<(), KiteError> {
        self.inner
            .trust_host(host_id, fingerprint)
            .await
            .map_err(Into::into)
    }
    pub async fn forget_host(&self, host_id: String) -> Result<(), KiteError> {
        self.inner.forget_host(host_id).await.map_err(Into::into)
    }
    pub async fn disconnect(&self) -> Result<(), KiteError> {
        self.inner.disconnect().await.map_err(Into::into)
    }
    pub async fn select_agent(&self, agent_id: String) -> Result<(), KiteError> {
        self.inner.select_agent(agent_id).await.map_err(Into::into)
    }
    pub async fn prompt_agent(&self, agent_id: String, text: String) -> Result<(), KiteError> {
        self.inner
            .prompt_agent(agent_id, text)
            .await
            .map_err(Into::into)
    }
    pub async fn clear_failure(&self) -> Result<(), KiteError> {
        self.inner.clear_failure().await.map_err(Into::into)
    }
    pub async fn start_desktop(&self, media_target: u64) -> Result<(), KiteError> {
        let sink = media_sink(media_target)?;
        self.inner.start_desktop(sink).await.map_err(Into::into)
    }
    pub async fn approve_pairing(&self, pin: String) -> Result<(), KiteError> {
        self.inner.approve_pairing(pin).await.map_err(Into::into)
    }
    pub async fn set_takeover(&self, enabled: bool) -> Result<(), KiteError> {
        self.inner.set_takeover(enabled).await.map_err(Into::into)
    }
    pub async fn stop_desktop(&self) -> Result<(), KiteError> {
        self.inner.stop_desktop().await.map_err(Into::into)
    }
}
fn media_sink(id: u64) -> Result<Arc<dyn kite_app::MediaSink>, KiteError> {
    #[cfg(target_os = "android")]
    {
        crate::android::media_sink(id).ok_or(KiteError::MediaUnavailable)
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = id;
        Err(KiteError::MediaUnavailable)
    }
}
