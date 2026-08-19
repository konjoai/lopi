//! Unit tests for `maxx_loop.rs` — split out to keep the tick module under
//! the 500-line file gate. Included via `#[path]` from `maxx_loop.rs` so
//! `super::*` still resolves to the tick module's items.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::chain_schedule_manager::ChainScheduleManager;
use crate::queue::TaskQueue;
use lopi_core::{AgentEvent, EventBus};
use lopi_memory::{ChainStepInput, ScheduleChainInput};

fn spec(id: &str) -> MaxxSpec {
    MaxxSpec {
        id: id.into(),
        goal: "work the backlog".into(),
        repo: None,
        priority: "low".into(),
        allowed_dirs: vec![],
        forbidden_dirs: vec![],
        autonomy_level: AutonomyLevel::default(),
        quiet_hours: None,
        headroom_gate: false,
        windows: vec![],
        chain_id: None,
    }
}

// ── quiet_hours_favorable ────────────────────────────────────────────

#[test]
fn quiet_hours_none_is_never_favorable() {
    assert!(!quiet_hours_favorable(None, 2));
}

#[test]
fn quiet_hours_wraps_past_midnight() {
    let qh = Some((23, 7));
    assert!(quiet_hours_favorable(qh, 23), "11PM is in range");
    assert!(quiet_hours_favorable(qh, 0), "midnight is in range");
    assert!(quiet_hours_favorable(qh, 6), "6AM is in range");
    assert!(!quiet_hours_favorable(qh, 7), "7AM is the exclusive end");
    assert!(!quiet_hours_favorable(qh, 12), "noon is not in range");
}

#[test]
fn quiet_hours_non_wrapping_range() {
    let qh = Some((1, 5));
    assert!(quiet_hours_favorable(qh, 1));
    assert!(quiet_hours_favorable(qh, 4));
    assert!(!quiet_hours_favorable(qh, 5), "exclusive end");
    assert!(!quiet_hours_favorable(qh, 0));
}

#[test]
fn quiet_hours_degenerate_equal_bounds_never_favorable() {
    assert!(!quiet_hours_favorable(Some((5, 5)), 5));
}

// ── window_favorable / headroom_favorable — pre-registered numbers ─────
// Favorable requires utilization <= 0.5 AND 0 < time-to-reset <= 2h.
// These are exercised against a simulated resets_at timeline, not real
// wall-clock time, per the sprint's kill-test verify requirement.

fn obs(utilization: f32, resets_at: Option<i64>) -> crate::quota_tracker::QuotaObservation {
    crate::quota_tracker::QuotaObservation {
        status: "allowed".into(),
        utilization,
        resets_at,
        observed_at: "2026-01-01T00:00:00Z".into(),
    }
}

#[test]
fn window_favorable_high_headroom_near_reset() {
    let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    // Resets in exactly 1 hour, 30% used — comfortably inside both bounds.
    let o = obs(0.30, Some(now.timestamp() + 3600));
    assert!(window_favorable(Some(&o), now));
}

#[test]
fn window_favorable_rejects_high_utilization() {
    let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let o = obs(0.51, Some(now.timestamp() + 3600));
    assert!(
        !window_favorable(Some(&o), now),
        "just above the 0.5 ceiling"
    );
}

#[test]
fn window_favorable_boundary_utilization_is_favorable() {
    let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let o = obs(0.50, Some(now.timestamp() + 3600));
    assert!(
        window_favorable(Some(&o), now),
        "0.5 exactly is still favorable"
    );
}

#[test]
fn window_favorable_rejects_reset_too_far_out() {
    let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    // Resets in 2h1m — one second past the 2-hour "nearing reset" window.
    let o = obs(0.10, Some(now.timestamp() + 7201));
    assert!(!window_favorable(Some(&o), now));
}

#[test]
fn window_favorable_rejects_already_passed_reset() {
    let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    // resets_at in the past — stale/expired data, treat as unknown.
    let o = obs(0.10, Some(now.timestamp() - 60));
    assert!(!window_favorable(Some(&o), now));
}

#[test]
fn window_favorable_rejects_missing_resets_at() {
    let now = Utc::now();
    let o = obs(0.10, None);
    assert!(!window_favorable(Some(&o), now));
}

#[test]
fn window_favorable_rejects_no_observation() {
    assert!(!window_favorable(None, Utc::now()));
}

async fn tracker_with(observations: &[(&str, f32, Option<i64>)]) -> QuotaTracker {
    let store = MemoryStore::open_in_memory().await.unwrap();
    for (limit_type, util, resets_at) in observations {
        store
            .upsert_quota_observation(limit_type, "allowed", *util, *resets_at)
            .await
            .unwrap();
    }
    let tracker = QuotaTracker::new(store);
    let bus: EventBus<AgentEvent> = EventBus::new(4);
    tracker.start(&bus).await.unwrap();
    tracker
}

#[tokio::test]
async fn headroom_favorable_requires_every_configured_window() {
    let now = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let near = now.timestamp() + 3600;
    // five_hour is favorable, seven_day is not (utilization too high) —
    // AND semantics mean the whole gate must be unfavorable.
    let tracker = tracker_with(&[
        ("five_hour", 0.2, Some(near)),
        ("seven_day", 0.9, Some(near)),
    ])
    .await;
    let windows = vec![LimitWindow::FiveHour, LimitWindow::SevenDay];
    assert!(!headroom_favorable(&windows, true, &tracker, now));

    let tracker2 = tracker_with(&[
        ("five_hour", 0.2, Some(near)),
        ("seven_day", 0.3, Some(near)),
    ])
    .await;
    assert!(headroom_favorable(&windows, true, &tracker2, now));
}

#[tokio::test]
async fn headroom_favorable_false_when_gate_off_or_windows_empty() {
    let now = Utc::now();
    let tracker = tracker_with(&[("five_hour", 0.1, Some(now.timestamp() + 60))]).await;
    assert!(!headroom_favorable(
        &[LimitWindow::FiveHour],
        false,
        &tracker,
        now
    ));
    assert!(!headroom_favorable(&[], true, &tracker, now));
}

#[tokio::test]
async fn is_favorable_ors_quiet_hours_and_headroom() {
    let now = DateTime::parse_from_rfc3339("2026-01-01T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let tracker = QuotaTracker::new(MemoryStore::open_in_memory().await.unwrap());
    let bus: EventBus<AgentEvent> = EventBus::new(4);
    tracker.start(&bus).await.unwrap();

    let mut s = spec("e1");
    s.quiet_hours = Some((23, 7));
    // Noon local hour — outside quiet hours, no quota observation either.
    assert!(!is_favorable(&s, &tracker, now, 12));
    // Inside quiet hours — favorable regardless of quota.
    assert!(is_favorable(&s, &tracker, now, 23));
}

// ── tick / cooldown ──────────────────────────────────────────────────

async fn pool_with_store() -> (AgentPool, MemoryStore, ChainScheduleManager) {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let queue = TaskQueue::new();
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let pool = AgentPool::new(1, PathBuf::from("."), queue, bus).with_store(store.clone());
    // `run_now` (the only thing `fire_chain` calls) doesn't touch the cron
    // `JobScheduler` at all — see `chain_schedule_manager_tests.rs`'s own
    // `run_now_submits_only_step_zero`, which also skips `.start()`.
    let chains = ChainScheduleManager::new(pool.clone(), store.clone());
    (pool, store, chains)
}

fn maxx_input(name: &str, quiet_hours: Option<(u8, u8)>) -> lopi_memory::MaxxInput {
    lopi_memory::MaxxInput {
        id: None,
        name: name.into(),
        goal: format!("goal for {name}"),
        repo: None,
        priority: "normal".into(),
        allowed_dirs: vec![],
        forbidden_dirs: vec![],
        enabled: true,
        autonomy_level: "draft_pr".into(),
        report: None,
        quiet_hours_start: quiet_hours.map(|(s, _)| s),
        quiet_hours_end: quiet_hours.map(|(_, e)| e),
        headroom_gate: false,
        windows: vec![],
        chain_id: None,
    }
}

/// A minimal single-step chain, so a chain-scoped MAXX entry has something
/// real to fire — `ChainScheduleManager::run_now` returns `Ok(None)`
/// (no-op) for a chain with zero steps, which would make a chain-fire test
/// indistinguishable from a broken one.
fn chain_input() -> ScheduleChainInput {
    ScheduleChainInput {
        id: None,
        name: "maxx-chain-test".into(),
        cron: "0 2 * * *".into(),
        repo: None,
        priority: "normal".into(),
        autonomy_level: "draft_pr".into(),
        on_fail: "stop".into(),
        enabled: false,
        steps: vec![ChainStepInput {
            goal: "chain step one".into(),
            allowed_dirs: vec![],
            forbidden_dirs: vec![],
        }],
    }
}

#[tokio::test]
async fn tick_fires_only_favorable_enabled_entries() {
    let (pool, store, chains) = pool_with_store().await;
    let quota = QuotaTracker::new(store.clone());
    let bus: EventBus<AgentEvent> = EventBus::new(4);
    quota.start(&bus).await.unwrap();

    // Favorable — quiet hours span every hour of the day.
    store
        .upsert_maxx_entry(&maxx_input("always-on", Some((0, 23))))
        .await
        .unwrap();
    // Never favorable — no quiet hours, headroom gate off.
    store
        .upsert_maxx_entry(&maxx_input("never-on", None))
        .await
        .unwrap();
    // Favorable conditions, but disabled — must not fire.
    let mut disabled = maxx_input("disabled-but-favorable", Some((0, 23)));
    disabled.enabled = false;
    store.upsert_maxx_entry(&disabled).await.unwrap();

    let tick = MaxxLoop::new(store.clone(), quota, pool, chains);
    let fired = tick.tick().await.unwrap();
    assert_eq!(fired, 1, "only the always-on entry should fire");
}

#[tokio::test]
async fn tick_respects_cooldown_on_repeat_ticks() {
    let (pool, store, chains) = pool_with_store().await;
    let quota = QuotaTracker::new(store.clone());
    let bus: EventBus<AgentEvent> = EventBus::new(4);
    quota.start(&bus).await.unwrap();
    store
        .upsert_maxx_entry(&maxx_input("always-on", Some((0, 23))))
        .await
        .unwrap();

    let tick = MaxxLoop::new(store, quota, pool, chains);
    assert_eq!(tick.tick().await.unwrap(), 1, "first tick fires");
    assert_eq!(
        tick.tick().await.unwrap(),
        0,
        "immediate second tick is within the cooldown window"
    );
}

#[tokio::test]
async fn in_cooldown_false_once_interval_elapsed() {
    let (pool, store, chains) = pool_with_store().await;
    let entry = store
        .upsert_maxx_entry(&maxx_input("e", None))
        .await
        .unwrap();
    store
        .record_maxx_run(&entry.id, Some("t1"), "queued")
        .await
        .unwrap();
    let quota = QuotaTracker::new(store.clone());
    let tick = MaxxLoop::new(store, quota, pool, chains);

    let just_now = Utc::now();
    assert!(tick.in_cooldown(&entry.id, just_now).await, "just fired");

    let well_past = just_now + chrono::Duration::seconds(MIN_REFIRE_INTERVAL_SECS + 1);
    assert!(
        !tick.in_cooldown(&entry.id, well_past).await,
        "cooldown elapsed"
    );
}

// ── chain-scoped MAXX (Stack-MAXX-1) ───────────────────────────────────

#[tokio::test]
async fn tick_fires_favorable_chain_scoped_entry_via_run_now() {
    let (pool, store, chains) = pool_with_store().await;
    let quota = QuotaTracker::new(store.clone());
    let bus: EventBus<AgentEvent> = EventBus::new(4);
    quota.start(&bus).await.unwrap();

    let chain = store.upsert_schedule_chain(&chain_input()).await.unwrap();
    let mut entry = maxx_input("chain-fires", Some((0, 23)));
    entry.chain_id = Some(chain.id.clone());
    let row = store.upsert_maxx_entry(&entry).await.unwrap();

    let tick = MaxxLoop::new(store.clone(), quota, pool, chains);
    assert_eq!(
        tick.tick().await.unwrap(),
        1,
        "the chain-scoped entry fires"
    );

    let runs = store.list_maxx_runs(&row.id, 1).await.unwrap();
    assert_eq!(runs[0].outcome, "queued");
    // Firing via `run_now` created a real `schedule_chain_runs` row —
    // `task_id` on the maxx_runs row holds that run's id, not a task id.
    let chain_run = store
        .get_chain_run(runs[0].task_id.as_ref().unwrap())
        .await
        .unwrap();
    assert!(chain_run.is_some(), "run_now must have started a chain run");
}

#[tokio::test]
async fn tick_records_error_when_chain_id_does_not_resolve() {
    let (pool, store, chains) = pool_with_store().await;
    let quota = QuotaTracker::new(store.clone());
    let bus: EventBus<AgentEvent> = EventBus::new(4);
    quota.start(&bus).await.unwrap();

    let mut entry = maxx_input("dangling-chain", Some((0, 23)));
    entry.chain_id = Some("does-not-exist".into());
    let row = store.upsert_maxx_entry(&entry).await.unwrap();

    let tick = MaxxLoop::new(store.clone(), quota, pool, chains);
    // `is_favorable` still counts this as "fired" (a fire attempt was made)
    // even though the dispatch itself failed — the failure is recorded as
    // an "error" run outcome, not silently dropped or panicked on.
    assert_eq!(tick.tick().await.unwrap(), 1);
    let runs = store.list_maxx_runs(&row.id, 1).await.unwrap();
    assert_eq!(runs[0].outcome, "error");
    assert!(runs[0].task_id.is_none());
}
