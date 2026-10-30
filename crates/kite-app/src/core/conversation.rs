use super::*;
use kite_herdr::api::AgentSession;
use std::collections::HashMap;

pub type MessageRef = Arc<Message>;
pub type SharedMessages = Arc<Vec<MessageRef>>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryStatus {
    pub loading: bool,
    pub complete: bool,
    pub failed: bool,
}

#[derive(Clone, Debug)]
pub struct ConversationEdit {
    pub revision: u64,
    pub at: usize,
    pub remove: usize,
    pub items: Vec<MessageRef>,
}
#[derive(Clone, Debug)]
pub struct ConversationDelta {
    pub context: String,
    pub revision: u64,
    pub reset: bool,
    pub edits: Vec<ConversationEdit>,
}

impl Snapshot {
    pub fn conversation_context(&self) -> String {
        format!(
            "{}/{}",
            self.selected_host.as_deref().unwrap_or_default(),
            self.selected_session
                .as_ref()
                .or(self.selected_agent.as_ref())
                .map(String::as_str)
                .unwrap_or_default()
        )
    }
    pub(super) fn replace_shared_conversation(
        &mut self,
        messages: Vec<MessageRef>,
        available: bool,
    ) {
        let old = self.conversation.as_ref();
        let equal = |a: &MessageRef, b: &MessageRef| Arc::ptr_eq(a, b) || a == b;
        let mut head = 0;
        while head < old.len().min(messages.len()) && equal(&old[head], &messages[head]) {
            head += 1;
        }
        if head != old.len() || head != messages.len() {
            let mut tail = 0;
            while tail < (old.len() - head).min(messages.len() - head)
                && equal(
                    &old[old.len() - 1 - tail],
                    &messages[messages.len() - 1 - tail],
                )
            {
                tail += 1;
            }
            self.conversation_revision = self.conversation_revision.wrapping_add(1);
            let edit = ConversationEdit {
                revision: self.conversation_revision,
                at: head,
                remove: old.len() - head - tail,
                items: messages[head..messages.len() - tail].to_vec(),
            };
            let journal = Arc::make_mut(&mut self.conversation_journal);
            journal.push_back(edit);
            while journal.len() > 1
                && (journal.len() > 128
                    || journal
                        .iter()
                        .map(|e| {
                            e.items
                                .iter()
                                .map(|m| {
                                    m.text.len()
                                        + m.tool
                                            .as_ref()
                                            .map(|t| t.arguments.len() + t.output.len())
                                            .unwrap_or(0)
                                })
                                .sum::<usize>()
                        })
                        .sum::<usize>()
                        > 8 * 1024 * 1024)
            {
                journal.pop_front();
            }
            self.conversation = Arc::new(messages);
        }
        self.conversation_available = available;
    }
}

#[derive(Clone)]
struct Range {
    start: u64,
    end: u64,
    start_cursor: String,
    end_cursor: String,
}
#[derive(Clone)]
enum Request {
    Before(String),
    After(String),
}
impl Request {
    fn cursor(&self) -> &str {
        match self {
            Self::Before(s) | Self::After(s) => s,
        }
    }
    fn newer(&self) -> bool {
        matches!(self, Self::After(_))
    }
}
fn position(cursor: &str) -> Option<u64> {
    cursor.split(':').nth(2)?.parse().ok()
}
fn identity(cursor: &str) -> String {
    cursor.split(':').take(2).collect::<Vec<_>>().join(":")
}
fn observed_size(cursor: &str) -> u64 {
    cursor
        .split(':')
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

#[derive(Default)]
pub(super) struct Transcript {
    scope: String,
    file: String,
    size: u64,
    records: HashMap<String, ((u64, u32), MessageRef)>,
    ranges: Vec<Range>,
    visible: Vec<MessageRef>,
    failed: bool,
    syncing: bool,
    generation: u64,
}

impl Transcript {
    fn select(&mut self, scope: &str, cached: &[MessageRef]) {
        if self.scope != scope {
            *self = Self {
                scope: scope.into(),
                visible: cached.to_vec(),
                generation: self.generation.wrapping_add(1),
                ..Self::default()
            };
        }
    }
    fn next(&self) -> Option<Request> {
        for pair in self.ranges.windows(2) {
            if pair[0].end < pair[1].start {
                return Some(Request::After(pair[0].end_cursor.clone()));
            }
        }
        self.ranges
            .first()
            .filter(|r| r.start > 0)
            .map(|r| Request::Before(r.start_cursor.clone()))
    }
    fn status(&self) -> HistoryStatus {
        HistoryStatus {
            loading: self.syncing,
            complete: !self.ranges.is_empty() && self.next().is_none(),
            failed: self.failed,
        }
    }
    fn cover(&mut self, mut range: Range) {
        let mut result = Vec::new();
        for old in self.ranges.drain(..) {
            if old.end < range.start || range.end < old.start {
                result.push(old);
                continue;
            }
            if old.start < range.start {
                range.start = old.start;
                range.start_cursor = old.start_cursor;
            }
            if old.end > range.end {
                range.end = old.end;
                range.end_cursor = old.end_cursor;
            }
        }
        result.push(range);
        result.sort_by_key(|r| r.start);
        self.ranges = result;
    }
    fn ingest(&mut self, history: &history::History, request: Option<&Request>) {
        if !history.available {
            return;
        }
        if let Some(info) = &history.page {
            let file = identity(&info.start);
            let size = observed_size(&info.end);
            if self.file != file || request.is_none() && size < self.size {
                self.records.clear();
                self.ranges.clear();
                self.file = file;
                self.failed = false;
                self.generation = self.generation.wrapping_add(1);
                self.syncing = false;
                self.size = 0;
            }
            if size > self.size {
                self.failed = false;
            }
            self.size = self.size.max(size);
            let mut range = Range {
                start: position(&info.start).unwrap_or(0),
                end: position(&info.end).unwrap_or(0),
                start_cursor: info.start.clone(),
                end_cursor: info.end.clone(),
            };
            match request {
                Some(Request::Before(cursor)) => {
                    range.end = position(cursor).unwrap_or(range.end);
                    range.end_cursor = cursor.clone();
                }
                Some(Request::After(cursor)) => {
                    range.start = position(cursor).unwrap_or(range.start);
                    range.start_cursor = cursor.clone();
                }
                None => {}
            }
            self.cover(range);
        }
        for (ordinal, message) in history.messages.iter().enumerate() {
            let order = history
                .positions
                .get(&message.id)
                .copied()
                .unwrap_or((ordinal as u64, 0));
            if let Some((old_order, old)) = self.records.get_mut(&message.id) {
                *old_order = (*old_order).min(order);
                if old.as_ref() == message {
                    continue;
                }
                let merged = if matches!(request, Some(Request::Before(_))) {
                    history::merge_message(message, old)
                } else {
                    history::merge_message(old, message)
                };
                if **old != merged {
                    *old = Arc::new(merged);
                }
            } else {
                self.records
                    .insert(message.id.clone(), (order, Arc::new(message.clone())));
            }
        }
    }
    fn presentation(&mut self, latest: Option<&[Message]>) -> Vec<MessageRef> {
        if self.status().complete {
            let mut ordered: Vec<_> = self.records.values().collect();
            ordered.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
            // Retain earlier cached facts if a provider rewrites/compacts its log.
            let mut result: Vec<_> = self
                .visible
                .iter()
                .filter(|m| !self.records.contains_key(&m.id) && !m.id.starts_with("local-"))
                .cloned()
                .collect();
            result.extend(ordered.into_iter().map(|(_, m)| m.clone()));
            if let Some(latest) = latest {
                result.extend(
                    latest
                        .iter()
                        .filter(|m| m.id.starts_with("local-"))
                        .cloned()
                        .map(Arc::new),
                );
            } else {
                result.extend(
                    self.visible
                        .iter()
                        .filter(|m| m.id.starts_with("local-"))
                        .cloned(),
                );
            }
            self.visible = result;
        } else if let Some(latest) = latest {
            if self.visible.is_empty() {
                return Vec::new();
            }
            // Initial synchronization never publishes partially ordered backfill.
            // Existing complete cache remains readable while missing data arrives.
            let pending: BTreeSet<_> = latest
                .iter()
                .filter(|m| m.id.starts_with("local-"))
                .map(|m| m.id.as_str())
                .collect();
            self.visible
                .retain(|m| !m.id.starts_with("local-") || pending.contains(m.id.as_str()));
            let mut index: HashMap<_, _> = self
                .visible
                .iter()
                .enumerate()
                .map(|(i, m)| (m.id.clone(), i))
                .collect();
            for incoming in latest {
                if let Some(&i) = index.get(&incoming.id) {
                    let merged = history::merge_message(&self.visible[i], incoming);
                    if *self.visible[i] != merged {
                        self.visible[i] = Arc::new(merged);
                    }
                } else {
                    index.insert(incoming.id.clone(), self.visible.len());
                    self.visible.push(Arc::new(incoming.clone()));
                }
            }
        }
        self.visible.clone()
    }
}

impl AppCore {
    pub fn conversation_since(&self, context: &str, revision: u64) -> ConversationDelta {
        let snapshot = self.snapshot();
        let current = snapshot.conversation_context();
        let reset = context != current
            || revision > snapshot.conversation_revision
            || (revision < snapshot.conversation_revision
                && snapshot
                    .conversation_journal
                    .front()
                    .is_none_or(|p| p.revision > revision + 1));
        let edits = if reset {
            vec![ConversationEdit {
                revision: snapshot.conversation_revision,
                at: 0,
                remove: 0,
                items: snapshot.conversation.as_ref().clone(),
            }]
        } else {
            snapshot
                .conversation_journal
                .iter()
                .filter(|e| e.revision > revision)
                .cloned()
                .collect()
        };
        ConversationDelta {
            context: current,
            revision: snapshot.conversation_revision,
            reset,
            edits,
        }
    }
    pub(super) fn reset_history_view(&self) {
        if let Some(job) = lock(&self.history_sync_job).take() {
            job.abort();
        }
        let mut transcript = lock(&self.history_window);
        *transcript = Transcript {
            generation: transcript.generation.wrapping_add(1),
            ..Transcript::default()
        };
    }
    fn history_context(&self, snapshot: &Snapshot) -> (String, Option<AgentSession>) {
        let host = snapshot.selected_host.as_deref().unwrap_or_default();
        let reference = snapshot
            .selected_agent
            .as_ref()
            .and_then(|id| snapshot.agents.iter().find(|a| &a.terminal_id == id))
            .and_then(|a| a.agent_session.clone())
            .or_else(|| {
                snapshot
                    .selected_session
                    .as_ref()
                    .and_then(|id| self.session_record(host, id))
                    .and_then(|r| r.reference)
            });
        let key = reference
            .as_ref()
            .map(history::key)
            .or_else(|| snapshot.selected_session.clone())
            .or_else(|| snapshot.selected_agent.clone())
            .unwrap_or_default();
        (format!("{host}/{key}"), reference)
    }
    pub(super) fn present_history(&self, value: &Arc<history::History>, messages: Vec<Message>) {
        let snapshot = self.snapshot();
        let (scope, reference) = self.history_context(&snapshot);
        if !value.available {
            if reference.is_some() {
                self.update(|s| {
                    if s.selected_host == snapshot.selected_host
                        && s.selected_session == snapshot.selected_session
                    {
                        s.history_status.failed = true;
                        s.history_status.loading = false;
                        s.history_status.complete = false;
                    }
                });
            }
            return;
        }
        if reference
            .as_ref()
            .is_some_and(|r| history::key(r) != value.key)
        {
            return;
        }
        let unchanged = {
            lock(&self.history)
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, value))
        };
        let selected = { lock(&self.history_window).scope == scope };
        if unchanged && selected {
            let status = lock(&self.history_window).status();
            if snapshot.history_status != status {
                self.update(|s| s.history_status = status);
            }
            self.ensure_history_sync();
            return;
        }
        let mut transcript = lock(&self.history_window);
        transcript.select(&scope, &snapshot.conversation);
        // An empty transient read never erases the established conversation.
        if !value.messages.is_empty() || snapshot.conversation.is_empty() {
            transcript.ingest(value, None);
        }
        let present = transcript.presentation(Some(&messages));
        let status = transcript.status();
        drop(transcript);
        *lock(&self.history) = Some(value.clone());
        self.update(|s| {
            if s.selected_host == snapshot.selected_host
                && s.selected_session == snapshot.selected_session
                && s.selected_agent == snapshot.selected_agent
            {
                s.replace_shared_conversation(present, true);
                s.history_status = status;
            }
        });
        self.ensure_history_sync();
    }
    fn ensure_history_sync(&self) {
        let (scope, reference) = self.history_context(&self.snapshot());
        let Some(reference) = reference else { return };
        if reference.agent != "codex" {
            return;
        }
        let mut job = lock(&self.history_sync_job);
        if job.as_ref().is_some_and(|j| !j.is_finished()) {
            return;
        }
        let mut transcript = lock(&self.history_window);
        if transcript.scope != scope || transcript.failed || transcript.next().is_none() {
            return;
        }
        transcript.syncing = true;
        let token = transcript.generation;
        let status = transcript.status();
        drop(transcript);
        let weak = self.self_weak.clone();
        self.update(|s| s.history_status = status);
        *job = Some(tokio::spawn(async move {
            loop {
                let Some(core) = weak.upgrade() else { return };
                let snapshot = core.snapshot();
                if core.history_context(&snapshot).0 != scope {
                    return;
                }
                let connection = match core.connection() {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let generation = core.generation.load(Ordering::SeqCst);
                let request = {
                    let transcript = lock(&core.history_window);
                    if transcript.scope != scope || transcript.generation != token {
                        return;
                    }
                    transcript.next()
                };
                let Some(request) = request else { return };
                let page = history::page(
                    &connection.ssh,
                    &reference,
                    request.cursor(),
                    request.newer(),
                )
                .await;
                if core.generation.load(Ordering::SeqCst) != generation
                    || core.history_context(&core.snapshot()).0 != scope
                {
                    return;
                }
                let pending: Vec<Message> = core
                    .snapshot()
                    .conversation
                    .iter()
                    .filter(|m| m.id.starts_with("local-"))
                    .map(|m| m.as_ref().clone())
                    .collect();
                let (done, messages, status) = {
                    let mut transcript = lock(&core.history_window);
                    if transcript.scope != scope || transcript.generation != token {
                        return;
                    }
                    let before = transcript
                        .ranges
                        .iter()
                        .map(|r| (r.start, r.end))
                        .collect::<Vec<_>>();
                    if page.available {
                        transcript.ingest(&page, Some(&request));
                    }
                    let progressed = before
                        != transcript
                            .ranges
                            .iter()
                            .map(|r| (r.start, r.end))
                            .collect::<Vec<_>>();
                    transcript.failed = !page.available || !progressed;
                    let done = transcript.failed || transcript.next().is_none();
                    transcript.syncing = !done;
                    let messages = if done && !transcript.failed {
                        Some(transcript.presentation(Some(&pending)))
                    } else {
                        None
                    };
                    let status = transcript.status();
                    (done, messages, status)
                };
                core.update(|s| {
                    if s.conversation_context() != snapshot.conversation_context()
                        || core.generation.load(Ordering::SeqCst) != generation
                    {
                        return;
                    }
                    if let Some(messages) = messages {
                        s.replace_shared_conversation(messages, true);
                    }
                    s.history_status = status;
                });
                if done {
                    if !core.snapshot().history_status.failed {
                        let _ = core.record_selected_history(&connection).await;
                        let _ = core.flush_history_caches(true);
                    }
                    return;
                }
                tokio::task::yield_now().await;
            }
        }));
    }
    pub async fn retry_history(self: &Arc<Self>) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            lock(&core.history_window).failed = false;
            *lock(&core.history) = None;
            core.update(|s| s.history_status.failed = false);
            core.ensure_history_sync();
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(i: usize) -> Message {
        Message {
            id: format!("message-{i}"),
            role: Role::Assistant,
            text: format!("record {i}"),
            intermediate: false,
            tool: None,
        }
    }
    fn page(start: usize, end: usize, total: usize) -> history::History {
        let cursor = |p: usize| format!("1:2:{p}:{total}");
        history::History {
            key: "source".into(),
            stamp: total.to_string(),
            available: true,
            messages: (start..end).map(message).collect(),
            positions: (start..end)
                .map(|i| (format!("message-{i}"), (i as u64, 0)))
                .collect(),
            page: Some(history::Page {
                start: cursor(start),
                end: cursor(end),
                has_older: start > 0,
                has_newer: end < total,
            }),
            ..Default::default()
        }
    }
    #[test]
    fn ten_thousand_records_survive_full_sync_following_and_tail_updates() {
        let mut transcript = Transcript::default();
        transcript.select("host/thread", &[]);
        let latest = page(9000, 10000, 10000);
        transcript.ingest(&latest, None);
        assert!(transcript.presentation(Some(&latest.messages)).is_empty());
        for start in (0..9000).step_by(1000).rev() {
            let request = transcript.next().unwrap();
            assert!(matches!(request, Request::Before(_)));
            transcript.ingest(&page(start, start + 1000, 10000), Some(&request));
        }
        assert!(transcript.status().complete);
        let all = transcript.presentation(None);
        assert_eq!(all.len(), 10000);
        assert_eq!(all[0].id, "message-0");
        let original = all[5000].clone();
        let new = page(9500, 10010, 10010);
        transcript.ingest(&new, None);
        let all = transcript.presentation(Some(&new.messages));
        assert_eq!(all.len(), 10010);
        assert!(Arc::ptr_eq(&original, &all[5000]));
        assert_eq!(all[10009].id, "message-10009");
    }
    #[test]
    fn missing_forward_ranges_are_filled_before_reporting_complete() {
        let mut transcript = Transcript::default();
        transcript.select("host/thread", &[]);
        let first = page(0, 10, 10);
        transcript.ingest(&first, None);
        transcript.presentation(Some(&first.messages));
        let latest = page(90, 100, 100);
        transcript.ingest(&latest, None);
        assert!(!transcript.status().complete);
        let request = transcript.next().unwrap();
        assert!(request.newer());
        assert_eq!(position(request.cursor()), Some(10));
        transcript.ingest(&page(10, 90, 100), Some(&request));
        let all = transcript.presentation(None);
        assert!(transcript.status().complete);
        assert_eq!(
            all.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
            (0..100).map(|i| format!("message-{i}")).collect::<Vec<_>>()
        );
    }
    #[test]
    fn stale_backfill_size_is_not_mistaken_for_truncation_after_an_append() {
        let mut transcript = Transcript::default();
        transcript.select("host/thread", &[]);
        transcript.ingest(&page(900, 1000, 1000), None);
        let request = transcript.next().unwrap();
        let token = transcript.generation;
        transcript.ingest(&page(900, 1010, 1010), None);
        transcript.ingest(&page(800, 900, 1000), Some(&request));
        assert_eq!(transcript.size, 1010);
        assert_eq!(transcript.generation, token);
        assert_eq!(transcript.records.len(), 210);
    }
    #[test]
    fn a_failed_or_empty_read_preserves_cache_and_pending_prompts() {
        let mut transcript = Transcript::default();
        let mut pending = message(42);
        pending.id = "local-pending".into();
        let cached = vec![Arc::new(message(0)), Arc::new(pending)];
        transcript.select("host/thread", &cached);
        transcript.ingest(&history::History::default(), None);
        assert_eq!(transcript.presentation(None), cached);
        let data = page(0, 2, 2);
        transcript.ingest(&data, None);
        let all = transcript.presentation(None);
        assert_eq!(all.len(), 3);
        assert_eq!(all.last().unwrap().id, "local-pending");
    }
    struct Secrets;
    impl SecretStorage for Secrets {
        fn read(&self, _: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(None)
        }
        fn write(&self, _: &str, _: &[u8]) -> Result<(), String> {
            Ok(())
        }
    }
    struct Silent;
    impl Observer for Silent {
        fn changed(&self, _: Snapshot) {}
    }
    #[test]
    fn telemetry_sends_no_messages_and_a_tail_edit_sends_one_record() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        core.update(|s| {
            s.selected_host = Some("host".into());
            s.selected_session = Some("thread".into());
            s.replace_conversation((0..10000).map(message).collect(), true);
        });
        let before = core.snapshot();
        for _ in 0..100 {
            core.update(|s| s.desktop.received += 1);
        }
        let after = core.snapshot();
        assert!(Arc::ptr_eq(&before.conversation, &after.conversation));
        assert_eq!(before.conversation_revision, after.conversation_revision);
        let no_change =
            core.conversation_since(&before.conversation_context(), before.conversation_revision);
        assert!(!no_change.reset && no_change.edits.is_empty());
        core.update(|s| {
            let mut records = s.conversation.as_ref().clone();
            let mut tail = message(9999);
            tail.text = "streamed tail changed".into();
            records[9999] = Arc::new(tail);
            s.replace_shared_conversation(records, true);
        });
        let changed =
            core.conversation_since(&before.conversation_context(), before.conversation_revision);
        assert!(!changed.reset);
        assert_eq!(changed.edits.len(), 1);
        assert_eq!(changed.edits[0].at, 9999);
        assert_eq!(changed.edits[0].remove, 1);
        assert_eq!(changed.edits[0].items.len(), 1);
    }
    #[test]
    fn context_changes_force_a_complete_reset_instead_of_replaying_another_session() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        core.update(|s| {
            s.selected_session = Some("one".into());
            s.replace_conversation(vec![message(1)], true);
        });
        let before = core.snapshot();
        core.update(|s| {
            s.selected_session = Some("two".into());
            s.replace_conversation(vec![message(2)], true);
        });
        let delta =
            core.conversation_since(&before.conversation_context(), before.conversation_revision);
        assert!(delta.reset);
        assert_eq!(delta.edits[0].items[0].id, "message-2");
    }
}
