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
/// Account-level earned resets. Opaque credit IDs never enter snapshots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetCredits {
    pub available_count: u64,
    pub earliest_expires_at: Option<i64>,
    pub expiry_complete: bool,
    observed_ms: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireResetCredits {
    available_count: i64,
    credits: Option<Vec<Value>>,
}
impl WireResetCredits {
    fn into_summary(self, observed_ms: i64) -> Result<ResetCredits, &'static str> {
        if self.available_count < 0 {
            return Err("negative reset-credit count");
        }
        let count = self.available_count as u64;
        let mut seen = std::collections::BTreeSet::new();
        let mut earliest = None;
        let mut known = self.credits.is_some();
        for credit in self.credits.into_iter().flatten() {
            if credit["status"] != "available" {
                continue;
            }
            let Some(id) = credit["id"].as_str().filter(|id| !id.is_empty()) else {
                known = false;
                continue;
            };
            if !seen.insert(id.to_owned()) {
                known = false;
                continue;
            }
            match credit.get("expiresAt") {
                Some(Value::Null) => {}
                Some(value) => {
                    if let Some(at) = value
                        .as_i64()
                        .filter(|at| *at > 0 && *at <= i64::MAX / 1000)
                    {
                        earliest = Some(earliest.map_or(at, |old: i64| old.min(at)));
                    } else {
                        known = false;
                    }
                }
                None => known = false,
            }
        }
        Ok(ResetCredits {
            available_count: count,
            earliest_expires_at: earliest,
            expiry_complete: known && seen.len() as u64 == count,
            observed_ms,
        })
    }
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
    pub reset_credits: Option<ResetCredits>,
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
    rate_limit_reset_credits: Option<WireResetCredits>,
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
    let reset_credits = wire
        .rate_limit_reset_credits
        .map(|credits| credits.into_summary(time_ms))
        .transpose()?;
    let available = !buckets.is_empty() || reset_credits.is_some();
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
        reset_credits,
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
                .any(|w| w.resets_at.is_some_and(|at| at <= time_ms / 1000))
            || value.reset_credits.as_ref().is_some_and(|c| {
                time_ms.saturating_sub(c.observed_ms) > 90_000
                    || c.earliest_expires_at.is_some_and(|at| at <= time_ms / 1000)
            });
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
                    // Rolling window updates do not carry reset-card facts.
                    self.value.reset_credits = value.reset_credits.take();
                    if self.value.reset_credits.is_some()
                        && self.value.unavailable == Unavailable::NoData
                    {
                        self.value.status = Status::Ready;
                        self.value.unavailable = Unavailable::None;
                        self.value.updated_ms = value.updated_ms;
                    }
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
                    let available =
                        !self.value.buckets.is_empty() || self.value.reset_credits.is_some();
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
    fn reset_limits(count: i64, credits: Value) -> Value {
        json!({"rateLimits":{},"rateLimitResetCredits":{"availableCount":count,"credits":credits}})
    }
    fn credit(id: &str, expires: Value) -> Value {
        json!({"id":id,"status":"available","expiresAt":expires,"grantedAt":1,"resetType":"codexRateLimits"})
    }
    #[test]
    fn reset_count_is_authoritative_and_earliest_expiry_is_order_independent() {
        let input = reset_limits(
            3,
            json!([
                credit("later", json!(300)),
                credit("never", Value::Null),
                credit("first", json!(200))
            ]),
        );
        let value = decode(&account(), Some(&input), 1000).unwrap();
        assert_eq!(value.status, Status::Ready);
        let cards = value.reset_credits.unwrap();
        assert_eq!(cards.available_count, 3);
        assert_eq!(cards.earliest_expires_at, Some(200));
        assert!(cards.expiry_complete);
        let input = reset_limits(5, json!([credit("only-returned-row", json!(300))]));
        let cards = decode(&account(), Some(&input), 1000)
            .unwrap()
            .reset_credits
            .unwrap();
        assert_eq!(cards.available_count, 5);
        assert!(!cards.expiry_complete);
        assert_eq!(cards.earliest_expires_at, Some(300));
    }
    #[test]
    fn missing_details_zero_cards_and_no_expiration_remain_distinct() {
        assert!(
            decode(&account(), Some(&limits(20)), 1000)
                .unwrap()
                .reset_credits
                .is_none()
        );
        let unknown = decode(&account(), Some(&reset_limits(2, Value::Null)), 1000)
            .unwrap()
            .reset_credits
            .unwrap();
        assert!(!unknown.expiry_complete);
        assert_eq!(unknown.available_count, 2);
        let zero = decode(&account(), Some(&reset_limits(0, json!([]))), 1000)
            .unwrap()
            .reset_credits
            .unwrap();
        assert_eq!(zero.available_count, 0);
        assert!(zero.expiry_complete);
        let no_expiry = decode(
            &account(),
            Some(&reset_limits(1, json!([credit("never", Value::Null)]))),
            1000,
        )
        .unwrap()
        .reset_credits
        .unwrap();
        assert!(no_expiry.expiry_complete);
        assert!(no_expiry.earliest_expires_at.is_none());
        assert!(decode(&account(), Some(&reset_limits(-1, Value::Null)), 1000).is_err());
    }
    #[test]
    fn duplicate_redeemed_or_missing_expiry_rows_do_not_claim_complete_card_details() {
        for rows in [
            json!([
                credit("duplicate", json!(500)),
                credit("duplicate", json!(500))
            ]),
            json!([{"id":"redeemed","status":"redeemed","expiresAt":500}]),
            json!([{"id":"missing-expiry","status":"available"}]),
            json!([credit("invalid-time", json!(-1))]),
        ] {
            let cards = decode(&account(), Some(&reset_limits(1, rows)), 1000)
                .unwrap()
                .reset_credits
                .unwrap();
            assert!(!cards.expiry_complete);
        }
    }
    #[test]
    fn window_notifications_keep_reset_cards_and_a_racing_read_updates_them() {
        let mut tracker = Tracker::default();
        let request = tracker.begin(false, Instant::now()).unwrap();
        tracker.ingest(&json!({"method":"account/rateLimits/updated","params":{"rateLimits":limits(50)["rateLimits"]}}), 1000);
        tracker.finish(
            request,
            decode(
                &account(),
                Some(&reset_limits(
                    2,
                    json!([credit("first", json!(200)), credit("later", json!(300))]),
                )),
                1000,
            ),
        );
        assert_eq!(
            tracker
                .snapshot(1000)
                .reset_credits
                .as_ref()
                .unwrap()
                .available_count,
            2
        );
        assert_eq!(
            tracker.snapshot(1000).buckets[0]
                .primary
                .as_ref()
                .unwrap()
                .remaining_percent,
            50
        );
        tracker.ingest(&json!({"method":"account/rateLimits/updated","params":{"rateLimits":limits(60)["rateLimits"]}}), 95000);
        assert!(tracker.snapshot(95000).stale);
        assert_eq!(
            tracker
                .snapshot(95000)
                .reset_credits
                .as_ref()
                .unwrap()
                .available_count,
            2
        );
        let request = tracker.begin(true, Instant::now()).unwrap();
        tracker.ingest(&json!({"method":"account/updated"}), 96000);
        tracker.finish(
            request,
            decode(&account(), Some(&reset_limits(2, Value::Null)), 95000),
        );
        assert!(tracker.snapshot(96000).reset_credits.is_none());
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
