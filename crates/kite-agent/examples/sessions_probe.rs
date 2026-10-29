//! Manual lifecycle validation. Only operates on an explicitly named validation pane.
use kite_agent::{composer::Client, history, interaction::Activity, sessions::Repository};
use kite_herdr::api::Api;
use kite_ssh::{Session, Target};
use std::{sync::Arc, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        return Err(
            "usage: sessions_probe host user private-key-file host-public-key-file disposable-pane"
                .into(),
        );
    }
    let ssh = Arc::new(
        Session::connect(
            &Target {
                host: args[1].clone(),
                username: args[2].clone(),
                port: 22,
            },
            &std::fs::read_to_string(&args[3])?,
            Some(&std::fs::read_to_string(&args[4])?),
        )
        .await?,
    );
    let checked = kite_herdr::check_api_bridge(&ssh, None).await?;
    let api = Api::new(ssh.clone(), &checked.herdr_path);
    let before = api.agents().await?;
    let agent = before
        .iter()
        .find(|a| a.pane_id == args[5])
        .ok_or("fixture missing")?
        .clone();
    if !agent
        .name
        .as_deref()
        .is_some_and(|n| n.contains("validation"))
    {
        return Err("disposable validation agent required".into());
    }
    let reference = agent
        .agent_session
        .as_ref()
        .ok_or("fixture thread missing")?
        .clone();
    let client = Client::connect(&ssh, &agent.pane_id, &reference.value).await?;
    let repo = Repository::new(&ssh);
    let catalog = repo.observe(&before).await?;
    let record = catalog
        .sessions
        .iter()
        .find(|r| r.reference.as_ref() == Some(&reference))
        .ok_or("record missing")?
        .clone();
    let mut random = [0u8; 16];
    openssl::rand::rand_bytes(&mut random)?;
    let draft = hex::encode(random);
    repo.create(
        draft.clone(),
        "claude".into(),
        "Claude validation".into(),
        agent.cwd.clone(),
    )
    .await?;
    repo.archive(&draft, true).await?;
    let saved = repo
        .read()
        .await?
        .sessions
        .into_iter()
        .find(|r| r.id == draft)
        .ok_or("Claude draft missing")?;
    if saved.provider != "claude"
        || saved.reference.is_some()
        || !saved.archived
        || api.agents().await?.len() != before.len()
    {
        return Err("saving a Claude draft connected a backend or lost its provider".into());
    }
    println!("claude_draft_saved=true claude_connection_attempted=false");
    client.request("turn/start",serde_json::json!({"threadId":reference.value,"model":"gpt-6.1-sol","effort":"low","collaborationMode":{"mode":"default","settings":{"model":"gpt-6.1-sol","reasoning_effort":"low","developer_instructions":null}},"input":[{"type":"text","text":"Run sleep 3 using the shell tool, then reply exactly KITE-ARCHIVE-CONTINUED. Do not inspect files or run any other command."}]})).await?;
    repo.archive(&record.id, true).await?;
    if !api
        .agents()
        .await?
        .iter()
        .any(|a| a.terminal_id == agent.terminal_id)
    {
        return Err("archive stopped the original terminal".into());
    }
    let mut finished = false;
    for _ in 0..300 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if client.runtime().phase() == Activity::Idle {
            finished = true;
            break;
        }
    }
    if !finished {
        return Err("archived task did not complete".into());
    }
    let history = history::read(&ssh, &reference, None).await;
    if !history
        .messages
        .iter()
        .any(|m| m.text == "KITE-ARCHIVE-CONTINUED")
    {
        return Err("background reply missing after archive".into());
    }
    repo.archive(&record.id, false).await?;
    drop(client);
    if !matches!(
        api.agents()
            .await?
            .iter()
            .find(|a| a.terminal_id == agent.terminal_id)
            .map(|a| a.agent_status.as_str()),
        Some("idle" | "done")
    ) {
        return Err("fixture became active before closing its terminal".into());
    }
    api.request(
        "workspace.close",
        serde_json::json!({"workspace_id":agent.workspace_id,"close_group":false}),
    )
    .await?;
    let (one, two) = tokio::join!(
        repo.resume(&record.id, &checked.herdr_path),
        repo.resume(&record.id, &checked.herdr_path)
    );
    let one = one?;
    let two = two?;
    if one.terminal_id != two.terminal_id {
        return Err("concurrent resume created two terminals".into());
    }
    let live = api.agents().await?;
    let matched: Vec<_> = live
        .iter()
        .filter(|a| a.agent_session.as_ref() == Some(&reference))
        .collect();
    if matched.len() != 1 {
        return Err("restoration duplicated or lost the provider thread".into());
    }
    println!(
        "archive_kept_task=true original_thread={} restored_pane={} restored_terminal={} concurrent_resume_single_owner=true session={} claude_draft={}",
        reference.value, one.pane_id, one.terminal_id, record.id, draft
    );
    Ok(())
}
