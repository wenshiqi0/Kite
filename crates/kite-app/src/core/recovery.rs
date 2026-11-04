use super::*;
use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct ConnectionIntent {
    pub host_id: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Connections {
    pub hosts: BTreeSet<String>,
}

pub(super) fn retryable(error: &AppError) -> bool {
    match error {
        AppError::Ssh(error)
        | AppError::Composer(composer::Error::Ssh(error))
        | AppError::Herdr(kite_herdr::CheckError::Ssh(error))
        | AppError::Api(kite_herdr::api::ApiError::Ssh(error)) => matches!(
            error,
            SshError::Io(_) | SshError::Timeout(_) | SshError::Protocol(_)
        ),
        AppError::Herdr(kite_herdr::CheckError::Timeout)
        | AppError::Api(kite_herdr::api::ApiError::Timeout | kite_herdr::api::ApiError::Io(_)) => {
            true
        }
        AppError::Composer(composer::Error::NodeUnavailable(_)) => true,
        _ => false,
    }
}

fn retry_delay(attempt: u64) -> Duration {
    Duration::from_secs((1u64 << attempt.saturating_sub(1).min(5)).min(30))
}

pub(super) async fn probe(ssh: &Session) -> bool {
    matches!(tokio::time::timeout(Duration::from_secs(3), ssh.exec("true", &[])).await,
        Ok(Ok(output)) if output.exit_status == Some(0))
}

impl AppCore {
    pub(super) fn set_connection_intent(&self, host_id: Option<String>) -> Result<(), AppError> {
        let intent = ConnectionIntent {
            host_id: host_id.clone(),
        };
        let mut intents = lock(&self.connection_intents);
        let mut next = intents.clone();
        if let Some(previous) = self
            .workspace_host
            .as_ref()
            .cloned()
            .or_else(|| lock(&self.desired_connection).clone())
        {
            next.remove(&previous);
        }
        next.extend(host_id.iter().cloned());
        if let Some(store) = lock(&self.store).as_ref() {
            if self.workspace_host.is_some() {
                store.write_document(
                    "connections",
                    &Connections {
                        hosts: next.clone(),
                    },
                )?;
            } else {
                store.write_document("connection-intent", &intent)?;
            }
        } else if host_id.is_some() {
            return Err(AppError::State("core is not initialized".into()));
        }
        *intents = next;
        *lock(&self.desired_connection) = host_id;
        Ok(())
    }

    pub async fn restore_connection(self: &Arc<Self>) -> Result<(), AppError> {
        let core = self.clone();
        dispatch(async move {
            let _guard = core.operations.lock().await;
            if !matches!(
                core.snapshot().connection,
                ConnectionPhase::Disconnected | ConnectionPhase::Failed
            ) {
                return Ok(());
            }
            let target = lock(&core.desired_connection).clone();
            if let Some(target) = target {
                core.begin_connection(core.host(&target)?, true).await?;
            }
            Ok(())
        })
        .await
    }

    /// Native platforms supply connectivity facts; all retry and host policy stays in Rust.
    pub fn network_changed(&self, available: bool) {
        self.network_available.store(available, Ordering::SeqCst);
        self.network_revision.fetch_add(1, Ordering::SeqCst);
        self.network_wake.notify_one();
    }

    pub(super) fn schedule_recovery(self: &Arc<Self>, host: &str, generation: u64) {
        if self.generation.load(Ordering::SeqCst) != generation
            || lock(&self.desired_connection).as_deref() != Some(host)
        {
            return;
        }
        if let Some(old) = lock(&self.recovery_job).take() {
            old.abort();
        }
        let attempt = self
            .recovery_attempt
            .fetch_add(1, Ordering::SeqCst)
            .saturating_add(1);
        self.update(|s| {
            s.connection = ConnectionPhase::Reconnecting;
            s.composer.ready = false;
            s.approval = None;
            s.question = None;
            s.desktop.takeover = false;
            if s.desktop.phase != DesktopPhase::Idle {
                s.desktop.phase = DesktopPhase::Failed;
                s.desktop.pin = None;
            }
        });
        if let Some(media) = lock(&self.media).as_ref() {
            media.set_takeover(false);
        }
        if let Ok(job) = self.desktop_job.try_lock()
            && let Some(job) = job.as_ref()
        {
            let _ = job.cancel.send(true);
        }
        for job in [&self.resource_job, &self.notification_job] {
            if let Some(job) = lock(job).take() {
                job.abort();
            }
        }
        self.reset_history_view();
        let core = self.clone();
        let host = host.to_owned();
        let job = tokio::spawn(async move {
            let delay = tokio::time::sleep(retry_delay(attempt));
            tokio::pin!(delay);
            loop {
                if core.network_available.load(Ordering::SeqCst) {
                    tokio::select! { _ = &mut delay => break, _ = core.network_wake.notified() => {
                        if core.network_available.load(Ordering::SeqCst) { break; }
                    } }
                } else {
                    core.network_wake.notified().await;
                }
            }
            let _guard = core.operations.lock().await;
            if core.generation.load(Ordering::SeqCst) != generation
                || lock(&core.desired_connection).as_deref() != Some(&host)
            {
                return;
            }
            // Detach our own handle before begin_connection cancels the old connection generation.
            lock(&core.recovery_job).take();
            match core.host(&host) {
                Ok(host) => {
                    if let Err(error) = core.begin_connection(host, true).await {
                        core.report(&error);
                    }
                }
                Err(error) => core.report(&error),
            }
        });
        *lock(&self.recovery_job) = Some(job);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retries_are_bounded_and_never_retry_authentication_or_trust_errors() {
        assert_eq!(
            (1..=8)
                .map(|n| retry_delay(n).as_secs())
                .collect::<Vec<_>>(),
            vec![1, 2, 4, 8, 16, 30, 30, 30]
        );
        assert_eq!(retry_delay(u64::MAX), Duration::from_secs(30));
        for error in [
            SshError::AuthRejected("fixture".into()),
            SshError::InvalidKey("fixture".into()),
            SshError::HostKeyChanged {
                trusted: "old".into(),
                presented: "new".into(),
            },
            SshError::UnknownHostKey {
                fingerprint: "new".into(),
                public_key: "new".into(),
            },
        ] {
            assert!(!retryable(&AppError::Ssh(error)));
        }
        assert!(retryable(&AppError::Ssh(SshError::Timeout("connecting"))));
        assert!(!retryable(&AppError::Herdr(
            kite_herdr::CheckError::NotFound
        )));
    }
}
