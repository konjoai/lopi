#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_in_result
)]
mod common;

use lopi_context::{
    ContextError, ContextWindow, EvictionReason, Phase, PinPolicy, Role, TaggedMessage,
};

fn msg_tokens(text: &str, tokens: usize, pin: PinPolicy) -> TaggedMessage {
    common::make_msg(Role::User, text, Phase::Implementation, pin, tokens)
}

/// With budget=200 and threshold=0.75, auto-eviction fires when pressure > 75%.
/// Pushing m1..m4 (40 tokens each) leaves us at 160/200 = 80%.
/// Pushing m5 triggers auto-eviction of m1 (oldest `BudgetEvictable`) first.
#[test]
fn oldest_turns_evicted_first_under_budget_pressure() {
    let mut window = ContextWindow::new(200);

    let id1 = window
        .push(msg_tokens("m1", 40, PinPolicy::BudgetEvictable))
        .unwrap();
    let id2 = window
        .push(msg_tokens("m2", 40, PinPolicy::BudgetEvictable))
        .unwrap();
    window
        .push(msg_tokens("m3", 40, PinPolicy::BudgetEvictable))
        .unwrap();
    window
        .push(msg_tokens("m4", 40, PinPolicy::BudgetEvictable))
        .unwrap();

    // pressure = 160/200 = 0.80 > 0.75: next push auto-evicts oldest turns.
    window
        .push(msg_tokens("m5", 40, PinPolicy::BudgetEvictable))
        .unwrap();

    let remaining: Vec<_> = window.turns().iter().map(|t| t.id).collect();
    assert!(
        !remaining.contains(&id1) || !remaining.contains(&id2),
        "at least one of the oldest turns must have been evicted"
    );
}

#[test]
fn always_pinned_turns_never_evicted_by_budget() {
    let mut window = ContextWindow::new(200);

    let pinned_id = window
        .push(msg_tokens("pinned system", 100, PinPolicy::Always))
        .unwrap();
    window
        .push(msg_tokens("evictable 1", 50, PinPolicy::BudgetEvictable))
        .unwrap();
    window
        .push(msg_tokens("evictable 2", 50, PinPolicy::BudgetEvictable))
        .unwrap();

    // Force budget to zero — pinned turn must survive.
    window.evict_to_budget(0).unwrap();

    let ids: Vec<_> = window.turns().iter().map(|t| t.id).collect();
    assert!(
        ids.contains(&pinned_id),
        "Always-pinned turn must not be evicted by budget pressure"
    );
    assert_eq!(window.stats().active_tokens, 100);
}

#[test]
fn budget_full_returns_error_when_nothing_evictable() {
    let mut window = ContextWindow::new(100);

    window
        .push(msg_tokens("pinned 1", 50, PinPolicy::Always))
        .unwrap();
    window
        .push(msg_tokens("pinned 2", 50, PinPolicy::Always))
        .unwrap();

    // Budget exhausted with Always-pinned turns — nothing can be evicted.
    let result = window.push(msg_tokens("overflow", 10, PinPolicy::BudgetEvictable));
    assert!(
        matches!(result, Err(ContextError::Full { .. })),
        "expected Full error when budget exhausted with pinned turns"
    );
}

/// `token_pressure()` had no test that called it directly and asserted a
/// value — it was only ever exercised indirectly via the auto-eviction
/// threshold check inside `push`/`push_tool_pair`.
#[test]
fn token_pressure_reflects_current_usage_ratio() {
    let mut window = ContextWindow::new(200);
    assert!((window.token_pressure() - 0.0).abs() < f32::EPSILON);

    window
        .push(msg_tokens("m1", 100, PinPolicy::BudgetEvictable))
        .unwrap();
    assert!((window.token_pressure() - 0.5).abs() < 1e-6);

    window
        .push(msg_tokens("m2", 50, PinPolicy::BudgetEvictable))
        .unwrap();
    assert!((window.token_pressure() - 0.75).abs() < 1e-6);
}

/// A zero token budget is a documented special case (`push` never rejects
/// on capacity), and `token_pressure` must report 0.0 rather than divide by
/// zero.
#[test]
fn token_pressure_is_zero_with_an_unbounded_budget() {
    let mut window = ContextWindow::new(0);
    window
        .push(msg_tokens(
            "unbounded",
            1_000_000,
            PinPolicy::BudgetEvictable,
        ))
        .unwrap();
    assert!((window.token_pressure() - 0.0).abs() < f32::EPSILON);
}

/// `evict_toward_threshold` (private, only reachable through `push`'s
/// pre-insert pressure check) is documented to evict down to
/// `budget_threshold - 0.1`. Pin the exact math: budget=1000,
/// threshold=0.75 (default) ⇒ target=650. 8 turns of 100 tokens each bring
/// pressure to 800/1000=0.8 the moment a 9th push's pre-insert check runs;
/// eviction must remove the two oldest turns (800→600, the first point
/// at-or-under 650) — not one turn (still 700, over target) or three
/// (500, further than necessary).
#[test]
fn evict_toward_threshold_evicts_down_to_exactly_threshold_minus_one_tenth() {
    let mut window = ContextWindow::new(1_000);
    let mut ids = Vec::new();
    for i in 0..8 {
        ids.push(
            window
                .push(msg_tokens(
                    &format!("m{i}"),
                    100,
                    PinPolicy::BudgetEvictable,
                ))
                .unwrap(),
        );
    }
    assert!((window.token_pressure() - 0.8).abs() < 1e-6);

    // This push's pre-insert pressure check (800/1000=0.8 > 0.75) fires
    // evict_toward_threshold before the new turn is added.
    window
        .push(msg_tokens("trigger", 100, PinPolicy::BudgetEvictable))
        .unwrap();

    let remaining: Vec<_> = window.turns().iter().map(|t| t.id).collect();
    let evicted_count = ids.iter().filter(|id| !remaining.contains(id)).count();
    assert_eq!(
        evicted_count, 2,
        "must evict exactly the 2 oldest turns to land at/under the 650 target"
    );
    // 600 (post-eviction) + 100 (the triggering push itself) = 700.
    assert_eq!(window.stats().active_tokens, 700);
}

#[test]
fn explicit_evict_to_budget_frees_oldest_first() {
    let mut window = ContextWindow::new(10_000);

    let id_old = window
        .push(msg_tokens("oldest", 100, PinPolicy::BudgetEvictable))
        .unwrap();
    let id_new = window
        .push(msg_tokens("newest", 100, PinPolicy::BudgetEvictable))
        .unwrap();

    // Current = 200. Evict to 150 — oldest (100) removed first, leaving newest (100).
    let stats = window.evict_to_budget(150).unwrap();
    assert_eq!(stats.turns_evicted, 1);
    assert_eq!(stats.tokens_freed, 100);

    let ids: Vec<_> = window.turns().iter().map(|t| t.id).collect();
    assert!(!ids.contains(&id_old), "oldest turn must be evicted first");
    assert!(ids.contains(&id_new), "newest turn must remain");
}

/// Regression test: `record()` used to be a permanent no-op — it updated
/// the running totals and logged via `tracing::debug!`, but never actually
/// pushed anything into `eviction_log`, so `eviction_log()` always returned
/// empty regardless of how much eviction had happened.
#[test]
fn eviction_log_is_populated_after_eviction() {
    let mut window = ContextWindow::new(10_000);
    assert!(window.eviction_log().is_empty(), "starts empty");

    let evicted_id = window
        .push(msg_tokens("m1", 100, PinPolicy::BudgetEvictable))
        .unwrap();
    window
        .push(msg_tokens("m2", 100, PinPolicy::BudgetEvictable))
        .unwrap();

    let stats = window.evict_to_budget(150).unwrap();
    assert_eq!(stats.turns_evicted, 1);

    let log = window.eviction_log();
    assert_eq!(log.len(), 1, "one EvictionRecord per evicted turn");
    assert_eq!(log[0].turn_id, evicted_id);
    assert_eq!(log[0].tokens, 100);
    assert!(matches!(log[0].reason, EvictionReason::BudgetFifo));
}
