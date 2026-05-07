#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result)]
use lopi_context::{ContentBlock, ContextWindow, Phase, PinPolicy, Role, TaggedMessage};
use uuid::Uuid;

fn msg_at(text: &str, phase: Phase) -> TaggedMessage {
    TaggedMessage {
        id: Uuid::new_v4(),
        role: Role::User,
        content: vec![ContentBlock::Text(text.to_string())],
        tokens: 10,
        pin: PinPolicy::BudgetEvictable,
        phase,
        evict_after: None,
        tool_pair_id: None,
        is_conclusion: false,
    }
}

/// Push 10 turns, evict at positions 2, 5, 7. Assert remaining 7 are in original order.
#[test]
fn eviction_preserves_insertion_order() {
    let mut window = ContextWindow::new(10_000);
    let phases = [
        Phase::Boot,
        Phase::Discovery,
        Phase::Discovery,
        Phase::Planning,
        Phase::Planning,
        Phase::Implementation,
        Phase::Implementation,
        Phase::Testing,
        Phase::Testing,
        Phase::Conclusion,
    ];

    let mut ids = Vec::new();
    for (i, &phase) in phases.iter().enumerate() {
        let id = window.push(msg_at(&format!("message {i}"), phase)).unwrap();
        ids.push(id);
    }
    assert_eq!(window.to_api_messages().len(), 10);

    // Evict at original positions 2, 5, 7 by ID.
    window.evict_turn(ids[2], true).unwrap();
    window.evict_turn(ids[5], true).unwrap();
    window.evict_turn(ids[7], true).unwrap();

    let remaining = window.to_api_messages();
    assert_eq!(remaining.len(), 7);

    let remaining_ids: Vec<_> = window.turns().iter().map(|t| t.id).collect();
    let expected_ids: Vec<_> = ids
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 2 && *i != 5 && *i != 7)
        .map(|(_, id)| *id)
        .collect();

    assert_eq!(
        remaining_ids, expected_ids,
        "turns must remain in original insertion order"
    );
}

#[test]
fn to_api_messages_returns_all_active_turns() {
    let mut window = ContextWindow::new(10_000);
    for i in 0..5 {
        window
            .push(msg_at(&format!("t{i}"), Phase::Implementation))
            .unwrap();
    }
    assert_eq!(window.to_api_messages().len(), 5);
}

#[test]
fn to_api_messages_empty_after_all_evicted() {
    let mut window = ContextWindow::new(10_000);
    let id = window
        .push(msg_at("only turn", Phase::Implementation))
        .unwrap();
    window.evict_turn(id, true).unwrap();
    assert!(window.to_api_messages().is_empty());
}
