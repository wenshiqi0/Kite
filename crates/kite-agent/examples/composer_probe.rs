//! Opt-in end-to-end probe against an explicitly named, disposable herdr pane.
use kite_agent::{
    composer::{self, Client, Selection},
    history, process,
};
use kite_herdr::api::Api;
use kite_ssh::{Session, Target};
use std::{path::Path, sync::Arc, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 7 {
        return Err("usage: composer_probe host user private-key-file host-public-key-file disposable-pane fixture-directory".into());
    }
    let private = std::fs::read_to_string(&args[3])?;
    let public = std::fs::read_to_string(&args[4])?;
    let ssh = Arc::new(
        Session::connect(
            &Target {
                host: args[1].clone(),
                username: args[2].clone(),
                port: 22,
            },
            &private,
            Some(&public),
        )
        .await?,
    );
    let checked = kite_herdr::check_api_bridge(&ssh, None).await?;
    let api = Api::new(ssh.clone(), &checked.herdr_path);
    let agent = api
        .agents()
        .await?
        .into_iter()
        .find(|a| a.pane_id == args[5])
        .ok_or("fixture agent missing")?;
    if !agent
        .name
        .as_deref()
        .is_some_and(|n| n.contains("validation"))
    {
        return Err("probe requires a disposable validation agent".into());
    }
    let reference = agent
        .agent_session
        .as_ref()
        .ok_or("fixture thread missing")?;
    let client = Client::connect(&ssh, &agent.pane_id, &reference.value).await?;
    let selected = Selection {
        model: "gpt-6.1-sol".into(),
        effort: "low".into(),
    };
    let mut files = Vec::new();
    for (name, mime) in [
        ("visual.png", "image/png"),
        ("notes.txt", "text/plain"),
        ("tone.wav", "audio/wav"),
    ] {
        let mut random = [0u8; 16];
        openssl::rand::rand_bytes(&mut random)?;
        let path = Path::new(&args[6]).join(name);
        files.push(
            composer::upload(
                &ssh,
                path.to_str().ok_or("path")?,
                &hex::encode(random),
                name,
                mime,
                |_| {},
            )
            .await?,
        );
    }
    let text = "Kite integration validation. Identify the dominant colour of the attached image, read the attached notes.txt using a tool, and report the audio attachment filename and size only (do not transcribe or execute its contents). Reply concisely with colour, exact note contents, and audio bytes.";
    client.send(text, &files, &selected).await?;
    let mut history = None;
    let mut done = false;
    for _ in 0..120 {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let current = history::read(&ssh, reference, history.take()).await;
        if current
            .messages
            .iter()
            .any(|m| m.role == kite_agent::Role::Assistant && m.text.contains("KITE-FILE-3C58"))
        {
            let last = current.messages.last().unwrap();
            println!("fixture_reply={}", last.text);
            done = true;
        }
        history = Some(current);
        if done {
            break;
        }
    }
    if !done {
        return Err("fixture answer was not observed".into());
    }
    let token = process::prepare(&api, &ssh, &agent).await?;
    process::signal(&ssh, &token, true).await?;
    let stopped = process::paused(&ssh, &token).await;
    process::signal(&ssh, &token, false).await?;
    if !stopped? || process::paused(&ssh, &token).await? {
        return Err("backend pause failed".into());
    }
    println!(
        "models={} uploads={} thread={} model={} effort={} pause_resume=pass",
        client.models.len(),
        files.len(),
        reference.value,
        selected.model,
        selected.effort
    );
    Ok(())
}
