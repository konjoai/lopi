//! In-memory agent health monitoring.
//!
//! Every dispatcher (constellation router, pool worker) calls
//! [`HealthRegistry::heartbeat`] when an agent reports liveness, and
//! [`record_success`] / [`record_failure`] (with the call's latency) when
//! a request completes. A background sweeper runs every `sweeper_period`
//! and transitions agents based on time-since-last-beat:
//!
//! - `last_seen` within `heartbeat_interval * 2` → [`AgentHealth::Healthy`]
//! - within `* 5` → [`AgentHealth::Degraded`]
//! - beyond `* 5` → [`AgentHealth::Dead`]
//!
//! Health is intentionally **ephemeral** — there is no SQLite persistence.
//! Restarting `lopi sail` re-derives health from incoming heartbeats; the
//! audit log captures lifecycle transitions if durable history is needed.
//!
//! The registry is lock-free on the read path (`DashMap` per-entry +
//! atomic last-seen instant) so `/metrics` exposition over thousands of
//! agents stays cheap.
//!
//! [`HealthRegistry::heartbeat`]: HealthRegistry::heartbeat

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Coarse health label exposed via `GET /api/agents/:id/health`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentHealth {
    /// Heartbeat received within `heartbeat_interval * 2`.
    Healthy,
    /// Heartbeat stale — between `× 2` and `× 5` of the interval.
    Degraded,
    /// Heartbeat absent for more than `× 5` of the interval, or never seen.
    Dead,
}

impl AgentHealth {
    /// Stable wire label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Dead => "dead",
        }
    }
}

/// Bounded ring buffer used to track recent latency samples.
const LATENCY_WINDOW: usize = 64;
/// Rolling failure-rate window length in seconds — last hour.
const ERROR_WINDOW_SECS: u64 = 3600;

#[derive(Debug)]
struct HealthEntry {
    /// Monotonic wall-clock anchor — atomic so the sweeper can read without
    /// taking a lock.
    last_seen: AtomicU64,
    /// Sticky cached classification — sweeper updates this; readers just
    /// load it for the cheap path.
    status: std::sync::Mutex<AgentHealth>,
    /// Latency samples + recent failure timestamps. Behind one async lock
    /// because heartbeats are infrequent; reads tolerate the wait.
    inner: RwLock<HealthInner>,
}

#[derive(Debug, Default)]
struct HealthInner {
    /// Rolling latency window (milliseconds).
    latencies_ms: VecDeque<u64>,
    /// Timestamps (epoch secs) of failures in the last hour.
    recent_failures: VecDeque<u64>,
    /// Timestamps (epoch secs) of successes in the last hour.
    recent_successes: VecDeque<u64>,
    /// Cumulative consecutive failure count — resets on success.
    consecutive_failures: u32,
    /// Wall-clock `DateTime<Utc>` of the last heartbeat — for human-readable
    /// `last_seen` in the API response. `Instant` drives the sweeper math;
    /// this just carries the printable label.
    last_seen_wall: Option<DateTime<Utc>>,
}

/// Public health snapshot returned by `GET /api/agents/:id/health`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSnapshot {
    pub agent_id: String,
    pub status: AgentHealth,
    pub last_seen: Option<DateTime<Utc>>,
    pub error_rate_1h: f32,
    pub avg_latency_ms: f32,
    pub consecutive_failures: u32,
    /// Number of latency samples backing `avg_latency_ms`.
    pub samples: u32,
}

/// Fleet-wide rollup for `GET /api/agents/health/summary`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub total: u32,
    pub healthy: u32,
    pub degraded: u32,
    pub dead: u32,
}

/// Configuration for the sweeper + classification thresholds.
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Expected interval between heartbeats from a healthy agent.
    pub heartbeat_interval: Duration,
    /// How often the sweeper runs.
    pub sweeper_period: Duration,
    /// Multiplier on `heartbeat_interval` — staler than this is `Degraded`.
    pub degraded_after: u32,
    /// Multiplier on `heartbeat_interval` — staler than this is `Dead`.
    pub dead_after: u32,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(30),
            sweeper_period: Duration::from_secs(10),
            degraded_after: 2,
            dead_after: 5,
        }
    }
}

/// Registry of per-agent health. Cheap to clone — backed by `Arc<DashMap>`.
#[derive(Clone)]
pub struct HealthRegistry {
    inner: Arc<DashMap<String, Arc<HealthEntry>>>,
    config: HealthConfig,
    /// Anchor for `Instant` math — `last_seen` is stored as milliseconds
    /// since this anchor so an `AtomicU64` can carry it lock-free.
    epoch: Arc<Instant>,
}

impl HealthRegistry {
    /// Build a registry with the given config. No sweeper task is started
    /// automatically — call [`spawn_sweeper`] when ready.
    #[must_use]
    pub fn new(config: HealthConfig) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            config,
            epoch: Arc::new(Instant::now()),
        }
    }

    /// Snapshot the active configuration.
    #[must_use]
    pub fn config(&self) -> &HealthConfig {
        &self.config
    }

    /// Record a heartbeat from `agent_id`. Creates the entry on first
    /// contact. Immediately marks the agent `Healthy`.
    pub async fn heartbeat(&self, agent_id: &str) {
        let entry = self.entry(agent_id);
        let ms = self.now_ms();
        entry.last_seen.store(ms, Ordering::Relaxed);
        *entry
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = AgentHealth::Healthy;
        let mut inner = entry.inner.write().await;
        inner.last_seen_wall = Some(Utc::now());
    }

    /// Record a successful call. Updates the latency window and resets the
    /// consecutive-failure counter. Implies a heartbeat.
    pub async fn record_success(&self, agent_id: &str, latency: Duration) {
        let entry = self.entry(agent_id);
        let ms = self.now_ms();
        entry.last_seen.store(ms, Ordering::Relaxed);
        *entry
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = AgentHealth::Healthy;
        let mut inner = entry.inner.write().await;
        let lat_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);
        push_bounded(&mut inner.latencies_ms, lat_ms);
        inner.consecutive_failures = 0;
        let now = epoch_secs();
        push_recent(&mut inner.recent_successes, now);
        prune_window(&mut inner.recent_failures, now);
        prune_window(&mut inner.recent_successes, now);
        inner.last_seen_wall = Some(Utc::now());
    }

    /// Record a failed call. Implies a heartbeat (the agent *did* report
    /// back, just with bad news).
    pub async fn record_failure(&self, agent_id: &str) {
        let entry = self.entry(agent_id);
        let ms = self.now_ms();
        entry.last_seen.store(ms, Ordering::Relaxed);
        let mut inner = entry.inner.write().await;
        inner.consecutive_failures = inner.consecutive_failures.saturating_add(1);
        let now = epoch_secs();
        push_recent(&mut inner.recent_failures, now);
        prune_window(&mut inner.recent_failures, now);
        prune_window(&mut inner.recent_successes, now);
        inner.last_seen_wall = Some(Utc::now());
    }

    /// Build the public snapshot for `agent_id`, or `None` if no heartbeat
    /// has ever been recorded.
    pub async fn snapshot(&self, agent_id: &str) -> Option<HealthSnapshot> {
        let entry = self.inner.get(agent_id)?.clone();
        let status = *entry
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let inner = entry.inner.read().await;
        let total = u32::try_from(inner.recent_failures.len() + inner.recent_successes.len())
            .unwrap_or(u32::MAX);
        let failures = u32::try_from(inner.recent_failures.len()).unwrap_or(u32::MAX);
        let error_rate_1h = if total == 0 {
            0.0
        } else {
            f32::from(u16::try_from(failures.min(u32::from(u16::MAX))).unwrap_or(u16::MAX))
                / f32::from(u16::try_from(total.min(u32::from(u16::MAX))).unwrap_or(u16::MAX))
        };
        let avg_latency_ms = if inner.latencies_ms.is_empty() {
            0.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let sum: u64 = inner.latencies_ms.iter().copied().sum();
            (sum as f32) / (inner.latencies_ms.len() as f32)
        };
        Some(HealthSnapshot {
            agent_id: agent_id.to_string(),
            status,
            last_seen: inner.last_seen_wall,
            error_rate_1h,
            avg_latency_ms,
            consecutive_failures: inner.consecutive_failures,
            samples: u32::try_from(inner.latencies_ms.len()).unwrap_or(u32::MAX),
        })
    }

    /// Rollup of every registered agent's current status.
    pub fn summary(&self) -> HealthSummary {
        let mut s = HealthSummary {
            total: 0,
            healthy: 0,
            degraded: 0,
            dead: 0,
        };
        for entry in self.inner.iter() {
            s.total += 1;
            let st = *entry
                .value()
                .status
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match st {
                AgentHealth::Healthy => s.healthy += 1,
                AgentHealth::Degraded => s.degraded += 1,
                AgentHealth::Dead => s.dead += 1,
            }
        }
        s
    }

    /// Run one classification pass — exposed for tests so the sweeper
    /// loop is not a black box. Production code calls `spawn_sweeper`.
    pub fn sweep_once(&self) {
        let now = self.now_ms();
        let interval_ms =
            u64::try_from(self.config.heartbeat_interval.as_millis()).unwrap_or(u64::MAX);
        let degraded_threshold_ms = interval_ms.saturating_mul(u64::from(self.config.degraded_after));
        let dead_threshold_ms = interval_ms.saturating_mul(u64::from(self.config.dead_after));
        for entry in self.inner.iter() {
            let last = entry.value().last_seen.load(Ordering::Relaxed);
            let age_ms = now.saturating_sub(last);
            let next = if age_ms >= dead_threshold_ms {
                AgentHealth::Dead
            } else if age_ms >= degraded_threshold_ms {
                AgentHealth::Degraded
            } else {
                AgentHealth::Healthy
            };
            *entry
                .value()
                .status
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
        }
    }

    /// Spawn a long-running sweeper that runs `sweep_once` every
    /// `config.sweeper_period`. The handle is returned so the caller can
    /// `.abort()` it on shutdown.
    pub fn spawn_sweeper(&self) -> tokio::task::JoinHandle<()> {
        let me = self.clone();
        let period = me.config.sweeper_period;
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(period);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                me.sweep_once();
            }
        })
    }

    fn entry(&self, agent_id: &str) -> Arc<HealthEntry> {
        if let Some(existing) = self.inner.get(agent_id) {
            return existing.clone();
        }
        let new = Arc::new(HealthEntry {
            last_seen: AtomicU64::new(0),
            status: std::sync::Mutex::new(AgentHealth::Dead),
            inner: RwLock::new(HealthInner::default()),
        });
        self.inner.insert(agent_id.to_string(), new.clone());
        new
    }

    fn now_ms(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

fn epoch_secs() -> u64 {
    use std::time::SystemTime;
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn push_bounded(buf: &mut VecDeque<u64>, v: u64) {
    if buf.len() >= LATENCY_WINDOW {
        buf.pop_front();
    }
    buf.push_back(v);
}

fn push_recent(buf: &mut VecDeque<u64>, ts: u64) {
    buf.push_back(ts);
}

fn prune_window(buf: &mut VecDeque<u64>, now: u64) {
    let cutoff = now.saturating_sub(ERROR_WINDOW_SECS);
    while buf.front().is_some_and(|&t| t < cutoff) {
        buf.pop_front();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn fast_config() -> HealthConfig {
        HealthConfig {
            heartbeat_interval: Duration::from_millis(50),
            sweeper_period: Duration::from_millis(20),
            degraded_after: 2,
            dead_after: 5,
        }
    }

    #[tokio::test]
    async fn unknown_agent_returns_none() {
        let r = HealthRegistry::new(fast_config());
        assert!(r.snapshot("nope").await.is_none());
    }

    #[tokio::test]
    async fn heartbeat_marks_agent_healthy() {
        let r = HealthRegistry::new(fast_config());
        r.heartbeat("alpha").await;
        let s = r.snapshot("alpha").await.unwrap();
        assert_eq!(s.status, AgentHealth::Healthy);
        assert!(s.last_seen.is_some());
        assert_eq!(s.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn sweep_transitions_through_degraded_then_dead() {
        let r = HealthRegistry::new(fast_config());
        r.heartbeat("alpha").await;
        // Just after heartbeat → still Healthy.
        r.sweep_once();
        assert_eq!(r.snapshot("alpha").await.unwrap().status, AgentHealth::Healthy);
        // After 2× interval (>= 100ms) → Degraded.
        tokio::time::sleep(Duration::from_millis(110)).await;
        r.sweep_once();
        assert_eq!(r.snapshot("alpha").await.unwrap().status, AgentHealth::Degraded);
        // After 5× interval (>= 250ms total) → Dead.
        tokio::time::sleep(Duration::from_millis(160)).await;
        r.sweep_once();
        assert_eq!(r.snapshot("alpha").await.unwrap().status, AgentHealth::Dead);
    }

    #[tokio::test]
    async fn record_success_resets_failures_and_tracks_latency() {
        let r = HealthRegistry::new(fast_config());
        r.record_failure("alpha").await;
        r.record_failure("alpha").await;
        let s = r.snapshot("alpha").await.unwrap();
        assert_eq!(s.consecutive_failures, 2);
        r.record_success("alpha", Duration::from_millis(120)).await;
        r.record_success("alpha", Duration::from_millis(80)).await;
        let s = r.snapshot("alpha").await.unwrap();
        assert_eq!(s.consecutive_failures, 0);
        assert_eq!(s.samples, 2);
        // Avg ≈ 100ms.
        assert!((s.avg_latency_ms - 100.0).abs() < 1.0);
    }

    #[tokio::test]
    async fn error_rate_reflects_recent_window() {
        let r = HealthRegistry::new(fast_config());
        r.record_success("alpha", Duration::from_millis(10)).await;
        r.record_success("alpha", Duration::from_millis(10)).await;
        r.record_success("alpha", Duration::from_millis(10)).await;
        r.record_failure("alpha").await;
        let s = r.snapshot("alpha").await.unwrap();
        // 1 fail / 4 total = 0.25
        assert!((s.error_rate_1h - 0.25).abs() < 0.01);
    }

    #[tokio::test]
    async fn summary_counts_every_status() {
        let r = HealthRegistry::new(fast_config());
        r.heartbeat("healthy-1").await;
        r.heartbeat("healthy-2").await;
        r.heartbeat("dies").await;
        // Force the third agent's last_seen far into the past.
        tokio::time::sleep(Duration::from_millis(300)).await;
        // Refresh the first two with a fresh heartbeat so they stay Healthy
        // through the sweep.
        r.heartbeat("healthy-1").await;
        r.heartbeat("healthy-2").await;
        r.sweep_once();
        let sum = r.summary();
        assert_eq!(sum.total, 3);
        assert_eq!(sum.healthy, 2);
        assert_eq!(sum.dead, 1);
    }

    #[tokio::test]
    async fn sweeper_runs_periodically() {
        let r = HealthRegistry::new(fast_config());
        r.heartbeat("alpha").await;
        let handle = r.spawn_sweeper();
        // Wait long enough for ≥ 5× heartbeat_interval to elapse so the
        // sweeper has classified the agent as Dead.
        tokio::time::sleep(Duration::from_millis(350)).await;
        assert_eq!(r.snapshot("alpha").await.unwrap().status, AgentHealth::Dead);
        handle.abort();
    }
}
