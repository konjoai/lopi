#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result)]
use lopi_context::{
    ContentBlock, ContextError, ContextWindow, Phase, PinPolicy, Role, TaggedMessage,
};
use uuid::Uuid;

fn msg_tokens(text: &str, tokens: usize, pin: PinPolicy) -> TaggedMessage {
    TaggedMessage {
        id: Uuid::new_v4(),
        role: Role::User,
        content: vec![ContentBlock::Text(text.to_string())],
        tokens,
        pin,
        phase: Phase::Implementation,
        evict_after: None,
        tool_pair_id: None,
        is_conclusion: false,
    }
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
