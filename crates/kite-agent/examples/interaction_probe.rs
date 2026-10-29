//! Explicit live validation, restricted to a disposable validation pane.
use kite_agent::{
    composer::Client,
    history,
    interaction::{Activity, Answer},
};
use kite_herdr::api::Api;
use kite_ssh::{Session, Target};
use std::{sync::Arc, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 6 {
        return Err("usage: interaction_probe host user private-key-file host-public-key-file disposable-pane".into());
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
        return Err("a disposable validation agent is required".into());
    }
    let reference = agent
        .agent_session
        .as_ref()
        .ok_or("fixture thread missing")?;
    let first = Client::connect(&ssh, &agent.pane_id, &reference.value).await?;
    first.request("turn/start",serde_json::json!({"threadId":reference.value,"model":"gpt-6.1-sol","effort":"low","collaborationMode":{"mode":"plan","settings":{"model":"gpt-6.1-sol","reasoning_effort":"low","developer_instructions":null}},"input":[{"type":"text","text":"Call request_user_input with exactly two questions: id colour, header Colour, options Blue and Green; and id note, header Note, options Short and Long, allowing another text answer. Ask no other questions and run no other tools. After receiving the answers, reply KITE-FORM-ANSWERS followed by both exact answers."}]})).await?;
    let mut saw_thinking = false;
    let mut prompt = None;
    for _ in 0..200 {
        let runtime = first.runtime();
        saw_thinking |= matches!(
            runtime.phase(),
            Activity::Thinking | Activity::Tool | Activity::Responding
        );
        if let Some(question) = runtime.question() {
            prompt = Some(question);
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let prompt = prompt.ok_or("model did not ask the expected question")?;
    if prompt.questions.len() != 2 {
        return Err("expected two actual model questions".into());
    }
    println!(
        "model_question_count={} waiting_phase={:?} observed_processing={saw_thinking}",
        prompt.questions.len(),
        first.runtime().phase()
    );
    let second = Client::connect(&ssh, &agent.pane_id, &reference.value).await?;
    for _ in 0..50 {
        if second.runtime().question().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let resumed = second
        .runtime()
        .question()
        .ok_or("question was not replayed to the new client")?;
    if resumed.id != prompt.id {
        return Err("question identity changed on reconnect".into());
    }
    drop(first);
    let answers: Vec<_> = resumed
        .questions
        .iter()
        .map(|q| Answer {
            question_id: q.id.clone(),
            value: if q.id == "colour" {
                "Blue"
            } else {
                "Rust freeform"
            }
            .into(),
        })
        .collect();
    second.answer(&resumed.id, &answers).await?;
    let mut done = false;
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if second.runtime().phase() == Activity::Idle && second.runtime().question().is_none() {
            done = true;
            break;
        }
    }
    if !done {
        return Err("model did not complete after the answer".into());
    }
    let history = history::read(&ssh, reference, None).await;
    let reply = history
        .messages
        .iter()
        .rev()
        .find(|m| m.role == kite_agent::Role::Assistant && !m.intermediate)
        .ok_or("reply missing")?;
    if !reply.text.contains("KITE-FORM-ANSWERS")
        || !reply.text.contains("Rust freeform")
        || !reply.text.contains("Blue")
    {
        return Err("the actual model reply did not contain the submitted answers".into());
    }
    println!(
        "reconnected_answer_pass=true final_phase=idle choice_and_freeform_verified=true history_records={}",
        history.messages.len()
    );
    Ok(())
}
