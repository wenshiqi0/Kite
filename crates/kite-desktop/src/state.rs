use serde_json::Value;
use std::collections::VecDeque;
#[derive(Default)]
pub struct Model {
    pub value: Option<Value>,
    pub health: &'static str,
    pub error: Option<String>,
    pub action_error: Option<String>,
    pub failed_action: Option<Vec<String>>,
    pub active: Option<Vec<String>>,
    pub queue: VecDeque<Vec<String>>,
    pub status_pending: bool,
    pub refresh_requested: bool,
    pub update_pending: bool,
}
impl Model {
    pub fn enqueue(&mut self, args: Vec<String>) {
        if self.active.as_ref() != Some(&args) && !self.queue.contains(&args) {
            self.queue.push_back(args);
        }
    }
    pub fn next(&mut self) -> Option<Vec<String>> {
        if self.active.is_some() {
            return None;
        }
        let next = self.queue.pop_front()?;
        self.active = Some(next.clone());
        Some(next)
    }
    pub fn status(&mut self, value: Result<Value, String>, timeout: bool) {
        self.status_pending = false;
        match value {
            Ok(value) if matches!(value["health"].as_str(), Some("ready" | "degraded")) => {
                self.health = if value["health"] == "ready" {
                    "ready"
                } else {
                    "degraded"
                };
                self.value = Some(value);
                self.error = None;
            }
            result => {
                self.health = if timeout { "unresponsive" } else { "offline" };
                self.value = None;
                self.error = Some(result.err().unwrap_or_else(|| "Invalid node health".into()));
            }
        }
    }
    pub fn settled(&mut self, error: Option<String>) {
        let completed = self.active.take();
        if let Some(error) = error {
            self.action_error = Some(error);
            self.failed_action = completed;
        } else if self.failed_action == completed {
            self.action_error = None;
            self.failed_action = None;
        }
    }
    pub fn can_upgrade(&self, embedded: bool, visible: bool) -> bool {
        self.update_pending
            && embedded
            && !visible
            && self.active.is_none()
            && self.queue.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn coalesces_queued_commands_and_preserves_distinct_operations() {
        let mut m = Model::default();
        let ensure = vec!["ensure".into(), "all".into()];
        m.enqueue(ensure.clone());
        m.enqueue(ensure.clone());
        assert_eq!(m.next(), Some(ensure.clone()));
        m.enqueue(ensure);
        m.enqueue(vec!["sessions".into()]);
        assert_eq!(m.queue.len(), 1);
        assert!(m.next().is_none());
        m.settled(Some("failed".into()));
        assert_eq!(m.next(), Some(vec!["sessions".into()]));
        m.settled(None);
        assert_eq!(m.action_error.as_deref(), Some("failed"));
    }
    #[test]
    fn invalid_and_timed_out_status_never_look_ready() {
        let mut m = Model::default();
        m.status(Ok(json!({"health":"ready"})), false);
        assert_eq!(m.health, "ready");
        m.status(Ok(json!({"health":"invented"})), false);
        assert_eq!(m.health, "offline");
        assert!(m.value.is_none());
        m.status(Err("deadline".into()), true);
        assert_eq!(m.health, "unresponsive");
    }
    #[test]
    fn upgrades_wait_for_hidden_panel_and_settled_actions() {
        let mut m = Model {
            update_pending: true,
            ..Default::default()
        };
        assert!(!m.can_upgrade(true, true));
        assert!(!m.can_upgrade(false, false));
        assert!(m.can_upgrade(true, false));
        m.enqueue(vec!["ensure".into()]);
        assert!(!m.can_upgrade(true, false));
    }
}
