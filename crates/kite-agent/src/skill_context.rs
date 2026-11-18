//! Display selected skills and provider-injected instructions in the transcript.
use crate::{Block, Item, Role, SkillLabel};
use std::{borrow::Borrow, collections::BTreeSet};

struct Context<'a> {
    name: &'a str,
    path: &'a str,
    content: &'a str,
}

fn line(text: &str) -> Option<(&str, &str)> {
    let (head, tail) = text.split_once('\n')?;
    Some((head.trim_end_matches('\r'), tail))
}

fn contexts(source: &str) -> Option<Vec<Context<'_>>> {
    let mut rest = source.trim();
    let mut found = Vec::new();
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
        found.push(Context {
            name,
            path,
            content: body[..close.0].trim_end_matches(['\r', '\n']),
        });
        rest = body[close.1..].trim_start();
    }
    (!found.is_empty()).then_some(found)
}

pub(crate) fn decode(source: &str) -> Option<Vec<Block>> {
    Some(
        contexts(source)?
            .into_iter()
            .map(|context| Block::SkillContext {
                name: context.name.into(),
                path: context.path.into(),
                content: context.content.into(),
            })
            .collect(),
    )
}

fn selection_header(item: &Item) -> Option<(&str, &str)> {
    let Some(Block::Text(text)) = item.blocks.first() else {
        return None;
    };
    if item.role != Role::User || !text.starts_with('$') {
        return None;
    }
    match text.split_once('\n') {
        Some((header, tail)) => Some((
            header.trim_end_matches('\r'),
            tail.strip_prefix('\n')
                .or_else(|| tail.strip_prefix("\r\n"))?,
        )),
        None => Some((text, "")),
    }
}

pub(crate) fn selection_body<'a>(item: &'a Item, labels: &[SkillLabel]) -> Option<&'a str> {
    if labels.is_empty() {
        return None;
    }
    let (header, body) = selection_header(item)?;
    header
        .split(' ')
        .map(|token| token.strip_prefix('$'))
        .eq(labels.iter().map(|skill| Some(skill.name.as_str())))
        .then_some(body)
}

fn context_names(block: &Block) -> Option<Vec<&str>> {
    match block {
        Block::Text(text) => Some(
            contexts(text)?
                .into_iter()
                .map(|context| context.name)
                .collect(),
        ),
        Block::SkillContext { name, .. } => Some(vec![name]),
        _ => None,
    }
}

/// Recover labels in legacy history using intact skill contexts in this user
/// item or in the consecutive context items that follow it.
pub fn resolve_skill_labels<T: Borrow<Item>>(
    items: &mut [T],
    make_mut: impl Fn(&mut T) -> &mut Item,
) {
    for index in 0..items.len() {
        let item = items[index].borrow();
        if !item.skill_labels.is_empty() {
            continue;
        }
        let Some((header, _)) = selection_header(item) else {
            continue;
        };
        let Some(names): Option<Vec<_>> = header
            .split(' ')
            .map(|token| token.strip_prefix('$').filter(|name| !name.is_empty()))
            .collect()
        else {
            continue;
        };
        let mut missing: BTreeSet<_> = names.iter().copied().collect();
        for name in item.blocks[1..].iter().filter_map(context_names).flatten() {
            missing.remove(name);
        }
        for next in &items[index + 1..] {
            let next = next.borrow();
            if missing.is_empty() || next.role != Role::User || next.blocks.is_empty() {
                break;
            }
            let Some(contexts): Option<Vec<_>> = next.blocks.iter().map(context_names).collect()
            else {
                break;
            };
            for name in contexts.into_iter().flatten() {
                missing.remove(name);
            }
        }
        if missing.is_empty() {
            let labels = names
                .into_iter()
                .map(|name| SkillLabel {
                    name: name.into(),
                    label: name.into(),
                })
                .collect();
            make_mut(&mut items[index]).skill_labels = labels;
        }
    }
}

pub(crate) fn project(item: &Item) -> Option<Vec<Block>> {
    let blocks = &item.blocks;
    let (first, mut projected) = if let Some(body) = selection_body(item, &item.skill_labels) {
        let mut projected = vec![Block::SkillSelection {
            names: item
                .skill_labels
                .iter()
                .map(|skill| skill.label.clone())
                .collect(),
        }];
        if !body.is_empty() {
            projected.push(Block::Text(body.into()));
        }
        (1, projected)
    } else {
        let first = blocks
            .iter()
            .position(|block| matches!(block, Block::Text(text) if contexts(text).is_some()))?;
        (first, blocks[..first].to_vec())
    };
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
    fn legacy_skill_headers_use_the_adjacent_context_and_keep_the_raw_prompt() {
        let prompt = "$report $plugin:review\n\n检查测试。预算 $100。";
        let second = CONTEXT.replace("report", "plugin:review");
        let wire = |role: &str, text: &str| {
            json!({"type":"response_item","payload":{"type":"message","role":role,
                "content":[{"type":"input_text","text":text}]}})
            .to_string()
        };
        let plain = "$report\n\nAn example of a shell variable.";
        let data = [
            wire("user", plain),
            wire("assistant", "Understood."),
            wire("user", prompt),
            wire("user", &second),
            wire("user", CONTEXT),
        ]
        .join("\n");
        let items = parse_history("codex", data.as_bytes());
        assert_eq!(items[0].presentation().blocks, [Block::Text(plain.into())]);
        assert_eq!(items[2].text(), prompt);
        assert_eq!(items[2].preview_text(), "检查测试。预算 $100。");
        assert_eq!(
            items[2].presentation().blocks,
            [
                Block::SkillSelection {
                    names: vec!["report".into(), "plugin:review".into()]
                },
                Block::Text("检查测试。预算 $100。".into()),
            ]
        );
        assert_eq!(items[4].presentation().blocks, [context_block()]);
    }

    #[test]
    fn a_skill_only_header_can_be_resolved_after_its_context_arrives() {
        let item = Item::new(
            "user".into(),
            Role::User,
            vec![Block::Text("$report".into())],
        );
        let mut page = vec![std::sync::Arc::new(item)];
        resolve_skill_labels(&mut page, std::sync::Arc::make_mut);
        assert_eq!(page[0].presentation().text(), "$report");
        page.push(std::sync::Arc::new(Item::new(
            "context".into(),
            Role::User,
            vec![Block::Text(CONTEXT.into())],
        )));
        resolve_skill_labels(&mut page, std::sync::Arc::make_mut);
        assert_eq!(page[0].preview_text(), "report");
        assert_eq!(
            page[0].presentation().blocks,
            [Block::SkillSelection {
                names: vec!["report".into()]
            }]
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
