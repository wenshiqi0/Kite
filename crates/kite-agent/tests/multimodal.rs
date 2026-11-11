use kite_agent::{Block, Role, composer::Attachment, parse_history};
use serde_json::{Value, json};

fn file() -> Value {
    json!({"id":"0123456789abcdef0123456789abcdef", "name":"clip.wav", "mime":"audio/wav", "size":12, "path":"/home/owned/.local/share/kite/uploads/0123456789abcdef0123456789abcdef/clip.wav"})
}

#[test]
fn all_authors_and_providers_share_ordered_content_and_cache_round_trips() {
    for provider in ["codex", "claude", "dsh", "ply"] {
        for (role, expected) in [("user", Role::User), ("assistant", Role::Assistant)] {
            let content = json!([
                {"type":"text", "text":"**Before**"},
                {"type":"kite_attachment", "file":file()},
                {"type":"text", "text":"After"}
            ]);
            let record = if provider == "claude" {
                json!({"uuid":"owned", "type":role, "message":{"content":content}})
            } else {
                json!({"type":"response_item", "payload":{"id":"owned", "type":"message", "role":role, "content":content}})
            };
            let items = parse_history(provider, record.to_string().as_bytes());
            assert_eq!(items.len(), 1, "{provider}/{role}");
            assert_eq!(items[0].role, expected);
            assert_eq!(
                items[0].blocks,
                vec![
                    Block::Text("**Before**".into()),
                    Block::Attachment(serde_json::from_value(file()).unwrap()),
                    Block::Text("After".into())
                ]
            );
            let cached: kite_agent::Item =
                serde_json::from_slice(&serde_json::to_vec(&items[0]).unwrap()).unwrap();
            assert_eq!(cached, items[0]);
            assert_eq!(items[0].presentation().blocks, items[0].blocks);
        }
    }
}

#[test]
fn media_only_messages_and_incremental_replacements_survive() {
    let content = json!([{"type":"kite_attachment", "file":file()}]);
    let first = json!({"type":"response_item", "payload":{"id":"same", "type":"message", "role":"assistant", "content":content}});
    let second = json!({"type":"response_item", "payload":{"id":"same", "type":"message", "role":"assistant", "content":[{"type":"kite_attachment", "file":file()}, {"type":"text", "text":"done"}]}});
    let one = parse_history("codex", first.to_string().as_bytes());
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].attachments().count(), 1);
    let all = parse_history("codex", format!("{first}\n{second}\n{second}").as_bytes());
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].attachments().count(), 1);
    assert_eq!(all[0].text(), "done");
}

#[test]
fn tool_output_media_is_visible_beside_the_tool_instead_of_being_collapsed() {
    for provider in ["codex", "claude"] {
        let records = if provider == "claude" {
            vec![
                json!({"type":"assistant", "message":{"content":[{"type":"tool_use", "id":"call", "name":"create_audio", "input":{}}]}}),
                json!({"type":"user", "message":{"content":[{"type":"tool_result", "tool_use_id":"call", "content":[{"type":"text", "text":"Created"}, {"type":"kite_attachment", "file":file()}]}]}}),
            ]
        } else {
            vec![
                json!({"type":"response_item", "payload":{"type":"function_call", "call_id":"call", "name":"create_audio", "arguments":"{}"}}),
                json!({"type":"response_item", "payload":{"type":"function_call_output", "call_id":"call", "output":[{"type":"text", "text":"Created"}, {"type":"kite_attachment", "file":file()}]}}),
            ]
        };
        let log = records
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        let items = parse_history(provider, log.as_bytes());
        let tool = items.iter().find(|item| item.id == "call").unwrap();
        assert_eq!(tool.blocks.len(), 2);
        assert_eq!(tool.tool().unwrap().output, "Created");
        assert_eq!(tool.attachments().count(), 1);
    }
}

#[test]
fn native_transport_copies_do_not_duplicate_envelope_attachments() {
    let mut file: Attachment = serde_json::from_value(file()).unwrap();
    file.mime = "image/png".into();
    let text = kite_agent::composer::envelope::encode(&[
        Block::Text("Look".into()),
        Block::Attachment(file.clone()),
    ])
    .unwrap();
    let record = json!({"type":"response_item", "payload":{"id":"owned", "type":"message", "role":"user", "content":[
        {"type":"text", "text":text}, {"type":"text", "text":"<image name=[Image #1]>"},
        {"type":"kite_attachment", "native_type":"input_image", "file":file}, {"type":"text", "text":"</image>"}
    ]}});
    let items = parse_history("codex", record.to_string().as_bytes());
    assert_eq!(items[0].text(), "Look");
    assert_eq!(items[0].attachments().count(), 1);
}

#[test]
fn markdown_projection_acknowledges_the_original_optimistic_input() {
    let source = "Before ![photo](https://example.test/photo.png) after";
    let local = kite_agent::Item::new(
        "local-1".into(),
        Role::User,
        vec![Block::Text(source.into())],
    );
    let record = json!({"type":"response_item", "payload":{"id":"saved", "type":"message", "role":"user", "_kite_content_fingerprint":kite_agent::fingerprint(source), "content":[
        {"type":"text", "text":"Before "}, {"type":"kite_attachment", "file":file()}, {"type":"text", "text":" after"}
    ]}});
    let items = parse_history("codex", record.to_string().as_bytes());
    assert!(items[0].same_content(&local));
    assert!(local.same_content(&items[0]));
    let other = kite_agent::Item::new(
        "local-2".into(),
        Role::User,
        vec![Block::Text("another prompt".into())],
    );
    assert!(!items[0].same_content(&other));
}

#[test]
fn projected_media_keeps_the_id_of_previously_cached_provider_records() {
    let raw = json!({"type":"response_item", "payload":{"type":"message", "role":"user", "content":[{"type":"input_text", "text":"Look"}, {"type":"input_image", "image_url":""}]}});
    let mut projected = raw.clone();
    projected["_kite_record_id"] = json!(kite_agent::fingerprint(&raw.to_string()));
    projected["payload"]["content"][1] = json!({"type":"kite_attachment", "file":file()});
    assert_eq!(
        parse_history("codex", raw.to_string().as_bytes())[0].id,
        parse_history("codex", projected.to_string().as_bytes())[0].id
    );
}
