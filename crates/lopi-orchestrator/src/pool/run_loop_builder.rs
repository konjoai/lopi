//! Split out of `run_loop.rs` purely to keep that file under the 500-line
//! CI gate — the [`AgentRunner`] builder assembly and its budget-resolution
//! helper have no dependency on `run_one`'s dispatch logic.

use lopi_agent::AgentRunner;
use lopi_core::{AgentEvent, EventBus, ScoreWeights, Task};
use lopi_memory::MemoryStore;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Build the configured [`AgentRunner`] for one task's attempt-loop.
///
/// Pure assembly of an already-resolved builder chain — no I/O happens here,
/// so the Verifier-as-Explicit-Gate wiring can be proven at this seam without
/// exercising a real agent run: `.with_verifier()` is called when the task
/// carries `verifier_required` or an explicit `verifier_model`, the first
/// real call site this path has ever had.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_runner(
    task: Task,
    work_repo: PathBuf,
    bus: EventBus<AgentEvent>,
    store: Option<MemoryStore>,
    cancel_rx: oneshot::Receiver<()>,
    attempt_counter: Arc<AtomicUsize>,
    weights: ScoreWeights,
    self_prompt: lopi_core::SelfPromptStrategy,
    escalate: bool,
    skills: lopi_skill::SkillRegistry,
    budget_tokens: u64,
    budget_usd: f64,
    permission_allow: Vec<String>,
    permission_deny: Vec<String>,
    repo_max_iterations: u8,
    repo_guardrails: RepoGuardrails,
    reflect_cross_run: bool,
    plan_decision_rx: oneshot::Receiver<lopi_core::PlanDecision>,
    test_command: Option<String>,
) -> AgentRunner {
    let verifier_needed = task.verifier_required || task.verifier_model.is_some();
    // Loop-as-code: a task-level override always wins over the repo's
    // `.lopi/loop.toml` ceiling when set — mirrors verifier_model's "explicit
    // wins over default" precedent, falling back to the repo config.
    let max_turns = u32::from(task.max_iterations.unwrap_or(repo_max_iterations));
    // Guardrails — same "explicit task override wins over repo default"
    // precedent as `max_turns` above.
    let gate = task.gate.clone().or(repo_guardrails.gate);
    let until = task.until.clone().or(repo_guardrails.until);
    let on_fail = task.on_fail.unwrap_or(repo_guardrails.on_fail);
    // Progress-Gating (A3) — the repo budget seeds `task_budget`; a positive
    // per-task `budget_tokens` overrides it as the loop's hard cap in the runner
    // (`AgentRunner::effective_budget_tokens`), so no extra folding is needed.
    let mut runner = AgentRunner::new(task, work_repo, bus, store, cancel_rx, attempt_counter)
        .with_score_weights(weights)
        .with_self_prompt(self_prompt)
        .with_strategy_escalation(escalate)
        .with_skills(skills)
        .with_task_budget(budget_tokens)
        .with_cli_budget_usd(budget_usd)
        .with_tool_permissions(permission_allow, permission_deny)
        .with_cross_run_reflection(reflect_cross_run)
        .with_plan_gate(plan_decision_rx)
        .with_test_command(test_command);
    runner.max_turns = max_turns;
    runner.gate = gate;
    runner.until = until;
    runner.on_fail = on_fail;
    if verifier_needed {
        runner.with_verifier()
    } else {
        runner
    }
}

/// Budget & Guardrail Controls Part 2/3 — resolve the effective budget for
/// one task: the repo's `[budget]` preset (plus any explicit overrides under
/// it), then any per-task override layered on top (`lopi run --budget`/
/// `--budget-preset`/`--budget-tokens`, Telegram `/budget`). A bare per-task
/// USD/token override never touches the tool allow/deny lists on its own —
/// see [`lopi_core::BudgetOverride::apply`]'s "fan-out stays opt-in" doc
/// comment. Pure (no I/O) so it's unit-testable without the pool's
/// git/worktree machinery.
pub(super) fn effective_task_budget(
    task: &Task,
    cfg: &lopi_core::LoopConfig,
) -> lopi_core::ResolvedBudget {
    task.budget_override.as_ref().map_or_else(
        || cfg.resolved_budget(),
        |ov| ov.apply(cfg.resolved_budget()),
    )
}

/// The repo-level (`.lopi/loop.toml`) guardrail defaults a task's own
/// `gate`/`until`/`on_fail` may override. Bundled into one struct — rather
/// than three more positional args on [`build_runner`] — since they're
/// always loaded and passed together.
#[derive(Debug, Clone, Default)]
pub(super) struct RepoGuardrails {
    pub(super) gate: Option<String>,
    pub(super) until: Option<String>,
    pub(super) on_fail: lopi_core::loop_config::OnFail,
}
