//! lopi-orchestrator — concurrent agent pool, priority task queue, and scheduler.

pub mod agent_rate_limit;
pub mod constellation;
pub mod health;
/// Concurrent agent pool that drives task execution from a shared `TaskQueue`.
pub mod pool;
/// Priority task queue with deduplication and async blocking pop.
pub mod queue;
/// Runtime-mutable cron scheduler backing the dashboard cron UI.
pub mod schedule_manager;
/// Cron-style task scheduler that injects recurring tasks into the queue.
pub mod scheduler;

pub use agent_rate_limit::{AgentRateLimit, AgentRateLimitSnapshot};
pub use schedule_manager::{build_task as build_schedule_task, ScheduleManager, ScheduleSpec};
pub use constellation::{
    Constellation, ConstellationMember, ConstellationRouter, ConstellationStats, DispatchDecision,
    MemberLoad, RoutingError, RoutingStrategy,
};
pub use health::{AgentHealth, HealthConfig, HealthRegistry, HealthSnapshot, HealthSummary};
pub use pool::{AgentPool, PoolStats, RunningAgentInfo};
pub use queue::TaskQueue;
pub use scheduler::{boot as boot_scheduler, next_run_times};
