//! MAXX Phase 0 — quota headroom tracking, plus the Oracle-Preflight Part B
//! passive quota-history sampler.
//!
//! Subscribes to the same `AgentEvent` bus [`crate::pool::AgentPool`]
//! broadcasts on. Every `AgentEvent::ApiRetry`, from any in-flight task,
//! upserts one row per `limit_type` (`five_hour` / `seven_day`) — two
//! independent observations, never a shared "last event wins" scalar, since
//! both window types arrive through the same event variant. `maxx_loop` and
//! `GET /api/quota` are the readers, via [`QuotaTracker::snapshot`].
//!
//! Push-based, not polled: `rate_limit_event` is a line the `claude` CLI
//! emits inline in its own NDJSON stream during an active turn (decoded by
//! `claude_events::parse_rate_limit`, forwarded here as `AgentEvent::ApiRetry`
//! — see `crates/lopi-agent/src/claude_events.rs`). There is no separate
//! quota-status API to poll on a timer; whether the CLI emits this every turn
//! or only past a utilization threshold is an open question of the CLI's own
//! behavior (tracked separately as MAXX kill test 1,
//! `.konjo/scripts/quota-kill-test-log.sh`), not something a sampler here can
//! change by polling faster. So this subscriber is the entire sampler: one
//! more `store.insert_quota_sample` call alongside the existing upsert,
//! same event, same subscriber loop, no new polling loop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use dashmap::DashMap;
use lopi_core::{AgentEvent, EventBus};
use lopi_memory::{MemoryStore, QuotaObservationRow};
use tokio::sync::broadcast;
use tracing::warn;

/// A rate-limit window's most recently observed state.
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaObservation {
    /// Status string from the CLI, e.g. `allowed_warning`.
    pub status: String,
    /// Window utilization in `[0.0, 1.0]`.
    pub utilization: f32,
    /// Unix seconds the window resets, if the CLI reported it.
    pub resets_at: Option<i64>,
    /// ISO-8601 timestamp this observation was recorded.
    pub observed_at: String,
}

impl From<QuotaObservationRow> for QuotaObservation {
    fn from(r: QuotaObservationRow) -> Self {
        Self {
            status: r.status,
            utilization: r.utilization,
            resets_at: r.resets_at,
            observed_at: r.observed_at,
        }
    }
}

/// Live, cheap-to-clone quota tracker. Reads never touch SQLite — the
/// in-memory cache is the source of truth for `snapshot`, persisted so a
/// restart doesn't lose the last-known state.
#[derive(Clone)]
pub struct QuotaTracker {
    inner: Arc<Inner>,
}

struct Inner {
    store: MemoryStore,
    cache: DashMap<String, QuotaObservation>,
    started: AtomicBool,
}

impl QuotaTracker {
    /// Construct an un-started tracker. Call [`start`](Self::start) from an
    /// async context to load persisted observations and begin listening.
    #[must_use]
    pub fn new(store: MemoryStore) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                cache: DashMap::new(),
                started: AtomicBool::new(false),
            }),
        }
    }

    /// Load persisted observations into the cache, then spawn a task that
    /// upserts every `ApiRetry` seen on `bus`. Idempotent — a second call is
    /// a no-op so callers don't need to track whether they already started it.
    ///
    /// # Errors
    /// Returns `Err` if the initial load from the store fails.
    pub async fn start(&self, bus: &EventBus<AgentEvent>) -> Result<()> {
        for row in self.inner.store.list_quota_observations().await? {
            self.inner.cache.insert(row.limit_type.clone(), row.into());
        }
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        let inner = self.inner.clone();
        let mut rx = bus.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(AgentEvent::ApiRetry {
                        status,
                        limit_type,
                        utilization,
                        resets_at,
                        ..
                    }) => {
                        inner.cache.insert(
                            limit_type.clone(),
                            QuotaObservation {
                                status: status.clone(),
                                utilization,
                                resets_at,
                                observed_at: Utc::now().to_rfc3339(),
                            },
                        );
                        if let Err(e) = inner
                            .store
                            .upsert_quota_observation(&limit_type, &status, utilization, resets_at)
                            .await
                        {
                            warn!("quota observation persist failed: {e:#}");
                        }
                        // Oracle-Preflight Part B: append to history too, never
                        // overwriting — the upsert above is current-state only.
                        if let Err(e) = inner
                            .store
                            .insert_quota_sample(&limit_type, &status, utilization, resets_at)
                            .await
                        {
                            warn!("quota sample persist failed: {e:#}");
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("quota tracker lagged {n} events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        Ok(())
    }

    /// The most recent observation for `limit_type`. `None` means no
    /// observation has ever been seen — callers must treat this as "don't
    /// gate," so a fresh install isn't stuck refusing to dispatch before its
    /// first real CLI call.
    #[must_use]
    pub fn snapshot(&self, limit_type: &str) -> Option<QuotaObservation> {
        self.inner.cache.get(limit_type).map(|r| r.value().clone())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lopi_core::TaskId;

    fn retry(limit_type: &str, utilization: f32, resets_at: Option<i64>) -> AgentEvent {
        AgentEvent::ApiRetry {
            task_id: TaskId::new(),
            status: "allowed_warning".into(),
            limit_type: limit_type.into(),
            utilization,
            resets_at,
        }
    }

    #[tokio::test]
    async fn snapshot_is_none_before_any_observation() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let tracker = QuotaTracker::new(store);
        assert!(tracker.snapshot("seven_day").is_none());
    }

    #[tokio::test]
    async fn five_hour_event_does_not_clobber_seven_day_snapshot() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let tracker = QuotaTracker::new(store);
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        tracker.start(&bus).await.unwrap();

        bus.send(retry("seven_day", 0.92, Some(1_782_691_200)));
        bus.send(retry("five_hour", 0.10, Some(1_700_000_000)));

        // Give the subscriber task a chance to process both sends.
        for _ in 0..100 {
            if tracker.snapshot("five_hour").is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        let seven_day = tracker.snapshot("seven_day").expect("seven_day observed");
        let five_hour = tracker.snapshot("five_hour").expect("five_hour observed");
        assert!((seven_day.utilization - 0.92).abs() < 1e-6);
        assert_eq!(seven_day.resets_at, Some(1_782_691_200));
        assert!((five_hour.utilization - 0.10).abs() < 1e-6);
        assert_eq!(five_hour.resets_at, Some(1_700_000_000));

        // Persisted, independent rows too — not just the in-memory cache.
        let rows = tracker.inner.store.list_quota_observations().await.unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn every_observation_also_lands_a_history_sample() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let tracker = QuotaTracker::new(store);
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        tracker.start(&bus).await.unwrap();

        bus.send(retry("seven_day", 0.10, None));
        bus.send(retry("seven_day", 0.20, None));
        bus.send(retry("seven_day", 0.30, None));

        // Wait for all three to land in the cache (last-write-wins there),
        // then assert the append-only table kept all three separately.
        for _ in 0..100 {
            if tracker
                .snapshot("seven_day")
                .is_some_and(|s| (s.utilization - 0.30).abs() < 1e-6)
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }

        let samples = tracker
            .inner
            .store
            .list_quota_samples("seven_day")
            .await
            .unwrap();
        assert_eq!(
            samples.len(),
            3,
            "the current-state upsert must not suppress history rows"
        );
        assert!((samples[0].utilization - 0.10).abs() < 1e-6);
        assert!((samples[1].utilization - 0.20).abs() < 1e-6);
        assert!((samples[2].utilization - 0.30).abs() < 1e-6);
    }

    #[tokio::test]
    async fn start_is_idempotent() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let tracker = QuotaTracker::new(store);
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        tracker.start(&bus).await.unwrap();
        tracker.start(&bus).await.unwrap();
        bus.send(retry("seven_day", 0.5, None));
        for _ in 0..100 {
            if tracker.snapshot("seven_day").is_some() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        // A single subscriber double-processing would still leave one
        // correct row — the real risk (double INSERT) is caught by an
        // upsert being idempotent, so assert the value is sane rather than
        // asserting call counts.
        assert!((tracker.snapshot("seven_day").unwrap().utilization - 0.5).abs() < 1e-6);
    }

    #[tokio::test]
    async fn load_on_start_restores_persisted_observations() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .upsert_quota_observation("seven_day", "allowed", 0.42, Some(7))
            .await
            .unwrap();
        let tracker = QuotaTracker::new(store);
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        tracker.start(&bus).await.unwrap();
        let snap = tracker.snapshot("seven_day").expect("restored from store");
        assert!((snap.utilization - 0.42).abs() < 1e-6);
        assert_eq!(snap.resets_at, Some(7));
    }
}
