//! UniFFI commands/state and platform media adapters. Business logic lives in kite-app.
use kite_app::{AppError, CheckError, SshError};
uniffi::setup_scaffolding!();
mod core_bridge;
pub use core_bridge::*;
#[cfg(target_os = "android")]
mod android;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum KiteError {
    #[error("host key {fingerprint} is not trusted")]
    UnknownHostKey {
        fingerprint: String,
        host_key: String,
    },
    #[error("host key changed")]
    HostKeyChanged {
        trusted_fingerprint: String,
        presented_fingerprint: String,
    },
    #[error("public key authentication rejected for {username}")]
    AuthRejected { username: String },
    #[error("invalid key: {details}")]
    InvalidKey { details: String },
    #[error("connection: {details}")]
    Connection { details: String },
    #[error("herdr not found")]
    HerdrNotFound,
    #[error("herdr unsupported at {path}")]
    HerdrUnsupported { path: String, capability: String },
    #[error("probe: {details}")]
    Probe { details: String },
    #[error("storage: {details}")]
    Storage { details: String },
    #[error("herdr API: {details}")]
    HerdrApi { details: String },
    #[error("stream: {details}")]
    Stream { details: String },
    #[error("invalid state: {details}")]
    InvalidState { details: String },
    #[error("media target unavailable")]
    MediaUnavailable,
}
impl From<SshError> for KiteError {
    fn from(e: SshError) -> Self {
        match e {
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
            SshError::InvalidKey(details) => Self::InvalidKey { details },
            other => Self::Connection {
                details: other.to_string(),
            },
        }
    }
}
impl From<CheckError> for KiteError {
    fn from(e: CheckError) -> Self {
        match e {
            CheckError::Ssh(e) => e.into(),
            CheckError::NotFound => Self::HerdrNotFound,
            CheckError::Unsupported { path, capability } => {
                Self::HerdrUnsupported { path, capability }
            }
            other => Self::Probe {
                details: other.to_string(),
            },
        }
    }
}
impl From<AppError> for KiteError {
    fn from(e: AppError) -> Self {
        match e {
            AppError::Ssh(e) => e.into(),
            AppError::Herdr(e) => e.into(),
            AppError::Store(e) => Self::Storage {
                details: e.to_string(),
            },
            AppError::Api(e) => Self::HerdrApi {
                details: e.to_string(),
            },
            AppError::AgentProcess(e) => Self::HerdrApi {
                details: e.to_string(),
            },
            AppError::Stream(e) => Self::Stream {
                details: e.to_string(),
            },
            AppError::State(details) => Self::InvalidState { details },
        }
    }
}

#[uniffi::export]
pub fn init_logging() -> Result<(), KiteError> {
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("kite"),
    );
    #[cfg(target_os = "ios")]
    if log::set_logger(&ios::LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    Ok(())
}
#[cfg(target_os = "ios")]
mod ios {
    pub static LOGGER: Logger = Logger;
    pub struct Logger;
    impl log::Log for Logger {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Info
        }
        fn log(&self, r: &log::Record) {
            if self.enabled(r.metadata()) {
                eprintln!("[kite] {} {}: {}", r.level(), r.target(), r.args());
            }
        }
        fn flush(&self) {}
    }
}
