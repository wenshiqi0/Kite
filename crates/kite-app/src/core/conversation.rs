use super::*;
use kite_herdr::api::AgentSession;
use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryStatus {
    pub has_older: bool,
    pub has_newer: bool,
    pub following_latest: bool,
    pub loading: bool,
    pub failed: bool,
}
impl Default for HistoryStatus {
    fn default() -> Self {
        Self {
            has_older: false,
            has_newer: false,
            following_latest: true,
            loading: false,
            failed: false,
        }
    }
}
#[derive(Clone)]
struct Page {
    info: history::Page,
    messages: Vec<Message>,
}
pub(super) struct Window {
    scope: String,
    pages: VecDeque<Page>,
    following: bool,
    loading: bool,
    failed: bool,
    request_generation: u64,
}
impl Default for Window {
    fn default() -> Self {
        Self {
            scope: String::new(),
            pages: VecDeque::new(),
            following: true,
            loading: false,
            failed: false,
            request_generation: 0,
        }
    }
}
impl Window {
    fn select(&mut self, scope: &str) {
        if self.scope != scope {
            *self = Self {
                scope: scope.into(),
                request_generation: self.request_generation.wrapping_add(1),
                ..Self::default()
            };
        }
    }
    fn status(&self) -> HistoryStatus {
        HistoryStatus {
            has_older: self.pages.front().is_some_and(|p| p.info.has_older),
            has_newer: self.pages.back().is_some_and(|p| p.info.has_newer),
            following_latest: self.following,
            loading: self.loading,
            failed: self.failed,
        }
    }
    fn messages(&self) -> Vec<Message> {
        self.pages.iter().fold(Vec::new(), |messages, page| {
            history::merge(&messages, &page.messages)
        })
    }
    fn latest(&mut self, value: &history::History) {
        self.pages.clear();
        self.pages.push_back(Page {
            info: value.page.clone().unwrap_or_default(),
            messages: value.messages.clone(),
        });
        self.failed = false;
    }
    fn add(&mut self, value: &history::History, newer: bool) {
        if value.messages.is_empty() {
            if newer {
                if let Some(p) = self.pages.back_mut() {
                    p.info.has_newer = false;
                }
            } else if let Some(p) = self.pages.front_mut() {
                p.info.has_older = false;
            }
            return;
        }
        let page = Page {
            info: value.page.clone().unwrap_or_default(),
            messages: value.messages.clone(),
        };
        if newer {
            self.pages.push_back(page);
        } else {
            self.pages.push_front(page);
        }
        // Retain whole pages: edge cursors must not skip records evicted from view.
        while self.pages.len() > 1 {
            let messages = self.messages();
            let bytes: usize = messages
                .iter()
                .map(|m| {
                    m.text.len()
                        + m.tool
                            .as_ref()
                            .map(|t| t.arguments.len() + t.output.len())
                            .unwrap_or(0)
                })
                .sum();
            if messages.len() <= 300 && bytes <= 256 * 1024 {
                break;
            }
            if newer {
                self.pages.pop_front();
                if let Some(p) = self.pages.front_mut() {
                    p.info.has_older = true;
                }
            } else {
                self.pages.pop_back();
                if let Some(p) = self.pages.back_mut() {
                    p.info.has_newer = true;
                }
            }
        }
    }
}

impl AppCore {
    pub(super) fn reset_history_view(&self) {
        let mut window = lock(&self.history_window);
        *window = Window {
            request_generation: window.request_generation.wrapping_add(1),
            ..Window::default()
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
    pub(super) fn present_history(&self, value: &history::History, messages: Vec<Message>) {
        let snapshot = self.snapshot();
        let (scope, reference) = self.history_context(&snapshot);
        if value.available
            && reference
                .as_ref()
                .is_some_and(|r| history::key(r) != value.key)
        {
            return;
        }
        let mut window = lock(&self.history_window);
        window.select(&scope);
        let changed = lock(&self.history)
            .as_ref()
            .is_none_or(|old| old.messages != value.messages);
        let present = window.following
            && value.available
            && (!messages.is_empty() || snapshot.conversation.is_empty());
        if value.available {
            if present {
                window.latest(value);
            } else if !window.following
                && changed
                && let Some(p) = window.pages.back_mut()
            {
                p.info.has_newer = true;
            }
            *lock(&self.history) = Some(value.clone());
        }
        let status = window.status();
        drop(window);
        self.update(|s| {
            if s.selected_host != snapshot.selected_host
                || s.selected_session != snapshot.selected_session
                || s.selected_agent != snapshot.selected_agent
            {
                return;
            }
            if present {
                s.replace_conversation(messages, true);
            }
            s.history_status = status;
        });
    }
    pub(super) fn return_to_latest(&self) {
        let snapshot = self.snapshot();
        let (scope, reference) = self.history_context(&snapshot);
        let value = lock(&self.history).clone().filter(|h| {
            h.available && reference.as_ref().is_some_and(|r| history::key(r) == h.key)
        });
        let mut window = lock(&self.history_window);
        window.select(&scope);
        window.following = true;
        window.loading = false;
        window.failed = false;
        window.request_generation = window.request_generation.wrapping_add(1);
        if let Some(v) = value.as_ref().filter(|h| !h.messages.is_empty()) {
            window.latest(v);
        }
        let status = window.status();
        drop(window);
        self.update(|s| {
            if let Some(v) = value.filter(|h| !h.messages.is_empty()) {
                s.replace_conversation(v.messages, true);
            }
            s.history_status = status;
        });
    }
    pub async fn set_history_following(self: &Arc<Self>, following: bool) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            if following {
                core.return_to_latest();
            } else {
                let (scope, _) = core.history_context(&core.snapshot());
                let mut window = lock(&core.history_window);
                window.select(&scope);
                window.following = false;
                let status = window.status();
                drop(window);
                core.update(|s| s.history_status = status);
            }
            Ok(())
        })
        .await
    }
    pub async fn load_history_page(self: &Arc<Self>, newer: bool) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            let generation = core.generation.load(Ordering::SeqCst);
            let (scope, reference) = core.history_context(&core.snapshot());
            let reference =
                reference.ok_or_else(|| AppError::State("history source unavailable".into()))?;
            let (cursor, token) = {
                let mut window = lock(&core.history_window);
                window.select(&scope);
                if window.loading {
                    return Ok(());
                }
                let edge = if newer {
                    window.pages.back()
                } else {
                    window.pages.front()
                };
                let Some(edge) = edge.filter(|p| {
                    if newer {
                        p.info.has_newer
                    } else {
                        p.info.has_older
                    }
                }) else {
                    return Ok(());
                };
                let cursor = if newer {
                    edge.info.end.clone()
                } else {
                    edge.info.start.clone()
                };
                window.following = false;
                window.loading = true;
                window.failed = false;
                window.request_generation = window.request_generation.wrapping_add(1);
                let token = window.request_generation;
                let status = window.status();
                drop(window);
                core.update(|s| s.history_status = status);
                (cursor, token)
            };
            drop(_guard);
            let page = history::page(&c.ssh, &reference, &cursor, newer).await;
            if core.history_context(&core.snapshot()).0 != scope {
                return Ok(());
            }
            let mut window = lock(&core.history_window);
            if window.request_generation != token {
                return Ok(());
            }
            let available = page.available && core.generation.load(Ordering::SeqCst) == generation;
            window.loading = false;
            window.failed = !available;
            if available {
                window.add(&page, newer);
            }
            let messages = window.messages();
            let status = window.status();
            drop(window);
            core.update(|s| {
                if available && !messages.is_empty() {
                    s.replace_conversation(messages, true);
                }
                s.history_status = status;
            });
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn page(start: usize) -> history::History {
        history::History {
            messages: (start..start + 80)
                .map(|i| Message {
                    id: i.to_string(),
                    role: Role::Assistant,
                    text: format!("message {i}"),
                    intermediate: false,
                    tool: None,
                })
                .collect(),
            page: Some(history::Page {
                start: start.to_string(),
                end: (start + 80).to_string(),
                has_older: start > 0,
                has_newer: true,
            }),
            available: true,
            ..Default::default()
        }
    }
    #[test]
    fn bounded_window_keeps_navigation_edges_when_evicting_whole_pages() {
        let mut w = Window::default();
        w.select("host/thread");
        w.latest(&page(320));
        w.following = false;
        for start in [240, 160, 80, 0] {
            w.add(&page(start), false);
        }
        assert_eq!(w.messages().first().unwrap().id, "0");
        assert!(w.messages().len() <= 300 && w.status().has_newer);
        assert_eq!(w.pages.back().unwrap().info.end, "240");
        for start in [240, 320] {
            w.add(&page(start), true);
        }
        assert_eq!(w.messages().last().unwrap().id, "399");
        assert!(w.messages().len() <= 300 && w.status().has_older);
    }
    #[test]
    fn another_host_or_thread_does_not_reuse_the_previous_page() {
        let mut w = Window::default();
        w.select("host/a");
        w.latest(&page(0));
        w.following = false;
        w.select("host/b");
        assert!(w.pages.is_empty() && w.status().following_latest);
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
    fn failed_or_empty_refresh_preserves_visible_history_and_content_revision() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        core.update(|s| {
            s.selected_host = Some("host".into());
            s.selected_session = Some("thread".into());
        });
        let value = page(0);
        core.present_history(&value, value.messages.clone());
        let before = core.snapshot();
        core.present_history(&history::History::default(), Vec::new());
        assert_eq!(core.snapshot().conversation, before.conversation);
        core.present_history(
            &history::History {
                available: true,
                ..Default::default()
            },
            Vec::new(),
        );
        assert_eq!(core.snapshot().conversation, before.conversation);
        assert_eq!(
            core.snapshot().conversation_revision,
            before.conversation_revision
        );
        core.update(|s| s.desktop.received += 1);
        assert_eq!(
            core.snapshot().conversation_revision,
            before.conversation_revision
        );
    }
    #[test]
    fn live_updates_do_not_replace_a_detached_reading_window() {
        let core = AppCore::new(Arc::new(Secrets), Arc::new(Silent));
        core.update(|s| {
            s.selected_host = Some("host".into());
            s.selected_session = Some("thread".into());
        });
        let old = page(0);
        core.present_history(&old, old.messages.clone());
        lock(&core.history_window).following = false;
        let new = page(80);
        core.present_history(&new, new.messages.clone());
        assert_eq!(core.snapshot().conversation.first().unwrap().id, "0");
        assert!(
            core.snapshot().history_status.has_newer
                && !core.snapshot().history_status.following_latest
        );
    }
}
