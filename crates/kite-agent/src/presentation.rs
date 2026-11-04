use serde_json::Value;

/// The report envelope is a machine receipt; its Markdown field is the public body.
/// Match the complete contract so ordinary JSON, code samples and partial writes
/// retain their original text. This never changes the persisted provider record.
pub(super) fn report_markdown(text: &str) -> Option<String> {
    if !text.trim_start().starts_with('{') {
        return None;
    }
    let value: Value = serde_json::from_str(text).ok()?;
    let report = value.as_object()?;
    if report.len() != 5
        || !matches!(
            report.get("status")?.as_str()?,
            "completed" | "partial" | "blocked" | "no_action"
        )
        || !["deliverables", "risks", "followups"].iter().all(|key| {
            report
                .get(*key)
                .and_then(Value::as_array)
                .is_some_and(|items| items.iter().all(Value::is_string))
        })
    {
        return None;
    }
    let markdown = report.get("summary_markdown")?.as_str()?;
    (!markdown.trim().is_empty()).then(|| markdown.to_owned())
}

#[cfg(test)]
mod tests {
    use crate::{Message, Role, parse_history};
    use serde_json::{Value, json};

    fn report() -> Value {
        json!({
            "status": "completed",
            "summary_markdown": "## 晨间查询\n\n**首要风险**\n\n- 核对验收结果。\n- [查看 CI](https://example.test/ci)",
            "deliverables": ["报告"],
            "risks": ["等待验收"],
            "followups": ["跟进 CI"]
        })
    }

    fn message(text: String) -> Message {
        Message {
            id: "saved-worker-answer".into(),
            role: Role::Assistant,
            text,
            intermediate: false,
            tool: None,
            question_replies: vec![],
        }
    }

    #[test]
    fn saved_reports_show_real_markdown_without_rewriting_the_json_receipt() {
        let report = report();
        for status in ["completed", "partial", "blocked", "no_action"] {
            let mut report = report.clone();
            report["status"] = status.into();
            let original = message(serde_json::to_string_pretty(&report).unwrap());
            let displayed = original.presentation();
            assert_eq!(displayed.text, report["summary_markdown"].as_str().unwrap());
            assert_eq!(displayed.id, original.id);
            assert_eq!(displayed.role, Role::Assistant);
            assert!(!displayed.text.contains("\\n"));
            assert!(!displayed.text.contains("summary_markdown"));
            assert_eq!(
                serde_json::from_str::<Value>(&original.text).unwrap(),
                report
            );
            let cached: Message =
                serde_json::from_slice(&serde_json::to_vec(&original).unwrap()).unwrap();
            assert_eq!(cached.presentation(), displayed);
        }
    }

    #[test]
    fn incomplete_or_different_json_contracts_keep_their_exact_source() {
        let mut cases = vec![
            r#"{"status":"completed","summary_markdown":"example"}"#.into(),
            serde_json::to_string(&report())
                .unwrap()
                .trim_end_matches('}')
                .into(),
            format!("```json\n{}\n```", report()),
            "Plain Markdown\n\n**Done**".into(),
        ];
        for (key, value) in [
            ("status", json!("unknown")),
            ("summary_markdown", json!(" \n ")),
            ("summary_markdown", Value::Null),
            ("risks", json!([{"text": "different schema"}])),
            ("deliverables", Value::Null),
            ("extra_field", json!(true)),
        ] {
            let mut report = report();
            report[key] = value;
            cases.push(report.to_string());
        }
        for text in cases {
            let original = message(text);
            assert_eq!(original.presentation().as_ref(), &original);
        }
    }

    #[test]
    fn user_messages_tools_and_intermediate_text_are_never_interpreted_as_reports() {
        for role in [Role::User, Role::Tool, Role::Error] {
            let mut original = message(report().to_string());
            original.role = role;
            assert_eq!(original.presentation().as_ref(), &original);
        }
        let mut original = message(report().to_string());
        original.intermediate = true;
        assert_eq!(original.presentation().as_ref(), &original);
    }

    #[test]
    fn final_report_history_keeps_its_identity_before_and_after_cache_restore() {
        let row = json!({"type":"response_item","payload":{
            "type":"message", "id":"worker-final", "role":"assistant", "phase":"final_answer",
            "content":[{"type":"output_text","text":report().to_string()}]
        }});
        let parsed = parse_history("codex", row.to_string().as_bytes());
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].presentation().id, "worker-final");
        assert_eq!(
            parsed[0].presentation().text,
            report()["summary_markdown"].as_str().unwrap()
        );
        assert!(parsed[0].text.contains("summary_markdown"));
    }

    #[test]
    #[ignore = "requires KITE_REPORT_HISTORY_FIXTURE pointing to an authorized local transcript"]
    fn existing_transcript_report_reaches_presentation_without_editing_source() {
        let path = std::env::var("KITE_REPORT_HISTORY_FIXTURE").expect("transcript path required");
        let bytes = std::fs::read(&path).unwrap();
        let parsed = parse_history("codex", &bytes);
        let original = parsed
            .iter()
            .find(|m| m.role == Role::Assistant && super::report_markdown(&m.text).is_some())
            .expect("a completed report in the authorized transcript");
        let report: Value = serde_json::from_str(&original.text).unwrap();
        assert_eq!(
            original.presentation().text,
            report["summary_markdown"].as_str().unwrap()
        );
        assert!(!original.presentation().text.contains("\\n"));
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        eprintln!(
            "Existing report: rendered {} Markdown characters; original transcript unchanged",
            original.presentation().text.chars().count()
        );
    }
}
