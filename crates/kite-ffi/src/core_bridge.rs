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
pub enum ConnectionStep {
    Idle,
    Ssh,
    HerdrDiscovery,
    HerdrApi,
    Agent,
    Ready,
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
    AgentStart,
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
    pub can_pause: bool,
    pub launch_pending: bool,
    pub managed_ready: bool,
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum MessageKind {
    User,
    Assistant,
    Tool,
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum AgentPauseStatus {
    Running,
    Paused,
    Unknown,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ToolInfo {
    pub label: String,
    pub summary: String,
    pub name: String,
    pub arguments: String,
    pub output: String,
    pub complete: bool,
    pub failed: bool,
    pub computer: bool,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ChatModelInfo {
    pub model: String,
    pub name: String,
    pub description: String,
    pub efforts: Vec<String>,
    pub default_effort: String,
    pub modalities: Vec<String>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct AttachmentInfo {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
}
impl From<kite_app::ChatAttachment> for AttachmentInfo {
    fn from(a: kite_app::ChatAttachment) -> Self {
        Self {
            id: a.id,
            name: a.name,
            mime: a.mime,
            size: a.size,
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct UploadInfo {
    pub file: AttachmentInfo,
    pub sent: u64,
    pub ready: bool,
    pub failed: bool,
    pub local_path: String,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ComposerInfo {
    pub ready: bool,
    pub loading: bool,
    pub details: String,
    pub models: Vec<ChatModelInfo>,
    pub model: String,
    pub effort: String,
    pub uploads: Vec<UploadInfo>,
}
impl From<kite_app::Composer> for ComposerInfo {
    fn from(c: kite_app::Composer) -> Self {
        Self {
            ready: c.ready,
            loading: c.loading,
            details: c.details,
            model: c.selection.model,
            effort: c.selection.effort,
            models: c
                .models
                .into_iter()
                .map(|m| ChatModelInfo {
                    model: m.model,
                    name: m.display_name,
                    description: m.description,
                    efforts: m
                        .supported_reasoning_efforts
                        .into_iter()
                        .map(|e| e.reasoning_effort)
                        .collect(),
                    default_effort: m.default_reasoning_effort,
                    modalities: m.input_modalities,
                })
                .collect(),
            uploads: c
                .uploads
                .into_iter()
                .map(|u| UploadInfo {
                    ready: !u.failed && !u.attachment.path.is_empty(),
                    file: u.attachment.into(),
                    sent: u.sent,
                    failed: u.failed,
                    local_path: u.local_path,
                })
                .collect(),
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct MessageInfo {
    pub attachments: Vec<AttachmentInfo>,
    pub id: String,
    pub kind: MessageKind,
    pub text: String,
    pub intermediate: bool,
    pub tool: Option<ToolInfo>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ApprovalChoice {
    pub label: String,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ApprovalInfo {
    pub id: String,
    pub prompt: String,
    pub choices: Vec<ApprovalChoice>,
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
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum ResponsePhase {
    Idle,
    Sending,
    Thinking,
    Tool,
    Responding,
    Waiting,
    Failed,
}
impl From<kite_app::ResponseActivity> for ResponsePhase {
    fn from(a: kite_app::ResponseActivity) -> Self {
        use kite_app::ResponseActivity as A;
        match a {
            A::Idle => Self::Idle,
            A::Sending => Self::Sending,
            A::Thinking => Self::Thinking,
            A::Tool => Self::Tool,
            A::Responding => Self::Responding,
            A::Waiting => Self::Waiting,
            A::Failed => Self::Failed,
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct QuestionOptionInfo {
    pub label: String,
    pub description: String,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct QuestionFieldInfo {
    pub id: String,
    pub header: String,
    pub text: String,
    pub allow_other: bool,
    pub secret: bool,
    pub options: Vec<QuestionOptionInfo>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct QuestionInfo {
    pub id: String,
    pub blocking: bool,
    pub fields: Vec<QuestionFieldInfo>,
}
impl From<kite_app::QuestionPrompt> for QuestionInfo {
    fn from(q: kite_app::QuestionPrompt) -> Self {
        Self {
            id: q.id,
            blocking: q.blocking,
            fields: q
                .questions
                .into_iter()
                .map(|f| QuestionFieldInfo {
                    id: f.id,
                    header: f.header,
                    text: f.question,
                    allow_other: f.is_other,
                    secret: f.is_secret,
                    options: f
                        .options
                        .unwrap_or_default()
                        .into_iter()
                        .map(|o| QuestionOptionInfo {
                            label: o.label,
                            description: o.description,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct QuestionAnswerInfo {
    pub question_id: String,
    pub value: String,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct SessionInfo {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub preview: String,
    pub cwd: Option<String>,
    pub archived: bool,
    pub updated_ms: i64,
    pub agent_id: Option<String>,
    pub status: String,
    pub recorded: bool,
}
impl From<kite_app::ConversationSession> for SessionInfo {
    fn from(s: kite_app::ConversationSession) -> Self {
        Self {
            id: s.id,
            provider: s.provider,
            title: s.title,
            preview: s.preview,
            cwd: s.cwd,
            archived: s.archived,
            updated_ms: s.updated_ms,
            agent_id: s.agent_id,
            status: s.status,
            recorded: s.recorded,
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct CoreSnapshot {
    pub revision: u64,
    pub initialized: bool,
    pub device_key: Option<DeviceKeyInfo>,
    pub hosts: Vec<HostRecord>,
    pub selected_host: Option<String>,
    pub connection: ConnectionStatus,
    pub connection_step: ConnectionStep,
    pub pending_host_key: Option<HostKeyConfirmation>,
    pub herdr_version: Option<String>,
    pub herdr_path: Option<String>,
    pub capability: Option<String>,
    pub host_key_fingerprint: Option<String>,
    pub agents: Vec<AgentInfo>,
    pub selected_agent: Option<String>,
    pub sessions: Vec<SessionInfo>,
    pub selected_session: Option<String>,
    pub agent_output: String,
    pub conversation: Vec<MessageInfo>,
    pub conversation_available: bool,
    pub composer: ComposerInfo,
    pub approval: Option<ApprovalInfo>,
    pub question: Option<QuestionInfo>,
    pub response_phase: ResponsePhase,
    pub agent_pause: AgentPauseStatus,
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
            connection_step: match s.connection_step {
                kite_app::ConnectionStep::Idle => ConnectionStep::Idle,
                kite_app::ConnectionStep::Ssh => ConnectionStep::Ssh,
                kite_app::ConnectionStep::HerdrDiscovery => ConnectionStep::HerdrDiscovery,
                kite_app::ConnectionStep::HerdrApi => ConnectionStep::HerdrApi,
                kite_app::ConnectionStep::Ready => ConnectionStep::Ready,
                kite_app::ConnectionStep::Agent => ConnectionStep::Agent,
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
                    can_pause: a
                        .agent_session
                        .as_ref()
                        .is_some_and(|s| matches!(s.agent.as_str(), "claude" | "codex"))
                        || a.agent
                            .as_deref()
                            .is_some_and(|s| matches!(s, "claude" | "codex")),
                    launch_pending: a.launch_pending,
                    managed_ready: a.interactive_ready,
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
            sessions: s.sessions.into_iter().map(Into::into).collect(),
            selected_session: s.selected_session,
            agent_output: s.agent_output,
            conversation: s
                .conversation
                .into_iter()
                .map(|m| {
                    let (text, attachments) = kite_app::split_chat_attachments(&m.text);
                    MessageInfo {
                        attachments: attachments.into_iter().map(Into::into).collect(),
                        id: m.id,
                        kind: match m.role {
                            kite_app::ChatRole::User => MessageKind::User,
                            kite_app::ChatRole::Assistant => MessageKind::Assistant,
                            kite_app::ChatRole::Tool => MessageKind::Tool,
                        },
                        text,
                        intermediate: m.intermediate,
                        tool: m.tool.map(|t| ToolInfo {
                            label: t.label(),
                            summary: t.summary(),
                            name: t.name,
                            arguments: t.arguments,
                            output: t.output,
                            complete: t.complete,
                            failed: t.failed,
                            computer: t.computer,
                        }),
                    }
                })
                .collect(),
            conversation_available: s.conversation_available,
            composer: s.composer.into(),
            question: s.question.map(Into::into),
            response_phase: s.response_activity.into(),
            approval: s.approval.map(|a| ApprovalInfo {
                id: a.id,
                prompt: a.prompt,
                choices: a
                    .choices
                    .into_iter()
                    .map(|c| ApprovalChoice { label: c.label })
                    .collect(),
            }),
            agent_pause: match s.agent_pause {
                kite_app::AgentPause::Running => AgentPauseStatus::Running,
                kite_app::AgentPause::Paused => AgentPauseStatus::Paused,
                kite_app::AgentPause::Unknown => AgentPauseStatus::Unknown,
            },
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
                    F::AgentStart => FailureKind::AgentStart,
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
    pub async fn ensure_agent(&self) -> Result<(), KiteError> {
        self.inner.ensure_agent().await.map_err(Into::into)
    }
    pub async fn create_session(
        &self,
        provider: String,
        cwd: String,
        title: String,
    ) -> Result<(), KiteError> {
        self.inner
            .create_session(provider, cwd, title)
            .await
            .map_err(Into::into)
    }
    pub async fn open_session(&self, session_id: String) -> Result<(), KiteError> {
        self.inner
            .open_session(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn resume_session(&self, session_id: String) -> Result<(), KiteError> {
        self.inner
            .resume_session(session_id)
            .await
            .map_err(Into::into)
    }
    pub async fn archive_session(
        &self,
        session_id: String,
        archived: bool,
    ) -> Result<(), KiteError> {
        self.inner
            .archive_session(session_id, archived)
            .await
            .map_err(Into::into)
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
    pub async fn reload_composer(&self) -> Result<(), KiteError> {
        self.inner.reload_composer().await.map_err(Into::into)
    }
    pub async fn set_chat_model(
        &self,
        agent_id: String,
        model: String,
        effort: String,
    ) -> Result<(), KiteError> {
        self.inner
            .set_chat_model(agent_id, model, effort)
            .await
            .map_err(Into::into)
    }
    pub async fn upload_attachment(
        &self,
        agent_id: String,
        local_path: String,
        name: String,
        mime: String,
        retry_id: Option<String>,
    ) -> Result<(), KiteError> {
        self.inner
            .upload_attachment(agent_id, local_path, name, mime, retry_id)
            .await
            .map_err(Into::into)
    }
    pub async fn remove_attachment(
        &self,
        agent_id: String,
        upload_id: String,
    ) -> Result<(), KiteError> {
        self.inner
            .remove_attachment(agent_id, upload_id)
            .await
            .map_err(Into::into)
    }
    pub async fn answer_question(
        &self,
        agent_id: String,
        request_id: String,
        answers: Vec<QuestionAnswerInfo>,
    ) -> Result<(), KiteError> {
        self.inner
            .answer_question(
                agent_id,
                request_id,
                answers
                    .into_iter()
                    .map(|a| kite_app::QuestionAnswer {
                        question_id: a.question_id,
                        value: a.value,
                    })
                    .collect(),
            )
            .await
            .map_err(Into::into)
    }
    pub async fn send_agent_key(&self, agent_id: String, key: String) -> Result<(), KiteError> {
        self.inner
            .send_agent_key(agent_id, key)
            .await
            .map_err(Into::into)
    }
    pub async fn respond_approval(
        &self,
        agent_id: String,
        request_id: String,
        choice: u32,
    ) -> Result<(), KiteError> {
        self.inner
            .respond_approval(agent_id, request_id, choice)
            .await
            .map_err(Into::into)
    }
    pub async fn set_agent_paused(&self, agent_id: String, paused: bool) -> Result<(), KiteError> {
        self.inner
            .set_agent_paused(agent_id, paused)
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
