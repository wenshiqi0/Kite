//! Shared host resource facts and memory admission policy.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const GIB: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_AGENT_MEMORY_LIMIT: u64 = 10 * GIB;
pub const STARTUP_RESERVATION: u64 = GIB;

pub fn default_agent_memory_limit() -> u64 {
    DEFAULT_AGENT_MEMORY_LIMIT
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPressure {
    #[default]
    Unknown,
    Normal,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceReason {
    None,
    HostMemory,
    AgentBudget,
    StartupReservation,
    MemoryStall,
    RecentOom,
    #[default]
    EnforcementUnavailable,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProcessResource {
    pub pid: u32,
    pub name: String,
    pub memory_bytes: u64,
    pub cpu_percent: Option<f64>,
    pub managed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HostResources {
    pub available: bool,
    pub problem: Option<String>,
    pub sampled_at_ms: u64,
    pub cpu_percent: Option<f64>,
    pub cpu_cores: u32,
    pub load_one: f64,
    pub load_five: f64,
    pub load_fifteen: f64,
    pub memory_total_bytes: u64,
    pub memory_available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub memory_pressure_some: f64,
    pub memory_pressure_full: f64,
    pub agent_memory_used_bytes: u64,
    pub agent_reclaimable_bytes: u64,
    pub agent_memory_limit_bytes: u64,
    pub agent_migration_reserved_bytes: u64,
    pub agent_startup_reserved_bytes: u64,
    pub agent_kernel_limit_bytes: u64,
    pub agent_cpu_percent: Option<f64>,
    pub agent_process_count: u32,
    pub agent_oom_kills: u64,
    pub limit_enforced: bool,
    pub memory_pressure: MemoryPressure,
    pub reason: ResourceReason,
    pub can_start_agents: bool,
    pub processes: Vec<ProcessResource>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RawProcessResource {
    pub pid: u32,
    pub name: String,
    pub started: String,
    pub cpu_ticks: u64,
    pub memory_bytes: u64,
    pub managed: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RawResources {
    pub sampled_at_ms: u64,
    pub cpu_total_ticks: u64,
    pub cpu_idle_ticks: u64,
    pub cpu_cores: u32,
    pub load_average: [f64; 3],
    pub memory_total_bytes: u64,
    pub memory_available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub memory_pressure_some: f64,
    pub memory_pressure_full: f64,
    pub agent_current_bytes: u64,
    pub agent_reclaimable_bytes: u64,
    pub agent_reserved_bytes: u64,
    pub agent_effective_limit_bytes: u64,
    pub agent_cpu_usage_usec: u64,
    pub agent_process_count: u32,
    pub agent_oom_kills: u64,
    pub enforced: bool,
    pub processes: Vec<RawProcessResource>,
}

#[derive(Default)]
pub struct ResourceTracker {
    previous: Option<RawResources>,
    pressure: MemoryPressure,
    healthy_samples: u8,
    last_oom_ms: Option<u64>,
}

fn ratio(current: u64, previous: u64, total: u64) -> Option<f64> {
    let difference = current.checked_sub(previous)?;
    (total > 0).then(|| (difference as f64 * 100.0 / total as f64).clamp(0.0, 100.0))
}

impl ResourceTracker {
    pub fn observe(
        &mut self,
        raw: RawResources,
        limit: u64,
        startup_reserved: u64,
        configured: bool,
    ) -> HostResources {
        if raw.memory_total_bytes == 0
            || raw.memory_available_bytes > raw.memory_total_bytes
            || raw.cpu_cores == 0
            || raw.processes.len() > 16
        {
            return HostResources::default();
        }
        let previous = self.previous.as_ref();
        let total_delta = previous
            .and_then(|old| raw.cpu_total_ticks.checked_sub(old.cpu_total_ticks))
            .unwrap_or(0);
        let cpu_percent = previous.and_then(|old| {
            let idle = raw.cpu_idle_ticks.checked_sub(old.cpu_idle_ticks)?;
            (total_delta > 0 && idle <= total_delta)
                .then(|| 100.0 * (total_delta - idle) as f64 / total_delta as f64)
        });
        let agent_cpu_percent = previous.and_then(|old| {
            let elapsed = raw.sampled_at_ms.checked_sub(old.sampled_at_ms)?;
            ratio(
                raw.agent_cpu_usage_usec,
                old.agent_cpu_usage_usec,
                elapsed
                    .saturating_mul(1000)
                    .saturating_mul(raw.cpu_cores as u64),
            )
        });
        let old_processes: BTreeMap<_, _> = previous
            .map(|old| old.processes.iter().map(|p| (p.pid, p)).collect())
            .unwrap_or_default();
        let processes = raw
            .processes
            .iter()
            .map(|process| ProcessResource {
                pid: process.pid,
                name: process.name.chars().take(64).collect(),
                memory_bytes: process.memory_bytes,
                managed: process.managed,
                cpu_percent: old_processes.get(&process.pid).and_then(|old| {
                    (old.started == process.started)
                        .then(|| ratio(process.cpu_ticks, old.cpu_ticks, total_delta))
                        .flatten()
                }),
            })
            .collect();
        if previous.is_some_and(|old| raw.agent_oom_kills > old.agent_oom_kills) {
            self.last_oom_ms = Some(raw.sampled_at_ms);
        }
        let used = raw
            .agent_current_bytes
            .saturating_add(raw.agent_reserved_bytes);
        let allocated = used
            .saturating_sub(raw.agent_reclaimable_bytes)
            .saturating_add(startup_reserved);
        let enforced = configured
            && raw.enforced
            && raw.agent_effective_limit_bytes > 0
            && raw
                .agent_effective_limit_bytes
                .saturating_add(raw.agent_reserved_bytes)
                <= limit;
        let available_fraction = raw.memory_available_bytes as f64 / raw.memory_total_bytes as f64;
        let budget_fraction = allocated as f64 / limit.max(1) as f64;
        let host_critical =
            raw.memory_available_bytes < 512 * 1024 * 1024 || available_fraction < 0.05;
        let host_warning = available_fraction < 0.15;
        let budget_critical = budget_fraction >= 0.90;
        let budget_warning = budget_fraction >= 0.80;
        let stalled = raw.memory_pressure_full >= 1.0 || raw.memory_pressure_some >= 10.0;
        let severe_stall = raw.memory_pressure_full >= 10.0;
        let recent_oom = self
            .last_oom_ms
            .is_some_and(|time| raw.sampled_at_ms.saturating_sub(time) < 60_000);
        let (requested, reason) = if !enforced {
            (
                MemoryPressure::Unknown,
                ResourceReason::EnforcementUnavailable,
            )
        } else if host_critical {
            (MemoryPressure::Critical, ResourceReason::HostMemory)
        } else if budget_critical {
            (MemoryPressure::Critical, ResourceReason::AgentBudget)
        } else if severe_stall {
            (MemoryPressure::Critical, ResourceReason::MemoryStall)
        } else if recent_oom {
            (MemoryPressure::Warning, ResourceReason::RecentOom)
        } else if host_warning {
            (MemoryPressure::Warning, ResourceReason::HostMemory)
        } else if budget_warning {
            (MemoryPressure::Warning, ResourceReason::AgentBudget)
        } else if stalled {
            (MemoryPressure::Warning, ResourceReason::MemoryStall)
        } else {
            (MemoryPressure::Normal, ResourceReason::None)
        };
        // Pressure escalates immediately; recovery needs three healthy samples.
        if requested == MemoryPressure::Normal
            && matches!(
                self.pressure,
                MemoryPressure::Warning | MemoryPressure::Critical
            )
        {
            self.healthy_samples = self.healthy_samples.saturating_add(1);
            if self.healthy_samples >= 3 {
                self.pressure = requested;
            }
        } else {
            self.healthy_samples = 0;
            self.pressure = requested;
        }
        let startup = STARTUP_RESERVATION.min(raw.memory_total_bytes / 4);
        let host_headroom = (raw.memory_total_bytes / 10).max(512 * 1024 * 1024);
        let can_start = enforced
            && self.pressure != MemoryPressure::Critical
            && raw.memory_available_bytes >= host_headroom.saturating_add(startup)
            && allocated.saturating_add(startup) <= limit;
        let reason = if !can_start && reason == ResourceReason::None {
            ResourceReason::StartupReservation
        } else {
            reason
        };
        let result = HostResources {
            available: true,
            problem: None,
            sampled_at_ms: raw.sampled_at_ms,
            cpu_percent,
            cpu_cores: raw.cpu_cores,
            load_one: raw.load_average[0],
            load_five: raw.load_average[1],
            load_fifteen: raw.load_average[2],
            memory_total_bytes: raw.memory_total_bytes,
            memory_available_bytes: raw.memory_available_bytes,
            swap_total_bytes: raw.swap_total_bytes,
            swap_used_bytes: raw.swap_used_bytes,
            memory_pressure_some: raw.memory_pressure_some,
            memory_pressure_full: raw.memory_pressure_full,
            agent_memory_used_bytes: used,
            agent_reclaimable_bytes: raw.agent_reclaimable_bytes,
            agent_memory_limit_bytes: limit,
            agent_migration_reserved_bytes: raw.agent_reserved_bytes,
            agent_startup_reserved_bytes: startup_reserved,
            agent_kernel_limit_bytes: raw.agent_effective_limit_bytes,
            agent_cpu_percent,
            agent_process_count: raw.agent_process_count,
            agent_oom_kills: raw.agent_oom_kills,
            limit_enforced: enforced,
            memory_pressure: self.pressure,
            reason,
            can_start_agents: can_start,
            processes,
        };
        self.previous = Some(raw);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> RawResources {
        RawResources {
            sampled_at_ms: 1000,
            cpu_total_ticks: 100,
            cpu_idle_ticks: 50,
            cpu_cores: 4,
            load_average: [1.0, 0.5, 0.2],
            memory_total_bytes: 20 * GIB,
            memory_available_bytes: 10 * GIB,
            swap_total_bytes: 4 * GIB,
            swap_used_bytes: GIB,
            memory_pressure_some: 0.0,
            memory_pressure_full: 0.0,
            agent_current_bytes: GIB,
            agent_reclaimable_bytes: 0,
            agent_reserved_bytes: 0,
            agent_effective_limit_bytes: 10 * GIB,
            agent_cpu_usage_usec: 1_000_000,
            agent_process_count: 4,
            agent_oom_kills: 0,
            enforced: true,
            processes: vec![RawProcessResource {
                pid: 42,
                name: "codex".into(),
                started: "7".into(),
                cpu_ticks: 10,
                memory_bytes: GIB / 2,
                managed: true,
            }],
        }
    }

    #[test]
    fn cpu_uses_counter_deltas_and_does_not_invent_a_first_sample() {
        let mut tracker = ResourceTracker::default();
        let first = tracker.observe(raw(), 10 * GIB, 0, true);
        assert!(first.cpu_percent.is_none());
        assert!(first.processes[0].cpu_percent.is_none());
        let mut next = raw();
        next.sampled_at_ms = 2000;
        next.cpu_total_ticks = 140;
        next.cpu_idle_ticks = 60;
        next.agent_cpu_usage_usec = 3_000_000;
        next.processes[0].cpu_ticks = 14;
        let second = tracker.observe(next, 10 * GIB, 0, true);
        assert_eq!(second.cpu_percent, Some(75.0));
        assert_eq!(second.processes[0].cpu_percent, Some(10.0));
        assert_eq!(second.agent_cpu_percent, Some(50.0));
    }

    #[test]
    fn reclaimable_memory_is_available_and_host_pressure_blocks_new_starts() {
        let mut tracker = ResourceTracker::default();
        let healthy = tracker.observe(raw(), 10 * GIB, 0, true);
        assert_eq!(healthy.memory_pressure, MemoryPressure::Normal);
        assert!(healthy.can_start_agents);
        let mut low = raw();
        low.memory_available_bytes = 400 * 1024 * 1024;
        let critical = tracker.observe(low, 10 * GIB, 0, true);
        assert_eq!(critical.reason, ResourceReason::HostMemory);
        assert!(!critical.can_start_agents);
        for index in 0..3 {
            let profile = tracker.observe(raw(), 10 * GIB, 0, true);
            assert_eq!(profile.can_start_agents, index == 2);
        }
    }

    #[test]
    fn agent_budget_includes_migration_and_parallel_startup_reservations() {
        let mut tracker = ResourceTracker::default();
        let mut value = raw();
        value.agent_current_bytes = 5 * GIB;
        value.agent_reserved_bytes = 3 * GIB;
        value.agent_effective_limit_bytes = 7 * GIB;
        let warning = tracker.observe(value.clone(), 10 * GIB, 0, true);
        assert_eq!(warning.agent_memory_used_bytes, 8 * GIB);
        assert_eq!(warning.memory_pressure, MemoryPressure::Warning);
        assert!(warning.can_start_agents);
        let blocked = tracker.observe(value, 10 * GIB, GIB, true);
        assert_eq!(blocked.memory_pressure, MemoryPressure::Critical);
        assert!(!blocked.can_start_agents);
    }

    #[test]
    fn missing_or_looser_kernel_limits_never_claim_enforcement() {
        let mut tracker = ResourceTracker::default();
        for effective in [0, 11 * GIB] {
            let mut value = raw();
            value.agent_effective_limit_bytes = effective;
            let result = tracker.observe(value, 10 * GIB, 0, true);
            assert!(!result.limit_enforced);
            assert!(!result.can_start_agents);
        }
        let result = tracker.observe(raw(), 10 * GIB, 0, false);
        assert!(!result.limit_enforced);
    }

    #[test]
    fn pid_reuse_and_counter_resets_do_not_make_spurious_cpu_spikes() {
        let mut tracker = ResourceTracker::default();
        tracker.observe(raw(), 10 * GIB, 0, true);
        let mut next = raw();
        next.cpu_total_ticks = 150;
        next.processes[0].started = "new-process".into();
        next.processes[0].cpu_ticks = 999999;
        assert!(
            tracker.observe(next, 10 * GIB, 0, true).processes[0]
                .cpu_percent
                .is_none()
        );
        let mut reset = raw();
        reset.cpu_total_ticks = 0;
        assert!(
            tracker
                .observe(reset, 10 * GIB, 0, true)
                .cpu_percent
                .is_none()
        );
    }

    #[test]
    fn stalls_and_a_new_oom_are_visible_even_with_free_host_memory() {
        let mut tracker = ResourceTracker::default();
        tracker.observe(raw(), 10 * GIB, 0, true);
        let mut stalled = raw();
        stalled.memory_pressure_full = 12.0;
        assert!(!tracker.observe(stalled, 10 * GIB, 0, true).can_start_agents);
        let mut killed = raw();
        killed.agent_oom_kills = 1;
        assert_eq!(
            tracker.observe(killed, 10 * GIB, 0, true).reason,
            ResourceReason::RecentOom
        );
    }
}
