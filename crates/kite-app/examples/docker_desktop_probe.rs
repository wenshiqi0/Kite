//! Owned Docker integration only: localhost QA ports, encoded media and native input.
use kite_app::{HostSpec, Target, check_host};
use kite_stream::{
    FRAME_CONFIG, FrameInfo, Input, InputSender, MediaSink, StreamConfig, VideoFormat,
    control::{Client, Identity, new_device_id, new_pin},
    native::NativeSession,
};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Default)]
struct Sink {
    frames: AtomicU64,
    audio: AtomicU64,
    input: Mutex<Option<Arc<dyn InputSender>>>,
}
impl MediaSink for Sink {
    fn configure_audio(&self, _: kite_stream::AudioFormat) {}
    fn submit_audio(&self, _: &[u8]) {
        self.audio.fetch_add(1, Ordering::Relaxed);
    }
    fn supports(&self, _: VideoFormat) -> bool {
        true
    }
    fn configure(&self, f: VideoFormat) -> bool {
        println!("video_format={}x{}@{}", f.width, f.height, f.fps);
        true
    }
    fn submit(&self, _: &[u8], info: FrameInfo) -> bool {
        if info.flags & FRAME_CONFIG == 0 {
            self.frames.fetch_add(1, Ordering::Relaxed);
        }
        true
    }
    fn set_input(&self, sender: Option<Arc<dyn InputSender>>) {
        *self.input.lock().unwrap() = sender;
    }
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
            "usage: docker_desktop_probe ssh-port sunshine-port private-key-file host-public-key-file".into(),
        );
    }
    let ssh_port: u16 = args[1].parse()?;
    let sunshine_port: u16 = args[2].parse()?;
    if !(23000..24000).contains(&ssh_port) || !(33000..34000).contains(&sunshine_port) {
        return Err("only owned Docker QA ports are allowed".into());
    }
    let private = std::fs::read_to_string(PathBuf::from(&args[3]))?;
    let trusted = std::fs::read_to_string(PathBuf::from(&args[4]))?;
    let target = Target {
        host: "127.0.0.1".into(),
        port: ssh_port,
        username: "kite".into(),
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
        "127.0.0.1".into(),
        sunshine_port,
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
        let sink = Arc::new(Sink::default());
        let media = sink.clone();
        let session =
            tokio::task::spawn_blocking(move || NativeSession::connect(launch, config, media))
                .await??;
        let mut success = false;
        for _ in 0..100 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let stats = session.stats();
            if stats.received >= 30 && sink.audio.load(Ordering::Relaxed) > 0 {
                success = true;
                break;
            }
            if stats.terminated {
                break;
            }
        }
        println!("encoded_frames_received={}", session.stats().received);
        println!(
            "opus_packets_received={}",
            sink.audio.load(Ordering::Relaxed)
        );
        let input_result = async {
            if !success {
                return Err("missing encoded video or Opus packets".into());
            }
            let output = ssh
                .exec(
                    "cat /home/kite/.local/state/kite/docker-qa/input-state.json",
                    &[],
                )
                .await?;
            let state: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            if state["input_ready"] != true {
                return Err("owned GTK fixture is not ready".into());
            }
            let input = sink
                .input
                .lock()
                .unwrap()
                .clone()
                .ok_or("missing native input sender")?;
            let target = &state["target"];
            let x = target["x"].as_i64().ok_or("missing target x")?
                + target["width"].as_i64().ok_or("missing width")? / 2;
            let y = target["y"].as_i64().ok_or("missing target y")?
                + target["height"].as_i64().ok_or("missing height")? / 2;
            input.send(Input::Position {
                x: i16::try_from(x)?,
                y: i16::try_from(y)?,
                width: 1920,
                height: 1080,
            })?;
            tokio::time::sleep(Duration::from_millis(150)).await;
            input.send(Input::MouseButton {
                button: 1,
                down: true,
            })?;
            input.send(Input::MouseButton {
                button: 1,
                down: false,
            })?;
            input.send(Input::Key {
                key: 0x77,
                down: true,
                modifiers: 0,
            })?;
            input.send(Input::Key {
                key: 0x77,
                down: false,
                modifiers: 0,
            })?;
            tokio::time::sleep(Duration::from_millis(600)).await;
            let output = ssh
                .exec(
                    "cat /home/kite/.local/state/kite/docker-qa/input-state.json",
                    &[],
                )
                .await?;
            let state: serde_json::Value = serde_json::from_slice(&output.stdout)?;
            if state["left"].as_u64().unwrap_or(0) < 1 || state["f8"] != true {
                return Err(
                    "Sunshine mouse or keyboard did not reach the owned GTK desktop".into(),
                );
            }
            println!("sunshine_native_mouse_keyboard=PASS");
            Ok::<(), Box<dyn std::error::Error>>(())
        }
        .await;
        tokio::task::spawn_blocking(move || session.stop()).await?;
        input_result?;
        println!("probe=PASS (encoded video/audio and native input; no decoder assertion)");
        Ok::<(), Box<dyn std::error::Error>>(())
    }
    .await;
    let _ = paired.unpair().await;
    ssh.close().await?;
    outcome
}
