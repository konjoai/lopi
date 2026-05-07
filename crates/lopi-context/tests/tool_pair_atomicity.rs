#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result)]
use lopi_context::{
    ContentBlock, ContextError, ContextWindow, Phase, PinPolicy, Role, TaggedMessage,
};
use uuid::Uuid;

fn make_msg(role: Role, text: &str, phase: Phase, pin: PinPolicy, tokens: usize) -> TaggedMessage {
    TaggedMessage {
        id: Uuid::new_v4(),
        role,
        content: vec![ContentBlock::Text(text.to_string())],
        tokens,
        pin,
        phase,
        evict_after: None,
        tool_pair_id: None,
        is_conclusion: false,
    }
}

#[test]
fn evicting_one_half_of_pair_errors() {
    let mut window = ContextWindow::new(10_000);
    let (call_id, _result_id) = window
        .push_tool_pair(
            make_msg(
                Role::User,
                "tool call",
                Phase::Implementation,
                PinPolicy::BudgetEvictable,
                20,
            ),
            make_msg(
                Role::User,
                "tool result",
                Phase::Implementation,
                PinPolicy::BudgetEvictable,
                30,
            ),
        )
        .unwrap();

    let err = window.evict_turn(call_id, false).unwrap_err();
    assert!(
        matches!(err, ContextError::OrphanedToolPair { .. }),
        "expected OrphanedToolPair, got: {err}"
    );
}

#[test]
fn force_evict_removes_both_turns() {
    let mut window = ContextWindow::new(10_000);
    let (call_id, _result_id) = window
        .push_tool_pair(
            make_msg(
                Role::User,
                "tool call",
                Phase::Implementation,
                PinPolicy::BudgetEvictable,
                20,
            ),
            make_msg(
                Role::User,
                "tool result",
                Phase::Implementation,
                PinPolicy::BudgetEvictable,
                30,
            ),
        )
        .unwrap();

    let stats = window.evict_turn(call_id, true).unwrap();
    assert_eq!(stats.turns_evicted, 2);
    assert_eq!(stats.tokens_freed, 50);
    assert!(window.to_api_messages().is_empty());
}

#[test]
fn token_count_updated_after_pair_eviction() {
    let mut window = ContextWindow::new(10_000);
    let (call_id, _) = window
        .push_tool_pair(
            make_msg(
                Role::User,
                "call",
                Phase::Implementation,
                PinPolicy::BudgetEvictable,
                20,
            ),
            make_msg(
                Role::User,
                "result",
                Phase::Implementation,
                PinPolicy::BudgetEvictable,
                30,
            ),
        )
        .unwrap();

    assert_eq!(window.stats().active_tokens, 50);
    window.evict_turn(call_id, true).unwrap();
    assert_eq!(window.stats().active_tokens, 0);
}

#[test]
fn api_messages_empty_after_pair_eviction() {
    let mut window = ContextWindow::new(10_000);
    let (call_id, _) = window
        .push_tool_pair(
            make_msg(
                Role::User,
                "call",
                Phase::Implementation,
                PinPolicy::BudgetEvictable,
                20,
            ),
            make_msg(
                Role::User,
                "result",
                Phase::Implementation,
                PinPolicy::BudgetEvictable,
                30,
            ),
        )
        .unwrap();

    window.evict_turn(call_id, true).unwrap();
    assert!(window.to_api_messages().is_empty());
}
