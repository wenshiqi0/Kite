//! kite application layer: drives the domain crates on behalf of the native
//! apps. It is platform-agnostic and knows nothing about FFI; `kite-ffi`
//! adapts it to UniFFI.
//!
//! M1 surface: device keys and a one-shot connect-and-check. Long-lived host
//! sessions and view models arrive with the chat panel.

use kite_ssh::Session;

pub mod core;
pub use core::*;
pub use kite_agent::composer::split_attachments as split_chat_attachments;
pub use kite_agent::composer::{
    Attachment as ChatAttachment, Composer, Effort as ChatEffort, Model as ChatModel,
    Selection as ChatSelection, Upload as ChatUpload,
};
pub use kite_agent::{
    Approval, Choice, Message as ChatMessage, Role as ChatRole, Tool as ChatTool,
};
pub use kite_store::Host as SavedHost;
pub use kite_stream::{
    FRAME_CONFIG, FRAME_KEY, FrameInfo, Input, InputSender, MediaSink, StreamConfig, VideoFormat,
};

pub use kite_herdr::{BridgeCheck, CheckError};
pub use kite_ssh::{KeyPair, SshError, Target};

/// Device key operations the native apps need.
pub mod keys {
    pub use kite_ssh::{describe_private_key, generate_ed25519};
}

/// A host as the user configured it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSpec {
    pub target: Target,
    /// Absolute path of herdr on the host; `None` searches the usual places.
    pub herdr_path: Option<String>,
}

/// What a successful connect-and-check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCheck {
    pub bridge: BridgeCheck,
    pub host_key_fingerprint: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error(transparent)]
    Herdr(#[from] CheckError),
    #[error(transparent)]
    Store(#[from] kite_store::StoreError),
    #[error(transparent)]
    Api(#[from] kite_herdr::api::ApiError),
    #[error(transparent)]
    Stream(#[from] kite_stream::StreamError),
    #[error(transparent)]
    AgentProcess(#[from] kite_agent::process::Error),
    #[error(transparent)]
    Composer(#[from] kite_agent::composer::Error),
    #[error("{0}")]
    State(String),
}

/// Connects to the host, verifies its key against `trusted_host_key` (an
/// OpenSSH public key line), and runs `herdr remote-api-bridge --check` there.
/// The connection is closed afterwards.
pub async fn check_host(
    host: &HostSpec,
    private_key_openssh: &str,
    trusted_host_key: Option<&str>,
) -> Result<HostCheck, AppError> {
    let session = Session::connect(&host.target, private_key_openssh, trusted_host_key).await?;
    let host_key_fingerprint = session.host_key_fingerprint();
    let result = kite_herdr::check_api_bridge(&session, host.herdr_path.as_deref()).await;
    if let Err(error) = session.close().await {
        log::warn!("closing SSH session failed: {error}");
    }
    let bridge = result?;
    log::info!("herdr {} answered {}", bridge.herdr_path, bridge.capability);
    Ok(HostCheck {
        bridge,
        host_key_fingerprint,
    })
}
