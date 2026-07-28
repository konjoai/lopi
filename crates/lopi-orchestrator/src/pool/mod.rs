//! Concurrent agent pool — spawns runners, enforces concurrency limits, and
//! emits events.
//!
//! The pool's surface is split across this module for readability:
//! - [`types`] — the plain handle/counter/snapshot structs.
//! - `registry` — per-agent rate limiting.
//! - `run_loop` — `submit`, the dispatch `run` loop, and single-task execution.
//! - `worktree` — per-task `git worktree` isolation setup/teardown.

mod dead_letter;
/// Sprint E — budget-aware admission (`submit_economically`) and
/// reservation reconciliation on task completion.
mod economics_admit;
mod registry;
mod run_loop;
mod run_loop_builder;
/// Sprint E, Part 4 — subscribes to the event bus, tracks live per-session
/// spend/progress, and pauses (cancel + handoff) any session a detector trips.
mod runaway_monitor;
mod skills;
mod types;
mod worktree;

use dashmap::DashMap;
use lopi_core::{AgentEvent, EventBus, TaskId};
use lopi_memory::MemoryStore;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio::task::JoinSet;

use crate::budget::reserve::ReservationId;
use crate::budget::Economics;
use crate::queue::TaskQueue;

pub use economics_admit::AdmissionOutcome;
pub use run_loop::effective_topology;
pub use types::{AgentHandle, PoolCounters, PoolStats, RunningAgentInfo};

/// Concurrent agent pool — spawns runners, enforces concurrency limits, and emits events.
#[derive(Clone)]
pub struct AgentPool {
    /// Global concurrency cap — across all repos.
    permits: Arc<Semaphore>,
    /// Per-repo semaphore — prevents one repo from monopolising all agent slots.
    repo_permits: Arc<DashMap<PathBuf, Arc<Semaphore>>>,
    max_agents: usize,
    queue: TaskQueue,
    repo_path: PathBuf,
    /// All `AgentEvent`s: `TaskQueued`, `TaskStarted`, `StatusChanged`, `LogLine`, `TaskCompleted`.
    bus: EventBus<AgentEvent>,
    store: Option<MemoryStore>,
    /// Live handles — entries removed when the task completes or is cancelled.
    handles: Arc<DashMap<TaskId, Arc<RwLock<AgentHandle>>>>,
    counters: Arc<PoolCounters>,
    started_at: Arc<std::time::Instant>,
    /// Structured task tracker — allows `shutdown()` to abort all running agents.
    join_set: Arc<Mutex<JoinSet<()>>>,
    /// P2 — per-agent rate limits (token bucket + concurrency cap),
    /// manageable via the `/api/agents/:id/rate-limit` REST surface.
    agent_rate_limits: Arc<DashMap<String, crate::agent_rate_limit::AgentRateState>>,
    /// Sprint E — the economics layer facade. `None` unless `[economics]`
    /// configures a pool (opt-in, per the brief).
    economics: Option<Arc<Economics>>,
    /// Sprint E — open reservations awaiting reconciliation, keyed by task.
    /// A reservation is inserted on successful `submit_economically` and
    /// removed (reconciled or released) at the task's one terminal choke
    /// point in `run_one` — never left to leak.
    economics_reservations: Arc<DashMap<TaskId, ReservationId>>,
}

impl AgentPool {
    /// Create a new pool bound to `repo_path`, capped at `max_agents` concurrent runners.
    #[must_use]
    pub fn new(
        max_agents: usize,
        repo_path: PathBuf,
        queue: TaskQueue,
        bus: EventBus<AgentEvent>,
    ) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(max_agents)),
            repo_permits: Arc::new(DashMap::new()),
            max_agents,
            queue,
            repo_path,
            bus,
            store: None,
            handles: Arc::new(DashMap::new()),
            counters: Arc::new(PoolCounters::default()),
            started_at: Arc::new(std::time::Instant::now()),
            join_set: Arc::new(Mutex::new(JoinSet::new())),
            agent_rate_limits: Arc::new(DashMap::new()),
            economics: None,
            economics_reservations: Arc::new(DashMap::new()),
        }
    }

    /// Abort all running agent tasks and wait for them to finish.
    /// Call this on graceful shutdown to avoid orphaned git operations.
    pub async fn shutdown(&self) {
        let mut js = self.join_set.lock().await;
        js.abort_all();
        while js.join_next().await.is_some() {}
    }

    /// Sprint E — attach the economics layer facade. No-op-equivalent
    /// (`submit_economically` falls back to plain `submit`) unless
    /// `[economics]` configured a pool.
    #[must_use]
    pub fn with_economics(mut self, economics: Economics) -> Self {
        self.economics = Some(Arc::new(economics));
        self
    }

    /// Attach a memory store; enables pattern persistence and cost tracking.
    #[must_use]
    pub fn with_store(mut self, store: MemoryStore) -> Self {
        self.store = Some(store);
        self
    }

    /// Clone the underlying task queue handle.
    #[must_use]
    pub fn queue(&self) -> TaskQueue {
        self.queue.clone()
    }

    /// Clone the event bus handle for subscribing to agent events.
    #[must_use]
    pub fn bus(&self) -> EventBus<AgentEvent> {
        self.bus.clone()
    }

    /// The economics layer facade, if `[economics]` configured a pool —
    /// for surfaces (web `/api/economics`, `lopi cost`, WhatsApp `cost`)
    /// that need to read current tier/headroom/runway.
    #[must_use]
    pub fn economics(&self) -> Option<Arc<Economics>> {
        self.economics.clone()
    }

    /// This pool's default repo path — the one `.lopi/loop.toml` (and hence
    /// [`LoopConfig::resolved_budget`](lopi_core::LoopConfig::resolved_budget))
    /// resolves against for a task with no per-task `repo_path` override.
    #[must_use]
    pub fn repo_path(&self) -> &std::path::Path {
        &self.repo_path
    }

    /// Cancel the first running task whose UUID string starts with `id_prefix`.
    /// Returns `true` if a cancel signal was sent.
    pub async fn cancel_by_prefix(&self, id_prefix: &str) -> bool {
        for entry in self.handles.iter() {
            if entry.key().to_string().starts_with(id_prefix) {
                let key = *entry.key();
                drop(entry); // release DashMap read lock before taking write
                return self.cancel(&key).await;
            }
        }
        false
    }

    /// Cancel a running task. Returns true if the cancel signal was sent.
    pub async fn cancel(&self, task_id: &TaskId) -> bool {
        if let Some(handle_ref) = self.handles.get(task_id) {
            let mut handle = handle_ref.write().await;
            if let Some(tx) = handle.cancel_tx.take() {
                let _ = tx.send(());
                self.bus
                    .send(AgentEvent::TaskCancelled { task_id: *task_id });
                return true;
            }
        }
        false
    }

    /// Phase 11 — deliver a plan-approval decision to a paused runner. Returns
    /// true if the task was awaiting approval and the decision was delivered.
    pub async fn decide_plan(&self, task_id: &TaskId, decision: lopi_core::PlanDecision) -> bool {
        if let Some(handle_ref) = self.handles.get(task_id) {
            let mut handle = handle_ref.write().await;
            if let Some(tx) = handle.plan_decision_tx.take() {
                return tx.send(decision).is_ok();
            }
        }
        false
    }

    /// Return a snapshot of current stats.
    #[must_use]
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            running: self.counters.running.load(Ordering::Relaxed),
            queued: self.queue.len(),
            succeeded: self.counters.succeeded.load(Ordering::Relaxed),
            failed: self.counters.failed.load(Ordering::Relaxed),
            uptime_secs: self.started_at.elapsed().as_secs(),
        }
    }

    /// Count of `PoolStats` events this pool has actually broadcast since
    /// it started (Sprint F3 Phase 5). Idle ticks with no subscriber are
    /// gated out of `run()`'s 1Hz loop and don't count here — this is the
    /// number that should stay flat while the pool sits idle with nobody
    /// watching.
    #[must_use]
    pub fn pool_stats_sent_count(&self) -> u64 {
        self.counters.pool_stats_sent.load(Ordering::Relaxed)
    }

    /// Snapshot of all currently running agents — suitable for fleet display.
    ///
    /// Uses non-blocking `try_read`; handles that cannot be locked are silently
    /// skipped (extremely rare in practice — only happens if the runner is in the
    /// middle of updating the handle at the same instant).
    #[must_use]
    pub fn running_agents(&self) -> Vec<RunningAgentInfo> {
        self.handles
            .iter()
            .filter_map(|entry| {
                let handle = entry.value().try_read().ok()?;
                Some(RunningAgentInfo {
                    task_id: entry.key().to_string(),
                    goal: handle.goal.clone(),
                    attempt: handle.attempt.load(Ordering::Relaxed),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod budget_tests;
#[cfg(test)]
mod tests;
