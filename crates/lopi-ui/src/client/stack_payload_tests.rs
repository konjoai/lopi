//! KT-T0.3 — permanent regression test pinning `card_to_task_payload`'s
//! output against three fixtures lifted verbatim from
//! `web/src/lib/stores/stack.test.ts` (lines ~409-442, ~607-628). This is
//! the load-bearing test for the whole sprint's premise: a mismatch here
//! means the Rust port doesn't actually match the shipped web behavior.

#![allow(clippy::unwrap_used)]

use super::*;
use lopi_core::loop_config::OnFail;
use lopi_core::stack::{
    baseline_eval, default_cron, default_guardrails, default_maxx, Budget, BudgetPresetChoice,
    CardConfig, CardStatus, Guardrails, IsolationChoice,
};

fn plain_defaults() -> PaneDefaults {
    PaneDefaults {
        model: "sonnet".to_string(),
        effort: "medium".to_string(),
        repo: "konjoai/lopi".to_string(),
        branch: None,
        autonomy: None,
        permission_mode: None,
    }
}

/// The Rust equivalent of `stack.ts::buildCard(raw)` for a literal
/// (no-alias, no-preset) goal — the exact shape a fresh composer card has.
fn build_card(id: &str, goal: &str) -> StackCard {
    StackCard {
        id: id.to_string(),
        preset: None,
        goal: goal.to_string(),
        alias: None,
        literal: true,
        evals: vec![baseline_eval()],
        status: CardStatus::Idle,
        max_iterations: 0,
        iteration: None,
        scheduled: false,
        cron: default_cron(),
        guardrails: default_guardrails(),
        config: CardConfig::default(),
        task_id: None,
        tpl: None,
        tpl_kind: None,
        maxx: default_maxx(),
        maxx_entry_id: None,
        block_reason: None,
    }
}

/// Fixture A (`stack.test.ts:409-419`) — a bare, untouched card.
#[test]
fn fixture_a_bare_card() {
    let card = build_card("card-a", "do the thing");
    let payload = card_to_task_payload(&card, &plain_defaults());

    assert_eq!(payload.goal, "do the thing");
    assert_eq!(payload.repo, Some("konjoai/lopi".to_string()));
    assert_eq!(payload.model, Some("sonnet".to_string()));
    assert_eq!(payload.max_iterations, Some(1));
    assert_eq!(payload.on_fail, Some(OnFail::Stop));
    assert_eq!(payload.gate, None);
}

/// Fixture B (`stack.test.ts:420-442`) — fully guarded card with a repo
/// override.
#[test]
fn fixture_b_guarded_card_with_repo_override() {
    let mut card = build_card("card-b", "do the thing");
    card.config.repo = Some("squish".to_string());
    card.guardrails = Guardrails {
        gate: true,
        gate_cmd: "./kill_test.sh".to_string(),
        until: true,
        until_cmd: "cargo test".to_string(),
        on_fail: OnFail::Backoff,
        budget: Budget::K200,
        budget_preset: BudgetPresetChoice::Inherit,
        budget_usd: None,
        isolation: IsolationChoice::Inherit,
        no_progress_limit: None,
    };

    let payload = card_to_task_payload(&card, &plain_defaults());

    assert_eq!(payload.budget_tokens, Some(200_000));
    assert_eq!(payload.repo, Some("squish".to_string()));
    assert_eq!(payload.gate, Some("./kill_test.sh".to_string()));
    assert_eq!(payload.until, Some("cargo test".to_string()));
    assert_eq!(payload.on_fail, Some(OnFail::Backoff));
}

/// Fixture C (`stack.test.ts:607-628`) — key-set completeness. In TS this
/// asserts `Object.keys(payload.options).sort()`; the Rust `CreateTaskRequest`
/// has no `options` sub-object (every field is flat), so the equivalent
/// assertion is: exactly this set of optional fields is `Some`, everything
/// else is `None`. `goal`/`repo`/`priority` are excluded from both sides —
/// TS keeps them outside `options`; Rust always populates them regardless
/// of guardrail state, so they carry no signal for this test.
#[test]
fn fixture_c_key_set_completeness() {
    let mut card = build_card("card-c", "x");
    card.guardrails = Guardrails {
        gate: true,
        gate_cmd: "g".to_string(),
        until: true,
        until_cmd: "u".to_string(),
        on_fail: OnFail::Stop,
        budget: Budget::Auto,
        budget_preset: BudgetPresetChoice::Inherit,
        budget_usd: None,
        isolation: IsolationChoice::Inherit,
        no_progress_limit: None,
    };

    let payload = card_to_task_payload(&card, &plain_defaults());

    assert!(payload.acceptance.is_some(), "acceptance");
    assert_eq!(payload.client_ref, Some("card-c".to_string()), "client_ref");
    assert_eq!(payload.effort, Some("medium".to_string()), "effort");
    assert_eq!(payload.gate, Some("g".to_string()), "gate");
    assert_eq!(payload.max_iterations, Some(1), "max_iterations");
    assert_eq!(payload.model, Some("sonnet".to_string()), "model");
    assert_eq!(payload.on_fail, Some(OnFail::Stop), "on_fail");
    assert_eq!(payload.until, Some("u".to_string()), "until");

    // Everything else must be None — pins the "no field leaks onto the wire
    // unless the card actually touched it" contract.
    assert_eq!(payload.constraints, None, "constraints");
    assert_eq!(payload.allowed_dirs, None, "allowed_dirs");
    assert_eq!(payload.forbidden_dirs, None, "forbidden_dirs");
    assert_eq!(payload.max_retries, None, "max_retries");
    assert_eq!(payload.require_plan_approval, None, "require_plan_approval");
    assert_eq!(payload.verifier_required, None, "verifier_required");
    assert_eq!(payload.verifier_model, None, "verifier_model");
    assert_eq!(payload.verifier_effort, None, "verifier_effort");
    assert_eq!(payload.report, None, "report");
    assert_eq!(payload.autonomy_level, None, "autonomy_level");
    assert_eq!(payload.no_progress_limit, None, "no_progress_limit");
    assert_eq!(payload.isolation, None, "isolation");
    assert_eq!(payload.permission_mode, None, "permission_mode");
    assert_eq!(payload.deliverable, None, "deliverable");
    assert_eq!(payload.verifier_fail_open, None, "verifier_fail_open");
    assert_eq!(payload.budget_tokens, None, "budget_tokens");
    assert_eq!(payload.budget_override, None, "budget_override");
}

#[test]
fn run_once_forces_a_single_iteration() {
    let mut card = build_card("card-d", "goal");
    card.max_iterations = 7;
    let payload = card_to_task_payload_for_run_once(&card, &plain_defaults());
    assert_eq!(payload.max_iterations, Some(1));
}

#[test]
fn pane_submit_payload_omits_untouched_fields() {
    let launch = PaneLaunch {
        goal: "g".to_string(),
        repo: "r".to_string(),
        priority: None,
        model: None,
        effort: None,
        branch: None,
        permission_mode: None,
    };
    let payload = pane_submit_payload(&launch);
    assert_eq!(payload.goal, "g");
    assert_eq!(payload.repo, Some("r".to_string()));
    assert_eq!(payload.priority, Some("normal".to_string()));
    assert_eq!(payload.model, None);
    assert_eq!(payload.effort, None);
    assert_eq!(payload.permission_mode, None);
    assert_eq!(payload.constraints, None);
}

#[test]
fn pane_submit_payload_auto_model_is_omitted() {
    let launch = PaneLaunch {
        goal: "g".to_string(),
        repo: "r".to_string(),
        priority: None,
        model: Some("auto".to_string()),
        effort: None,
        branch: None,
        permission_mode: None,
    };
    assert_eq!(pane_submit_payload(&launch).model, None);
}

#[test]
fn pane_submit_payload_branch_becomes_a_constraint() {
    let launch = PaneLaunch {
        goal: "g".to_string(),
        repo: "r".to_string(),
        priority: None,
        model: None,
        effort: None,
        branch: Some("feat/x".to_string()),
        permission_mode: None,
    };
    assert_eq!(
        pane_submit_payload(&launch).constraints,
        Some(vec!["Target branch: feat/x".to_string()])
    );
}
