use super::*;
use kite_herdr::api::AgentSession;
use std::collections::HashMap;

pub type ItemRef = Arc<Item>;
pub type SharedItems = Arc<Vec<ItemRef>>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryStatus {
    pub loading: bool,
    pub complete: bool,
    pub failed: bool,
    pub waiting: bool,
    pub details: String,
}

#[derive(Clone, Debug)]
pub struct ConversationEdit {
    pub revision: u64,
    pub at: usize,
    pub remove: usize,
    pub items: Vec<ItemRef>,
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
        self.context.clone()
    }
}

impl LiveSession {
    pub(super) fn replace_conversation(&mut self, messages: Vec<ItemRef>, available: bool) {
        let old = self.conversation.as_ref();
        let equal = |a: &ItemRef, b: &ItemRef| Arc::ptr_eq(a, b) || a == b;
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
            self.revision = self.revision.wrapping_add(1);
            let edit = ConversationEdit {
                revision: self.revision,
                at: head,
                remove: old.len() - head - tail,
                items: messages[head..messages.len() - tail].to_vec(),
            };
            let journal = Arc::make_mut(&mut self.journal);
            journal.push_back(edit);
            while journal.len() > 1
                && (journal.len() > 128
                    || journal
                        .iter()
                        .map(|e| e.items.iter().map(|m| m.payload_len()).sum::<usize>())
                        .sum::<usize>()
                        > 8 * 1024 * 1024)
            {
                journal.pop_front();
            }
            self.conversation = Arc::new(messages);
        }
        self.available = available;
    }
    fn sync_ready(&self, scope: &str) -> bool {
        self.sync_job.as_ref().is_none_or(JoinHandle::is_finished)
            && self.transcript.scope == scope
            && self.transcript.next().is_some()
            && (!self.transcript.failed || self.transcript.retry.ready(Instant::now()))
    }
    /// Apply one backfill page. `None` once this task no longer owns the transcript.
    fn ingest_page(
        &mut self,
        scope: &str,
        token: u64,
        request: &Request,
        page: &history::History,
    ) -> Option<(bool, bool)> {
        if self.transcript.scope != scope || self.transcript.generation != token {
            return None;
        }
        let pending: Vec<Item> = self
            .conversation
            .iter()
            .filter(|m| m.id.starts_with("local-"))
            .map(|m| m.as_ref().clone())
            .collect();
        let transcript = &mut self.transcript;
        let before = transcript
            .ranges
            .iter()
            .map(|r| (r.start, r.end))
            .collect::<Vec<_>>();
        if page.available {
            transcript.ingest(page, Some(request));
        }
        let progressed = before
            != transcript
                .ranges
                .iter()
                .map(|r| (r.start, r.end))
                .collect::<Vec<_>>();
        transcript.failed = !page.available || !progressed;
        if transcript.failed {
            transcript.problem = if page.problem.is_empty() {
                "history page made no progress".into()
            } else {
                page.problem.clone()
            };
            transcript.retry.failed(Instant::now());
        } else {
            transcript.retry.reset();
        }
        let failed = transcript.failed;
        let done = failed || transcript.next().is_none();
        transcript.syncing = !done;
        let messages = (done && !failed).then(|| transcript.presentation(Some(&pending)));
        self.history_status = self.transcript.status();
        if let Some(messages) = messages {
            self.publish_conversation(messages, true);
        }
        Some((done, failed))
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
    records: HashMap<String, ((u64, u32), ItemRef)>,
    ranges: Vec<Range>,
    visible: Vec<ItemRef>,
    failed: bool,
    syncing: bool,
    generation: u64,
    retry: ReadRetry,
    problem: String,
}

#[derive(Default)]
struct ReadRetry {
    attempts: u32,
    after: Option<Instant>,
}
impl ReadRetry {
    fn ready(&self, now: Instant) -> bool {
        self.after.is_none_or(|after| now >= after)
    }
    fn failed(&mut self, now: Instant) {
        self.attempts = (self.attempts + 1).min(5);
        self.after = Some(now + Duration::from_secs((1u64 << self.attempts).min(30)));
    }
    fn reset(&mut self) {
        *self = Self::default();
    }
}

impl Transcript {
    fn select(&mut self, scope: &str, cached: &[ItemRef]) {
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
            waiting: false,
            details: if self.failed {
                self.problem.clone()
            } else {
                String::new()
            },
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
                self.retry.reset();
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
        for (ordinal, item) in history.items.iter().enumerate() {
            let order = history
                .positions
                .get(&item.id)
                .copied()
                .unwrap_or((ordinal as u64, 0));
            if let Some((old_order, old)) = self.records.get_mut(&item.id) {
                *old_order = (*old_order).min(order);
                if old.as_ref() == item {
                    continue;
                }
                let merged = if matches!(request, Some(Request::Before(_))) {
                    history::merge_item(item, old)
                } else {
                    history::merge_item(old, item)
                };
                if **old != merged {
                    *old = Arc::new(merged);
                }
            } else {
                self.records
                    .insert(item.id.clone(), (order, Arc::new(item.clone())));
            }
        }
    }
    fn presentation(&mut self, latest: Option<&[Item]>) -> Vec<ItemRef> {
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
                    let merged = history::merge_item(&self.visible[i], incoming);
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
    /// A conversation's provider source: its live agent's, else its saved record's.
    fn conversation_reference(&self, key: &ConversationKey) -> Option<AgentSession> {
        let live = {
            let state = lock(&self.state);
            state
                .live
                .get(key)
                .and_then(|live| live.agent.as_ref())
                .and_then(|bound| {
                    state
                        .snapshot
                        .agents
                        .iter()
                        .find(|a| a.terminal_id == bound.terminal)
                })
                .and_then(|agent| agent.agent_session.clone())
        };
        live.or_else(|| {
            self.session_record(&key.host, &key.conversation)
                .and_then(|record| record.reference)
        })
    }
    pub(super) fn present_history(&self, key: &ConversationKey, value: &Arc<history::History>) {
        let reference = self.conversation_reference(key);
        if !value.available {
            if reference.is_some() {
                self.update_live(key, |live| {
                    live.history_status = HistoryStatus {
                        failed: !value.source_pending,
                        waiting: value.source_pending,
                        details: value.problem.clone(),
                        ..HistoryStatus::default()
                    };
                });
            }
            return;
        }
        if reference
            .as_ref()
            .is_some_and(|r| ConversationKey::history_key(r) != value.key)
        {
            return;
        }
        let scope = key.history_scope(reference.as_ref());
        let current = self.read_live(key, |live| {
            let unchanged = live
                .history
                .as_ref()
                .is_some_and(|old| Arc::ptr_eq(old, value))
                && live.transcript.scope == scope;
            let status = live.transcript.status();
            (unchanged, status != live.history_status, status)
        });
        match current {
            None => return,
            Some((true, false, _)) => {}
            Some((true, true, status)) => {
                self.update_live(key, |live| live.history_status = status);
            }
            Some((false, ..)) => {
                self.update_live(key, |live| {
                    let shown = live.conversation.clone();
                    live.transcript.select(&scope, &shown);
                    // An empty transient read never erases the established conversation.
                    if !value.items.is_empty() || shown.is_empty() {
                        live.transcript.ingest(value, None);
                    }
                    let present = live.transcript.presentation(Some(&value.items));
                    live.history_status = live.transcript.status();
                    live.history = Some(value.clone());
                    live.publish_conversation(present, true);
                });
            }
        }
        self.ensure_history_sync(key);
    }
    fn ensure_history_sync(&self, key: &ConversationKey) {
        let Some(reference) = self.conversation_reference(key) else {
            return;
        };
        if !providers::managed(&reference.agent) {
            return;
        }
        let scope = key.history_scope(Some(&reference));
        if self.read_live(key, |live| live.sync_ready(&scope)) != Some(true) {
            return;
        }
        let weak = self.self_weak.clone();
        self.update_live(key, |live| {
            if !live.sync_ready(&scope) {
                return;
            }
            live.transcript.failed = false;
            live.transcript.syncing = true;
            live.history_status = live.transcript.status();
            let token = live.transcript.generation;
            live.sync_job = Some(tokio::spawn(sync_history(
                weak,
                key.clone(),
                scope.clone(),
                token,
                reference,
            )));
        });
    }
    pub async fn retry_history(self: &Arc<Self>) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let Some(key) = core.selected() else {
                return Ok(());
            };
            core.update_live(&key, |live| {
                live.transcript.failed = false;
                live.transcript.retry.reset();
                live.history = None;
                live.history_status.failed = false;
            });
            core.ensure_history_sync(&key);
            Ok(())
        })
        .await
    }
}

/// One conversation's backfill; it runs on while another is on screen and is
/// aborted when its `LiveSession` is dropped.
async fn sync_history(
    weak: std::sync::Weak<AppCore>,
    key: ConversationKey,
    scope: String,
    token: u64,
    reference: AgentSession,
) {
    loop {
        let Some(core) = weak.upgrade() else { return };
        let Ok(connection) = core.connection() else {
            return;
        };
        let generation = core.generation.load(Ordering::SeqCst);
        let request = core
            .read_live(&key, |live| {
                let transcript = &live.transcript;
                if transcript.scope != scope || transcript.generation != token {
                    return None;
                }
                transcript.next()
            })
            .flatten();
        let Some(request) = request else { return };
        let page = history::page(
            &connection.ssh,
            &reference,
            request.cursor(),
            request.newer(),
        )
        .await;
        if core.generation.load(Ordering::SeqCst) != generation {
            return;
        }
        let progress = core
            .update_live(&key, |live| {
                live.ingest_page(&scope, token, &request, &page)
            })
            .flatten();
        let Some((done, failed)) = progress else {
            return;
        };
        if done {
            if !failed {
                let _ = core.record_history(&connection, &key).await;
                let _ = core.flush_history_caches(true);
            }
            return;
        }
        tokio::task::yield_now().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_read_retry_is_bounded_and_manual_retry_can_reset_it() {
        let now = Instant::now();
        let mut retry = ReadRetry::default();
        for delay in [2, 4, 8, 16, 30, 30] {
            retry.failed(now);
            assert!(!retry.ready(now + Duration::from_secs(delay - 1)));
            assert!(retry.ready(now + Duration::from_secs(delay)));
        }
        retry.reset();
        assert!(retry.ready(now));
    }

    #[test]
    fn pending_source_and_transport_failure_keep_the_existing_conversation() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        let mut record = kite_agent::sessions::Record::draft(
            "session".into(),
            "codex".into(),
            String::new(),
            None,
            0,
        );
        record.reference = Some(AgentSession {
            source: "herdr:codex".into(),
            agent: "codex".into(),
            kind: "id".into(),
            value: "thread".into(),
        });
        core.install_catalog(
            "host",
            &kite_agent::sessions::Catalog {
                revision: 1,
                now_ms: 0,
                sessions: vec![record],
            },
        )
        .unwrap();
        let key = ConversationKey::new("host", "session");
        select(&core, &key);
        fill(&core, &key, vec![message(0)]);
        let before = core.snapshot().conversation;
        core.present_history(
            &key,
            &Arc::new(history::History {
                source_pending: true,
                ..Default::default()
            }),
        );
        let pending = core.snapshot();
        assert!(pending.history_status.waiting);
        assert!(!pending.history_status.failed);
        assert!(Arc::ptr_eq(&before, &pending.conversation));
        core.present_history(
            &key,
            &Arc::new(history::History {
                problem: "history request timed out".into(),
                ..Default::default()
            }),
        );
        let failed = core.snapshot();
        assert!(failed.history_status.failed);
        assert!(!failed.history_status.waiting);
        assert_eq!(failed.history_status.details, "history request timed out");
        assert!(Arc::ptr_eq(&before, &failed.conversation));
    }
    fn select(core: &AppCore, key: &ConversationKey) {
        core.select(Some(key.clone()), |s| {
            s.selected_host = Some(key.host.clone());
        });
    }
    fn fill(core: &AppCore, key: &ConversationKey, messages: Vec<Item>) {
        core.update_live(key, |live| {
            live.replace_conversation(messages.into_iter().map(Arc::new).collect(), true);
        });
    }
    fn message(i: usize) -> Item {
        Item::new(
            format!("message-{i}"),
            Role::Assistant,
            vec![Block::Text(format!("record {i}"))],
        )
    }
    fn page(start: usize, end: usize, total: usize) -> history::History {
        let cursor = |p: usize| format!("1:2:{p}:{total}");
        history::History {
            key: "source".into(),
            stamp: total.to_string(),
            available: true,
            items: (start..end).map(message).collect(),
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
        assert!(transcript.presentation(Some(&latest.items)).is_empty());
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
        let all = transcript.presentation(Some(&new.items));
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
        transcript.presentation(Some(&first.items));
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
        let key = ConversationKey::new("host", "thread");
        select(&core, &key);
        fill(&core, &key, (0..10000).map(message).collect());
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
        core.update_live(&key, |live| {
            let mut records = live.conversation.as_ref().clone();
            let mut tail = message(9999);
            tail.blocks = vec![Block::Text("streamed tail changed".into())];
            records[9999] = Arc::new(tail);
            live.replace_conversation(records, true);
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
        let (one, two) = (
            ConversationKey::new("host", "one"),
            ConversationKey::new("host", "two"),
        );
        select(&core, &one);
        fill(&core, &one, vec![message(1)]);
        let before = core.snapshot();
        select(&core, &two);
        fill(&core, &two, vec![message(2)]);
        let delta =
            core.conversation_since(&before.conversation_context(), before.conversation_revision);
        assert!(delta.reset);
        assert_eq!(delta.edits[0].items[0].id, "message-2");
    }
}
