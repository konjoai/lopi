//! lopi-orchestrator — concurrent agent pool, priority task queue, and scheduler.

pub mod agent_rate_limit;
/// Stack-Chain-1 — server-side whole-stack cron scheduling: fires an ordered
/// sequence of independent goals, one per stack card, entirely server-side.
pub mod chain_schedule_manager;
pub mod health;
/// MAXX Phase 1 — opportunistic backlog dispatch tick.
pub mod maxx_loop;
/// Concurrent agent pool that drives task execution from a shared `TaskQueue`.
pub mod pool;
/// Sprint T — epsilon-greedy Q-learning router over task-type/agent-config.
pub mod q_router;
/// Priority task queue with deduplication and async blocking pop.
pub mod queue;
/// MAXX Phase 0 — quota headroom tracking, subscribed to the event bus.
pub mod quota_tracker;
/// Runtime-mutable cron scheduler backing the dashboard cron UI.
pub mod schedule_manager;
/// Cron-style task scheduler that injects recurring tasks into the queue.
pub mod scheduler;
/// Shared `Task`-from-spec construction for `schedule_manager` and `maxx_loop`.
mod task_build;
/// Sprint T — keyword-heuristic topology classifier for task dispatch.
pub mod topology;

pub use agent_rate_limit::{AgentRateLimit, AgentRateLimitSnapshot};
pub use chain_schedule_manager::{ChainScheduleManager, ChainSpec, OnFail};
pub use health::{AgentHealth, HealthConfig, HealthRegistry, HealthSnapshot, HealthSummary};
pub use maxx_loop::{
    build_task as build_maxx_task, is_favorable as maxx_is_favorable, MaxxLoop, MaxxSpec,
};
pub use pool::{AgentPool, PoolStats, RunningAgentInfo};
pub use q_router::{QRouter, QValueEntry, DEFAULT_ALPHA, DEFAULT_EPSILON};
pub use queue::TaskQueue;
pub use quota_tracker::{QuotaObservation, QuotaTracker};
pub use schedule_manager::{build_task as build_schedule_task, ScheduleManager, ScheduleSpec};
pub use scheduler::{boot as boot_scheduler, next_run_times};
pub use topology::{classify as classify_topology, TopologyClassification, CONFIDENCE_THRESHOLD};
