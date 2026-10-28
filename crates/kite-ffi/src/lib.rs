//! kite's boundary to the native apps, exported through UniFFI.
//!
//! This crate only adapts `kite-app` to FFI types: records, the error enum,
//! and platform logging. Behaviour belongs in `kite-app` and the domain crates.

use kite_app::{AppError, CheckError, HostSpec, SshError, Target};

uniffi::setup_scaffolding!();

#[derive(Debug, Clone, uniffi::Record)]
pub struct SshKeyPair {
    pub private_key_openssh: String,
    pub public_key_openssh: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct HostConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Absolute path of herdr on the host; `None` searches the usual places.
    pub herdr_path: Option<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct HerdrCheck {
    pub herdr_path: String,
    pub capability: String,
    pub version: Option<String>,
    pub host_key_fingerprint: String,
}

/// Errors carry structured fields only. User-facing text is written by the
/// native apps; `message` fields are diagnostics.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum KiteError {
    /// The host key is not trusted yet; show `fingerprint` and retry with
    /// `host_key` trusted if the user accepts it.
    #[error("host key {fingerprint} is not trusted yet")]
    UnknownHostKey {
        fingerprint: String,
        host_key: String,
    },
    #[error(
        "host key changed: trusted {trusted_fingerprint}, server presented {presented_fingerprint}"
    )]
    HostKeyChanged {
        trusted_fingerprint: String,
        presented_fingerprint: String,
    },
    #[error("server rejected public key authentication for {username}")]
    AuthRejected { username: String },
    #[error("invalid key: {message}")]
    InvalidKey { message: String },
    #[error("{message}")]
    Connection { message: String },
    #[error("herdr was not found on the host")]
    HerdrNotFound,
    #[error("herdr at {path} does not support the API bridge (answered `{capability}`)")]
    HerdrUnsupported { path: String, capability: String },
    #[error("{message}")]
    Probe { message: String },
}

impl From<SshError> for KiteError {
    fn from(error: SshError) -> Self {
        match error {
            SshError::UnknownHostKey {
                fingerprint,
                public_key,
            } => Self::UnknownHostKey {
                fingerprint,
                host_key: public_key,
            },
            SshError::HostKeyChanged { trusted, presented } => Self::HostKeyChanged {
                trusted_fingerprint: trusted,
                presented_fingerprint: presented,
            },
            SshError::AuthRejected(username) => Self::AuthRejected { username },
            SshError::InvalidKey(message) => Self::InvalidKey { message },
            other => Self::Connection {
                message: other.to_string(),
            },
        }
    }
}

impl From<CheckError> for KiteError {
    fn from(error: CheckError) -> Self {
        match error {
            CheckError::Ssh(error) => error.into(),
            CheckError::NotFound => Self::HerdrNotFound,
            CheckError::Unsupported { path, capability } => {
                Self::HerdrUnsupported { path, capability }
            }
            other => Self::Probe {
                message: other.to_string(),
            },
        }
    }
}

impl From<AppError> for KiteError {
    fn from(error: AppError) -> Self {
        match error {
            AppError::Ssh(error) => error.into(),
            AppError::Herdr(error) => error.into(),
        }
    }
}

impl From<kite_app::KeyPair> for SshKeyPair {
    fn from(pair: kite_app::KeyPair) -> Self {
        Self {
            private_key_openssh: pair.private_key_openssh,
            public_key_openssh: pair.public_key_openssh,
            fingerprint: pair.fingerprint,
        }
    }
}

/// Routes Rust logs to logcat on Android and to stderr (the Xcode console) on
/// iOS. Safe to call more than once.
#[uniffi::export]
pub fn init_logging() {
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("kite"),
    );
    #[cfg(target_os = "ios")]
    if log::set_logger(&ios::STDERR_LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

#[cfg(target_os = "ios")]
mod ios {
    pub static STDERR_LOGGER: StderrLogger = StderrLogger;

    pub struct StderrLogger;

    impl log::Log for StderrLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Info
        }

        fn log(&self, record: &log::Record) {
            if self.enabled(record.metadata()) {
                eprintln!(
                    "[kite] {} {}: {}",
                    record.level(),
                    record.target(),
                    record.args()
                );
            }
        }

        fn flush(&self) {}
    }
}

/// Generates this device's ed25519 key pair.
#[uniffi::export]
pub fn generate_ssh_key(comment: String) -> Result<SshKeyPair, KiteError> {
    Ok(kite_app::keys::generate_ed25519(&comment)?.into())
}

/// Re-derives the public key and fingerprint of a stored private key.
#[uniffi::export]
pub fn describe_ssh_key(private_key_openssh: String) -> Result<SshKeyPair, KiteError> {
    Ok(kite_app::keys::describe_private_key(&private_key_openssh)?.into())
}

/// Connects to `host`, verifies its host key against `trusted_host_key`,
/// and runs `herdr remote-api-bridge --check` there.
#[uniffi::export(async_runtime = "tokio")]
pub async fn check_herdr(
    host: HostConfig,
    private_key_openssh: String,
    trusted_host_key: Option<String>,
) -> Result<HerdrCheck, KiteError> {
    let spec = HostSpec {
        target: Target {
            host: host.host,
            port: host.port,
            username: host.username,
        },
        herdr_path: host.herdr_path,
    };
    let check =
        kite_app::check_host(&spec, &private_key_openssh, trusted_host_key.as_deref()).await?;
    Ok(HerdrCheck {
        herdr_path: check.bridge.herdr_path,
        capability: check.bridge.capability,
        version: check.bridge.version,
        host_key_fingerprint: check.host_key_fingerprint,
    })
}
