#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_in_result
)]
mod common;

use common::make_msg;
use lopi_context::{ContextError, ContextWindow, Phase, PinPolicy, Role};

/// Push a 20-token call / 30-token result pair (the fixture every test in
/// this file uses), returning `(call_id, result_id)`.
fn push_pair(
    window: &mut ContextWindow,
    call_text: &str,
    result_text: &str,
) -> (uuid::Uuid, uuid::Uuid) {
    window
        .push_tool_pair(
            make_msg(
                Role::User,
                call_text,
                Phase::Implementation,
                PinPolicy::BudgetEvictable,
                20,
            ),
            make_msg(
                Role::User,
                result_text,
                Phase::Implementation,
                PinPolicy::BudgetEvictable,
                30,
            ),
        )
        .unwrap()
}

#[test]
fn evicting_one_half_of_pair_errors() {
    let mut window = ContextWindow::new(10_000);
    let (call_id, _result_id) = push_pair(&mut window, "tool call", "tool result");

    let err = window.evict_turn(call_id, false).unwrap_err();
    assert!(
        matches!(err, ContextError::OrphanedToolPair { .. }),
        "expected OrphanedToolPair, got: {err}"
    );
}

#[test]
fn force_evict_removes_both_turns() {
    let mut window = ContextWindow::new(10_000);
    let (call_id, _result_id) = push_pair(&mut window, "tool call", "tool result");

    let stats = window.evict_turn(call_id, true).unwrap();
    assert_eq!(stats.turns_evicted, 2);
    assert_eq!(stats.tokens_freed, 50);
    assert!(window.to_api_messages().is_empty());
}

#[test]
fn token_count_updated_after_pair_eviction() {
    let mut window = ContextWindow::new(10_000);
    let (call_id, _) = push_pair(&mut window, "call", "result");

    assert_eq!(window.stats().active_tokens, 50);
    window.evict_turn(call_id, true).unwrap();
    assert_eq!(window.stats().active_tokens, 0);
}

#[test]
fn api_messages_empty_after_pair_eviction() {
    let mut window = ContextWindow::new(10_000);
    let (call_id, _) = push_pair(&mut window, "call", "result");

    window.evict_turn(call_id, true).unwrap();
    assert!(window.to_api_messages().is_empty());
}
