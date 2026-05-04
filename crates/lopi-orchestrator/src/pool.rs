use anyhow::Result;
use lopi_agent::AgentRunner;
use lopi_core::{EventBus, Task, TaskStatus};
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
        }
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
            tokio::spawn(async move {
                let _permit = permit;
                if let Err(e) = run_one(task, repo, bus).await {
                    error!("agent run error: {e}");
                }
            });
        }
    }
}

async fn run_one(task: Task, repo: PathBuf, bus: EventBus<TaskStatus>) -> Result<()> {
    info!(task_id = %task.id, "starting agent");
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
    Ok(())
}
