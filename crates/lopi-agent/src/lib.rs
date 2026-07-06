//! lopi-agent — Claude Code subprocess wrapper, API client, retry runner, and scoring.
/// Phase 16.6 — per-run token-budget enforcement (Anthropic `task_budget` beta).
pub mod api_budget;
/// Anthropic API communication layer.
pub mod api_client;
/// Claude Code subprocess management and model selection.
pub mod claude;
/// Single decoder for `claude -p --output-format stream-json` NDJSON: derives
/// both the log-panel status line and the structured `AgentEvent`s the panes consume.
pub mod claude_events;
mod claude_stream;
/// Sprint U — DAG-structured execution trace for partial-restart retry.
pub mod dag;
/// Sprint U — reconstruct an `AgentDag` from persisted rows.
mod dag_rows;
/// Pattern enrichment from memory history.
pub mod pattern_enricher;
/// Agent execution runner — plan, implement, test, score, retry.
pub mod runner;
/// Task scoring — test pass rate, lint, diff lines.
pub mod scorer;
/// Stability harness for reproducibility testing.
pub mod stability;
/// Konjo Verifier — rubric-guided Opus second-score pass (Sprint S).
pub mod verifier;

pub use api_client::{AnthropicClient, ApiUsage, LOPI_SYSTEM_PROMPT};
pub use claude::{select_model, ClaudeCode, MODEL_HAIKU, MODEL_OPUS, MODEL_SONNET};
pub use dag::{AgentDag, DagNode, NodeKind, NodeStatus};
pub use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
pub use pattern_enricher::PatternEnricher;
pub use runner::AgentRunner;
pub use scorer::Scorer;
pub use stability::{StabilityConfig, StabilityHarness, StabilityVerdict};
pub use verifier::{default_rubric, VerifierAgent};
