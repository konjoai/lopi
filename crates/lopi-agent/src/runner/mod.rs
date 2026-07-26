mod api_plan;
mod builder;
mod capture;
mod eval_runner;
mod finalize;
mod guardrails;
mod lifecycle;
mod plan_gate;
mod plan_steps;
pub mod postmortem;
mod postmortem_cli;
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
use crate::stability::StabilityHarness;
use lopi_context::ContextWindow;
use lopi_core::loop_config::OnFail;
use lopi_core::{AgentEvent, EventBus, PlanDecision, ScoreWeights, SelfPromptStrategy, Task};
use lopi_memory::MemoryStore;
use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize};
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
    /// Sprint F2 Phase 1 — `.lopi/loop.toml`'s `test_command` override,
    /// forwarded to the `Scorer` so an operator-named command wins over
    /// stack detection. `None` (the default) leaves detection as the sole
    /// source.
    pub(super) test_command: Option<String>,
}

impl AgentRunner {
    /// Token budget for the context window — 75% of a 200K-context Claude model.
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
            test_command: None,
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
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
