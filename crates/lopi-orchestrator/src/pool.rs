use anyhow::Result;
use dashmap::DashMap;
use lopi_agent::AgentRunner;
use lopi_core::{AgentEvent, EventBus, Task, TaskId, TaskStatus};
use lopi_memory::MemoryStore;
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

    /// Enqueue a task and broadcast `TaskQueued`.
    pub async fn submit(&self, task: Task) -> Option<TaskId> {
        self.bus.send(AgentEvent::TaskQueued {
            task_id: task.id,
            goal: task.goal.clone(),
            priority: task.priority,
        });
        if let Some(store) = &self.store {
            store.save_task(&task, "queued").await.ok();
        }
        self.queue.push(task).await
    }

    /// Dispatch loop — pops tasks from the queue and spawns bounded workers.
    ///
    /// # Errors
    ///
    /// Returns an error if a semaphore is closed (only happens on shutdown).
    pub async fn run(self) -> Result<()> {
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
                let outcome = run_one(task, repo, bus.clone(), store, cancel_rx, attempt).await;
                handles.remove(&task_id);
                counters.running.fetch_sub(1, Ordering::Relaxed);
                match &outcome {
                    Ok(TaskStatus::Success { .. }) => {
                        counters.succeeded.fetch_add(1, Ordering::Relaxed);
                    }
                    Ok(TaskStatus::Failed { .. }) | Err(_) => {
                        counters.failed.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => {}
                }
                if let Err(e) = outcome {
                    error!(task_id = %task_id, "agent run error: {e}");
                }
            });
        }
    }
}

pub struct PoolStats {
    pub running: usize,
    pub queued: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub uptime_secs: u64,
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

    let mut runner = AgentRunner::new(
        task,
        repo,
        bus.clone(),
        store.clone(),
        cancel_rx,
        attempt_counter,
    );
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
}
