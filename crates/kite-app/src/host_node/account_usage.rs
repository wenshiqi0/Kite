use super::*;
use kite_agent::usage::{self, ProviderAccountUsage};
use std::future::Future;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);

#[derive(Deserialize)]
pub(super) struct Probe {
    provider: String,
    account: Option<Value>,
    limits: Option<Value>,
}

#[derive(Default)]
struct CacheState {
    last_attempt: Option<Instant>,
    rows: Vec<ProviderAccountUsage>,
    available: bool,
}

#[derive(Default)]
pub(super) struct AccountUsageCache(Mutex<CacheState>);

impl AccountUsageCache {
    pub async fn read(
        &self,
        fetch: impl Future<Output = Result<Vec<Probe>>>,
    ) -> Result<Vec<ProviderAccountUsage>> {
        let mut state = self.0.lock().await;
        let now = Instant::now();
        if state
            .last_attempt
            .is_none_or(|last| now.saturating_duration_since(last) >= REFRESH_INTERVAL)
        {
            state.last_attempt = Some(now);
            match fetch.await {
                Ok(probes) => {
                    let old = std::mem::take(&mut state.rows);
                    let time = usage::now_ms();
                    state.rows = probes
                        .into_iter()
                        .map(|probe| {
                            let decoded = probe.account.as_ref().and_then(|account| {
                                let value =
                                    usage::decode(account, probe.limits.as_ref(), time).ok()?;
                                (value.buckets.len() <= 64).then_some(value)
                            });
                            let usage = decoded.unwrap_or_else(|| {
                                let mut value = old
                                    .iter()
                                    .find(|row| row.provider == probe.provider)
                                    .map(|row| row.usage.clone())
                                    .unwrap_or_default();
                                value.mark_failed();
                                value
                            });
                            ProviderAccountUsage {
                                provider: probe.provider,
                                usage,
                            }
                        })
                        .collect();
                    state.available = true;
                }
                Err(_) => {
                    for row in &mut state.rows {
                        row.usage.mark_failed();
                    }
                    state.available = !state.rows.is_empty();
                }
            }
        }
        if !state.available {
            return Err(Error::Protocol("host account usage unavailable"));
        }
        let time = usage::now_ms();
        Ok(state
            .rows
            .iter()
            .map(|row| ProviderAccountUsage {
                provider: row.provider.clone(),
                usage: row.usage.snapshot(time, REFRESH_INTERVAL),
            })
            .collect())
    }
}

pub(super) async fn fetch(node: &Node) -> Result<Vec<Probe>> {
    let config = node.config.current()?;
    let mut command = Command::new(&config.bun);
    command
        .arg(&config.backend)
        .arg("account-usage")
        .envs(&config.environment)
        .env("KITE_HOST_BACKEND_PATH", &node.config.backend);
    let output = bounded_output_with_timeout(command, Duration::from_secs(24)).await?;
    if output.len() > 256 * 1024 {
        return Err(Error::Protocol("account usage exceeds limit"));
    }
    let rows: Vec<Probe> = serde_json::from_slice(&output)?;
    if rows.len() > 2
        || rows.iter().enumerate().any(|(index, row)| {
            !matches!(row.provider.as_str(), "codex" | "antigravity")
                || rows[..index].iter().any(|old| old.provider == row.provider)
        })
    {
        return Err(Error::Protocol("invalid account usage providers"));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_reads_share_a_refresh_and_failures_preserve_previous_windows() {
        let cache = AccountUsageCache::default();
        let attempts = AtomicU64::new(0);
        let probe = async {
            attempts.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok(vec![Probe {
                provider: "codex".into(),
                account: Some(
                    json!({"account":{"type":"chatgpt","planType":"pro","email":"private"}}),
                ),
                limits: Some(
                    json!({"rateLimits":{"limitId":"codex","primary":{"usedPercent":30,"windowDurationMins":300}}}),
                ),
            }])
        };
        let (first, second) = tokio::join!(
            cache.read(probe),
            cache.read(async {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(Error::Protocol("unexpected duplicate request"))
            })
        );
        let first = first.unwrap();
        assert_eq!(first, second.unwrap());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert!(!serde_json::to_string(&first).unwrap().contains("private"));
        assert_eq!(
            first[0].usage.buckets[0]
                .primary
                .as_ref()
                .unwrap()
                .remaining_percent,
            70
        );

        cache.0.lock().await.last_attempt = Some(Instant::now() - REFRESH_INTERVAL);
        let failed = cache
            .read(async {
                Ok(vec![Probe {
                    provider: "codex".into(),
                    account: None,
                    limits: None,
                }])
            })
            .await
            .unwrap();
        assert_eq!(failed[0].usage.status, usage::Status::Failed);
        assert!(failed[0].usage.stale);
        assert_eq!(failed[0].usage.buckets, first[0].usage.buckets);
    }
}
