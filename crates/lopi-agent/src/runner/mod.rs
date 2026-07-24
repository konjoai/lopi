mod api_plan;
mod capture;
mod eval_runner;
mod finalize;
mod guardrails;
mod lifecycle;
mod plan_gate;
mod plan_steps;
pub mod postmortem;
mod postmortem_runner;
mod progress;
mod reflection;
mod run_loop;
mod schema_gate;
mod seed;
mod speculative;
mod stability_runner;
mod stream;
mod terminal_errors;
mod test_phase;
mod verifier_runner;

use crate::api_client::AnthropicClient;
use crate::stability::{StabilityConfig, StabilityHarness};
use lopi_context::ContextWindow;
use lopi_core::loop_config::OnFail;
use lopi_core::{AgentEvent, EventBus, PlanDecision, ScoreWeights, SelfPromptStrategy, Task};
use lopi_memory::MemoryStore;
use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Full-jitter exponential backoff for transient failures.
///
/// Computes: sleep = Uniform(0, min(cap, base * 2^attempt))
///
/// This is the "Full Jitter" strategy from the AWS Architecture blog:
/// avoids thundering-herd by randomising the wait uniformly over [0, ceiling].
pub(super) fn backoff_secs(attempt: u8, base_ms: u64) -> Duration {
    let cap_ms: u64 = 30_000;
    let ceiling = (base_ms * (1u64 << attempt.min(10))).min(cap_ms);
    // rand::random is seeded from OS entropy — safe and lock-free.
    let jitter = rand::random::<u64>() % ceiling.max(1);
    Duration::from_millis(jitter)
}

/// Orchestrates the plan → implement → test → score → retry loop for a single task.
pub struct AgentRunner {
    /// The task being executed by this runner.
    pub task: Task,
    /// Filesystem path to the git repository being modified.
    pub repo_path: PathBuf,
    /// Event bus for broadcasting agent lifecycle and status events.
    pub bus: EventBus<AgentEvent>,
    /// Optional persistent memory store for patterns and task history.
    pub store: Option<MemoryStore>,
    /// When true: generate and print the plan, then exit without touching git.
    pub dry_run: bool,
    /// When true: apply plan steps speculatively as they stream instead of waiting for the full plan.
    pub speculative: bool,
    /// Session context window — tracks phase transitions and token pressure across the agent run.
    pub context: ContextWindow,
    /// Hard upper bound on total attempt iterations before the runner gives up.
    /// Prevents runaway agents from looping indefinitely when `task.max_retries` is very high.
    /// `0` is the infinite-loop sentinel (set from `Task::max_iterations`'s
    /// `Some(0)`, or the repo's `.lopi/loop.toml` when unset) — the cap is
    /// skipped entirely rather than firing on the very first turn, matching
    /// the "0 = disabled" convention already used by `no_progress_limit`.
    pub max_turns: u32,
    /// Optional direct-API client. When present (and the breaker is closed),
    /// the planning step uses `AnthropicClient::stream_plan` with prompt
    /// caching instead of the `claude` CLI subprocess. CLI remains the
    /// implementation path because it has full filesystem tool access.
    pub(super) api_client: Option<Arc<AnthropicClient>>,
    /// Optional rate limiter — concurrent TPM + RPM enforcement. Acquired
    /// before every API request.
    pub(super) limiter: Option<Arc<AnthropicLimiter>>,
    /// Optional circuit breaker — opens on consecutive failures or hourly
    /// cost cap. Checked before every API request; cost recorded on success.
    pub(super) breaker: Option<Arc<CircuitBreaker>>,
    /// Sprint I — optional Layer 5 patch stability gate. When set, `run()`
    /// generates N plan samples before the first implementation attempt and
    /// blocks if pairwise variance exceeds the configured threshold.
    pub(super) stability_harness: Option<StabilityHarness>,
    /// Sprint I — the stability gate's consensus plan (the sample closest to
    /// every other sample), stashed by `run_stability_preflight` on a
    /// `Stable`/`Warning` verdict so `gather_seed` can seed the first
    /// attempt's planning prompt with it instead of discarding it. `None`
    /// when no harness is configured, the gate blocked the run, or seeding
    /// has already consumed it.
    pub(super) consensus_plan_hint: Option<String>,
    /// Sprint H — when true, retries inject the previous attempt's error
    /// log into the next planning prompt (Reflexion-style adaptive retry).
    /// Also enables the failure post-mortem when all retries fail.
    pub(super) adaptive_retry: bool,
    /// Sprint H — stash the most recent attempt failure context so the
    /// next attempt's prompt can include it. Cleared on success.
    pub(super) last_error: Option<String>,
    /// Phase 16.4 — self-prompting strategy: how a failed attempt is reframed
    /// into the next attempt's self-prompt. [`Direct`](SelfPromptStrategy::Direct)
    /// reproduces the legacy raw-failure injection; richer strategies add a
    /// Reflexion / Self-Refine / Plan-Then-Act preamble. Only consulted when
    /// `adaptive_retry` is enabled.
    pub(super) self_prompt: SelfPromptStrategy,
    /// Phase 16.5 — when `true`, the self-prompt strategy escalates one rung up
    /// the S1→S4 ladder on each failed attempt (from `self_prompt`) instead of
    /// staying pinned. Only consulted when `adaptive_retry` is enabled.
    pub(super) escalate_strategy: bool,
    /// Phase 16.6 — per-run token budget forwarded to the Anthropic `task_budget`
    /// parameter on the direct-API planning path. `None` (the default) sends no
    /// budget; `Some(n)` lets the model self-pace within `n` tokens on supported
    /// models. Wired from [`LoopConfig::budget_tokens`](lopi_core::LoopConfig).
    pub(super) task_budget: Option<u64>,
    /// Per-session USD cost ceiling passed to `claude -p` as `--max-budget-usd`
    /// on the streaming path. `None` (the default) sets no CLI budget cap.
    pub(super) cli_budget_usd: Option<f64>,
    /// Wired from `LoopConfig::permission_allow` — forwarded as `claude -p`'s
    /// `--allowedTools`. Empty (the default) adds nothing.
    pub(super) permission_allow: Vec<String>,
    /// Wired from `LoopConfig::permission_deny` — forwarded as `claude -p`'s
    /// `--disallowedTools`. Empty (the default) denies nothing.
    pub(super) permission_deny: Vec<String>,
    /// Sprint S — when true, the Konjo Verifier second-score pass runs after
    /// the heuristic score passes. Requires `api_client` to be set.
    pub(super) verifier_enabled: bool,
    /// Sprint S — plan text from the most recent planning step, used by the
    /// verifier to provide intent context when grading the diff.
    pub(super) last_plan: Option<String>,
    /// Stable session id used by `TurnMetrics.session_id`.
    pub(super) session_id: Uuid,
    pub(super) cancel_rx: Option<oneshot::Receiver<()>>,
    /// Phase 11 — receives the human plan-approval decision when the task is
    /// gated. `None` for ungated runs (standalone/CLI), in which case the gate
    /// auto-approves rather than stalling without a UI to decide.
    pub(super) plan_decision_rx: Option<oneshot::Receiver<PlanDecision>>,
    /// Second cancellation mechanism — compatible with `tokio_util::sync::CancellationToken`
    /// for structured cancellation from the pool `JoinSet`.
    pub(super) cancel_token: CancellationToken,
    pub(super) attempt_counter: Arc<AtomicUsize>,
    pub(super) attempts_made: u8,
    pub(super) turn_count: u32,
    /// Phase 5b — score weights for weighted scoring during retry loops.
    pub(super) score_weights: ScoreWeights,
    /// Phase 5b — lessons learned from past patterns (injected into planning prompt).
    pub(super) task_lessons: Vec<String>,
    /// Pentad M2.2 — skills available to inject into the planning prompt. Those
    /// whose triggers match the task goal are added as context (and recorded in
    /// the audit trail) during seeding. Empty by default — no skills, no change.
    pub(super) skills: lopi_skill::SkillRegistry,
    /// Guardrail precondition — the task's or repo's effective `gate`
    /// command (task overrides repo; see `lopi_orchestrator::pool::run_loop::build_runner`).
    /// `None` (the default) means no precondition. Set by the pool the same
    /// way `max_turns` is — hence `pub`, not `pub(super)`.
    pub gate: Option<String>,
    /// Guardrail exit-condition — the effective `until` command. `None`
    /// (the default) means scoring/`max_iterations` remain the sole stop
    /// conditions, unchanged from before this field existed.
    pub until: Option<String>,
    /// Effective on-fail policy for a failed iteration. Defaults to
    /// [`OnFail::Stop`].
    pub on_fail: OnFail,
    /// Progress-Gating (A3) — cumulative token usage metered across the whole
    /// run, summed from every streamed `TokenUsage` event (input + output).
    /// Shared with the stream-forwarding closures so metering happens at the
    /// one point tokens are actually observed. Read by the budget gate to stop
    /// the loop with [`StopReason::Budget`](lopi_core::StopReason) on exceed.
    pub(super) tokens_used: Arc<AtomicU64>,
    /// A2 (reflection) — when `true`, the runner **captures** a durable learning
    /// from every rejected/rolled-back attempt (before A3's rollback discards it)
    /// and **retrieves** relevance-filtered, bounded learnings from memory into
    /// the next planning prompt. `false` (the default) is behavior-identical to
    /// before A2: no capture, no injection. Off-by-default is the §2 discipline —
    /// cross-run reflection stays flagged until a live three-arm run clears the
    /// pre-registered margin against blind retry.
    pub(super) reflect_cross_run: bool,
    /// Sprint Successor-1 — a successor task stashed by
    /// `finalize::derive_and_stash_successor`, collected once `run()`
    /// returns. See `take_pending_successor`'s own doc comment.
    pub(super) pending_successor: Option<Task>,
}

impl AgentRunner {
    /// Token budget for the context window — 75% of Claude claude-sonnet-4-6's 200K context.
    const CONTEXT_BUDGET: usize = 150_000;

    /// Create a new runner wired into the given bus, store, and cancellation channel.
    pub fn new(
        task: Task,
        repo_path: PathBuf,
        bus: EventBus<AgentEvent>,
        store: Option<MemoryStore>,
        cancel_rx: oneshot::Receiver<()>,
        attempt_counter: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            task,
            repo_path,
            bus,
            store,
            dry_run: false,
            speculative: false,
            context: ContextWindow::new(Self::CONTEXT_BUDGET),
            max_turns: 25,
            api_client: None,
            limiter: None,
            breaker: None,
            stability_harness: None,
            consensus_plan_hint: None,
            adaptive_retry: false,
            last_error: None,
            self_prompt: SelfPromptStrategy::default(),
            escalate_strategy: false,
            task_budget: None,
            cli_budget_usd: None,
            permission_allow: Vec::new(),
            permission_deny: Vec::new(),
            verifier_enabled: false,
            last_plan: None,
            session_id: Uuid::new_v4(),
            cancel_rx: Some(cancel_rx),
            plan_decision_rx: None,
            cancel_token: CancellationToken::new(),
            attempt_counter,
            attempts_made: 0,
            turn_count: 0,
            score_weights: ScoreWeights::default(),
            task_lessons: vec![],
            skills: lopi_skill::SkillRegistry::default(),
            gate: None,
            until: None,
            on_fail: OnFail::default(),
            tokens_used: Arc::new(AtomicU64::new(0)),
            reflect_cross_run: false,
            pending_successor: None,
        }
    }

    /// One-shot constructor — creates a standalone bus for `lopi run`.
    ///
    /// Delegates to [`new`](Self::new) for all field defaults so the two
    /// constructors cannot drift; it only supplies a fresh bus, a dropped
    /// cancel channel, and a zeroed attempt counter.
    #[must_use]
    pub fn standalone(task: Task, repo_path: PathBuf) -> (Self, EventBus<AgentEvent>) {
        let bus: EventBus<AgentEvent> = EventBus::new(128);
        let (_cancel_tx, cancel_rx) = oneshot::channel();
        let runner = Self::new(
            task,
            repo_path,
            bus.clone(),
            None,
            cancel_rx,
            Arc::new(AtomicUsize::new(0)),
        );
        (runner, bus)
    }

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

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
