#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unwrap_in_result)]
use lopi_context::{ContentBlock, ContextWindow, Phase, PinPolicy, Role, TaggedMessage};
use uuid::Uuid;

fn msg(role: Role, text: &str, phase: Phase, pin: PinPolicy) -> TaggedMessage {
    TaggedMessage {
        id: Uuid::new_v4(),
        role,
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
fn transition_to_planning_evicts_discovery_turns() {
    let mut window = ContextWindow::new(10_000);

    window
        .push(msg(
            Role::User,
            "boot prompt",
            Phase::Boot,
            PinPolicy::Always,
        ))
        .unwrap();
    window
        .push(msg(
            Role::User,
            "arxiv result 1",
            Phase::Discovery,
            PinPolicy::Never,
        ))
        .unwrap();
    window
        .push(msg(
            Role::User,
            "arxiv result 2",
            Phase::Discovery,
            PinPolicy::BudgetEvictable,
        ))
        .unwrap();
    window
        .push(msg(
            Role::User,
            "sprint plan",
            Phase::Planning,
            PinPolicy::BudgetEvictable,
        ))
        .unwrap();

    assert_eq!(window.to_api_messages().len(), 4);

    window.transition_phase(Phase::Planning);

    let turns = window.turns();
    assert_eq!(
        turns.len(),
        2,
        "Boot + Planning must remain, Discovery must be gone"
    );
    assert!(turns.iter().all(|t| t.phase != Phase::Discovery));
    assert!(turns.iter().any(|t| t.phase == Phase::Boot));
    assert!(turns.iter().any(|t| t.phase == Phase::Planning));
}

#[test]
fn discovery_conclusion_survives_phase_transition() {
    let mut window = ContextWindow::new(10_000);

    window
        .push(msg(
            Role::User,
            "raw discovery",
            Phase::Discovery,
            PinPolicy::Never,
        ))
        .unwrap();
    let conclusion_id =
        window.pin_conclusion("distilled discovery summary".to_string(), Phase::Discovery);

    window.transition_phase(Phase::Planning);

    let turns = window.turns();
    assert!(
        turns.iter().any(|t| t.id == conclusion_id),
        "conclusion must survive transition_phase"
    );
    assert!(
        turns
            .iter()
            .all(|t| t.is_conclusion || t.phase != Phase::Discovery),
        "non-conclusion Discovery turns must be evicted"
    );
}

#[test]
fn boot_always_pinned_turns_survive_any_transition() {
    let mut window = ContextWindow::new(10_000);

    window
        .push(msg(
            Role::User,
            "system prompt",
            Phase::Boot,
            PinPolicy::Always,
        ))
        .unwrap();
    window
        .push(msg(
            Role::User,
            "discovery noise",
            Phase::Discovery,
            PinPolicy::Never,
        ))
        .unwrap();

    window.transition_phase(Phase::Planning);

    assert!(
        window.turns().iter().any(|t| t.phase == Phase::Boot),
        "Boot/Always turn must survive"
    );
}

#[test]
fn until_phase_turns_evicted_on_matching_transition() {
    let mut window = ContextWindow::new(10_000);

    window
        .push(msg(
            Role::User,
            "pinned until planning",
            Phase::Discovery,
            PinPolicy::UntilPhase(Phase::Planning),
        ))
        .unwrap();
    window
        .push(msg(
            Role::User,
            "always stays",
            Phase::Boot,
            PinPolicy::Always,
        ))
        .unwrap();

    assert_eq!(window.turns().len(), 2);

    window.transition_phase(Phase::Planning);

    // UntilPhase(Planning) turn should be gone; Always Boot turn survives.
    let turns = window.turns();
    assert!(turns
        .iter()
        .all(|t| !matches!(t.pin, PinPolicy::UntilPhase(Phase::Planning))));
    assert!(turns.iter().any(|t| t.phase == Phase::Boot));
}
