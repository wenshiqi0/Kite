//! SSH client for kite.
//!
//! Host keys are checked strictly: an unknown key is never accepted silently,
//! and a key that differs from the trusted one is always rejected. The caller
//! asks the user to confirm the fingerprint and retries with the key trusted.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::ssh_key::LineEnding;
use russh::keys::{
    Algorithm, HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey, PublicKeyOrCertificate,
};
use russh::{ChannelMsg, Disconnect};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(15);

/// An OpenSSH-encoded key pair and the fingerprint of its public half.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPair {
    pub private_key_openssh: String,
    pub public_key_openssh: String,
    pub fingerprint: String,
}

/// Where to connect and as whom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    pub username: String,
}

/// Everything a finished exec channel produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// `None` when the server closed the channel without reporting a status.
    pub exit_status: Option<u32>,
}

#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("host key {fingerprint} is not trusted yet")]
    UnknownHostKey {
        fingerprint: String,
        public_key: String,
    },
    #[error("host key changed: trusted {trusted}, server presented {presented}")]
    HostKeyChanged { trusted: String, presented: String },
    #[error("server rejected public key authentication for {0}")]
    AuthRejected(String),
    #[error("server refused to run the command")]
    ExecRejected,
    #[error("timed out while {0}")]
    Timeout(&'static str),
    #[error("invalid key: {0}")]
    InvalidKey(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Protocol(#[from] russh::Error),
}

/// Generates a new ed25519 key pair for this device.
pub fn generate_ed25519(comment: &str) -> Result<KeyPair, SshError> {
    let mut key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).map_err(invalid_key)?;
    key.set_comment(comment);
    describe(&key)
}

/// Re-derives the public half and fingerprint of a stored private key.
pub fn describe_private_key(private_key_openssh: &str) -> Result<KeyPair, SshError> {
    describe(&parse_private_key(private_key_openssh)?)
}

/// `SHA256:…` fingerprint of an OpenSSH public key line.
pub fn fingerprint_of(public_key_openssh: &str) -> Result<String, SshError> {
    Ok(fingerprint(&parse_public_key(public_key_openssh)?))
}

/// An authenticated SSH connection.
pub struct Session {
    handle: Handle<HostKeyCheck>,
    host_key: PublicKey,
}

impl Session {
    /// Connects, checks the host key against `trusted_host_key` (an OpenSSH
    /// public key line), and authenticates with `private_key_openssh`.
    pub async fn connect(
        target: &Target,
        private_key_openssh: &str,
        trusted_host_key: Option<&str>,
    ) -> Result<Self, SshError> {
        let key = parse_private_key(private_key_openssh)?;
        let trusted = trusted_host_key.map(parse_public_key).transpose()?;
        let presented = Arc::new(Mutex::new(None));
        let handler = HostKeyCheck {
            trusted: trusted.clone(),
            presented: presented.clone(),
        };
        let config = Arc::new(client::Config {
            keepalive_interval: Some(KEEPALIVE_INTERVAL),
            keepalive_max: 3,
            nodelay: true,
            ..Default::default()
        });

        let connecting = client::connect(config, (target.host.as_str(), target.port), handler);
        let mut handle = match tokio::time::timeout(CONNECT_TIMEOUT, connecting).await {
            Err(_) => return Err(SshError::Timeout("connecting")),
            Ok(Err(russh::Error::UnknownKey)) => {
                let presented = presented.lock().unwrap().take();
                return Err(host_key_error(trusted.as_ref(), presented));
            }
            Ok(result) => result?,
        };
        let host_key = presented
            .lock()
            .unwrap()
            .take()
            .ok_or(russh::Error::UnknownKey)?;

        let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
        let auth = handle
            .authenticate_publickey(
                target.username.clone(),
                PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
            )
            .await?;
        if !auth.success() {
            return Err(SshError::AuthRejected(target.username.clone()));
        }

        Ok(Self { handle, host_key })
    }

    /// The host key the server presented, as an OpenSSH public key line.
    pub fn host_key_openssh(&self) -> String {
        encode_public_key(&self.host_key)
    }

    pub fn host_key_fingerprint(&self) -> String {
        fingerprint(&self.host_key)
    }

    /// Runs `command` in a new exec channel, writes `stdin` and closes it, then
    /// collects output until the channel closes.
    pub async fn exec(&self, command: &str, stdin: &[u8]) -> Result<ExecOutput, SshError> {
        let mut channel = self.handle.channel_open_session().await?;
        channel.exec(true, command).await?;
        if !stdin.is_empty() {
            channel.data_bytes(stdin.to_vec()).await?;
        }
        channel.eof().await?;

        let mut output = ExecOutput::default();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => output.stdout.extend_from_slice(&data),
                ChannelMsg::ExtendedData { data, ext: 1 } => output.stderr.extend_from_slice(&data),
                ChannelMsg::ExitStatus { exit_status } => output.exit_status = Some(exit_status),
                ChannelMsg::Failure => return Err(SshError::ExecRejected),
                _ => {}
            }
        }
        Ok(output)
    }

    pub async fn close(self) -> Result<(), SshError> {
        self.handle
            .disconnect(Disconnect::ByApplication, "", "en")
            .await?;
        Ok(())
    }
}

/// Records the presented host key and accepts it only if it matches the trusted one.
struct HostKeyCheck {
    trusted: Option<PublicKey>,
    presented: Arc<Mutex<Option<PublicKey>>>,
}

impl client::Handler for HostKeyCheck {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = match server_public_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(cert) => PublicKey::from(cert.public_key().clone()),
        };
        let accepted = self
            .trusted
            .as_ref()
            .is_some_and(|trusted| trusted.key_data() == key.key_data());
        *self.presented.lock().unwrap() = Some(key);
        Ok(accepted)
    }
}

fn host_key_error(trusted: Option<&PublicKey>, presented: Option<PublicKey>) -> SshError {
    let Some(presented) = presented else {
        return russh::Error::UnknownKey.into();
    };
    match trusted {
        Some(trusted) => SshError::HostKeyChanged {
            trusted: fingerprint(trusted),
            presented: fingerprint(&presented),
        },
        None => SshError::UnknownHostKey {
            fingerprint: fingerprint(&presented),
            public_key: encode_public_key(&presented),
        },
    }
}

fn describe(key: &PrivateKey) -> Result<KeyPair, SshError> {
    Ok(KeyPair {
        private_key_openssh: key
            .to_openssh(LineEnding::LF)
            .map_err(invalid_key)?
            .to_string(),
        public_key_openssh: encode_public_key(key.public_key()),
        fingerprint: fingerprint(key.public_key()),
    })
}

fn parse_private_key(openssh: &str) -> Result<PrivateKey, SshError> {
    PrivateKey::from_openssh(openssh).map_err(invalid_key)
}

fn parse_public_key(openssh: &str) -> Result<PublicKey, SshError> {
    PublicKey::from_openssh(openssh.trim()).map_err(invalid_key)
}

fn encode_public_key(key: &PublicKey) -> String {
    key.to_openssh()
        .expect("encoding an in-memory public key cannot fail")
}

fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

fn invalid_key(error: impl std::fmt::Display) -> SshError {
    SshError::InvalidKey(error.to_string())
}
