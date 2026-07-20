#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Verifier-as-explicit-gate, `Task.max_iterations` override, and Budget &
//! Guardrail Controls Part 2/3's `effective_task_budget` — all exercised
//! through `run_loop::build_runner`, the pool-construction seam.
//!
//! Split out of `tests.rs` purely to keep that file under the 500-line CI
//! file-size gate as the `effective_task_budget` tests were added; no
//! behavioral difference from being inline.

use super::*;
use lopi_core::loop_config::AutonomyLevel;
use lopi_core::{EventBus, ScoreWeights, SelfPromptStrategy, Task};
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

// `build_runner` is the pool-construction seam: it assembles the same
// builder chain `run_one` uses, without performing any I/O or running the
// loop, so the never-before-exercised `.with_verifier()` call site can be
// proven live here.

#[allow(clippy::too_many_arguments)]
fn runner_for(task: Task) -> lopi_agent::AgentRunner {
    let bus: EventBus<AgentEvent> = EventBus::new(16);
    let (_cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    let (_plan_tx, plan_decision_rx) = tokio::sync::oneshot::channel();
    run_loop::build_runner(
        task,
        PathBuf::from("."),
        bus,
        None,
        cancel_rx,
        Arc::new(AtomicUsize::new(0)),
        ScoreWeights::default(),
        SelfPromptStrategy::default(),
        false,
        lopi_skill::SkillRegistry::default(),
        0,
        lopi_core::LoopConfig::default().resolved_budget().usd,
        Vec::new(),
        Vec::new(),
        lopi_core::LoopConfig::default().max_iterations,
        run_loop::RepoGuardrails::default(),
        false,
        plan_decision_rx,
    )
}

#[test]
fn verifier_required_enables_the_gate_end_to_end() {
    let mut task = Task::new("ship a verified change");
    task.autonomy_level = AutonomyLevel::DraftPr; // L2 — would not force it alone
    task.verifier_required = true;
    let runner = runner_for(task);
    assert!(
        runner.verifier_enabled(),
        "verifier_required must wire .with_verifier() at pool construction"
    );
}

#[test]
fn an_explicit_verifier_model_also_enables_the_gate() {
    let mut task = Task::new("grade me with sonnet");
    task.autonomy_level = AutonomyLevel::DraftPr;
    task.verifier_model = Some("claude-sonnet-4-6".into());
    let runner = runner_for(task);
    assert!(runner.verifier_enabled());
}

#[test]
fn without_the_flag_or_model_the_gate_stays_off_below_l3() {
    let mut task = Task::new("plain draft-pr loop");
    task.autonomy_level = AutonomyLevel::DraftPr;
    let runner = runner_for(task);
    assert!(
        !runner.verifier_enabled(),
        "no accidental always-on: L1/L2 with no explicit gate must stay disabled"
    );
}

// ─── Task.max_iterations override (web task-create surface exposure) ────

#[test]
fn task_max_iterations_override_beats_the_repo_config() {
    let mut task = Task::new("bounded loop");
    task.max_iterations = Some(5);
    let runner = runner_for(task);
    assert_eq!(
        runner.max_turns, 5,
        "an explicit per-task override must win over the repo's LoopConfig default"
    );
}

#[test]
fn task_max_iterations_zero_is_the_infinite_sentinel_not_rejected() {
    let mut task = Task::new("unbounded loop");
    task.max_iterations = Some(0);
    let runner = runner_for(task);
    assert_eq!(
        runner.max_turns, 0,
        "Some(0) must flow through as the infinite sentinel, not be coerced to the repo default"
    );
}

#[test]
fn task_max_iterations_unset_falls_back_to_the_repo_config() {
    let task = Task::new("default-capped loop");
    let runner = runner_for(task);
    assert_eq!(
        runner.max_turns,
        u32::from(lopi_core::LoopConfig::default().max_iterations),
        "no override set — the repo's LoopConfig ceiling applies unchanged"
    );
}

// ── Budget & Guardrail Controls Part 2/3 — effective_task_budget ───────────

#[test]
fn effective_task_budget_with_no_override_is_the_repo_resolved_budget() {
    let task = Task::new("no budget override");
    let cfg = lopi_core::LoopConfig::default();
    let resolved = run_loop::effective_task_budget(&task, &cfg);
    assert_eq!(resolved, cfg.resolved_budget());
}

#[test]
fn effective_task_budget_bare_usd_override_never_reopens_workflow() {
    let mut task = Task::new("bump the cap only");
    task.budget_override = Some(lopi_core::BudgetOverride {
        usd: Some(9.0),
        ..Default::default()
    });
    let cfg = lopi_core::LoopConfig::default();
    let resolved = run_loop::effective_task_budget(&task, &cfg);
    assert_eq!(resolved.usd, 9.0);
    assert_eq!(
        resolved.deny,
        vec!["Workflow", "Task", "Agent"],
        "a bare USD override must not touch the tool deny list"
    );
}

#[test]
fn effective_task_budget_preset_override_replaces_the_repo_budget() {
    let mut task = Task::new("go deep for this one card");
    task.budget_override = Some(lopi_core::BudgetOverride {
        preset: Some(lopi_core::BudgetPreset::Deep),
        ..Default::default()
    });
    let cfg = lopi_core::LoopConfig::default();
    let resolved = run_loop::effective_task_budget(&task, &cfg);
    assert_eq!(resolved.usd, 10.0);
    assert_eq!(resolved.tokens, 5_000_000);
    assert!(resolved.deny.is_empty(), "deep preset re-enables Workflow");
}
