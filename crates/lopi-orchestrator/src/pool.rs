use anyhow::Result;
use dashmap::DashMap;
use lopi_agent::AgentRunner;
use lopi_core::{AgentEvent, EventBus, Task, TaskId, TaskStatus};
use lopi_memory::MemoryStore;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{oneshot, Semaphore};
use tracing::{error, info, warn};

use crate::queue::TaskQueue;

/// Live state of a single running agent.
#[derive(Debug)]
pub struct AgentHandle {
    pub goal: String,
    pub cancel_tx: Option<oneshot::Sender<()>>,
    pub status: Arc<tokio::sync::RwLock<TaskStatus>>,
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
    permits: Arc<Semaphore>,
    queue: TaskQueue,
    repo_path: PathBuf,
    /// All AgentEvents: TaskQueued, TaskStarted, StatusChanged, LogLine, TaskCompleted.
    bus: EventBus<AgentEvent>,
    store: Option<MemoryStore>,
    /// Live handles — entries removed when the task completes or is cancelled.
    handles: Arc<DashMap<TaskId, Arc<tokio::sync::RwLock<AgentHandle>>>>,
    counters: Arc<PoolCounters>,
    started_at: Arc<std::time::Instant>,
}

impl AgentPool {
    pub fn new(
        max_agents: usize,
        repo_path: PathBuf,
        queue: TaskQueue,
        bus: EventBus<AgentEvent>,
    ) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(max_agents)),
            queue,
            repo_path,
            bus,
            store: None,
            handles: Arc::new(DashMap::new()),
            counters: Arc::new(PoolCounters::default()),
            started_at: Arc::new(std::time::Instant::now()),
        }
    }

    pub fn with_store(mut self, store: MemoryStore) -> Self {
        self.store = Some(store);
        self
    }

    pub fn queue(&self) -> TaskQueue {
        self.queue.clone()
    }

    pub fn bus(&self) -> EventBus<AgentEvent> {
        self.bus.clone()
    }

    /// Cancel a running task. Returns true if the cancel signal was sent.
    pub async fn cancel(&self, task_id: &TaskId) -> bool {
        if let Some(handle_ref) = self.handles.get(task_id) {
            let mut handle = handle_ref.write().await;
            if let Some(tx) = handle.cancel_tx.take() {
                let _ = tx.send(());
                self.bus.send(AgentEvent::TaskCancelled { task_id: *task_id });
                return true;
            }
        }
        false
    }

    /// Return a snapshot of current stats.
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
    pub async fn run(self) -> Result<()> {
        loop {
            let task = self.queue.pop().await;
            let permit = self.permits.clone().acquire_owned().await?;
            let task_id = task.id;
            let goal = task.goal.clone();

            let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
            let status = Arc::new(tokio::sync::RwLock::new(TaskStatus::Queued));
            let attempt = Arc::new(AtomicUsize::new(0));

            let handle = Arc::new(tokio::sync::RwLock::new(AgentHandle {
                goal: goal.clone(),
                cancel_tx: Some(cancel_tx),
                status: status.clone(),
                attempt: attempt.clone(),
                started_at: std::time::Instant::now(),
            }));
            self.handles.insert(task_id, handle);
            self.counters.running.fetch_add(1, Ordering::Relaxed);

            let repo = self.repo_path.clone();
            let bus = self.bus.clone();
            let store = self.store.clone();
            let handles = self.handles.clone();
            let counters = self.counters.clone();

            tokio::spawn(async move {
                let _permit = permit;
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

    let mut runner = AgentRunner::new(task, repo, bus.clone(), store.clone(), cancel_rx, attempt_counter);
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
