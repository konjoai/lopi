use crate::budget::BudgetScope;
use crate::task::{TaskId, TaskStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Human decision on a proposed plan (Phase 11 — plan approval gate). Carried
/// over a `oneshot` channel from the REST approve/reject endpoint to the paused
/// runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanDecision {
    /// Proceed to implementation.
    Approve,
    /// Abandon the task.
    Reject,
}

/// Rich event emitted by agents and consumed by TUI, WebSocket, and log panels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// A new task has been added to the queue.
    TaskQueued {
        /// Identifier of the queued task.
        task_id: TaskId,
        /// Natural-language goal for the task.
        goal: String,
        /// Scheduling priority of the task.
        priority: crate::task::Priority,
    },
    /// An agent has begun executing a task attempt.
    TaskStarted {
        /// Identifier of the task being started.
        task_id: TaskId,
        /// Attempt number, starting at 1.
        attempt: u8,
        /// Git branch created for this attempt.
        branch: String,
    },
    /// The task's lifecycle status has changed.
    StatusChanged {
        /// Identifier of the task whose status changed.
        task_id: TaskId,
        /// New status value.
        status: TaskStatus,
        /// Attempt number associated with this status change.
        attempt: u8,
    },
    /// A plan has been generated and is paused awaiting human approval
    /// (Phase 11 — plan approval gate).
    PlanProposed {
        /// Task whose plan is pending approval.
        task_id: TaskId,
        /// Attempt number whose plan this is.
        attempt: u8,
        /// Best-effort parsed plan steps (may be empty for free-form plans).
        steps: Vec<String>,
        /// Full plan text for review.
        plan: String,
    },
    /// A line of agent log output.
    LogLine {
        /// Task that produced this log line.
        task_id: TaskId,
        /// The log message text.
        line: String,
        /// Severity level of the log line.
        level: LogLevel,
        /// Timestamp when the line was emitted.
        ts: DateTime<Utc>,
    },
    /// The scoring engine has produced updated metrics for a task attempt.
    ScoreUpdated {
        /// Task whose score was updated.
        task_id: TaskId,
        /// Fraction of tests passing in the range `[0.0, 1.0]`.
        test_pass_rate: f32,
        /// Number of lint errors found.
        lint_errors: u32,
        /// Lines changed in the diff.
        diff_lines: u32,
    },
    /// A task has reached a terminal state (success or failure).
    TaskCompleted {
        /// Identifier of the completed task.
        task_id: TaskId,
        /// Final outcome status of the task.
        outcome: TaskStatus,
        /// Total number of attempts made before reaching this outcome.
        total_attempts: u8,
    },
    /// A task was cancelled before reaching a terminal state.
    TaskCancelled {
        /// Identifier of the cancelled task.
        task_id: TaskId,
    },
    /// Periodic snapshot of agent pool utilization statistics.
    PoolStats {
        /// Number of agents currently executing tasks.
        running: usize,
        /// Number of tasks waiting in the queue.
        queued: usize,
        /// Cumulative number of successfully completed tasks.
        succeeded: usize,
        /// Cumulative number of failed tasks.
        failed: usize,
        /// Seconds since the pool was started.
        uptime_secs: u64,
    },
    /// Periodic per-agent cognition metrics emitted during a run.
    /// Drives the Forge's live shader uniforms in lopi-ui.
    /// `pressure` and `activity` are normalized to `[0.0, 1.0]`.
    TurnMetrics {
        /// Identifier of the task emitting these metrics.
        task_id: TaskId,
        /// Context window fill — `ContextWindow::token_pressure()`.
        pressure: f32,
        /// Generation intensity (tokens/sec normalized against a soft cap).
        activity: f32,
        /// Raw output tokens per second.
        tokens_per_sec: f32,
        /// Accumulated cost in USD for this run.
        cost_usd: f32,
    },
    /// The Konjo Verifier completed its rubric-guided second-score pass (Sprint S).
    ///
    /// Emitted after the heuristic score passes, before the final commit.
    /// When `passed = false`, `fix_hints` have already been appended to the
    /// task's constraints and the runner will roll back and retry.
    VerifierVerdict {
        /// Task that was evaluated.
        task_id: TaskId,
        /// Whether the output satisfied all rubric criteria.
        passed: bool,
        /// Criteria not met, one sentence each.
        gaps: Vec<String>,
        /// Fix hints injected into the next retry's planning prompt.
        fix_hints: Vec<String>,
        /// Verifier confidence in the verdict, `[0.0, 1.0]`.
        confidence: f64,
    },
    /// The agent invoked a tool (decoded from an assistant `tool_use` block in
    /// the `claude -p --output-format stream-json` output). Drives the
    /// `ThoughtStream` pane's interleaved tool timeline.
    ToolCall {
        /// Session/task that issued the call.
        task_id: TaskId,
        /// Tool name as reported by the CLI, e.g. `Bash`, `Read`, `Edit`.
        tool: String,
        /// Short human summary of the tool input (command, file path, etc.).
        summary: String,
    },
    /// Result of a tool invocation (decoded from a `user` `tool_result` line).
    ToolResult {
        /// Session/task the result belongs to.
        task_id: TaskId,
        /// Tool that produced this result.
        tool: String,
        /// Whether the tool call reported an error.
        is_error: bool,
        /// Truncated preview of the result text.
        preview: String,
    },
    /// Incremental token usage during a turn (decoded from a `stream_event`
    /// `message_delta.usage`). Drives the `TokenGauge` pane.
    TokenDelta {
        /// Session/task emitting tokens.
        task_id: TaskId,
        /// Output tokens reported so far this turn.
        output_tokens: u32,
        /// Input tokens for this turn.
        input_tokens: u32,
        /// Tokens served from cache for this turn.
        cache_read_tokens: u32,
    },
    /// Rate-limit / throttle signal from the CLI (decoded from a
    /// `rate_limit_event` line). Lets the pool back off instead of hammering.
    ApiRetry {
        /// Session/task that observed the limit.
        task_id: TaskId,
        /// Status string, e.g. `allowed_warning`, `rejected`.
        status: String,
        /// Window type, e.g. `seven_day`, `five_hour`.
        limit_type: String,
        /// Fraction of the window consumed, clamped to `[0.0, 1.0]`.
        utilization: f32,
        /// When this window resets, unix seconds — `None` if the CLI omitted
        /// `resetsAt`. Five-hour windows are rolling from first use, not
        /// wall-clock fixed, so this is the only reliable way to know when.
        #[serde(default)]
        resets_at: Option<i64>,
    },
    /// Cumulative session cost reported by the CLI (decoded from the `result`
    /// envelope's `total_cost_usd`). Drives the `CostAnalytics` pane.
    Cost {
        /// Session/task the cost belongs to.
        task_id: TaskId,
        /// Cumulative USD cost reported by the CLI for this session.
        cost_usd: f64,
        /// Number of turns completed so far.
        num_turns: u32,
        /// The CLI `session_id` (UUID), empty if not yet known. Lets a thread
        /// be resumed later with `--resume <session_id>`.
        session_id: String,
    },
    /// Coarse phase label for a session (decoded from `system` `status` /
    /// `post_turn_summary`). Drives the `PhaseWheel` and tile status without a
    /// hardcoded cycle.
    Phase {
        /// Session/task whose phase changed.
        task_id: TaskId,
        /// Phase label, e.g. `requesting`, `review_ready`, `completed`.
        phase: String,
    },
    /// Cost governor refused the next billable call because a scope reached
    /// its hourly cap or a breaker tripped. Emitted by the runner before the
    /// error propagates so the UI can flag the breach immediately.
    BudgetExceeded {
        /// Optional — `None` when the breach is fleet-wide (no specific task
        /// in flight).
        task_id: Option<TaskId>,
        /// Which scope refused (`fleet`, `agent`, or `task`).
        scope: BudgetScope,
        /// The scope's hourly cap in USD.
        limit_usd: f64,
        /// Amount already burned in the rolling 1-hour window in USD.
        burned_usd: f64,
    },
    /// Report on Finish (Loop Engineering primitive 6) — a finished run's
    /// summary, addressed to the channel declared on the task
    /// (`Task::report`). Emitted from the L1 report-only finalize hook;
    /// reused instead of a direct `lopi-agent` → `lopi-remote` call so the
    /// two crates stay decoupled — a subscriber (e.g. `lopi-remote`'s
    /// Telegram notifier) is responsible for actually delivering it, and
    /// drops anything addressed to a channel it doesn't recognize.
    ReportReady {
        /// Task the report is about.
        task_id: TaskId,
        /// Declared channel name (e.g. `"telegram"`) — validated upstream by
        /// `crate::report::ReportChannel::parse`, but subscribers must still
        /// check it themselves; this event carries the raw string.
        channel: String,
        /// Rendered plain-text summary.
        summary: String,
    },
}

/// Severity level attached to [`AgentEvent::LogLine`] events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Informational message.
    Info,
    /// Warning — non-fatal but noteworthy condition.
    Warn,
    /// Error — a failure occurred.
    Error,
    /// Debug — verbose diagnostic output.
    Debug,
}

impl AgentEvent {
    /// Construct a `LogLine` event with the given level and the current timestamp.
    pub fn log(task_id: TaskId, line: impl Into<String>, level: LogLevel) -> Self {
        Self::LogLine {
            task_id,
            line: line.into(),
            level,
            ts: Utc::now(),
        }
    }

    /// Construct an `Info`-level `LogLine` event.
    pub fn info(task_id: TaskId, line: impl Into<String>) -> Self {
        Self::log(task_id, line, LogLevel::Info)
    }

    /// Construct a `Warn`-level `LogLine` event.
    pub fn warn(task_id: TaskId, line: impl Into<String>) -> Self {
        Self::log(task_id, line, LogLevel::Warn)
    }

    /// Construct an `Error`-level `LogLine` event.
    pub fn error(task_id: TaskId, line: impl Into<String>) -> Self {
        Self::log(task_id, line, LogLevel::Error)
    }
}

#[cfg(test)]
#[path = "event_wire_format_tests.rs"]
mod wire_format_tests;

/// Thin wrapper around `tokio::sync::broadcast` for workspace-wide event fanout.
#[derive(Clone)]
pub struct EventBus<T: Clone> {
    tx: broadcast::Sender<T>,
}

impl<T: Clone + Send + 'static> EventBus<T> {
    /// Create an `EventBus` with a broadcast channel of the given `capacity`.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Broadcast `event` to all current subscribers; silently drops if no receivers exist.
    pub fn send(&self, event: T) {
        let _ = self.tx.send(event);
    }

    /// Return a new receiver that will receive all future events on this bus.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.tx.subscribe()
    }

    /// Clone the underlying sender for use outside the bus wrapper.
    #[must_use]
    pub fn sender(&self) -> broadcast::Sender<T> {
        self.tx.clone()
    }
}

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;
