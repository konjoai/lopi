mod api_plan;
mod helpers;
pub mod postmortem;
mod run_loop;

use crate::api_client::AnthropicClient;
use lopi_context::{ContentBlock, ContextWindow, Phase, PinPolicy, Role, TaggedMessage};
use lopi_core::{AgentEvent, EventBus, ScoreWeights, Task, TaskId};
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

pub struct AgentRunner {
    pub task: Task,
    pub repo_path: PathBuf,
    pub bus: EventBus<AgentEvent>,
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
    /// Sprint H — when true, retries inject the previous attempt's error
    /// log into the next planning prompt (Reflexion-style adaptive retry).
    /// Also enables the failure post-mortem when all retries fail.
    pub(super) adaptive_retry: bool,
    /// Sprint H — stash the most recent attempt failure context so the
    /// next attempt's prompt can include it. Cleared on success.
    pub(super) last_error: Option<String>,
    /// Stable session id used by `TurnMetrics.session_id`.
    pub(super) session_id: Uuid,
    pub(super) cancel_rx: Option<oneshot::Receiver<()>>,
    /// Second cancellation mechanism — compatible with `tokio_util::sync::CancellationToken`
    /// for structured cancellation from the pool `JoinSet`.
    pub(super) cancel_token: CancellationToken,
    pub(super) attempt_counter: Arc<AtomicUsize>,
    pub(super) attempts_made: u8,
    pub(super) turn_count: u32,
    /// Evolved score weights loaded from the pattern store's annotation signal.
    /// Loaded once per task run; defaults to `ScoreWeights::default()` when no
    /// annotated patterns are available.
    pub(super) score_weights: ScoreWeights,
}

impl AgentRunner {
    /// Token budget for the context window — 75% of Claude claude-sonnet-4-6's 200K context.
    const CONTEXT_BUDGET: usize = 150_000;

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
            adaptive_retry: false,
            last_error: None,
            session_id: Uuid::new_v4(),
            cancel_rx: Some(cancel_rx),
            cancel_token: CancellationToken::new(),
            attempt_counter,
            attempts_made: 0,
            turn_count: 0,
            score_weights: ScoreWeights::default(),
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
            adaptive_retry: false,
            last_error: None,
            session_id: Uuid::new_v4(),
            cancel_rx: Some(cancel_rx),
            cancel_token: CancellationToken::new(),
            attempt_counter: Arc::new(AtomicUsize::new(0)),
            attempts_made: 0,
            turn_count: 0,
            score_weights: ScoreWeights::default(),
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

    /// Returns true when adaptive retry is enabled.
    #[must_use]
    pub const fn adaptive_retry_enabled(&self) -> bool {
        self.adaptive_retry
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

    pub(super) fn check_cancel(&mut self) -> bool {
        // Check the structured CancellationToken first (pool JoinSet teardown path).
        if self.cancel_token.is_cancelled() {
            self.log("⛔ cancelled via token");
            return true;
        }
        // Then check the legacy oneshot cancel channel (web API / CLI path).
        if let Some(mut rx) = self.cancel_rx.take() {
            match rx.try_recv() {
                Ok(()) | Err(oneshot::error::TryRecvError::Closed) => {
                    self.log("⛔ cancelled by user");
                    return true;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.cancel_rx = Some(rx);
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
