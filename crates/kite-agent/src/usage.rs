//! Account-scoped, read-only Codex quota facts. No credentials or account identifiers.
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const REFRESH_INTERVAL: Duration = Duration::from_secs(60);

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    NotLoaded,
    Ready,
    Unavailable,
    Failed,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Unavailable {
    #[default]
    None,
    NoAccount,
    UnsupportedAuth,
    NoData,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Window {
    pub remaining_percent: u32,
    pub duration_mins: Option<u64>,
    pub resets_at: Option<i64>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Credits {
    pub has_credits: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bucket {
    pub id: String,
    pub name: Option<String>,
    pub primary: Option<Window>,
    pub secondary: Option<Window>,
    pub credits: Option<Credits>,
    pub limit_reached: bool,
    pub spend_control_reached: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountUsage {
    pub status: Status,
    pub unavailable: Unavailable,
    pub loading: bool,
    pub stale: bool,
    pub plan: Option<String>,
    pub buckets: Vec<Bucket>,
    pub ordinary_usage_allowed: Option<bool>,
    pub updated_ms: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireWindow {
    used_percent: i32,
    window_duration_mins: Option<i64>,
    resets_at: Option<i64>,
}
impl WireWindow {
    fn into_window(self) -> Result<Window, &'static str> {
        if self.used_percent < 0 {
            return Err("negative quota usage");
        }
        Ok(Window {
            remaining_percent: 100 - self.used_percent.min(100) as u32,
            duration_mins: self
                .window_duration_mins
                .filter(|v| *v > 0)
                .map(|v| v as u64),
            resets_at: self.resets_at.filter(|v| *v > 0),
        })
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireBucket {
    limit_id: Option<String>,
    limit_name: Option<String>,
    primary: Option<WireWindow>,
    secondary: Option<WireWindow>,
    credits: Option<Credits>,
    rate_limit_reached_type: Option<String>,
    spend_control_reached: Option<bool>,
}
impl WireBucket {
    fn into_bucket(self, id: Option<&str>) -> Result<Bucket, &'static str> {
        Ok(Bucket {
            id: id.map(str::to_owned).or(self.limit_id).unwrap_or_default(),
            name: self.limit_name,
            primary: self.primary.map(WireWindow::into_window).transpose()?,
            secondary: self.secondary.map(WireWindow::into_window).transpose()?,
            credits: self.credits,
            limit_reached: self.rate_limit_reached_type.is_some(),
            spend_control_reached: self.spend_control_reached == Some(true),
        })
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireLimits {
    rate_limits: WireBucket,
    rate_limits_by_limit_id: Option<BTreeMap<String, WireBucket>>,
    ordinary_usage_allowed: Option<bool>,
}

pub fn decode(
    account: &Value,
    limits: Option<&Value>,
    time_ms: i64,
) -> Result<AccountUsage, &'static str> {
    let kind = account["account"]["type"].as_str();
    let plan = account["account"]["planType"].as_str().map(str::to_owned);
    if kind != Some("chatgpt") {
        return Ok(AccountUsage {
            status: Status::Unavailable,
            unavailable: if kind.is_none() {
                Unavailable::NoAccount
            } else {
                Unavailable::UnsupportedAuth
            },
            plan,
            ..Default::default()
        });
    }
    let wire: WireLimits = serde_json::from_value(limits.ok_or("quota response missing")?.clone())
        .map_err(|_| "invalid quota response")?;
    // Both fields belong to the installed 0.164 schema: the map is an optional
    // expanded view, not another endpoint or an old-protocol retry.
    let buckets = if let Some(map) = wire.rate_limits_by_limit_id.filter(|m| !m.is_empty()) {
        map.into_iter()
            .map(|(id, bucket)| bucket.into_bucket(Some(&id)))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![wire.rate_limits.into_bucket(None)?]
    };
    let buckets: Vec<_> = buckets
        .into_iter()
        .filter(|b| {
            b.primary.is_some()
                || b.secondary.is_some()
                || b.credits.is_some()
                || b.limit_reached
                || b.spend_control_reached
        })
        .collect();
    let available = !buckets.is_empty();
    Ok(AccountUsage {
        status: if available {
            Status::Ready
        } else {
            Status::Unavailable
        },
        unavailable: if available {
            Unavailable::None
        } else {
            Unavailable::NoData
        },
        plan,
        buckets,
        ordinary_usage_allowed: wire.ordinary_usage_allowed,
        updated_ms: available.then_some(time_ms),
        ..Default::default()
    })
}

#[derive(Clone, Copy)]
pub struct Refresh {
    epoch: u64,
    revision: u64,
}
#[derive(Default)]
pub struct Tracker {
    value: AccountUsage,
    epoch: u64,
    revision: u64,
    last_attempt: Option<Instant>,
}
impl Tracker {
    pub fn snapshot(&self, time_ms: i64) -> AccountUsage {
        let mut value = self.value.clone();
        value.stale |= value
            .updated_ms
            .is_some_and(|at| time_ms.saturating_sub(at) > 90_000)
            || value
                .buckets
                .iter()
                .flat_map(|b| [&b.primary, &b.secondary])
                .flatten()
                .any(|w| w.resets_at.is_some_and(|at| at <= time_ms / 1000));
        value
    }
    pub fn begin(&mut self, force: bool, now: Instant) -> Option<Refresh> {
        if self.value.loading
            || (!force
                && self
                    .last_attempt
                    .is_some_and(|at| now.saturating_duration_since(at) < REFRESH_INTERVAL))
        {
            return None;
        }
        self.last_attempt = Some(now);
        self.value.loading = true;
        Some(Refresh {
            epoch: self.epoch,
            revision: self.revision,
        })
    }
    pub fn finish(&mut self, request: Refresh, result: Result<AccountUsage, &'static str>) {
        if request.epoch != self.epoch {
            return;
        }
        self.value.loading = false;
        match result {
            Ok(mut value) => {
                if request.revision != self.revision && value.status == Status::Ready {
                    // Account metadata may be filled, but later notifications own the windows.
                    self.value.plan = value.plan.take().or(self.value.plan.take());
                } else {
                    self.value = value;
                }
            }
            Err(_) if request.revision != self.revision => {}
            Err(_) => {
                self.value.status = Status::Failed;
                self.value.stale = self.value.updated_ms.is_some();
            }
        }
    }
    pub fn ingest(&mut self, event: &Value, time_ms: i64) {
        match event["method"].as_str() {
            Some("account/updated") => {
                self.epoch = self.epoch.wrapping_add(1);
                self.revision = 0;
                self.last_attempt = None;
                self.value = AccountUsage::default();
            }
            Some("account/rateLimits/updated") => {
                if let Ok(bucket) =
                    serde_json::from_value::<WireBucket>(event["params"]["rateLimits"].clone())
                        .map_err(|_| "invalid quota notification")
                        .and_then(|b| b.into_bucket(None))
                {
                    if self.value.plan.is_none() {
                        self.value.plan = event["params"]["rateLimits"]["planType"]
                            .as_str()
                            .map(str::to_owned);
                    }
                    if let Some(existing) =
                        self.value.buckets.iter_mut().find(|b| b.id == bucket.id)
                    {
                        *existing = bucket;
                    } else {
                        self.value.buckets.push(bucket);
                    }
                    self.value.buckets.retain(|b| {
                        b.primary.is_some()
                            || b.secondary.is_some()
                            || b.credits.is_some()
                            || b.limit_reached
                            || b.spend_control_reached
                    });
                    self.revision = self.revision.wrapping_add(1);
                    let available = !self.value.buckets.is_empty();
                    self.value.updated_ms = available.then_some(time_ms);
                    self.value.status = if available {
                        Status::Ready
                    } else {
                        Status::Unavailable
                    };
                    self.value.unavailable = if available {
                        Unavailable::None
                    } else {
                        Unavailable::NoData
                    };
                    self.value.stale = false;
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn account() -> Value {
        json!({"account":{"type":"chatgpt","planType":"pro","email":"not-retained@example.invalid"}})
    }
    fn limits(used: i32) -> Value {
        json!({"rateLimits":{"limitId":"codex","primary":{"usedPercent":used,"windowDurationMins":300,"resetsAt":2000000000}},"ordinaryUsageAllowed":false})
    }
    #[test]
    fn current_schema_preserves_independent_windows_and_exact_credit_balance() {
        let input = json!({"rateLimits":{},"rateLimitsByLimitId":{"codex":{"primary":{"usedPercent":27,"windowDurationMins":300},"secondary":{"usedPercent":82,"windowDurationMins":10080},"credits":{"hasCredits":true,"unlimited":false,"balance":"12.345"}},"review":{"limitName":"Review","primary":{"usedPercent":110}}}});
        let value = decode(&account(), Some(&input), 10).unwrap();
        assert_eq!(value.buckets.len(), 2);
        assert_eq!(
            value.buckets[0].primary.as_ref().unwrap().remaining_percent,
            73
        );
        assert_eq!(
            value.buckets[0]
                .secondary
                .as_ref()
                .unwrap()
                .remaining_percent,
            18
        );
        assert_eq!(
            value.buckets[0]
                .credits
                .as_ref()
                .unwrap()
                .balance
                .as_deref(),
            Some("12.345")
        );
        assert_eq!(
            value.buckets[1].primary.as_ref().unwrap().remaining_percent,
            0
        );
    }
    #[test]
    fn unavailable_is_not_zero_or_unlimited_and_permission_is_not_inferred() {
        for account in [
            json!({"account":null}),
            json!({"account":{"type":"apiKey"}}),
        ] {
            let value = decode(&account, None, 10).unwrap();
            assert_eq!(value.status, Status::Unavailable);
            assert!(value.buckets.is_empty());
        }
        assert_eq!(
            decode(&account(), Some(&json!({"rateLimits":{}})), 10)
                .unwrap()
                .unavailable,
            Unavailable::NoData
        );
        assert!(decode(&account(), Some(&limits(-1)), 10).is_err());
        assert_eq!(
            decode(&account(), Some(&limits(20)), 10)
                .unwrap()
                .ordinary_usage_allowed,
            Some(false)
        );
    }
    #[test]
    fn late_read_cannot_overwrite_a_newer_notification_or_another_account() {
        let mut tracker = Tracker::default();
        let token = tracker.begin(false, Instant::now()).unwrap();
        tracker.ingest(&json!({"method":"account/rateLimits/updated","params":{"rateLimits":limits(50)["rateLimits"]}}),20);
        tracker.finish(token, decode(&account(), Some(&limits(10)), 10));
        assert_eq!(
            tracker.snapshot(20).buckets[0]
                .primary
                .as_ref()
                .unwrap()
                .remaining_percent,
            50
        );
        let token = tracker.begin(true, Instant::now()).unwrap();
        tracker.ingest(&json!({"method":"account/updated"}), 30);
        tracker.finish(token, decode(&account(), Some(&limits(30)), 30));
        assert!(tracker.snapshot(30).buckets.is_empty());
    }
    #[test]
    fn refresh_failure_keeps_last_facts_stale_and_never_resets_expired_windows() {
        let mut tracker = Tracker::default();
        let now = Instant::now();
        let token = tracker.begin(false, now).unwrap();
        tracker.finish(token, decode(&account(), Some(&limits(27)), 10));
        assert!(
            tracker
                .begin(false, now + Duration::from_secs(59))
                .is_none()
        );
        let token = tracker.begin(false, now + REFRESH_INTERVAL).unwrap();
        tracker.finish(token, Err("unavailable"));
        let value = tracker.snapshot(20);
        assert_eq!(value.status, Status::Failed);
        assert!(value.stale);
        assert_eq!(
            value.buckets[0].primary.as_ref().unwrap().remaining_percent,
            73
        );
        assert!(tracker.snapshot(2000000000000).stale);
    }
    #[test]
    fn a_notification_without_quota_facts_is_explicitly_unavailable() {
        let mut tracker = Tracker::default();
        let request = tracker.begin(false, Instant::now()).unwrap();
        tracker.finish(request, decode(&account(), Some(&limits(27)), 10));
        tracker.ingest(&json!({"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"codex"}}}),20);
        let value = tracker.snapshot(20);
        assert_eq!(value.status, Status::Unavailable);
        assert_eq!(value.unavailable, Unavailable::NoData);
        assert!(value.buckets.is_empty());
    }
}
