use anyhow::Result;
use lopi_agent::AgentRunner;
use lopi_core::{EventBus, Task, TaskStatus};
use lopi_memory::MemoryStore;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{error, info};

use crate::queue::TaskQueue;

#[derive(Clone)]
pub struct AgentPool {
    permits: Arc<Semaphore>,
    queue: TaskQueue,
    repo_path: PathBuf,
    bus: EventBus<TaskStatus>,
    store: Option<MemoryStore>,
}

impl AgentPool {
    pub fn new(
        max_agents: usize,
        repo_path: PathBuf,
        queue: TaskQueue,
        bus: EventBus<TaskStatus>,
    ) -> Self {
        Self {
            permits: Arc::new(Semaphore::new(max_agents)),
            queue,
            repo_path,
            bus,
            store: None,
        }
    }

    /// Attach a memory store so the pool can mine patterns after each run.
    pub fn with_store(mut self, store: MemoryStore) -> Self {
        self.store = Some(store);
        self
    }

    pub fn queue(&self) -> TaskQueue {
        self.queue.clone()
    }

    /// Dispatch loop — pops tasks and spawns bounded concurrent workers.
    pub async fn run(self) -> Result<()> {
        loop {
            let task = self.queue.pop().await;
            let permit = self.permits.clone().acquire_owned().await?;
            let repo = self.repo_path.clone();
            let bus = self.bus.clone();
            let store = self.store.clone();
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(e) = run_one(task, repo, bus, store).await {
                    error!("agent run error: {e}");
                }
            });
        }
    }
}

async fn run_one(
    task: Task,
    repo: PathBuf,
    bus: EventBus<TaskStatus>,
    store: Option<MemoryStore>,
) -> Result<()> {
    info!(task_id = %task.id, "starting agent");
    let task_id = task.id;
    let goal = task.goal.clone();
    let runner = AgentRunner::new(task, repo, bus);
    let outcome = runner.run().await?;

    match &outcome {
        TaskStatus::Success { branch, pr_url } => {
            info!("✅ success on branch {branch}, pr={pr_url:?}");
        }
        TaskStatus::Failed { reason } => {
            info!("❌ failed: {reason}");
        }
        other => info!("ended in state {other:?}"),
    }

    // Mine patterns from this run's attempts.
    if let Some(store) = store {
        let status_str = format!("{:?}", outcome);
        store.mark_completed(&task_id, &status_str).await.ok();
        if let Err(e) = store.mine_patterns(&task_id, &goal).await {
            tracing::warn!("pattern mining failed: {e}");
        }
    }

    Ok(())
}
