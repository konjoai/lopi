//! lopi-agent — Claude Code subprocess wrapper, API client, retry runner, and scoring.
/// Anthropic API communication layer.
pub mod api_client;
/// Claude Code subprocess management and model selection.
pub mod claude;
mod claude_stream;
/// Pattern enrichment from memory history.
pub mod pattern_enricher;
/// Agent execution runner — plan, implement, test, score, retry.
pub mod runner;
/// Task scoring — test pass rate, lint, diff lines.
pub mod scorer;
/// Stability harness for reproducibility testing.
pub mod stability;

pub use api_client::{AnthropicClient, ApiUsage, LOPI_SYSTEM_PROMPT};
pub use claude::{select_model, ClaudeCode, MODEL_HAIKU, MODEL_OPUS, MODEL_SONNET};
pub use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
pub use pattern_enricher::PatternEnricher;
pub use runner::AgentRunner;
pub use scorer::Scorer;
pub use stability::{StabilityConfig, StabilityHarness, StabilityVerdict};
