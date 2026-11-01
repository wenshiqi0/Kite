//! Real authenticated SSH channels, owned ephemeral keys and no local host changes.
use kite_agent::composer::{download_image, helper_command, quote};
use kite_ssh::{Session, Target};
use russh::keys::{Algorithm, PrivateKey, PublicKey};
use russh::server::{self, Auth, Msg, Session as ServerSession, VerifiedPublicKey};
use russh::{Channel, ChannelId};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio::net::TcpListener;

struct ImageHost {
    authorized: PublicKey,
    id: String,
    bytes: Arc<Vec<u8>>,
    pending: HashSet<ChannelId>,
}
impl server::Handler for ImageHost {
    type Error = russh::Error;
    async fn auth_publickey(
        &mut self,
        _: &str,
        key: VerifiedPublicKey<'_>,
    ) -> Result<Auth, Self::Error> {
        Ok(if key.public_key.key_data() == self.authorized.key_data() {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }
    async fn channel_open_session(
        &mut self,
        _: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _: &mut ServerSession,
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
        assert_eq!(
            data,
            helper_command(&format!("image-read --id {}", quote(&self.id))).as_bytes()
        );
        self.pending.insert(channel);
        session.channel_success(channel)
    }
    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        session: &mut ServerSession,
    ) -> Result<(), Self::Error> {
        assert!(self.pending.remove(&channel));
        // Like the real Node CLI, completion requires download-only stdin EOF.
        session.data(channel, self.bytes.as_ref().clone())?;
        session.exit_status_request(channel, 0)?;
        session.eof(channel)?;
        session.close(channel)
    }
}
async fn fixture(id: &str, bytes: Vec<u8>) -> (Session, tokio::task::JoinHandle<()>) {
    let client = kite_ssh::generate_ed25519("image-download-fixture").unwrap();
    let authorized = PublicKey::from_openssh(&client.public_key_openssh).unwrap();
    let host_key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
    let trusted = host_key.public_key().to_openssh().unwrap();
    let config = Arc::new(server::Config {
        keys: vec![host_key],
        auth_rejection_time: Duration::ZERO,
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });
    let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let target = Target {
        host: "127.0.0.1".into(),
        port: listener.local_addr().unwrap().port(),
        username: "fixture".into(),
    };
    let id = id.to_owned();
    let bytes = Arc::new(bytes);
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let handler = ImageHost {
            authorized,
            id,
            bytes,
            pending: HashSet::new(),
        };
        if let Ok(session) = server::run_stream(config, socket, handler).await {
            let _ = session.await;
        }
    });
    let ssh = Session::connect(&target, &client.private_key_openssh, Some(&trusted))
        .await
        .unwrap();
    (ssh, task)
}
#[tokio::test]
async fn large_images_finish_after_stdin_eof_and_verify_their_exact_bytes() {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend((0..6 * 1024 * 1024).map(|i| (i % 251) as u8));
    let id = hex::encode(openssl::sha::sha256(&bytes));
    let (ssh, task) = fixture(&id, bytes.clone()).await;
    let image = tokio::time::timeout(Duration::from_secs(5), download_image(&ssh, &id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(image, bytes);
    ssh.close().await.unwrap();
    task.abort();
}
#[tokio::test]
async fn changed_remote_images_are_rejected_instead_of_entering_the_cache() {
    let id = hex::encode(openssl::sha::sha256(b"original image"));
    let (ssh, task) = fixture(&id, b"changed image".to_vec()).await;
    assert!(download_image(&ssh, "../file").await.is_err());
    assert!(
        tokio::time::timeout(Duration::from_secs(5), download_image(&ssh, &id))
            .await
            .unwrap()
            .is_err()
    );
    ssh.close().await.unwrap();
    task.abort();
}
