//! Plain data types for the agent pool: live handles, counters, and the
//! snapshot structs returned to the dashboard.

use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Live state of a single running agent.
#[derive(Debug)]
pub struct AgentHandle {
    /// Task goal text.
    pub goal: String,
    /// One-shot sender that signals the runner to stop; `None` after cancellation.
    pub cancel_tx: Option<oneshot::Sender<()>>,
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
