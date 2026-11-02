//! The only codec for Kite's own structure inside provider history (RFC 0027).
//! Provider history stays authoritative and is never rewritten, so what Kite adds
//! travels in the message text. History written by any version reads through here.
use super::{Attachment, Error};
use crate::Block;

const ATTACHMENTS_OPEN: &str = "\n\n<kite_attachments>\n";
const ATTACHMENTS_CLOSE: &str = "\n</kite_attachments>";
const MAX_ATTACHMENTS: usize = 5;

/// The text a provider stores for a user message. Attachments keep their tail in
/// that text even when also sent natively: it is how they survive in history.
pub fn encode(blocks: &[Block]) -> Result<String, Error> {
    let text = text_of(blocks);
    let files: Vec<&Attachment> = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Attachment(file) => Some(file),
            _ => None,
        })
        .collect();
    if files.is_empty() {
        return Ok(text);
    }
    Ok(format!(
        "{text}{ATTACHMENTS_OPEN}{}{ATTACHMENTS_CLOSE}",
        serde_json::to_string(&files)?
    ))
}

/// The blocks a stored message carries. Text that is not a valid envelope stays text.
pub fn decode(text: &str) -> Vec<Block> {
    let (body, files) = split_attachments(text);
    let mut blocks = Vec::with_capacity(files.len() + 1);
    if !body.is_empty() {
        blocks.push(Block::Text(body.to_owned()));
    }
    blocks.extend(files.into_iter().map(Block::Attachment));
    blocks
}

fn split_attachments(text: &str) -> (&str, Vec<Attachment>) {
    if let Some((body, tail)) = text.rsplit_once(ATTACHMENTS_OPEN)
        && let Some(json) = tail.strip_suffix(ATTACHMENTS_CLOSE)
        && let Ok(files) = serde_json::from_str::<Vec<Attachment>>(json)
        && files.len() <= MAX_ATTACHMENTS
    {
        return (body, files);
    }
    (text, vec![])
}

fn text_of(blocks: &[Block]) -> String {
    let parts: Vec<&str> = blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> Attachment {
        Attachment {
            id: "a".into(),
            name: "photo.png".into(),
            mime: "image/png".into(),
            size: 3,
            path: "/home/user/.local/share/kite/uploads/a/photo.png".into(),
        }
    }

    #[test]
    fn history_written_before_blocks_keeps_reading_and_encoding_is_unchanged() {
        let stored = "Look at this\n\n<kite_attachments>\n[{\"id\":\"a\",\"name\":\"photo.png\",\"mime\":\"image/png\",\"size\":3,\"path\":\"/home/user/.local/share/kite/uploads/a/photo.png\"}]\n</kite_attachments>";
        let blocks = decode(stored);
        assert_eq!(
            blocks,
            vec![
                Block::Text("Look at this".into()),
                Block::Attachment(file())
            ]
        );
        assert_eq!(encode(&blocks).unwrap(), stored);
    }

    #[test]
    fn a_message_with_only_attachments_has_no_empty_text_block() {
        let blocks = vec![Block::Attachment(file())];
        let stored = encode(&blocks).unwrap();
        assert!(stored.starts_with("\n\n<kite_attachments>\n"));
        assert_eq!(decode(&stored), blocks);
    }

    #[test]
    fn ordinary_and_malformed_text_stays_text() {
        for text in [
            "plain text",
            "<kite_attachments>broken",
            "\n\n<kite_attachments>\nnot json\n</kite_attachments>",
            "talking about\n\n<kite_attachments>\n[]\n</kite_attachments> in the middle",
        ] {
            assert_eq!(decode(text), vec![Block::Text(text.into())]);
        }
        let many = encode(&vec![Block::Attachment(file()); MAX_ATTACHMENTS + 1]).unwrap();
        assert_eq!(decode(&many), vec![Block::Text(many.clone())]);
        assert!(decode("").is_empty());
    }
}
