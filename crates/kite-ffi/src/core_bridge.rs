use crate::KiteError;
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum ClipboardPlatformInfo {
    Linux,
    Macos,
}
impl From<kite_app::ClipboardPlatform> for ClipboardPlatformInfo {
    fn from(value: kite_app::ClipboardPlatform) -> Self {
        match value {
            kite_app::ClipboardPlatform::Linux => Self::Linux,
            kite_app::ClipboardPlatform::Macos => Self::Macos,
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct RemoteClipboardInfo {
    pub text: String,
    pub platform: ClipboardPlatformInfo,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct SessionCleanupInfo {
    pub reclaimed: u32,
    pub retained: u32,
    pub failed: u32,
}

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

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum HostMemoryPressure {
    Unknown,
    Normal,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum MemoryWarningReason {
    None,
    HostMemory,
    AgentBudget,
    StartupReservation,
    MemoryStall,
    RecentOom,
    EnforcementUnavailable,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct ResourceProcessInfo {
    pub pid: u32,
    pub name: String,
    pub memory_bytes: u64,
    pub cpu_percent: Option<f64>,
    pub managed: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct HostResourcesInfo {
    pub available: bool,
    pub problem: Option<String>,
    pub sampled_at_ms: u64,
    pub cpu_percent: Option<f64>,
    pub cpu_cores: u32,
    pub load_one: f64,
    pub load_five: f64,
    pub load_fifteen: f64,
    pub memory_total_bytes: u64,
    pub memory_available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub memory_pressure_some: f64,
    pub memory_pressure_full: f64,
    pub agent_memory_used_bytes: u64,
    pub agent_reclaimable_bytes: u64,
    pub agent_memory_limit_bytes: u64,
    pub agent_migration_reserved_bytes: u64,
    pub agent_startup_reserved_bytes: u64,
    pub agent_kernel_limit_bytes: u64,
    pub agent_cpu_percent: Option<f64>,
    pub agent_process_count: u32,
    pub agent_oom_kills: u64,
    pub limit_enforced: bool,
    pub memory_pressure: HostMemoryPressure,
    pub reason: MemoryWarningReason,
    pub can_start_agents: bool,
    pub processes: Vec<ResourceProcessInfo>,
}

impl From<kite_app::resources::HostResources> for HostResourcesInfo {
    fn from(value: kite_app::resources::HostResources) -> Self {
        use kite_app::resources::{MemoryPressure as P, ResourceReason as R};
        Self {
            available: value.available,
            problem: value.problem,
            sampled_at_ms: value.sampled_at_ms,
            cpu_percent: value.cpu_percent,
            cpu_cores: value.cpu_cores,
            load_one: value.load_one,
            load_five: value.load_five,
            load_fifteen: value.load_fifteen,
            memory_total_bytes: value.memory_total_bytes,
            memory_available_bytes: value.memory_available_bytes,
            swap_total_bytes: value.swap_total_bytes,
            swap_used_bytes: value.swap_used_bytes,
            memory_pressure_some: value.memory_pressure_some,
            memory_pressure_full: value.memory_pressure_full,
            agent_memory_used_bytes: value.agent_memory_used_bytes,
            agent_reclaimable_bytes: value.agent_reclaimable_bytes,
            agent_memory_limit_bytes: value.agent_memory_limit_bytes,
            agent_migration_reserved_bytes: value.agent_migration_reserved_bytes,
            agent_startup_reserved_bytes: value.agent_startup_reserved_bytes,
            agent_kernel_limit_bytes: value.agent_kernel_limit_bytes,
            agent_cpu_percent: value.agent_cpu_percent,
            agent_process_count: value.agent_process_count,
            agent_oom_kills: value.agent_oom_kills,
            limit_enforced: value.limit_enforced,
            can_start_agents: value.can_start_agents,
            memory_pressure: match value.memory_pressure {
                P::Unknown => HostMemoryPressure::Unknown,
                P::Normal => HostMemoryPressure::Normal,
                P::Warning => HostMemoryPressure::Warning,
                P::Critical => HostMemoryPressure::Critical,
            },
            reason: match value.reason {
                R::None => MemoryWarningReason::None,
                R::HostMemory => MemoryWarningReason::HostMemory,
                R::AgentBudget => MemoryWarningReason::AgentBudget,
                R::StartupReservation => MemoryWarningReason::StartupReservation,
                R::MemoryStall => MemoryWarningReason::MemoryStall,
                R::RecentOom => MemoryWarningReason::RecentOom,
                R::EnforcementUnavailable => MemoryWarningReason::EnforcementUnavailable,
            },
            processes: value
                .processes
                .into_iter()
                .map(|p| ResourceProcessInfo {
                    pid: p.pid,
                    name: p.name,
                    memory_bytes: p.memory_bytes,
                    cpu_percent: p.cpu_percent,
                    managed: p.managed,
                })
                .collect(),
        }
    }
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
    HostNode,
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
pub struct ProviderInfo {
    pub id: String,
    pub managed_chat: bool,
    pub models: bool,
    pub attachments: bool,
    pub skills: bool,
    pub usage: bool,
    pub pause: bool,
    pub terminal: bool,
    pub release: bool,
}
impl From<kite_app::Provider> for ProviderInfo {
    fn from(p: kite_app::Provider) -> Self {
        Self {
            id: p.id,
            managed_chat: p.managed_chat,
            models: p.models,
            attachments: p.attachments,
            skills: p.skills,
            usage: p.usage,
            pause: p.pause,
            terminal: p.terminal,
            release: p.release,
        }
    }
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
pub enum ItemRole {
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
    pub generated_image: bool,
}
impl From<kite_app::ChatAttachment> for AttachmentInfo {
    fn from(a: kite_app::ChatAttachment) -> Self {
        Self {
            generated_image: a.path == format!("kite-image:{}", a.id)
                && a.mime.starts_with("image/"),
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
pub struct UsageWindowInfo {
    pub remaining_percent: u32,
    pub duration_mins: Option<u64>,
    pub resets_at: Option<i64>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct UsageCreditsInfo {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct UsageBucketInfo {
    pub id: String,
    pub name: Option<String>,
    pub primary: Option<UsageWindowInfo>,
    pub secondary: Option<UsageWindowInfo>,
    pub credits: Option<UsageCreditsInfo>,
    pub limit_reached: bool,
    pub spend_control_reached: bool,
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum UsageStatus {
    NotLoaded,
    Ready,
    Unavailable,
    Failed,
}
#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum UsageUnavailable {
    None,
    NoAccount,
    UnsupportedAuth,
    NoData,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct UsageResetCreditsInfo {
    pub available_count: u64,
    pub earliest_expires_at: Option<i64>,
    pub expiry_complete: bool,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct AccountUsageInfo {
    pub status: UsageStatus,
    pub unavailable: UsageUnavailable,
    pub loading: bool,
    pub stale: bool,
    pub plan: Option<String>,
    pub buckets: Vec<UsageBucketInfo>,
    pub ordinary_usage_allowed: Option<bool>,
    pub reset_credits: Option<UsageResetCreditsInfo>,
    pub updated_ms: Option<i64>,
}
impl From<kite_app::UsageWindow> for UsageWindowInfo {
    fn from(w: kite_app::UsageWindow) -> Self {
        Self {
            remaining_percent: w.remaining_percent,
            duration_mins: w.duration_mins,
            resets_at: w.resets_at,
        }
    }
}
impl From<kite_app::AccountUsage> for AccountUsageInfo {
    fn from(u: kite_app::AccountUsage) -> Self {
        Self {
            status: match u.status {
                kite_app::UsageStatus::NotLoaded => UsageStatus::NotLoaded,
                kite_app::UsageStatus::Ready => UsageStatus::Ready,
                kite_app::UsageStatus::Unavailable => UsageStatus::Unavailable,
                kite_app::UsageStatus::Failed => UsageStatus::Failed,
            },
            unavailable: match u.unavailable {
                kite_app::UsageUnavailable::None => UsageUnavailable::None,
                kite_app::UsageUnavailable::NoAccount => UsageUnavailable::NoAccount,
                kite_app::UsageUnavailable::UnsupportedAuth => UsageUnavailable::UnsupportedAuth,
                kite_app::UsageUnavailable::NoData => UsageUnavailable::NoData,
            },
            loading: u.loading,
            stale: u.stale,
            plan: u.plan,
            ordinary_usage_allowed: u.ordinary_usage_allowed,
            reset_credits: u.reset_credits.map(|c| UsageResetCreditsInfo {
                available_count: c.available_count,
                earliest_expires_at: c.earliest_expires_at,
                expiry_complete: c.expiry_complete,
            }),
            updated_ms: u.updated_ms,
            buckets: u
                .buckets
                .into_iter()
                .map(|b| UsageBucketInfo {
                    id: b.id,
                    name: b.name,
                    primary: b.primary.map(Into::into),
                    secondary: b.secondary.map(Into::into),
                    credits: b.credits.map(|c| UsageCreditsInfo {
                        has_credits: c.has_credits,
                        unlimited: c.unlimited,
                        balance: c.balance,
                    }),
                    limit_reached: b.limit_reached,
                    spend_control_reached: b.spend_control_reached,
                })
                .collect(),
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ChatSkillInfo {
    pub name: String,
    pub description: String,
    pub path: String,
    pub display_name: Option<String>,
    pub short_description: Option<String>,
}
impl From<kite_app::ChatSkill> for ChatSkillInfo {
    fn from(s: kite_app::ChatSkill) -> Self {
        Self {
            name: s.name,
            description: s.description,
            path: s.path,
            display_name: s.display_name,
            short_description: s.short_description,
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct SkillDraftInfo {
    pub available: Vec<ChatSkillInfo>,
    pub selected: Vec<ChatSkillInfo>,
    pub loaded: bool,
    pub loading: bool,
    pub details: String,
}
impl From<kite_app::SkillDraft> for SkillDraftInfo {
    fn from(d: kite_app::SkillDraft) -> Self {
        Self {
            available: d.available.into_iter().map(Into::into).collect(),
            selected: d.selected.into_iter().map(Into::into).collect(),
            loaded: d.loaded,
            loading: d.loading,
            details: d.details,
        }
    }
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
    pub usage: AccountUsageInfo,
    pub skills: SkillDraftInfo,
    pub settings: Vec<SessionSettingInfo>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct SettingChoiceInfo {
    pub value: String,
    pub name: String,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct SessionSettingInfo {
    pub id: String,
    pub name: String,
    pub current_value: String,
    pub options: Vec<SettingChoiceInfo>,
}
impl From<kite_app::Composer> for ComposerInfo {
    fn from(c: kite_app::Composer) -> Self {
        Self {
            ready: c.ready,
            loading: c.loading,
            details: c.details,
            usage: c.usage.into(),
            skills: c.skills.into(),
            settings: c
                .settings
                .into_iter()
                .map(|setting| SessionSettingInfo {
                    id: setting.id,
                    name: setting.name,
                    current_value: setting.current_value,
                    options: setting
                        .options
                        .into_iter()
                        .map(|option| SettingChoiceInfo {
                            value: option.value,
                            name: option.name,
                        })
                        .collect(),
                })
                .collect(),
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
pub struct QuestionReplyInfo {
    pub id: String,
    pub question: String,
    pub answer: String,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ItemOriginInfo {
    pub conversation: String,
    pub item: String,
}
impl From<&kite_app::ChatItemOrigin> for ItemOriginInfo {
    fn from(origin: &kite_app::ChatItemOrigin) -> Self {
        Self {
            conversation: origin.conversation.clone(),
            item: origin.item.clone(),
        }
    }
}
#[derive(Clone, Debug, uniffi::Enum)]
pub enum ReferenceInfo {
    Live {
        conversation: String,
        item: String,
    },
    Snapshot {
        origin: ItemOriginInfo,
        captured_ms: i64,
        blocks: Vec<BlockInfo>,
    },
}
#[derive(Clone, Debug, uniffi::Enum)]
pub enum BlockInfo {
    Text { text: String },
    Attachment { file: AttachmentInfo },
    Tool { tool: ToolInfo },
    QuestionReply { reply: QuestionReplyInfo },
    Embed { reference: ReferenceInfo },
}
impl From<&kite_app::ChatBlock> for BlockInfo {
    fn from(block: &kite_app::ChatBlock) -> Self {
        use kite_app::{ChatBlock as B, ChatReference as R};
        match block {
            B::Text(text) => Self::Text { text: text.clone() },
            B::Attachment(file) => Self::Attachment {
                file: file.clone().into(),
            },
            B::Tool(t) => Self::Tool {
                tool: ToolInfo {
                    label: t.label(),
                    summary: t.summary(),
                    name: t.name.clone(),
                    arguments: t.arguments.clone(),
                    output: t.output.clone(),
                    complete: t.complete,
                    failed: t.failed,
                    computer: t.computer,
                },
            },
            B::QuestionReply(reply) => Self::QuestionReply {
                reply: QuestionReplyInfo {
                    id: reply.id.clone(),
                    question: reply.question.clone(),
                    answer: reply.answer.clone(),
                },
            },
            B::Embed(R::Live { conversation, item }) => Self::Embed {
                reference: ReferenceInfo::Live {
                    conversation: conversation.clone(),
                    item: item.clone(),
                },
            },
            B::Embed(R::Snapshot {
                origin,
                captured_ms,
                blocks,
            }) => Self::Embed {
                reference: ReferenceInfo::Snapshot {
                    origin: origin.into(),
                    captured_ms: *captured_ms,
                    blocks: blocks.iter().map(Into::into).collect(),
                },
            },
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ItemInfo {
    pub id: String,
    pub role: ItemRole,
    pub blocks: Vec<BlockInfo>,
    pub intermediate: bool,
    pub origin: Option<ItemOriginInfo>,
}
impl From<&kite_app::ChatItem> for ItemInfo {
    fn from(item: &kite_app::ChatItem) -> Self {
        Self {
            id: item.id.clone(),
            role: match item.role {
                kite_app::ChatRole::User => ItemRole::User,
                kite_app::ChatRole::Assistant => ItemRole::Assistant,
                kite_app::ChatRole::Tool => ItemRole::Tool,
            },
            blocks: item.blocks.iter().map(Into::into).collect(),
            intermediate: item.intermediate,
            origin: item.origin.as_ref().map(Into::into),
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ApprovalChoice {
    pub label: String,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ApprovalInfo {
    pub id: String,
    pub prompt: String,
    pub explanation: Option<String>,
    pub command: Option<String>,
    pub working_directory: Option<String>,
    pub choices: Vec<ApprovalChoice>,
    pub operation: Option<String>,
    pub details: Vec<ApprovalDetailInfo>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ApprovalDetailInfo {
    pub name: String,
    pub value: String,
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
pub struct HistoryInfo {
    pub loading: bool,
    pub complete: bool,
    pub failed: bool,
    pub waiting: bool,
    pub details: String,
}

#[derive(Clone, Copy, Debug, uniffi::Enum)]
pub enum AgentNoticeKind {
    Completed,
    Attention,
    Question,
    Failed,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct AgentNoticeInfo {
    pub id: String,
    pub host_id: String,
    pub session_id: String,
    pub title: String,
    pub kind: AgentNoticeKind,
    pub created_ms: i64,
    pub read: bool,
    pub alerted: bool,
}
impl From<kite_app::AgentNotice> for AgentNoticeInfo {
    fn from(value: kite_app::AgentNotice) -> Self {
        Self {
            id: value.id,
            host_id: value.host_id,
            session_id: value.session_id,
            title: value.title,
            created_ms: value.created_ms,
            read: value.read,
            alerted: value.alerted,
            kind: match value.kind {
                kite_app::AgentNoticeKind::Completed => AgentNoticeKind::Completed,
                kite_app::AgentNoticeKind::Attention => AgentNoticeKind::Attention,
                kite_app::AgentNoticeKind::Question => AgentNoticeKind::Question,
                kite_app::AgentNoticeKind::Failed => AgentNoticeKind::Failed,
            },
        }
    }
}
impl From<kite_app::HistoryStatus> for HistoryInfo {
    fn from(v: kite_app::HistoryStatus) -> Self {
        Self {
            loading: v.loading,
            complete: v.complete,
            failed: v.failed,
            waiting: v.waiting,
            details: v.details,
        }
    }
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationEditInfo {
    pub at: u64,
    pub remove: u64,
    pub items: Vec<ItemInfo>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct ConversationDeltaInfo {
    pub context: String,
    pub revision: u64,
    pub reset: bool,
    pub edits: Vec<ConversationEditInfo>,
}
#[derive(Clone, Debug, uniffi::Record)]
pub struct CoreSnapshot {
    pub notification_revision: u64,
    pub notification_problem: Option<String>,
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
    pub host_resources: Option<HostResourcesInfo>,
    pub agents: Vec<AgentInfo>,
    pub providers: Vec<ProviderInfo>,
    pub selected_agent: Option<String>,
    pub sessions: Vec<SessionInfo>,
    pub selected_session: Option<String>,
    pub agent_output: String,
    pub conversation_context: String,
    pub conversation_count: u64,
    pub conversation_revision: u64,
    pub conversation_available: bool,
    pub history: HistoryInfo,
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
        let conversation_context = s.conversation_context();
        use kite_app::{ConnectionPhase as C, DesktopPhase as D, FailureCode as F};
        Self {
            revision: s.revision,
            initialized: s.initialized,
            notification_revision: s.notification_revision,
            notification_problem: s.notification_problem,
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
            host_resources: s.host_resources.map(Into::into),
            agents: s
                .agents
                .into_iter()
                .map(|a| AgentInfo {
                    can_pause: s
                        .providers
                        .iter()
                        .any(|p| Some(p.id.as_str()) == a.agent.as_deref() && p.pause),
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
            providers: s.providers.into_iter().map(Into::into).collect(),
            selected_agent: s.selected_agent,
            sessions: s.sessions.into_iter().map(Into::into).collect(),
            selected_session: s.selected_session,
            agent_output: s.agent_output,
            conversation_context,
            conversation_count: s.conversation.len() as u64,
            conversation_available: s.conversation_available,
            conversation_revision: s.conversation_revision,
            history: s.history_status.into(),
            composer: s.composer.into(),
            question: s.question.map(Into::into),
            response_phase: s.response_activity.into(),
            approval: s.approval.map(|a| ApprovalInfo {
                id: a.id,
                prompt: a.prompt,
                explanation: a.explanation,
                command: a.command,
                working_directory: a.working_directory,
                operation: a.operation,
                details: a
                    .details
                    .into_iter()
                    .map(|detail| ApprovalDetailInfo {
                        name: detail.name,
                        value: detail.value,
                    })
                    .collect(),
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
                    F::HostNode => FailureKind::HostNode,
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
    pub async fn register_push(
        &self,
        device_id: String,
        token: Option<String>,
        project_id: String,
    ) -> Result<bool, KiteError> {
        self.inner
            .register_push(device_id, token, project_id)
            .await
            .map_err(Into::into)
    }
    pub async fn receive_push(
        &self,
        id: String,
        kind: AgentNoticeKind,
        session_id: String,
        created_ms: i64,
    ) -> Result<AgentNoticeInfo, KiteError> {
        let kind = match kind {
            AgentNoticeKind::Completed => kite_app::AgentNoticeKind::Completed,
            AgentNoticeKind::Attention => kite_app::AgentNoticeKind::Attention,
            AgentNoticeKind::Question => kite_app::AgentNoticeKind::Question,
            AgentNoticeKind::Failed => kite_app::AgentNoticeKind::Failed,
        };
        self.inner
            .receive_push(id, kind, session_id, created_ms)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
    pub fn notifications(&self) -> Vec<AgentNoticeInfo> {
        self.inner
            .notifications()
            .into_iter()
            .map(Into::into)
            .collect()
    }
    pub fn notification_host(&self, id: String) -> Result<String, KiteError> {
        self.inner.notification_host(&id).map_err(Into::into)
    }
    pub async fn load_notification(&self, id: String) -> Result<AgentNoticeInfo, KiteError> {
        self.inner
            .load_notification(id)
            .await
            .map(Into::into)
            .map_err(Into::into)
    }
    pub async fn acknowledge_notification(
        &self,
        id: String,
        read: bool,
        alerted: bool,
    ) -> Result<(), KiteError> {
        self.inner
            .acknowledge_notification(id, read, alerted)
            .await
            .map_err(Into::into)
    }
    pub async fn save_host(&self, host: HostRecord) -> Result<String, KiteError> {
        self.inner.save_host(host.into()).await.map_err(Into::into)
    }
    pub async fn connect(&self, host_id: String) -> Result<(), KiteError> {
        self.inner.connect(host_id).await.map_err(Into::into)
    }
    pub async fn refresh_host_resources(&self) -> Result<(), KiteError> {
        self.inner
            .refresh_host_resources()
            .await
            .map_err(Into::into)
    }
    pub async fn set_agent_memory_limit(
        &self,
        host_id: String,
        bytes: u64,
    ) -> Result<(), KiteError> {
        self.inner
            .set_agent_memory_limit(host_id, bytes)
            .await
            .map_err(Into::into)
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
    pub async fn release_archived_sessions(
        &self,
        host_id: String,
    ) -> Result<SessionCleanupInfo, KiteError> {
        let result = self.inner.release_archived_sessions(host_id).await?;
        Ok(SessionCleanupInfo {
            reclaimed: result.reclaimed,
            retained: result.retained,
            failed: result.failed,
        })
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
    pub async fn refresh_account_usage(&self) -> Result<(), KiteError> {
        self.inner.refresh_account_usage().await.map_err(Into::into)
    }
    pub async fn refresh_skills(&self, agent_id: String) -> Result<(), KiteError> {
        self.inner
            .refresh_skills(agent_id)
            .await
            .map_err(Into::into)
    }
    pub async fn select_skill(
        &self,
        agent_id: String,
        path: String,
        selected: bool,
    ) -> Result<(), KiteError> {
        self.inner
            .select_skill(agent_id, path, selected)
            .await
            .map_err(Into::into)
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
    pub async fn chat_attachment(&self, context: String, id: String) -> Result<Vec<u8>, KiteError> {
        self.inner
            .chat_attachment(context, id)
            .await
            .map_err(Into::into)
    }
    pub async fn set_chat_setting(
        &self,
        agent_id: String,
        setting: String,
        value: String,
    ) -> Result<(), KiteError> {
        self.inner
            .set_chat_setting(agent_id, setting, value)
            .await
            .map_err(Into::into)
    }
    pub async fn chat_image(&self, context: String, id: String) -> Result<Vec<u8>, KiteError> {
        self.inner.chat_image(context, id).await.map_err(Into::into)
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
    pub fn conversation_since(&self, context: String, revision: u64) -> ConversationDeltaInfo {
        let delta = self.inner.conversation_since(&context, revision);
        ConversationDeltaInfo {
            context: delta.context,
            revision: delta.revision,
            reset: delta.reset,
            edits: delta
                .edits
                .into_iter()
                .map(|e| ConversationEditInfo {
                    at: e.at as u64,
                    remove: e.remove as u64,
                    items: e.items.iter().map(|item| item.as_ref().into()).collect(),
                })
                .collect(),
        }
    }
    pub async fn retry_history(&self) -> Result<(), KiteError> {
        self.inner.retry_history().await.map_err(Into::into)
    }

    pub async fn read_clipboard(&self, host_id: String) -> Result<RemoteClipboardInfo, KiteError> {
        let value = self.inner.read_clipboard(host_id).await?;
        Ok(RemoteClipboardInfo {
            text: value.text,
            platform: value.platform.into(),
        })
    }
    pub async fn write_clipboard(
        &self,
        host_id: String,
        text: String,
    ) -> Result<ClipboardPlatformInfo, KiteError> {
        Ok(self.inner.write_clipboard(host_id, text).await?.into())
    }
    pub async fn synchronize_clipboard(
        &self,
        host_id: String,
        local: Option<String>,
    ) -> Result<Option<String>, KiteError> {
        self.inner
            .synchronize_clipboard(host_id, local)
            .await
            .map_err(Into::into)
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

#[cfg(test)]
mod tests {
    use super::*;
    use kite_app::{ChatAttachment, ChatBlock, ChatItem, ChatItemOrigin, ChatReference, ChatRole};

    #[test]
    fn items_cross_the_boundary_as_typed_blocks() {
        let image = ChatAttachment {
            id: "abc".into(),
            name: "generated.png".into(),
            mime: "image/png".into(),
            size: 3,
            path: "kite-image:abc".into(),
        };
        let origin = ChatItemOrigin {
            conversation: "source".into(),
            item: "first".into(),
        };
        let mut item = ChatItem::new(
            "item".into(),
            ChatRole::Assistant,
            vec![
                ChatBlock::Text("body".into()),
                ChatBlock::Attachment(image),
                ChatBlock::Embed(ChatReference::Snapshot {
                    origin: origin.clone(),
                    captured_ms: 7,
                    blocks: vec![ChatBlock::Text("copied".into())],
                }),
            ],
        );
        item.intermediate = true;
        item.origin = Some(origin);
        let info = ItemInfo::from(&item);
        assert_eq!(info.id, "item");
        assert!(matches!(info.role, ItemRole::Assistant));
        assert!(info.intermediate);
        assert_eq!(info.origin.unwrap().item, "first");
        assert!(matches!(&info.blocks[0], BlockInfo::Text { text } if text == "body"));
        assert!(matches!(&info.blocks[1], BlockInfo::Attachment { file } if file.generated_image));
        let BlockInfo::Embed {
            reference: ReferenceInfo::Snapshot { blocks, .. },
        } = &info.blocks[2]
        else {
            panic!("snapshot reference expected");
        };
        assert!(matches!(&blocks[0], BlockInfo::Text { text } if text == "copied"));
    }
}
