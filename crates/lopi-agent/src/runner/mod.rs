mod api_plan;
mod finalize;
mod plan_gate;
mod plan_steps;
pub mod postmortem;
mod postmortem_runner;
mod run_loop;
mod seed;
mod speculative;
mod stability_runner;
mod verifier_runner;

use crate::api_client::AnthropicClient;
use crate::stability::{StabilityConfig, StabilityHarness};
use lopi_context::{ContentBlock, ContextWindow, Phase, PinPolicy, Role, TaggedMessage};
use lopi_core::{
    AgentEvent, EventBus, PlanDecision, Score, ScoreWeights, SelfPromptStrategy, Task, TaskId,
    TaskStatus,
};
use lopi_memory::MemoryStore;
use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
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
            adaptive_retry: false,
            last_error: None,
            self_prompt: SelfPromptStrategy::default(),
            escalate_strategy: false,
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
        }
    }

    /// One-shot constructor — creates a standalone bus for `lopi run`.
    #[must_use]
    pub fn standalone(task: Task, repo_path: PathBuf) -> (Self, EventBus<AgentEvent>) {
        let bus: EventBus<AgentEvent> = EventBus::new(128);
        let (_cancel_tx, cancel_rx) = oneshot::channel();
        let runner = Self {
            bus: bus.clone(),
            task,
            repo_path,
            store: None,
            dry_run: false,
            speculative: false,
            context: ContextWindow::new(Self::CONTEXT_BUDGET),
            max_turns: 25,
            api_client: None,
            limiter: None,
            breaker: None,
            stability_harness: None,
            adaptive_retry: false,
            last_error: None,
            self_prompt: SelfPromptStrategy::default(),
            escalate_strategy: false,
            verifier_enabled: false,
            last_plan: None,
            session_id: Uuid::new_v4(),
            cancel_rx: Some(cancel_rx),
            plan_decision_rx: None,
            cancel_token: CancellationToken::new(),
            attempt_counter: Arc::new(AtomicUsize::new(0)),
            attempts_made: 0,
            turn_count: 0,
            score_weights: ScoreWeights::default(),
            task_lessons: vec![],
            skills: lopi_skill::SkillRegistry::default(),
        };
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

    pub(super) fn id(&self) -> TaskId {
        self.task.id
    }

    pub(super) fn log(&self, msg: impl Into<String>) {
        self.bus.send(AgentEvent::info(self.id(), msg));
    }

    pub(super) fn warn(&self, msg: impl Into<String>) {
        self.bus.send(AgentEvent::warn(self.id(), msg));
    }

    /// Broadcast a `StatusChanged` event and a `TurnMetrics` heartbeat.
    pub(super) fn status(&self, s: TaskStatus, attempt: u8) {
        let activity = match &s {
            TaskStatus::Planning => 0.45_f32,
            TaskStatus::AwaitingPlanApproval { .. } => 0.05_f32,
            TaskStatus::Implementing => 0.85_f32,
            TaskStatus::Testing => 0.55_f32,
            TaskStatus::Scoring => 0.30_f32,
            TaskStatus::Retrying { .. } => 0.40_f32,
            TaskStatus::Success { .. }
            | TaskStatus::Failed { .. }
            | TaskStatus::RolledBack
            | TaskStatus::Conflict { .. } => 0.0_f32,
            TaskStatus::Queued => 0.10_f32,
        };
        self.emit_turn_metrics(activity);
        self.bus.send(AgentEvent::StatusChanged {
            task_id: self.id(),
            status: s,
            attempt,
        });
    }

    /// Emit terminal bookkeeping for a finalized attempt and return its status.
    ///
    /// A genuine success pins the conclusion and marks an OTel `complete` span;
    /// a [`TaskStatus::Conflict`] (rebase collision) skips that — it is not a
    /// success — but both broadcast the status so the dashboards reflect reality.
    pub(super) fn conclude_finalized(
        &mut self,
        status: TaskStatus,
        score: &Score,
        attempt: u8,
    ) -> TaskStatus {
        if !matches!(status, TaskStatus::Conflict { .. }) {
            self.context.pin_conclusion(
                format!(
                    "Sprint succeeded — pass={:.0}% diff={}L",
                    score.test_pass_rate * 100.0,
                    score.diff_lines
                ),
                Phase::Conclusion,
            );
            tracing::info!(
                pressure = self.context.token_pressure(),
                "context at conclusion"
            );
            // OTel GenAI-aligned task-completion boundary span.
            let _ = tracing::info_span!(
                "lopi.agent.task.complete",
                task_id = %self.id(),
                outcome = "success",
                attempts = attempt,
            )
            .entered();
        }
        self.status(status.clone(), attempt);
        status
    }

    pub(super) fn emit_turn_metrics(&self, activity: f32) {
        let pressure = self.context.token_pressure();
        self.bus.send(AgentEvent::TurnMetrics {
            task_id: self.id(),
            pressure,
            activity,
            tokens_per_sec: 0.0,
            cost_usd: 0.0,
        });
    }

    pub(super) fn check_cancel(&mut self) -> bool {
        // Check the structured CancellationToken first (pool JoinSet teardown path).
        if self.cancel_token.is_cancelled() {
            self.log("⛔ cancelled via token");
            return true;
        }
        // Then check the legacy oneshot cancel channel (web API / CLI path).
        // A Closed channel means the sender was dropped (standalone/CLI path with no
        // active canceller) — that is NOT a cancellation, so we discard the receiver
        // and continue. Only an explicit Ok(()) send is a real cancel.
        if let Some(mut rx) = self.cancel_rx.take() {
            match rx.try_recv() {
                Ok(()) => {
                    self.log("⛔ cancelled by user");
                    return true;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.cancel_rx = Some(rx);
                }
                Err(oneshot::error::TryRecvError::Closed) => {
                    // Sender dropped (no active canceller) — proceed normally.
                }
            }
        }
        false
    }

    /// Pin the task goal as a Boot-phase turn so it's always visible across evictions.
    pub(super) fn boot_context(&mut self) {
        let content = vec![ContentBlock::Text(format!("Task goal: {}", self.task.goal))];
        let msg = TaggedMessage {
            id: Uuid::new_v4(),
            role: Role::User,
            content,
            tokens: 0,
            pin: PinPolicy::Always,
            phase: Phase::Boot,
            evict_after: None,
            tool_pair_id: None,
            is_conclusion: false,
        };
        self.context.push(msg).ok();
    }
}
