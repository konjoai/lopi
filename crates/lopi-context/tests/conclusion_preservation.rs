#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result)]
use lopi_context::{
    ContentBlock, ContextError, ContextWindow, Phase, PinPolicy, Role, TaggedMessage,
};
use uuid::Uuid;

fn msg(text: &str, phase: Phase, pin: PinPolicy) -> TaggedMessage {
    TaggedMessage {
        id: Uuid::new_v4(),
        role: Role::User,
        content: vec![ContentBlock::Text(text.to_string())],
        tokens: 10,
        pin,
        phase,
        evict_after: None,
        tool_pair_id: None,
        is_conclusion: false,
    }
}

#[test]
fn evict_phase_does_not_remove_conclusion() {
    let mut window = ContextWindow::new(10_000);

    window
        .push(msg(
            "raw discovery data",
            Phase::Discovery,
            PinPolicy::Never,
        ))
        .unwrap();
    let conclusion_id = window.pin_conclusion("distilled discovery".to_string(), Phase::Discovery);

    window.evict_phase(Phase::Discovery).unwrap();

    let ids: Vec<_> = window.turns().iter().map(|t| t.id).collect();
    assert!(
        ids.contains(&conclusion_id),
        "conclusion must survive evict_phase"
    );
    assert!(
        window
            .turns()
            .iter()
            .all(|t| t.is_conclusion || t.phase != Phase::Discovery),
        "non-conclusion Discovery turns must be gone"
    );
}

#[test]
fn evict_to_budget_zero_does_not_remove_conclusion() {
    let mut window = ContextWindow::new(10_000);

    window
        .push(msg(
            "evictable",
            Phase::Implementation,
            PinPolicy::BudgetEvictable,
        ))
        .unwrap();
    let conclusion_id = window.pin_conclusion("phase summary".to_string(), Phase::Implementation);

    window.evict_to_budget(0).unwrap();

    let ids: Vec<_> = window.turns().iter().map(|t| t.id).collect();
    assert!(
        ids.contains(&conclusion_id),
        "conclusion must survive evict_to_budget(0)"
    );
}

#[test]
fn conclusion_survives_phase_transition_of_its_own_phase() {
    let mut window = ContextWindow::new(10_000);

    window
        .push(msg("discovery noise", Phase::Discovery, PinPolicy::Never))
        .unwrap();
    let conclusion_id = window.pin_conclusion("discovery done".to_string(), Phase::Discovery);

    window.transition_phase(Phase::Planning);

    assert!(
        window.turns().iter().any(|t| t.id == conclusion_id),
        "conclusion must survive transition_phase"
    );
}

#[test]
fn force_evict_can_remove_conclusion() {
    let mut window = ContextWindow::new(10_000);
    let conclusion_id = window.pin_conclusion("important conclusion".to_string(), Phase::Planning);

    // Without force — must fail.
    let err = window.evict_turn(conclusion_id, false).unwrap_err();
    assert!(
        matches!(err, ContextError::ForcedPinViolation { .. }),
        "expected ForcedPinViolation without force flag"
    );

    // With force — must succeed.
    window.evict_turn(conclusion_id, true).unwrap();
    assert!(window.turns().is_empty());
}

#[test]
fn non_conclusion_always_pinned_also_requires_force() {
    let mut window = ContextWindow::new(10_000);

    let id = window
        .push(TaggedMessage {
            id: Uuid::new_v4(),
            role: Role::User,
            content: vec![ContentBlock::Text("system prompt".to_string())],
            tokens: 20,
            pin: PinPolicy::Always,
            phase: Phase::Boot,
            evict_after: None,
            tool_pair_id: None,
            is_conclusion: false,
        })
        .unwrap();

    let err = window.evict_turn(id, false).unwrap_err();
    assert!(matches!(err, ContextError::ForcedPinViolation { .. }));
}
