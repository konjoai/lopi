//! Plain data types for the agent pool: live handles, counters, and the
//! snapshot structs returned to the dashboard.

use lopi_core::PlanDecision;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::Arc;
use tokio::sync::oneshot;

/// Live state of a single running agent.
#[derive(Debug)]
pub struct AgentHandle {
    /// Task goal text.
    pub goal: String,
    /// One-shot sender that signals the runner to stop; `None` after cancellation.
    pub cancel_tx: Option<oneshot::Sender<()>>,
    /// Phase 11 — delivers the operator's plan-approval decision to a paused
    /// runner; `None` once consumed (or for ungated runs).
    pub plan_decision_tx: Option<oneshot::Sender<PlanDecision>>,
    /// Current attempt count — updated atomically by the runner, read lock-free.
    pub attempt: Arc<AtomicUsize>,
    /// Wall-clock time when this agent handle was created.
    pub started_at: std::time::Instant,
}

/// Shared counters for `/api/stats`.
#[derive(Default)]
pub struct PoolCounters {
    /// Number of agents currently executing.
    pub running: AtomicUsize,
    /// Cumulative count of successfully completed tasks.
    pub succeeded: AtomicUsize,
    /// Cumulative count of tasks that exhausted all retries.
    pub failed: AtomicUsize,
    /// Sprint F3 Phase 5 — count of `PoolStats` events this pool has
    /// actually broadcast (idle ticks with no subscriber are skipped, not
    /// counted). Scoped per-pool rather than a process-wide static so
    /// multi-repo mode's several pools (and tests spinning up several
    /// pools in the same process) don't share one counter.
    pub pool_stats_sent: AtomicU64,
}

/// Point-in-time snapshot of pool counters, returned by `AgentPool::stats()`.
pub struct PoolStats {
    /// Number of agents currently executing.
    pub running: usize,
    /// Number of tasks waiting in the queue.
    pub queued: usize,
    /// Cumulative successfully completed tasks since pool start.
    pub succeeded: usize,
    /// Cumulative failed tasks (exhausted retries) since pool start.
    pub failed: usize,
    /// Wall-clock seconds since the pool was created.
    pub uptime_secs: u64,
}

/// Snapshot of one running agent for display in fleet views.
pub struct RunningAgentInfo {
    /// Full UUID string — callers can truncate for display.
    pub task_id: String,
    /// The task goal text.
    pub goal: String,
    /// Current attempt number (1-based).
    pub attempt: usize,
}
