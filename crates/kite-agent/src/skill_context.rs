//! Display Codex's skill context without attributing its instructions to the user.
//! Only complete envelopes qualify; prose and malformed examples stay ordinary text.
use crate::Block;

fn line(text: &str) -> Option<(&str, &str)> {
    let (head, tail) = text.split_once('\n')?;
    Some((head.trim_end_matches('\r'), tail))
}

pub(crate) fn decode(source: &str) -> Option<Vec<Block>> {
    let mut rest = source.trim();
    let mut blocks = Vec::new();
    while !rest.is_empty() {
        let (open, tail) = line(rest)?;
        if open != "<skill>" {
            return None;
        }
        let (name, tail) = line(tail)?;
        let name = name.strip_prefix("<name>")?.strip_suffix("</name>")?;
        if name.is_empty()
            || name.len() > 256
            || name
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '<' | '>'))
        {
            return None;
        }
        let (path, body) = line(tail)?;
        let path = path.strip_prefix("<path>")?.strip_suffix("</path>")?;
        if !path.starts_with('/')
            || path
                .chars()
                .any(|c| c.is_control() || matches!(c, '<' | '>'))
        {
            return None;
        }
        let mut offset = 0;
        let close = body.split_inclusive('\n').find_map(|part| {
            let start = offset;
            offset += part.len();
            (part.trim_end_matches(['\r', '\n']) == "</skill>").then_some((start, offset))
        })?;
        blocks.push(Block::SkillContext {
            name: name.into(),
            path: path.into(),
            content: body[..close.0].trim_end_matches(['\r', '\n']).into(),
        });
        rest = body[close.1..].trim_start();
    }
    (!blocks.is_empty()).then_some(blocks)
}

pub(crate) fn project(blocks: &[Block]) -> Option<Vec<Block>> {
    let first = blocks
        .iter()
        .position(|block| matches!(block, Block::Text(text) if decode(text).is_some()))?;
    let mut projected = blocks[..first].to_vec();
    for block in &blocks[first..] {
        if let Block::Text(text) = block
            && let Some(skills) = decode(text)
        {
            projected.extend(skills);
        } else {
            projected.push(block.clone());
        }
    }
    Some(projected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Item, Role, parse_history};
    use serde_json::json;

    const CONTEXT: &str = "<skill>\n<name>report</name>\n<path>/skills/report/SKILL.md</path>\n# Report\n\nWrite **clearly**.\n</skill>";

    fn context_block() -> Block {
        Block::SkillContext {
            name: "report".into(),
            path: "/skills/report/SKILL.md".into(),
            content: "# Report\n\nWrite **clearly**.".into(),
        }
    }

    #[test]
    fn complete_context_is_structured_only_for_display_and_survives_cache_restore() {
        let item = Item::new(
            "context".into(),
            Role::User,
            vec![Block::Text(CONTEXT.into())],
        );
        assert_eq!(item.presentation().blocks, vec![context_block()]);
        assert_eq!(item.text(), CONTEXT);
        let saved = serde_json::to_string(&item).unwrap();
        let restored: Item = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored, item);
        assert_eq!(restored.presentation().blocks, vec![context_block()]);
        let assistant = Item {
            role: Role::Assistant,
            ..item
        };
        assert_eq!(assistant.presentation().text(), CONTEXT);
    }

    #[test]
    fn multiple_skills_and_crlf_keep_their_individual_sources_and_bodies() {
        let second = CONTEXT
            .replace("report", "plugin:review")
            .replace("Write **clearly**.", "检查修改。");
        let blocks = decode(&format!(
            " \n{CONTEXT}\n\n{}\n",
            second.replace('\n', "\r\n")
        ))
        .unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], context_block());
        assert!(
            matches!(&blocks[1], Block::SkillContext { name, path, content }
            if name == "plugin:review" && path == "/skills/plugin:review/SKILL.md" && content.ends_with("检查修改。"))
        );
    }

    #[test]
    fn ordinary_mentions_examples_partial_envelopes_and_appended_requests_stay_text() {
        for text in [
            "$report\n\nSummarize changes".into(),
            "Explain <skill> tags".into(),
            format!("Here is an example:\n{CONTEXT}"),
            format!("```xml\n{CONTEXT}\n```"),
            format!("{CONTEXT}\n\nPlease also check the tests."),
            CONTEXT.replace("</skill>", ""),
            CONTEXT.replace("<name>report</name>", "<name></name>"),
            CONTEXT.replace("/skills/report/SKILL.md", "relative/SKILL.md"),
            CONTEXT.replace("</path>", ""),
        ] {
            assert!(decode(&text).is_none(), "must stay text: {text}");
            let item = Item::new("ordinary".into(), Role::User, vec![Block::Text(text)]);
            assert_eq!(item.presentation().as_ref(), &item);
        }
    }

    #[test]
    fn mixed_provider_content_keeps_user_text_and_attachments_in_their_original_order() {
        let file = crate::composer::Attachment {
            id: "file".into(),
            name: "notes.txt".into(),
            mime: "text/plain".into(),
            size: 10,
            path: "/uploads/notes.txt".into(),
        };
        let after = crate::composer::envelope::encode(&[
            Block::Text("Check the tests too.".into()),
            Block::Attachment(file.clone()),
        ])
        .unwrap();
        let record = json!({"type":"response_item","payload":{"type":"message","id":"mixed","role":"user","content":[
            {"type":"input_text","text":"Summarize changes."},
            {"type":"input_text","text":CONTEXT},
            {"type":"input_text","text":after}
        ]}});
        let items = parse_history("codex", record.to_string().as_bytes());
        assert_eq!(
            items[0].presentation().blocks,
            vec![
                Block::Text("Summarize changes.".into()),
                context_block(),
                Block::Text("Check the tests too.".into()),
                Block::Attachment(file),
            ]
        );
        assert!(items[0].text().contains(CONTEXT));
    }
}
