use super::*;
use serde::Deserialize;

const LIMIT: usize = 256 * 1024;
const HELPER: &str = include_str!("../../../../backend/kite_clipboard.py");

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipboardPlatform {
    Linux,
    Macos,
}
#[derive(Clone, Debug, Deserialize)]
pub struct RemoteClipboard {
    pub text: String,
    pub platform: ClipboardPlatform,
}

#[derive(Clone)]
pub(super) struct Mirror {
    local: Option<String>,
    remote: String,
}
enum Action {
    Idle,
    Push,
    Pull,
}
fn mirror(previous: Option<&Mirror>, local: Option<&str>, remote: &str) -> (Mirror, Action) {
    let local_hash = local.map(kite_agent::fingerprint);
    let remote_hash = kite_agent::fingerprint(remote);
    let changed_local = previous.is_none_or(|p| p.local != local_hash);
    if changed_local && local.is_some() {
        return (
            Mirror {
                local: local_hash.clone(),
                remote: local_hash.unwrap(),
            },
            Action::Push,
        );
    }
    if !changed_local && previous.is_some_and(|p| p.remote != remote_hash) && !remote.is_empty() {
        return (
            Mirror {
                local: Some(remote_hash.clone()),
                remote: remote_hash,
            },
            Action::Pull,
        );
    }
    (
        Mirror {
            local: local_hash,
            remote: remote_hash,
        },
        Action::Idle,
    )
}

impl AppCore {
    pub async fn read_clipboard(
        self: &Arc<Self>,
        host: String,
    ) -> Result<RemoteClipboard, AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.clipboard_operations.lock().await;
            core.clipboard(host, None).await
        })
        .await
    }
    pub async fn write_clipboard(
        self: &Arc<Self>,
        host: String,
        text: String,
    ) -> Result<ClipboardPlatform, AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.clipboard_operations.lock().await;
            Ok(core.clipboard(host, Some(text)).await?.platform)
        })
        .await
    }
    pub async fn synchronize_clipboard(
        self: &Arc<Self>,
        host: String,
        local: Option<String>,
    ) -> Result<Option<String>, AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.clipboard_operations.lock().await;
            if local.as_ref().is_some_and(|s| s.len() > LIMIT) {
                return Err(AppError::State("clipboard text exceeds limit".into()));
            }
            let previous = lock(&core.clipboard_mirrors).get(&host).cloned();
            let local_changed = local.as_ref().is_some_and(|value| {
                previous
                    .as_ref()
                    .is_none_or(|old| old.local.as_ref() != Some(&kite_agent::fingerprint(value)))
            });
            let remote = if local_changed {
                String::new()
            } else {
                core.clipboard(host.clone(), None).await?.text
            };
            let (next, action) = mirror(previous.as_ref(), local.as_deref(), &remote);
            if matches!(action, Action::Push) {
                core.clipboard(host.clone(), local).await?;
            }
            lock(&core.clipboard_mirrors).insert(host, next);
            Ok(if matches!(action, Action::Pull) {
                Some(remote)
            } else {
                None
            })
        })
        .await
    }
    async fn clipboard(
        self: &Arc<Self>,
        host: String,
        text: Option<String>,
    ) -> Result<RemoteClipboard, AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            let c = core.connection()?;
            let snapshot = core.snapshot();
            if c.host.id != host
                || snapshot.selected_host.as_ref() != Some(&host)
                || !snapshot.desktop.takeover
                || snapshot.desktop.phase != DesktopPhase::Streaming
            {
                return Err(AppError::State(
                    "clipboard requires the selected operating desktop".into(),
                ));
            }
            if c.host
                .stream_host
                .as_ref()
                .is_some_and(|s| s != &c.host.host)
            {
                return Err(AppError::State(
                    "a separate streaming host needs its own clipboard adapter".into(),
                ));
            }
            if text.as_ref().is_some_and(|s| s.len() > LIMIT) {
                return Err(AppError::State("clipboard text exceeds limit".into()));
            }
            let mode = if text.is_some() { "write" } else { "read" };
            let command = format!(
                "python3 -c {} {mode} {}",
                composer::quote(HELPER),
                c.host.sunshine_port
            );
            let output = tokio::time::timeout(
                Duration::from_secs(6),
                c.ssh
                    .exec(&command, text.as_deref().unwrap_or_default().as_bytes()),
            )
            .await
            .map_err(|_| {
                AppError::State(
                    "clipboard operation timed out; inspect before repeating a write".into(),
                )
            })??;
            if output.exit_status != Some(0) || output.stdout.len() > LIMIT * 6 + 1024 {
                return Err(AppError::State(format!(
                    "clipboard unavailable: {}",
                    String::from_utf8_lossy(&output.stderr)
                        .chars()
                        .take(300)
                        .collect::<String>()
                )));
            }
            let value: RemoteClipboard = serde_json::from_slice(&output.stdout)
                .map_err(|_| AppError::State("invalid clipboard response".into()))?;
            if value.text.len() > LIMIT {
                return Err(AppError::State("clipboard text exceeds limit".into()));
            }
            Ok(value)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_mirror_prefers_new_local_copies_and_never_echoes_a_remote_import() {
        let (first, action) = mirror(None, Some("phone"), "desktop");
        assert!(matches!(action, Action::Push));
        let (same, action) = mirror(Some(&first), Some("phone"), "phone");
        assert!(matches!(action, Action::Idle));
        let (incoming, action) = mirror(Some(&same), Some("phone"), "new desktop");
        assert!(matches!(action, Action::Pull));
        let (_, action) = mirror(Some(&incoming), Some("new desktop"), "new desktop");
        assert!(matches!(action, Action::Idle));
        let (_, action) = mirror(Some(&incoming), Some("new phone"), "other desktop");
        assert!(matches!(action, Action::Push));
    }
    #[test]
    fn non_text_local_content_is_not_replaced_by_an_old_remote_value() {
        let (first, _) = mirror(None, Some("text"), "text");
        let (image, action) = mirror(Some(&first), None, "text");
        assert!(matches!(action, Action::Idle));
        let (_, action) = mirror(Some(&image), None, "new text");
        assert!(matches!(action, Action::Pull));
    }
}
