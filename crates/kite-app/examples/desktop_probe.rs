//! Explicit manual integration probe, separate from the offline test suite.
use kite_app::{HostSpec, Target, check_host};
use kite_stream::{
    FRAME_CONFIG, FrameInfo, InputSender, MediaSink, StreamConfig, VideoFormat,
    control::{Client, Identity, new_device_id, new_pin},
    native::NativeSession,
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

struct Sink(AtomicU64);
impl MediaSink for Sink {
    fn supports(&self, _: VideoFormat) -> bool {
        true
    }
    fn configure(&self, f: VideoFormat) -> bool {
        println!("video_format={}x{}@{}", f.width, f.height, f.fps);
        true
    }
    fn submit(&self, _: &[u8], info: FrameInfo) -> bool {
        if info.flags & FRAME_CONFIG == 0 {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
        true
    }
    fn set_input(&self, _: Option<Arc<dyn InputSender>>) {}
    fn rendered_frames(&self) -> u64 {
        0
    }
    fn failure_code(&self) -> i32 {
        0
    }
    fn set_takeover(&self, _: bool) {}
    fn stop(&self) {}
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() != 5 {
        return Err(
            "usage: desktop_probe host username private-key-file host-public-key-file".into(),
        );
    }
    let private = std::fs::read_to_string(PathBuf::from(&args[3]))?;
    let trusted = std::fs::read_to_string(PathBuf::from(&args[4]))?;
    let target = Target {
        host: args[1].clone(),
        port: 22,
        username: args[2].clone(),
    };
    let check = check_host(
        &HostSpec {
            target: target.clone(),
            herdr_path: None,
        },
        &private,
        Some(&trusted),
    )
    .await?;
    println!("bridge={}", check.bridge.capability);
    let ssh = Arc::new(kite_ssh::Session::connect(&target, &private, Some(&trusted)).await?);
    let api = kite_herdr::api::Api::new(ssh.clone(), &check.bridge.herdr_path);
    println!("herdr_version={}", api.ping().await?);
    println!("agent_count={}", api.agents().await?.len());
    let client = Client::new(
        ssh.clone(),
        args[1].clone(),
        47989,
        new_device_id()?,
        Arc::new(Identity::generate()?),
        None,
    )?;
    println!("sunshine_name={}", client.info().await?.name);
    let pin = new_pin()?;
    let pairing = client.clone().pair(&pin);
    let approve = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        client.approve_pin_from_saved_credentials(&pin).await
    };
    let (paired, approved) = tokio::join!(pairing, approve);
    approved?;
    let paired = paired?;
    println!("paired=true");
    let outcome = async {
        let source = paired.desktop_size().await?;
        let config = StreamConfig::for_desktop(source, |_| true)?;
        let launch = paired.launch_desktop(config).await?;
        let sink = Arc::new(Sink(AtomicU64::new(0)));
        let media = sink.clone();
        let session =
            tokio::task::spawn_blocking(move || NativeSession::connect(launch, config, media))
                .await??;
        let mut success = false;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let stats = session.stats();
            if stats.received >= 30 {
                success = true;
                break;
            }
            if stats.terminated {
                break;
            }
        }
        println!("encoded_frames_received={}", session.stats().received);
        tokio::task::spawn_blocking(move || session.stop()).await?;
        if !success {
            return Err("did not receive 30 real encoded video frames".into());
        }
        println!("probe=PASS (encoded transport only; no decoder assertion)");
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    let _ = paired.unpair().await;
    ssh.close().await?;
    outcome
}
