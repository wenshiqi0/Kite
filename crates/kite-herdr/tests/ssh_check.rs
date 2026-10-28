//! End-to-end bridge checks against an in-process SSH server. The server runs
//! the probe script with a real `/bin/sh`, under a temporary HOME that may
//! hold a fake herdr.

use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use kite_herdr::{API_BRIDGE_CAPABILITY, CheckError, check_api_bridge};
use kite_ssh::{Session, SshError, Target};
use russh::keys::{Algorithm, PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Session as ServerSession, VerifiedPublicKey};
use russh::{Channel, ChannelId};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;

const CURRENT_HERDR: &str = r#"#!/bin/sh
case "$1" in
    remote-api-bridge) [ "$2" = --check ] && echo herdr-api-bridge-v1 ;;
    --version) echo "herdr 0.9.5" ;;
esac
"#;

/// A herdr from before the API bridge existed: the check prints nothing.
const OLD_HERDR: &str = "#!/bin/sh\nexit 2\n";

struct TestHost {
    port: u16,
    host_key: PublicKey,
    home: TempDir,
}

impl TestHost {
    async fn start(authorized: PublicKey) -> Self {
        let host_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let public_host_key = host_key.public_key().clone();
        let config = Arc::new(server::Config {
            keys: vec![host_key],
            auth_rejection_time: Duration::ZERO,
            auth_rejection_time_initial: Some(Duration::ZERO),
            ..Default::default()
        });
        let home = TempDir::new().unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let home_path = home.path().to_owned();
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                let handler = ShellHost {
                    authorized: authorized.clone(),
                    home: home_path.clone(),
                    channels: HashMap::new(),
                };
                let session = server::run_stream(config.clone(), socket, handler);
                tokio::spawn(async move {
                    if let Ok(session) = session.await {
                        let _ = session.await;
                    }
                });
            }
        });
        Self {
            port,
            host_key: public_host_key,
            home,
        }
    }

    fn install_herdr(&self, relative: &str, script: &str) -> PathBuf {
        let path = self.home.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn target(&self) -> Target {
        Target {
            host: "127.0.0.1".into(),
            port: self.port,
            username: "kite".into(),
        }
    }

    fn host_key_openssh(&self) -> String {
        self.host_key.to_openssh().unwrap()
    }
}

/// Accepts one client key and runs `/bin/sh -s` with the channel's stdin.
struct ShellHost {
    authorized: PublicKey,
    home: PathBuf,
    channels: HashMap<ChannelId, (String, Vec<u8>)>,
}

impl server::Handler for ShellHost {
    type Error = russh::Error;

    async fn auth_publickey(
        &mut self,
        _user: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        if key.public_key.key_data() == self.authorized.key_data() {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).into_owned();
        self.channels.insert(channel, (command, Vec::new()));
        session.channel_success(channel)
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        if let Some((_, stdin)) = self.channels.get_mut(&channel) {
            stdin.extend_from_slice(data);
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        let Some((command, stdin)) = self.channels.remove(&channel) else {
            return Ok(());
        };
        assert_eq!(command, "/bin/sh -s", "the probe must go through stdin");
        let output = run_shell(&self.home, &stdin).await;
        session.data(channel, output.stdout)?;
        session.extended_data(channel, 1, output.stderr)?;
        session.exit_status_request(channel, output.status.code().unwrap_or(255) as u32)?;
        session.eof(channel)?;
        session.close(channel)
    }
}

/// Runs the script with a minimal environment so a herdr installed on the
/// machine running the tests can't be found.
async fn run_shell(home: &Path, script: &[u8]) -> std::process::Output {
    let mut child = tokio::process::Command::new("/bin/sh")
        .arg("-s")
        .env_clear()
        .env("HOME", home)
        .env("PATH", "/usr/bin:/bin")
        .env("SHELL", "/nonexistent-shell")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(script).await.unwrap();
    child.wait_with_output().await.unwrap()
}

fn client_key() -> (String, PublicKey) {
    let pair = kite_ssh::generate_ed25519("kite-test").unwrap();
    let public = PublicKey::from_openssh(&pair.public_key_openssh).unwrap();
    (pair.private_key_openssh, public)
}

#[tokio::test]
async fn finds_herdr_in_the_default_install_location() {
    let (private_key, public_key) = client_key();
    let host = TestHost::start(public_key).await;
    let herdr = host.install_herdr(".local/bin/herdr", CURRENT_HERDR);

    let session = Session::connect(&host.target(), &private_key, Some(&host.host_key_openssh()))
        .await
        .unwrap();
    let check = check_api_bridge(&session, None).await.unwrap();
    session.close().await.unwrap();

    assert_eq!(check.herdr_path, herdr.to_str().unwrap());
    assert_eq!(check.capability, API_BRIDGE_CAPABILITY);
    assert_eq!(check.version.as_deref(), Some("herdr 0.9.5"));
}

#[tokio::test]
async fn reports_an_old_herdr_as_unsupported() {
    let (private_key, public_key) = client_key();
    let host = TestHost::start(public_key).await;
    let herdr = host.install_herdr("opt/herdr", OLD_HERDR);

    let session = Session::connect(&host.target(), &private_key, Some(&host.host_key_openssh()))
        .await
        .unwrap();
    let error = check_api_bridge(&session, herdr.to_str())
        .await
        .unwrap_err();

    assert!(
        matches!(&error, CheckError::Unsupported { path, .. } if Path::new(path) == herdr),
        "{error:?}"
    );
}

#[tokio::test]
async fn reports_a_missing_herdr() {
    let (private_key, public_key) = client_key();
    let host = TestHost::start(public_key).await;
    let missing = host.home.path().join("nowhere/herdr");

    let session = Session::connect(&host.target(), &private_key, Some(&host.host_key_openssh()))
        .await
        .unwrap();
    let error = check_api_bridge(&session, missing.to_str())
        .await
        .unwrap_err();

    assert!(matches!(error, CheckError::NotFound), "{error:?}");
}

#[tokio::test]
async fn refuses_an_untrusted_host_key_and_reports_its_fingerprint() {
    let (private_key, public_key) = client_key();
    let host = TestHost::start(public_key).await;

    let error = Session::connect(&host.target(), &private_key, None)
        .await
        .err()
        .unwrap();

    match error {
        SshError::UnknownHostKey {
            fingerprint,
            public_key,
        } => {
            assert_eq!(
                fingerprint,
                kite_ssh::fingerprint_of(&host.host_key_openssh()).unwrap()
            );
            assert_eq!(
                PublicKey::from_openssh(&public_key).unwrap().key_data(),
                host.host_key.key_data()
            );
        }
        other => panic!("expected UnknownHostKey, got {other:?}"),
    }
}

#[tokio::test]
async fn refuses_a_changed_host_key() {
    let (private_key, public_key) = client_key();
    let host = TestHost::start(public_key).await;
    let stale = kite_ssh::generate_ed25519("old-host")
        .unwrap()
        .public_key_openssh;

    let error = Session::connect(&host.target(), &private_key, Some(&stale))
        .await
        .err()
        .unwrap();

    assert!(
        matches!(error, SshError::HostKeyChanged { .. }),
        "{error:?}"
    );
}

#[tokio::test]
async fn reports_a_rejected_client_key() {
    let (_, authorized) = client_key();
    let (other_private_key, _) = client_key();
    let host = TestHost::start(authorized).await;

    let error = Session::connect(
        &host.target(),
        &other_private_key,
        Some(&host.host_key_openssh()),
    )
    .await
    .err()
    .unwrap();

    assert!(
        matches!(error, SshError::AuthRejected(ref user) if user == "kite"),
        "{error:?}"
    );
}
