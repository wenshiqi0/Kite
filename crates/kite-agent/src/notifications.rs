//! Persistent notification facts and transitions; no platform or delivery service.
use crate::fingerprint;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Completed,
    Attention,
    Question,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub sequence: u64,
    pub created_ms: i64,
    pub session_id: String,
    pub title: String,
    pub kind: Kind,
    pub source: String,
}
impl Event {
    pub fn valid(&self) -> bool {
        self.id.len() == 64
            && self.id.bytes().all(|b| b.is_ascii_hexdigit())
            && self.sequence > 0
            && !self.session_id.is_empty()
            && self.session_id.len() <= 128
            && self.title.len() <= 2048
            && self.source.len() <= 256
            && self.created_ms >= 0
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Fact {
    pub session_id: String,
    pub title: String,
    pub source: String,
    pub status: String,
    pub revision: u64,
    pub questions: Vec<String>,
    pub questions_known: bool,
    pub lifecycle: bool,
    pub terminal: Option<Terminal>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Terminal {
    pub id: String,
    pub kind: Kind,
}
pub fn terminal_from_controls(bytes: &[u8]) -> Option<Terminal> {
    bytes
        .split(|b| *b == b'\n')
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter_map(|record| {
            if record["type"] != "event_msg" {
                return None;
            }
            let kind = match record["payload"]["type"].as_str() {
                Some("task_complete") => Kind::Completed,
                Some("task_aborted") => Kind::Failed,
                _ => return None,
            };
            let id = record["payload"]["turn_id"].as_str()?.to_owned();
            if id.is_empty() || id.len() > 128 {
                return None;
            }
            Some(Terminal { id, kind })
        })
        .next_back()
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Seen {
    source: String,
    status: String,
    working: bool,
    questions: Vec<String>,
    terminal: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ledger {
    version: u32,
    pub node_id: String,
    sequence: u64,
    seen: BTreeMap<String, Seen>,
    events: VecDeque<Event>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Page {
    pub node_id: String,
    pub cursor: String,
    pub reset: bool,
    pub more: bool,
    pub events: Vec<Event>,
}
impl Page {
    pub fn valid(&self) -> bool {
        if self.node_id.is_empty() || self.node_id.len() > 128 || self.events.len() > 64 {
            return false;
        }
        let prefix = format!("{}:", self.node_id);
        let Some(sequence) = self
            .cursor
            .strip_prefix(&prefix)
            .and_then(|s| s.parse::<u64>().ok())
        else {
            return false;
        };
        self.events
            .iter()
            .all(|e| e.valid() && e.sequence <= sequence)
            && self
                .events
                .windows(2)
                .all(|pair| pair[0].sequence < pair[1].sequence)
    }
}
impl Ledger {
    pub fn new(node_id: String) -> Self {
        Self {
            version: 1,
            node_id,
            sequence: 0,
            seen: BTreeMap::new(),
            events: VecDeque::new(),
        }
    }
    pub fn valid(&self, node: &str) -> bool {
        self.version == 1
            && self.node_id == node
            && self.seen.len() <= 1024
            && self.events.len() <= 512
            && self
                .events
                .iter()
                .all(|event| event.valid() && event.sequence <= self.sequence)
            && self.seen.iter().all(|(id, seen)| {
                !id.is_empty()
                    && id.len() <= 128
                    && seen.source.len() <= 256
                    && seen.status.len() <= 64
                    && seen.questions.len() <= 64
                    && seen.questions.iter().all(|q| q.len() <= 256)
            })
    }
    fn emit(&mut self, fact: &Fact, kind: Kind, source: &str, now: i64) {
        let id = fingerprint(&format!(
            "{}/{}/{}/{:?}/{source}",
            self.node_id, fact.session_id, fact.source, kind
        ));
        if self.events.iter().any(|event| event.id == id) {
            return;
        }
        self.sequence = self.sequence.saturating_add(1);
        self.events.push_back(Event {
            id,
            sequence: self.sequence,
            created_ms: now,
            session_id: fact.session_id.clone(),
            title: fact.title.chars().take(160).collect(),
            kind,
            source: source.into(),
        });
        while self.events.len() > 512 {
            self.events.pop_front();
        }
    }
    pub fn observe(&mut self, facts: &[Fact], now: i64) -> bool {
        let old_sequence = self.sequence;
        let old_seen = self.seen.clone();
        for fact in facts.iter().take(1024) {
            if fact.session_id.is_empty()
                || fact.session_id.len() > 128
                || fact.source.is_empty()
                || fact.source.len() > 256
                || fact.questions.len() > 64
                || fact.status.len() > 64
                || fact.questions.iter().any(|question| question.len() > 256)
            {
                continue;
            }
            let old = self
                .seen
                .get(&fact.session_id)
                .filter(|old| old.source == fact.source)
                .cloned();
            let state_changed = old.as_ref().is_none_or(|old| old.status != fact.status);
            if let (Some(old), Some(terminal)) = (&old, &fact.terminal)
                && old.terminal.as_deref() != Some(&terminal.id)
            {
                self.emit(fact, terminal.kind, &terminal.id, now);
            }
            if state_changed {
                let transition = format!("{}:{}", fact.revision, self.sequence.saturating_add(1));
                match fact.status.as_str() {
                    "blocked" => self.emit(fact, Kind::Attention, &transition, now),
                    "failed" | "error" => self.emit(fact, Kind::Failed, &transition, now),
                    "idle" | "done"
                        if !fact.lifecycle && old.as_ref().is_some_and(|old| old.working) =>
                    {
                        self.emit(fact, Kind::Completed, &transition, now)
                    }
                    _ => {}
                }
            }
            let questions = if fact.questions_known {
                fact.questions.clone()
            } else {
                old.as_ref()
                    .map(|old| old.questions.clone())
                    .unwrap_or_default()
            };
            for question in &questions {
                if question.len() <= 256
                    && old
                        .as_ref()
                        .is_none_or(|old| !old.questions.contains(question))
                {
                    self.emit(fact, Kind::Question, question, now)
                }
            }
            self.seen.insert(
                fact.session_id.clone(),
                Seen {
                    source: fact.source.clone(),
                    status: fact.status.clone(),
                    working: fact.status == "working",
                    questions,
                    terminal: fact
                        .terminal
                        .as_ref()
                        .map(|t| t.id.clone())
                        .or_else(|| old.as_ref().and_then(|old| old.terminal.clone())),
                },
            );
        }
        // Missing agents can be a transient list snapshot; retain their deduplication
        // checkpoints, evicting absent sessions only when capacity requires it.
        while self.seen.len() > 1024 {
            let remove = self
                .seen
                .keys()
                .find(|id| !facts.iter().any(|fact| &fact.session_id == *id))
                .cloned()
                .or_else(|| self.seen.keys().next().cloned());
            if let Some(id) = remove {
                self.seen.remove(&id);
            } else {
                break;
            }
        }
        old_sequence != self.sequence || self.seen != old_seen
    }
    pub fn page(&self, after: Option<&str>) -> Page {
        let prefix = format!("{}:", self.node_id);
        let requested = after
            .and_then(|value| value.strip_prefix(&prefix))
            .and_then(|value| value.parse::<u64>().ok());
        let reset = after.is_some() && requested.is_none_or(|sequence| sequence > self.sequence);
        let start = if reset || after.is_none() {
            self.sequence
        } else {
            requested.unwrap_or(self.sequence)
        };
        let events: Vec<_> = self
            .events
            .iter()
            .filter(|event| event.sequence > start)
            .take(64)
            .cloned()
            .collect();
        let cursor = events.last().map_or(self.sequence, |event| event.sequence);
        Page {
            node_id: self.node_id.clone(),
            cursor: format!("{}:{cursor}", self.node_id),
            reset: reset
                || self
                    .events
                    .front()
                    .is_some_and(|event| start < event.sequence.saturating_sub(1)),
            more: cursor < self.sequence,
            events,
        }
    }
    pub fn event(&self, id: &str) -> Option<Event> {
        self.events.iter().find(|event| event.id == id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fact(status: &str, revision: u64) -> Fact {
        Fact {
            session_id: "session".into(),
            title: "project".into(),
            source: "owner".into(),
            status: status.into(),
            revision,
            questions: Vec::new(),
            questions_known: true,
            lifecycle: false,
            terminal: None,
        }
    }
    #[test]
    fn idle_bootstrap_is_not_completion_but_work_to_idle_is() {
        let mut journal = Ledger::new("node".into());
        journal.observe(&[fact("idle", 0)], 1);
        assert!(journal.page(Some("node:0")).events.is_empty());
        journal.observe(&[fact("working", 1)], 2);
        journal.observe(&[fact("idle", 2)], 3);
        let page = journal.page(Some("node:0"));
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].kind, Kind::Completed);
        assert!(journal.page(None).events.is_empty());
    }
    #[test]
    fn persisted_state_deduplicates_reconnects_and_tracks_new_questions() {
        let mut journal = Ledger::new("node".into());
        let mut input = fact("working", 1);
        input.questions = vec!["question".into()];
        journal.observe(&[input.clone()], 1);
        let bytes = serde_json::to_vec(&journal).unwrap();
        let mut restored: Ledger = serde_json::from_slice(&bytes).unwrap();
        assert!(restored.valid("node"));
        assert!(!restored.observe(&[input.clone()], 2));
        input.questions.push("another".into());
        restored.observe(&[input], 3);
        assert_eq!(restored.page(Some("node:0")).events.len(), 2);
    }
    #[test]
    fn source_replacement_does_not_fabricate_completion_or_accept_another_nodes_cursor() {
        let mut journal = Ledger::new("node".into());
        journal.observe(&[fact("working", 1)], 1);
        let mut next = fact("idle", 0);
        next.source = "new-owner".into();
        journal.observe(&[next], 2);
        assert!(journal.page(Some("node:0")).events.is_empty());
        assert!(journal.page(Some("other:10")).reset);
    }
    #[test]
    fn short_codex_turns_use_source_completion_and_do_not_notify_old_bootstrap_or_polling_twice() {
        let mut journal = Ledger::new("node".into());
        let mut input = fact("idle", 0);
        input.lifecycle = true;
        input.terminal = Some(Terminal {
            id: "old-turn".into(),
            kind: Kind::Completed,
        });
        journal.observe(&[input.clone()], 1);
        assert!(journal.page(Some("node:0")).events.is_empty());
        input.terminal = Some(Terminal {
            id: "new-turn".into(),
            kind: Kind::Completed,
        });
        journal.observe(&[input.clone()], 2);
        journal.observe(&[input.clone()], 3);
        assert_eq!(journal.page(Some("node:0")).events.len(), 1);
        input.status = "working".into();
        journal.observe(&[input.clone()], 4);
        input.status = "idle".into();
        journal.observe(&[input.clone()], 5);
        assert_eq!(journal.page(Some("node:0")).events.len(), 1);
        input.terminal = Some(Terminal {
            id: "third-turn".into(),
            kind: Kind::Failed,
        });
        journal.observe(&[input], 6);
        assert_eq!(journal.page(Some("node:1")).events[0].kind, Kind::Failed);
    }
    #[test]
    fn missing_snapshot_does_not_repeat_attention_and_zero_revision_cycles_remain_distinct() {
        let mut journal = Ledger::new("node".into());
        journal.observe(&[fact("blocked", 0)], 1);
        journal.observe(&[], 2);
        journal.observe(&[fact("blocked", 0)], 3);
        assert_eq!(journal.page(Some("node:0")).events.len(), 1);
        for _ in 0..2 {
            journal.observe(&[fact("working", 0)], 4);
            journal.observe(&[fact("idle", 0)], 5);
        }
        assert_eq!(journal.page(Some("node:0")).events.len(), 3);
    }
    #[test]
    fn retained_pages_are_bounded_ordered_and_bad_pages_rejected() {
        let mut journal = Ledger::new("node".into());
        for i in 0..600 {
            journal.observe(&[fact("working", i * 2)], 1);
            journal.observe(&[fact("idle", i * 2 + 1)], 2);
        }
        let page = journal.page(Some("node:0"));
        assert!(page.reset);
        assert!(page.more);
        assert!(page.valid());
        assert_eq!(page.events.len(), 64);
        let mut wrong = page.clone();
        wrong.events.reverse();
        assert!(!wrong.valid());
        wrong = page;
        wrong.cursor = "other:600".into();
        assert!(!wrong.valid());
        assert!(journal.valid("node"));
    }
}
