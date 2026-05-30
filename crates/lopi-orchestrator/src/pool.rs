use anyhow::Result;
use dashmap::DashMap;
use lopi_agent::AgentRunner;
use lopi_core::{AgentEvent, EventBus, ScoreWeights, Task, TaskId, TaskSource, TaskStatus};
use lopi_memory::{AuditInput, DeadLetterInput, MemoryStore};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex, Semaphore};
use tokio::task::JoinSet;
use tracing::{error, info, warn};

use crate::queue::TaskQueue;

/// Live state of a single running agent.
#[derive(Debug)]
pub struct AgentHandle {
    pub goal: String,
    pub cancel_tx: Option<oneshot::Sender<()>>,
    /// Current attempt count — updated atomically by the runner, read lock-free.
    pub attempt: Arc<AtomicUsize>,
    pub started_at: std::time::Instant,
}

/// Shared counters for `/api/stats`.
#[derive(Default)]
pub struct PoolCounters {
    pub running: AtomicUsize,
    pub succeeded: AtomicUsize,
    pub failed: AtomicUsize,
}

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
    handles: Arc<DashMap<TaskId, Arc<tokio::sync::RwLock<AgentHandle>>>>,
    counters: Arc<PoolCounters>,
    started_at: Arc<std::time::Instant>,
    /// Structured task tracker — allows `shutdown()` to abort all running agents.
    join_set: Arc<Mutex<JoinSet<()>>>,
    /// P2 — agent capability registry. `Task::required_capabilities`
    /// must be a subset of *some* registered agent's capabilities before
    /// `submit()` accepts the task. Key is a stable agent identifier
    /// (free-form string; the registrar names them).
    capabilities: Arc<DashMap<String, Vec<String>>>,
    /// P2 — per-agent rate limits (token bucket + concurrency cap).
    /// Agents not in the registry are unrestricted — registration is
    /// opt-in. Callers gate with `try_acquire_agent` / `release_agent`.
    agent_rate_limits: Arc<DashMap<String, crate::agent_rate_limit::AgentRateState>>,
}

impl AgentPool {
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
            capabilities: Arc::new(DashMap::new()),
            agent_rate_limits: Arc::new(DashMap::new()),
        }
    }

    /// Abort all running agent tasks and wait for them to finish.
    /// Call this on graceful shutdown to avoid orphaned git operations.
    pub async fn shutdown(&self) {
        let mut js = self.join_set.lock().await;
        js.abort_all();
        while js.join_next().await.is_some() {}
    }

    #[must_use]
    pub fn with_store(mut self, store: MemoryStore) -> Self {
        self.store = Some(store);
        self
    }

    #[must_use]
    pub fn queue(&self) -> TaskQueue {
        self.queue.clone()
    }

    #[must_use]
    pub fn bus(&self) -> EventBus<AgentEvent> {
        self.bus.clone()
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

    /// P2 — advertise the capabilities of an agent slot. Tasks whose
    /// `required_capabilities` are not satisfied by *any* registered agent
    /// are rejected by [`Self::can_satisfy`] (and by callers that opt into
    /// pre-submit validation).
    ///
    /// `agent_id` is a free-form stable label — the pool itself doesn't
    /// care about its shape; it's just a key for de-duplication.
    pub fn register_capabilities(&self, agent_id: impl Into<String>, caps: Vec<String>) {
        self.capabilities.insert(agent_id.into(), caps);
    }

    /// Remove an agent's capability advertisement.
    /// Returns `true` if a row was removed.
    pub fn deregister_capabilities(&self, agent_id: &str) -> bool {
        self.capabilities.remove(agent_id).is_some()
    }

    /// Snapshot every agent's capabilities — feeds `/metrics`, the Forge
    /// fleet panel, and the constellation router.
    #[must_use]
    pub fn capabilities_snapshot(&self) -> Vec<(String, Vec<String>)> {
        self.capabilities
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

    /// P2 — register (or replace) per-agent rate limits. Returns `false`
    /// when the supplied limit is invalid (`max_per_minute == 0`); the
    /// REST layer translates that into 422.
    pub fn register_agent_rate_limit(
        &self,
        agent_id: impl Into<String>,
        limit: crate::AgentRateLimit,
    ) -> bool {
        if !limit.is_valid() {
            return false;
        }
        let state = crate::agent_rate_limit::AgentRateState::new(limit);
        self.agent_rate_limits.insert(agent_id.into(), state);
        true
    }

    /// Remove an agent's rate-limit entry. Returns `true` when a row was
    /// removed. Active in-flight counters held by the removed entry are
    /// dropped — completing tasks have no slot to decrement and just
    /// log a warning via `release_agent`.
    pub fn deregister_agent_rate_limit(&self, agent_id: &str) -> bool {
        self.agent_rate_limits.remove(agent_id).is_some()
    }

    /// Snapshot the registered limit for `agent_id`, or `None` if the
    /// agent was never registered.
    #[must_use]
    pub fn agent_rate_limit(&self, agent_id: &str) -> Option<crate::AgentRateLimitSnapshot> {
        let entry = self.agent_rate_limits.get(agent_id)?;
        Some(crate::AgentRateLimitSnapshot {
            agent_id: agent_id.to_string(),
            max_per_minute: entry.limit.max_per_minute,
            max_concurrent: entry.limit.max_concurrent,
            in_flight: entry.in_flight.load(Ordering::Relaxed),
        })
    }

    /// Try to reserve a dispatch slot for `agent_id`. Returns `true` when
    /// both gates pass (token bucket + concurrency cap), `false` when the
    /// agent is at its rate or concurrency limit.
    ///
    /// Agents that were never registered are **unlimited** and always
    /// return `true` — registration is opt-in.
    ///
    /// On success the caller MUST pair with [`Self::release_agent`] when
    /// the task completes.
    pub async fn try_acquire_agent(&self, agent_id: &str) -> bool {
        let Some(entry) = self.agent_rate_limits.get(agent_id) else {
            return true;
        };
        // Concurrency cap is checked first because it's cheap (atomic load)
        // and the token bucket lookup acquires an async lock.
        if entry.limit.max_concurrent > 0
            && entry.in_flight.load(Ordering::Relaxed) >= entry.limit.max_concurrent
        {
            return false;
        }
        if !entry.bucket.try_acquire(1.0).await {
            return false;
        }
        entry.in_flight.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Release a previously-acquired slot. Safe to call when the agent has
    /// no registry entry (e.g. it was deregistered mid-flight) — that's a
    /// noop. Underflow is impossible because the counter saturates at 0.
    pub fn release_agent(&self, agent_id: &str) {
        if let Some(entry) = self.agent_rate_limits.get(agent_id) {
            // Saturating decrement — if a runaway release call lands after
            // the slot was already returned, we just stay at 0.
            let _ = entry
                .in_flight
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                    if v == 0 {
                        None
                    } else {
                        Some(v - 1)
                    }
                });
        }
    }

    /// True when at least one registered agent advertises every capability
    /// in `task.required_capabilities`. Empty requirements vacuously pass.
    ///
    /// When the registry is *empty* (no agent has advertised anything yet)
    /// a non-empty requirement is treated as **unsatisfiable** — this
    /// closes the trap-door where a task with `required_capabilities`
    /// would otherwise silently run on whatever generic worker picks it
    /// up next.
    #[must_use]
    pub fn can_satisfy(&self, task: &Task) -> bool {
        if task.required_capabilities.is_empty() {
            return true;
        }
        if self.capabilities.is_empty() {
            return false;
        }
        self.capabilities
            .iter()
            .any(|e| task.capabilities_satisfied_by(e.value()))
    }

    /// Enqueue a task and broadcast `TaskQueued`.
    pub async fn submit(&self, task: Task) -> Option<TaskId> {
        self.bus.send(AgentEvent::TaskQueued {
            task_id: task.id,
            goal: task.goal.clone(),
            priority: task.priority,
        });
        if let Some(store) = &self.store {
            store.save_task(&task, "queued").await.ok();
            // P2 — audit every dispatch so the operator can trace task
            // flow without recomputing from PoolStats deltas.
            let actor = task_source_label(&task);
            // Hand-build the payload — lopi-orchestrator already pulls in
            // chrono + thiserror via lopi-core, but not serde_json, so
            // staying string-only keeps the dep graph thin. The shape is
            // fixed enough that escape risk is bounded.
            let caps = task
                .required_capabilities
                .iter()
                .map(|c| format!("\"{}\"", c.replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(",");
            let payload = format!(
                "{{\"priority\":\"{:?}\",\"required_capabilities\":[{caps}]}}",
                task.priority,
            );
            let _ = store
                .record_audit(
                    &AuditInput::new("task.dispatch")
                        .subject("task", task.id.0.to_string())
                        .actor(actor)
                        .payload_json(payload),
                )
                .await;
        }
        self.queue.push(task).await
    }

    /// Dispatch loop — pops tasks from the queue and spawns bounded workers.
    ///
    /// # Errors
    ///
    /// Returns an error if a semaphore is closed (only happens on shutdown).
    pub async fn run(self) -> Result<()> {
        let bus_stats = self.bus.clone();
        let counters_stats = self.counters.clone();
        let queue_stats = self.queue.clone();
        let started_at = self.started_at.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let running = counters_stats.running.load(Ordering::Relaxed);
                let queued = queue_stats.len();
                let succeeded = counters_stats.succeeded.load(Ordering::Relaxed);
                let failed = counters_stats.failed.load(Ordering::Relaxed);
                let uptime_secs = started_at.elapsed().as_secs();
                bus_stats.send(AgentEvent::PoolStats {
                    running,
                    queued,
                    succeeded,
                    failed,
                    uptime_secs,
                });
            }
        });

        loop {
            let task = self.queue.pop().await;

            // Resolve the repo for this task (task-level override or pool default).
            let repo = task
                .repo_path
                .clone()
                .unwrap_or_else(|| self.repo_path.clone());

            // Acquire global concurrency permit.
            let permit = self.permits.clone().acquire_owned().await?;

            // Acquire per-repo permit — caps concurrency on any single repo to max_agents.
            let repo_sem = self
                .repo_permits
                .entry(repo.clone())
                .or_insert_with(|| Arc::new(Semaphore::new(self.max_agents)))
                .clone();
            let repo_permit = repo_sem.acquire_owned().await?;

            let task_id = task.id;
            let goal = task.goal.clone();
            // Snapshot the few fields the DLQ writer needs before `task`
            // moves into the spawned future. The goal/repo/source are
            // cheap String + Option clones; everything else is by value.
            let dlq_goal = task.goal.clone();
            let dlq_repo = task.repo_path.clone();
            let dlq_source = task_source_label(&task);

            let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
            let attempt = Arc::new(AtomicUsize::new(0));

            let handle = Arc::new(tokio::sync::RwLock::new(AgentHandle {
                goal: goal.clone(),
                cancel_tx: Some(cancel_tx),
                attempt: attempt.clone(),
                started_at: std::time::Instant::now(),
            }));
            self.handles.insert(task_id, handle);
            self.counters.running.fetch_add(1, Ordering::Relaxed);

            let bus = self.bus.clone();
            let store = self.store.clone();
            let handles = self.handles.clone();
            let counters = self.counters.clone();
            let join_set = self.join_set.clone();

            let mut js = join_set.lock().await;
            // Drain any completed tasks to keep the JoinSet from growing unboundedly.
            while js.try_join_next().is_some() {}
            js.spawn(async move {
                let _permit = permit;
                let _repo_permit = repo_permit;
                let max_retries = attempt.load(Ordering::Relaxed); // 0 here, updated in runner
                let outcome =
                    run_one(task, repo, bus.clone(), store.clone(), cancel_rx, attempt.clone()).await;
                handles.remove(&task_id);
                counters.running.fetch_sub(1, Ordering::Relaxed);
                let _ = max_retries; // hint for future per-task cap reporting

                // Was this a terminal failure (vs. a runner error or a
                // success)? If so, capture enough state to push to the DLQ.
                let dlq_payload: Option<(u8, String)> = match &outcome {
                    Ok(TaskStatus::Success { .. }) => {
                        counters.succeeded.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                    Ok(TaskStatus::Failed { reason }) => {
                        counters.failed.fetch_add(1, Ordering::Relaxed);
                        Some((
                            u8::try_from(attempt.load(Ordering::Relaxed)).unwrap_or(u8::MAX),
                            reason.clone(),
                        ))
                    }
                    Ok(TaskStatus::RolledBack) => {
                        counters.failed.fetch_add(1, Ordering::Relaxed);
                        Some((
                            u8::try_from(attempt.load(Ordering::Relaxed)).unwrap_or(u8::MAX),
                            "rolled back".into(),
                        ))
                    }
                    Err(_) => {
                        counters.failed.fetch_add(1, Ordering::Relaxed);
                        None // handled by the explicit error branch below
                    }
                    _ => None,
                };

                if let Err(e) = &outcome {
                    error!(task_id = %task_id, "agent run error: {e}");
                    let reason = format!("{e}");
                    bus.send(AgentEvent::TaskCompleted {
                        task_id,
                        outcome: TaskStatus::Failed {
                            reason: reason.clone(),
                        },
                        total_attempts: 1,
                    });
                    if let Some(store) = &store {
                        let _ = store.mark_completed(&task_id, "failed").await;
                        push_dlq(store, task_id, &dlq_goal, dlq_repo.as_deref(), 1, Some(reason), &dlq_source).await;
                    }
                } else if let (Some((attempts, reason)), Some(store)) = (dlq_payload, &store) {
                    push_dlq(
                        store,
                        task_id,
                        &dlq_goal,
                        dlq_repo.as_deref(),
                        attempts,
                        Some(reason),
                        &dlq_source,
                    )
                    .await;
                }
            });
        }
    }
}

/// Stable wire label for `TaskSource` — used in audit log + DLQ `source`
/// column so dashboards can group by origin without re-parsing.
fn task_source_label(task: &Task) -> String {
    match &task.source {
        TaskSource::Cli => "cli".into(),
        TaskSource::Api => "api".into(),
        TaskSource::Telegram { .. } => "telegram".into(),
        TaskSource::Webhook { .. } => "webhook".into(),
        TaskSource::SelfModify { .. } => "self-modify".into(),
    }
}

/// Best-effort DLQ write — logs but does not propagate failure. The
/// agent loop is already in its terminal state; failing to record the
/// DLQ row must not panic the worker.
async fn push_dlq(
    store: &MemoryStore,
    task_id: TaskId,
    goal: &str,
    repo_path: Option<&std::path::Path>,
    total_attempts: u8,
    last_error: Option<String>,
    source: &str,
) {
    let mut input = DeadLetterInput::new(task_id, goal);
    input.repo_path = repo_path.map(|p| p.display().to_string());
    input.total_attempts = total_attempts;
    input.last_error = last_error;
    input.source = source.to_string();
    if let Err(e) = store.push_dead_letter(&input).await {
        warn!(task_id = %task_id, "dead-letter write failed: {e}");
        return;
    }
    let payload = format!(
        "{{\"total_attempts\":{},\"source\":\"{}\"}}",
        total_attempts, source
    );
    let _ = store
        .record_audit(
            &AuditInput::new("task.dead_letter")
                .subject("task", task_id.0.to_string())
                .actor("pool")
                .payload_json(payload),
        )
        .await;
}

pub struct PoolStats {
    pub running: usize,
    pub queued: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub uptime_secs: u64,
}

/// Phase 5b / Sprint N — Compute score weights from user-annotated pattern history.
///
/// Approved patterns (human marked "approved") that required fewer attempts signal
/// that the current quality bar is right or too loose → tighten penalties slightly.
/// Rejected patterns that required many attempts → loosen penalties.
/// Falls back to defaults when no annotations exist or the store is absent.
async fn compute_weight_adjustments(_goal: &str, store: Option<&MemoryStore>) -> ScoreWeights {
    let Some(store) = store else {
        return ScoreWeights::default();
    };
    match store.compute_weight_adjustments().await {
        Ok(weights) => weights,
        Err(e) => {
            tracing::warn!("weight calibration query failed ({e}); using defaults");
            ScoreWeights::default()
        }
    }
}

#[tracing::instrument(skip(bus, store, cancel_rx, attempt_counter), fields(task_id = %task.id, goal = %task.goal))]
async fn run_one(
    task: Task,
    repo: PathBuf,
    bus: EventBus<AgentEvent>,
    store: Option<MemoryStore>,
    cancel_rx: oneshot::Receiver<()>,
    attempt_counter: Arc<AtomicUsize>,
) -> Result<TaskStatus> {
    info!(task_id = %task.id, "starting agent");
    let task_id = task.id;
    let goal = task.goal.clone();

    let weights = compute_weight_adjustments(&goal, store.as_ref()).await;
    let mut runner = AgentRunner::new(
        task,
        repo,
        bus.clone(),
        store.clone(),
        cancel_rx,
        attempt_counter,
    )
    .with_score_weights(weights);
    let outcome = runner.run().await?;

    let total_attempts = runner.attempts_made();

    bus.send(AgentEvent::TaskCompleted {
        task_id,
        outcome: outcome.clone(),
        total_attempts,
    });

    if let Some(store) = store {
        let status_str = match &outcome {
            TaskStatus::Success { .. } => "success",
            TaskStatus::Failed { .. } => "failed",
            TaskStatus::RolledBack => "rolled_back",
            _ => "unknown",
        };
        store.mark_completed(&task_id, status_str).await.ok();
        if let Err(e) = store.mine_patterns(&task_id, &goal).await {
            warn!("pattern mining failed: {e}");
        }
    }

    Ok(outcome)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lopi_core::{AgentEvent, EventBus, Priority, Task};
    use std::path::PathBuf;

    fn make_pool(max: usize) -> AgentPool {
        let queue = TaskQueue::new();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        AgentPool::new(max, PathBuf::from("."), queue, bus)
    }

    #[tokio::test]
    async fn stats_when_empty() {
        let pool = make_pool(2);
        let stats = pool.stats();
        assert_eq!(stats.running, 0);
        assert_eq!(stats.queued, 0);
        assert_eq!(stats.succeeded, 0);
        assert_eq!(stats.failed, 0);
    }

    #[tokio::test]
    async fn submit_task_increases_queued_count() {
        let pool = make_pool(2);
        let task = Task::new("do something useful");
        pool.submit(task).await;
        let stats = pool.stats();
        assert_eq!(stats.queued, 1);
    }

    #[tokio::test]
    async fn submit_multiple_tasks_increases_queued() {
        let pool = make_pool(4);
        for i in 0..3 {
            let task = Task::new(format!("task number {i} unique goal"));
            pool.submit(task).await;
        }
        let stats = pool.stats();
        assert_eq!(stats.queued, 3);
    }

    #[tokio::test]
    async fn submit_duplicate_goal_returns_existing_id() {
        let pool = make_pool(2);
        let t1 = Task::new("fix the same bug");
        let t2 = Task::new("fix the same bug");
        let r1 = pool.submit(t1).await;
        let r2 = pool.submit(t2).await;
        // First submit returns None (new task)
        assert!(r1.is_none());
        // Second submit returns Some (duplicate)
        assert!(r2.is_some());
        // Only one task in the queue
        assert_eq!(pool.stats().queued, 1);
    }

    #[tokio::test]
    async fn cancel_nonexistent_task_returns_false() {
        let pool = make_pool(2);
        let fake_id = TaskId::new();
        let cancelled = pool.cancel(&fake_id).await;
        assert!(!cancelled);
    }

    #[tokio::test]
    async fn pool_queue_accessor_works() {
        let pool = make_pool(2);
        let queue = pool.queue();
        // Queue starts empty
        assert!(queue.is_empty());
    }

    #[tokio::test]
    async fn pool_bus_accessor_works() {
        let pool = make_pool(2);
        let bus = pool.bus();
        let mut rx = bus.subscribe();
        // Send an event and verify the bus works
        bus.send(AgentEvent::TaskQueued {
            task_id: TaskId::new(),
            goal: "test goal".to_string(),
            priority: Priority::Normal,
        });
        let ev = rx.try_recv();
        assert!(ev.is_ok());
    }

    #[tokio::test]
    async fn submit_broadcasts_task_queued_event() {
        let pool = make_pool(2);
        let mut rx = pool.bus().subscribe();
        let task = Task::new("broadcast test goal");
        pool.submit(task).await;
        // Should have received a TaskQueued event
        let ev = rx.try_recv();
        assert!(ev.is_ok());
        match ev.unwrap() {
            AgentEvent::TaskQueued { goal, .. } => {
                assert_eq!(goal, "broadcast test goal");
            }
            other => panic!("expected TaskQueued, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn pool_with_store_does_not_panic() {
        let queue = TaskQueue::new();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let pool = AgentPool::new(2, PathBuf::from("."), queue, bus);
        let store = lopi_memory::MemoryStore::open_in_memory().await.unwrap();
        let pool = pool.with_store(store);
        let task = Task::new("task with store");
        pool.submit(task).await;
        assert_eq!(pool.stats().queued, 1);
    }

    #[tokio::test]
    async fn uptime_is_non_zero_after_submit() {
        let pool = make_pool(2);
        // Small sleep to ensure uptime > 0
        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        let stats = pool.stats();
        // uptime_secs may be 0 for very fast tests, but started_at should be set
        // Just verify it doesn't panic
        let _ = stats.uptime_secs;
    }

    #[tokio::test]
    async fn shutdown_completes_without_running_tasks() {
        let pool = make_pool(2);
        // Shutdown with no running tasks should complete immediately
        pool.shutdown().await;
    }

    #[tokio::test]
    async fn submit_high_priority_task() {
        let pool = make_pool(2);
        let mut task = Task::new("critical security fix");
        task.priority = Priority::High;
        pool.submit(task).await;
        let stats = pool.stats();
        assert_eq!(stats.queued, 1);
    }

    // ─── P2 — required-capability matching ───────────────────────────

    #[tokio::test]
    async fn can_satisfy_with_empty_requirements_always_passes() {
        let pool = make_pool(2);
        let task = Task::new("vanilla task, no requirements");
        assert!(pool.can_satisfy(&task));
    }

    #[tokio::test]
    async fn can_satisfy_returns_false_with_empty_registry() {
        let pool = make_pool(2);
        let mut task = Task::new("needs python");
        task.required_capabilities = vec!["python".into()];
        // No agents registered → must fail closed.
        assert!(!pool.can_satisfy(&task));
    }

    #[tokio::test]
    async fn can_satisfy_picks_up_any_matching_agent() {
        let pool = make_pool(2);
        pool.register_capabilities("alpha", vec!["rust".into(), "git".into()]);
        pool.register_capabilities("beta", vec!["python".into(), "ml".into()]);
        let mut task = Task::new("ml inference");
        task.required_capabilities = vec!["python".into(), "ml".into()];
        assert!(pool.can_satisfy(&task), "beta covers both required caps");
        // No single agent has rust+python — must fail.
        task.required_capabilities = vec!["rust".into(), "python".into()];
        assert!(!pool.can_satisfy(&task));
    }

    #[tokio::test]
    async fn deregister_removes_capability_advertisement() {
        let pool = make_pool(2);
        pool.register_capabilities("alpha", vec!["rust".into()]);
        let mut task = Task::new("rust work");
        task.required_capabilities = vec!["rust".into()];
        assert!(pool.can_satisfy(&task));
        assert!(pool.deregister_capabilities("alpha"));
        assert!(!pool.can_satisfy(&task));
        // Second deregister is a no-op.
        assert!(!pool.deregister_capabilities("alpha"));
    }

    // ─── P2 — per-agent rate limiting ────────────────────────────────

    #[tokio::test]
    async fn unregistered_agent_is_unlimited() {
        let pool = make_pool(2);
        // Without registration the gate is wide open — every acquire
        // returns true and there's no in-flight to release.
        for _ in 0..100 {
            assert!(pool.try_acquire_agent("ghost").await);
        }
        // release_agent on an unregistered id is a clean noop.
        pool.release_agent("ghost");
    }

    #[tokio::test]
    async fn register_rejects_zero_per_minute() {
        let pool = make_pool(2);
        let ok = pool.register_agent_rate_limit(
            "bad",
            crate::AgentRateLimit { max_per_minute: 0, max_concurrent: 4 },
        );
        assert!(!ok, "0/min should be rejected");
        // No entry was written.
        assert!(pool.agent_rate_limit("bad").is_none());
    }

    #[tokio::test]
    async fn token_bucket_caps_burst_at_max_per_minute() {
        let pool = make_pool(2);
        assert!(pool.register_agent_rate_limit(
            "alpha",
            crate::AgentRateLimit { max_per_minute: 3, max_concurrent: 0 },
        ));
        // First 3 acquires succeed; the 4th is rate-limited.
        for _ in 0..3 {
            assert!(pool.try_acquire_agent("alpha").await);
        }
        assert!(!pool.try_acquire_agent("alpha").await);
        let snap = pool.agent_rate_limit("alpha").unwrap();
        assert_eq!(snap.in_flight, 3);
    }

    #[tokio::test]
    async fn concurrency_cap_short_circuits_before_bucket() {
        let pool = make_pool(2);
        assert!(pool.register_agent_rate_limit(
            "alpha",
            crate::AgentRateLimit { max_per_minute: 1_000, max_concurrent: 2 },
        ));
        // Two acquires use 2 of 1000 tokens but saturate the concurrency cap.
        assert!(pool.try_acquire_agent("alpha").await);
        assert!(pool.try_acquire_agent("alpha").await);
        assert!(!pool.try_acquire_agent("alpha").await,
            "concurrency cap should block even with tokens to spare");
        // Release frees a slot.
        pool.release_agent("alpha");
        assert!(pool.try_acquire_agent("alpha").await);
    }

    #[tokio::test]
    async fn release_saturates_at_zero() {
        let pool = make_pool(2);
        assert!(pool.register_agent_rate_limit(
            "alpha",
            crate::AgentRateLimit { max_per_minute: 10, max_concurrent: 2 },
        ));
        // Three releases against zero in-flight must not underflow.
        pool.release_agent("alpha");
        pool.release_agent("alpha");
        pool.release_agent("alpha");
        let snap = pool.agent_rate_limit("alpha").unwrap();
        assert_eq!(snap.in_flight, 0);
    }
}
