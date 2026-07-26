//! Split out of `runner/mod.rs` purely to keep that file under the 500-line
//! CI gate — this is a second `impl AgentRunner` block holding every builder
//! method after `new`/`standalone`. No new behavior; pure move.

use super::AgentRunner;
use crate::api_client::AnthropicClient;
use crate::stability::{StabilityConfig, StabilityHarness};
use lopi_core::{PlanDecision, ScoreWeights, SelfPromptStrategy};
use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

impl AgentRunner {
    /// Wire the direct-API planning path. When set, `run()` will try
    /// `AnthropicClient::stream_plan` with prompt caching before falling
    /// back to the `claude` CLI subprocess on any failure. The limiter
    /// gates request rate; the breaker opens on consecutive failures or
    /// the hourly cost cap.
    #[must_use]
    pub fn with_api(
        mut self,
        client: Arc<AnthropicClient>,
        limiter: Arc<AnthropicLimiter>,
        breaker: Arc<CircuitBreaker>,
    ) -> Self {
        self.api_client = Some(client);
        self.limiter = Some(limiter);
        self.breaker = Some(breaker);
        self
    }

    /// Sprint H — enable Reflexion-style adaptive retry.
    ///
    /// Two effects when enabled:
    ///   1. After a failed attempt, the next attempt's planning prompt
    ///      includes the previous attempt's error / test output. This
    ///      empirically lifts retry success ~30–50% on coding tasks.
    ///   2. After all retries exhausted, run a post-mortem session
    ///      (requires `with_api()`) that asks Claude for one imperative
    ///      constraint that would have prevented the failure. Persisted
    ///      to the `patterns` table with `derived_from_postmortem = 1`.
    #[must_use]
    pub const fn with_adaptive_retry(mut self) -> Self {
        self.adaptive_retry = true;
        self
    }

    /// Sprint S — enable the Konjo Verifier second-score pass.
    ///
    /// When enabled, the runner calls Opus with a rubric-guided prompt after the
    /// heuristic score passes. On rejection, fix hints are appended to
    /// `task.constraints` and the agent retries. Requires `with_api()` — silently
    /// skipped when no API client is configured.
    #[must_use]
    pub const fn with_verifier(mut self) -> Self {
        self.verifier_enabled = true;
        self
    }

    /// Verifier as Explicit Gate — whether the Konjo Verifier second-score
    /// pass is enabled for this runner, either via [`with_verifier`](Self::with_verifier)
    /// or (independently, at finalize time) a forcing `autonomy_level`.
    #[must_use]
    pub const fn verifier_enabled(&self) -> bool {
        self.verifier_enabled
    }

    /// Phase 5b — wire custom score weights for this task's retry loop.
    /// Allows the pool to adjust lint/diff penalties based on user-tuned
    /// preferences or derived from past attempt success patterns.
    #[must_use]
    pub fn with_score_weights(mut self, weights: ScoreWeights) -> Self {
        self.score_weights = weights;
        self
    }

    /// Attach the skill registry whose matching entries are injected into the
    /// planning prompt (Pentad M2.2).
    #[must_use]
    pub fn with_skills(mut self, skills: lopi_skill::SkillRegistry) -> Self {
        self.skills = skills;
        self
    }

    /// Returns true when adaptive retry is enabled.
    #[must_use]
    pub const fn adaptive_retry_enabled(&self) -> bool {
        self.adaptive_retry
    }

    /// A2 (reflection) — enable durable cross-run learning: capture a learning
    /// from every rejected attempt (rollback-safe) and inject relevance-filtered,
    /// bounded learnings into the next planning prompt. Off by default (§2
    /// discipline — flagged until a live comparison beats blind retry).
    #[must_use]
    pub const fn with_cross_run_reflection(mut self, on: bool) -> Self {
        self.reflect_cross_run = on;
        self
    }

    /// Whether durable cross-run reflection (capture + retrieval) is enabled.
    #[must_use]
    pub const fn cross_run_reflection_enabled(&self) -> bool {
        self.reflect_cross_run
    }

    /// Phase 16.4 — set the self-prompting strategy used to reframe a failed
    /// attempt into the next attempt's planning prompt. Only takes effect when
    /// adaptive retry is enabled (the strategy reframes the injected failure).
    #[must_use]
    pub const fn with_self_prompt(mut self, strategy: SelfPromptStrategy) -> Self {
        self.self_prompt = strategy;
        self
    }

    /// The currently configured self-prompting strategy.
    #[must_use]
    pub const fn self_prompt_strategy(&self) -> SelfPromptStrategy {
        self.self_prompt
    }

    /// Phase 16.5 — enable adaptive strategy escalation: each failed attempt
    /// climbs one rung up the S1→S4 ladder (from the configured base strategy)
    /// instead of staying pinned. Only takes effect with adaptive retry enabled.
    #[must_use]
    pub const fn with_strategy_escalation(mut self, escalate: bool) -> Self {
        self.escalate_strategy = escalate;
        self
    }

    /// The effective self-prompt strategy for a 1-based `attempt`, accounting for
    /// escalation. With escalation off this is always the pinned base strategy.
    #[must_use]
    pub fn effective_strategy(&self, attempt: u8) -> SelfPromptStrategy {
        if self.escalate_strategy {
            SelfPromptStrategy::escalated(self.self_prompt, attempt)
        } else {
            self.self_prompt
        }
    }

    /// Phase 16.6 — wire the per-run token budget from `.lopi/loop.toml`.
    ///
    /// `0` disables the budget (inherits the global cap); any positive value is
    /// forwarded to the Anthropic `task_budget` parameter on the direct-API
    /// planning path so the model self-paces instead of being hard-cut. The
    /// value is model-gated and clamped to the API minimum at request time.
    #[must_use]
    pub const fn with_task_budget(mut self, budget_tokens: u64) -> Self {
        self.task_budget = if budget_tokens == 0 {
            None
        } else {
            Some(budget_tokens)
        };
        self
    }

    /// Wire the per-`claude -p` session USD cap from `.lopi/loop.toml`'s
    /// `max_budget_usd` (or a task-level override, none exists yet). `0.0`
    /// disables it — the CLI receives no `--max-budget-usd` flag at all, same
    /// "0 = disabled" sentinel as `with_task_budget`.
    #[must_use]
    pub const fn with_cli_budget_usd(mut self, budget_usd: f64) -> Self {
        self.cli_budget_usd = if budget_usd <= 0.0 {
            None
        } else {
            Some(budget_usd)
        };
        self
    }

    /// The configured per-run token budget, if any.
    #[must_use]
    pub const fn task_budget(&self) -> Option<u64> {
        self.task_budget
    }

    /// The configured per-`claude -p` session USD cap, if any.
    #[must_use]
    pub const fn cli_budget_usd(&self) -> Option<f64> {
        self.cli_budget_usd
    }

    /// Wire the tool allow/deny lists from `.lopi/loop.toml`'s
    /// `permission_allow`/`permission_deny` — forwarded to `claude -p` as
    /// `--allowedTools`/`--disallowedTools`. Both empty (the default) changes
    /// nothing about which tools are available.
    #[must_use]
    pub fn with_tool_permissions(mut self, allow: Vec<String>, deny: Vec<String>) -> Self {
        self.permission_allow = allow;
        self.permission_deny = deny;
        self
    }

    /// Sprint F2 Phase 1 — wire the repo's `.lopi/loop.toml` `test_command`
    /// override. `None` (the default) leaves the `Scorer`'s stack detection
    /// as the sole source.
    #[must_use]
    pub fn with_test_command(mut self, test_command: Option<String>) -> Self {
        self.test_command = test_command;
        self
    }

    /// Sprint I — attach the Layer 5 patch stability gate.
    ///
    /// When set, `run()` generates `config.n_samples` plan proposals before
    /// the first implementation attempt and measures their pairwise Jaccard
    /// variance. High variance blocks the run (`TaskStatus::Failed` with
    /// a `StabilityGateBlocked` reason) so human review can intervene.
    ///
    /// Requires the same `client` / `limiter` / `breaker` used by `with_api`.
    /// If `with_api` is not set the harness will still work — it only needs
    /// an API client, which can be independent of the planning path.
    #[must_use]
    pub fn with_stability_gate(
        mut self,
        client: Arc<AnthropicClient>,
        limiter: Option<Arc<AnthropicLimiter>>,
        breaker: Option<Arc<CircuitBreaker>>,
        config: StabilityConfig,
    ) -> Self {
        self.stability_harness = Some(StabilityHarness::new(client, limiter, breaker, config));
        self
    }

    /// Phase 11 — wire the plan-approval gate. When set, the runner surfaces
    /// its first plan and pauses until this channel delivers a decision.
    #[must_use]
    pub fn with_plan_gate(mut self, rx: oneshot::Receiver<PlanDecision>) -> Self {
        self.plan_decision_rx = Some(rx);
        self
    }

    /// Return a child token derived from this runner's `CancellationToken`.
    /// The pool can cancel this token to abort the runner from a `JoinSet` teardown.
    #[must_use]
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Return the number of attempts made by this runner.
    #[must_use]
    pub fn attempts_made(&self) -> u8 {
        self.attempts_made
    }

    /// Cumulative tokens metered across the run so far (input + output),
    /// summed from every streamed `TokenUsage` event. The budget gate compares
    /// this against the effective per-loop [`task_budget`](Self::task_budget).
    #[must_use]
    pub fn tokens_used(&self) -> u64 {
        self.tokens_used.load(Ordering::Relaxed)
    }
}
