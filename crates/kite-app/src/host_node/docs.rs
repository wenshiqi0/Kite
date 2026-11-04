//! One compiled document registry shared by shell, skills and MCP consumers.
use super::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;

const ROOT: &str = include_str!("../../../../docs/agent/index.md");
pub const SKILL: &str = include_str!("../../../../skills/kite/SKILL.md");
struct Topic {
    id: &'static str,
    summary: &'static str,
    body: &'static str,
}
const TOPICS: &[Topic] = &[
    Topic {
        id: "desktop",
        summary: "Common X11 observe/input/launch workflow",
        body: include_str!("../../../../docs/agent/desktop.md"),
    },
    Topic {
        id: "desktop/input",
        summary: "Regions, coordinates, fields and limits",
        body: include_str!("../../../../docs/agent/desktop-input.md"),
    },
    Topic {
        id: "host",
        summary: "Node and external service health",
        body: include_str!("../../../../docs/agent/host.md"),
    },
    Topic {
        id: "sessions",
        summary: "Read saved references and notifications",
        body: include_str!("../../../../docs/agent/sessions.md"),
    },
    Topic {
        id: "harnesses",
        summary: "Portable CLI, MCP and shared skill integration",
        body: include_str!("../../../../docs/agent/harnesses.md"),
    },
    Topic {
        id: "collaboration",
        summary: "Authenticated network ACP peers and explicit remote tasks",
        body: include_str!("../../../../docs/agent/collaboration.md"),
    },
    Topic {
        id: "upgrades",
        summary: "Live handover and first maintenance migration",
        body: include_str!("../../../../docs/agent/upgrades.md"),
    },
    Topic {
        id: "skill",
        summary: "Minimal standard SKILL.md bundle",
        body: SKILL,
    },
];

fn revision(id: &str, summary: &str, body: &str) -> String {
    // An opaque content revision, not an authentication token. Short hashes keep
    // incremental requests cheap and independent of unrelated app releases.
    kite_agent::fingerprint(&format!("{id}\0{summary}\0{body}"))[..16].into()
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    #[serde(default)]
    pub topics: Vec<String>,
    #[serde(default)]
    pub known: BTreeMap<String, String>,
}
#[derive(Serialize)]
pub struct Document {
    pub topic: String,
    pub revision: String,
    pub unchanged: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

pub fn index() -> Value {
    json!({"revision":revision("index", "", &format!("{ROOT}{}", TOPICS.iter().map(|t|revision(t.id,t.summary,t.body)).collect::<String>())),
        "content":ROOT,"topics":TOPICS.iter().map(|t|json!({"id":t.id,"summary":t.summary,"revision":revision(t.id,t.summary,t.body),"bytes":t.body.len()})).collect::<Vec<_>>()})
}
pub fn read(request: Request) -> Result<Vec<Document>> {
    if request.topics.len() > 4 || request.known.len() > 16 {
        return Err(Error::Config("docs accepts at most four topics per call"));
    }
    let ids = if request.topics.is_empty() {
        vec!["index".into()]
    } else {
        request.topics
    };
    let mut documents = Vec::new();
    for id in ids {
        if documents.iter().any(|d: &Document| d.topic == id) {
            continue;
        }
        let (body, rev) = if id == "index" {
            (
                ROOT,
                index()["revision"].as_str().unwrap_or_default().into(),
            )
        } else {
            let topic = TOPICS.iter().find(|t| t.id == id).ok_or(Error::Config(
                "unknown docs topic; run kite docs --json for the index",
            ))?;
            (topic.body, revision(topic.id, topic.summary, topic.body))
        };
        let unchanged = request.known.get(&id) == Some(&rev);
        documents.push(Document {
            topic: id,
            revision: rev,
            unchanged,
            content: (!unchanged).then(|| body.to_owned()),
        });
    }
    Ok(documents)
}
pub fn text(documents: &[Document]) -> String {
    documents
        .iter()
        .map(|document| match &document.content {
            Some(body) => format!(
                "[kite docs {} revision={}]\n{body}",
                document.topic, document.revision
            ),
            None => format!(
                "[kite docs {} unchanged revision={}]",
                document.topic, document.revision
            ),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}
pub fn cli(args: &[String]) -> Result<i32> {
    let mut request = Request::default();
    let mut json_output = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--json" => json_output = true,
            "--known" => {
                i += 1;
                let (topic, rev) = args
                    .get(i)
                    .and_then(|s| s.split_once('='))
                    .ok_or(Error::Config("--known requires TOPIC=REV"))?;
                request.known.insert(topic.into(), rev.into());
            }
            "--help" | "-h" => {
                std::io::stdout().write_all(b"kite docs [TOPIC ...] [--json] [--known TOPIC=REV]\nRead desktop directly for common actions; revisions are caller-owned context caches.\n")?;
                return Ok(0);
            }
            option if option.starts_with('-') => return Err(Error::Config("unknown docs option")),
            topic => request.topics.push(topic.into()),
        }
        i += 1;
    }
    if !json_output && request.topics == ["skill"] && request.known.is_empty() {
        std::io::stdout().write_all(SKILL.as_bytes())?;
    } else if json_output && request.topics.is_empty() && request.known.is_empty() {
        writeln!(std::io::stdout(), "{}", index())?;
    } else {
        let documents = read(request)?;
        if json_output {
            writeln!(std::io::stdout(), "{}", json!({"documents":documents}))?;
        } else {
            writeln!(std::io::stdout(), "{}", text(&documents))?;
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_context_omits_only_matching_bodies_and_unknown_revisions_refetch() {
        let first = read(Request {
            topics: vec!["desktop".into(), "host".into()],
            ..Default::default()
        })
        .unwrap();
        let cached = read(Request {
            topics: vec!["desktop".into(), "host".into()],
            known: BTreeMap::from([
                ("desktop".into(), first[0].revision.clone()),
                ("host".into(), "obsolete".into()),
            ]),
        })
        .unwrap();
        assert!(cached[0].unchanged && cached[0].content.is_none());
        assert!(!cached[1].unchanged && cached[1].content.is_some());
        let fresh = read(Request {
            topics: vec!["desktop".into()],
            ..Default::default()
        })
        .unwrap();
        assert!(
            fresh[0].content.is_some(),
            "a new consumer must not inherit a global read marker"
        );
    }
    #[test]
    fn content_and_routing_changes_invalidate_the_revision() {
        assert_ne!(
            revision("x", "summary", "old"),
            revision("x", "summary", "new")
        );
        assert_ne!(revision("x", "old", "body"), revision("x", "new", "body"));
        assert!(
            read(Request {
                topics: vec!["../../private".into()],
                ..Default::default()
            })
            .is_err()
        );
    }
}
