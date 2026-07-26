//! lopi-agent — Claude Code subprocess wrapper, API client, retry runner, and scoring.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
/// Phase 16.6 — per-run token-budget enforcement (Anthropic `task_budget` beta).
pub mod api_budget;
/// Anthropic API communication layer.
pub mod api_client;
/// Claude Code subprocess management and model selection.
pub mod claude;
/// Fluent `with_*` builders for `ClaudeCode`, split out of `claude.rs` to
/// keep it under the file-size gate. See that module's doc comment.
mod claude_builders;
/// Single decoder for `claude -p --output-format stream-json` NDJSON: derives
/// both the log-panel status line and the structured `AgentEvent`s the panes consume.
pub mod claude_events;
/// Model identifiers, model-routing heuristic, and the CLI output envelope.
/// Re-exported from `claude` — see that module's doc comment.
mod claude_model;
mod claude_stream;
/// Sprint F2 Phase 4 — externalized worker model IDs (no recompile needed
/// to change one). See that module's doc comment.
pub mod model_config;
/// Subprocess-env scrubbing and fix-prompt error compression.
/// Re-exported from `claude` — see that module's doc comment.
mod claude_support;
/// Sprint U — DAG-structured execution trace for partial-restart retry.
pub mod dag;
/// Sprint U — reconstruct an `AgentDag` from persisted rows.
mod dag_rows;
/// Eval-Execution-1 (A1) — the tiered eval executor: goal/acceptance scoring
/// across execution-ok, shell-test, judge, and suite tiers, fail-closed.
pub mod eval;
mod prompt;
/// Sprint F2 Phase 3 — externalized per-model token pricing (no recompile
/// needed to change a rate). See that module's doc comment.
pub mod pricing;
/// MAXX kill-test prep — logs `rate_limit_event` cadence for a real session's
/// eventual run, off by default (see `docs/ops/NEXT_SESSION_PROMPT.md`).
pub mod quota_kill_log;
/// A2 §2 — the deterministic reflect-vs-blind measurement harness.
pub mod reflection_harness;
/// Agent execution runner — plan, implement, test, score, retry.
pub mod runner;
/// Task scoring — test pass rate, lint, diff lines.
pub mod scorer;
/// Stability harness for reproducibility testing.
pub mod stability;
#[cfg(test)]
mod test_support;
/// Onboarding-Import-1 — defensive decoder for historical
/// `~/.claude/projects/**/*.jsonl` session transcripts. See that module's
/// doc comment for how this differs from `claude_events`.
pub mod transcript_import;
/// Konjo Verifier — rubric-guided Opus second-score pass (Sprint S).
pub mod verifier;

pub use api_client::{AnthropicClient, ApiUsage, LOPI_SYSTEM_PROMPT};
pub use claude::{model_haiku, model_opus, model_sonnet, select_model, ClaudeCode};
pub use dag::{AgentDag, DagNode, NodeKind, NodeStatus};
pub use eval::{
    EvalContext, ExecutionOkEval, Judge, JudgeEval, ShellTestEval, SuiteEval, TierEvaluator,
    TieredEvaluator, VerifierJudge,
};
pub use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
pub use reflection_harness::{
    precision_sweep, reflection_fixtures, run_three_arm, Arm, ArmStats, FixtureTask, HarnessParams,
    ThreeArmReport,
};
pub use runner::AgentRunner;
pub use scorer::Scorer;
pub use stability::{StabilityConfig, StabilityHarness, StabilityVerdict};
pub use verifier::{default_rubric, VerifierAgent};
