use anyhow::Result;
use lopi_core::{EventBus, Task, TaskStatus};
use lopi_git::GitManager;
use crate::claude::ClaudeCode;
use crate::scorer::Scorer;
use std::path::PathBuf;
use tracing::{info, warn};

pub struct AgentRunner {
    pub task: Task,
    pub repo_path: PathBuf,
    pub bus: EventBus<TaskStatus>,
}

impl AgentRunner {
    pub fn new(task: Task, repo_path: PathBuf, bus: EventBus<TaskStatus>) -> Self {
        Self { task, repo_path, bus }
    }

    /// Convenience constructor that creates its own isolated `EventBus`.
    /// Useful for one-shot `lopi run` invocations.
    pub fn standalone(task: Task, repo_path: PathBuf) -> (Self, EventBus<TaskStatus>) {
        let bus: EventBus<TaskStatus> = EventBus::new(64);
        let runner = Self { task, repo_path, bus: bus.clone() };
        (runner, bus)
    }

    fn emit(&self, status: TaskStatus) {
        self.bus.send(status);
    }

    pub async fn run(&self) -> Result<TaskStatus> {
        let git = GitManager::new(&self.repo_path)?;
        let claude = ClaudeCode::new(&self.repo_path);
        let scorer = Scorer::new(&self.repo_path);

        for attempt in 0..self.task.max_retries {
            let branch = format!("lopi/{}-attempt-{}", self.task.id.0, attempt + 1);
            info!(task_id = %self.task.id, attempt = attempt + 1, %branch, "isolating");

            if let Err(e) = git.checkout_new_branch(&branch).await {
                warn!("checkout failed: {e}");
                self.emit(TaskStatus::Retrying { attempt: attempt + 1 });
                continue;
            }
            self.emit(TaskStatus::Planning);

            let plan = match claude.plan(&self.task).await {
                Ok(p) => p,
                Err(e) => {
                    warn!("plan failed: {e}");
                    git.hard_rollback().await.ok();
                    git.checkout_default().await.ok();
                    self.emit(TaskStatus::Retrying { attempt: attempt + 1 });
                    continue;
                }
            };

            self.emit(TaskStatus::Implementing);
            if let Err(e) = claude.implement(&self.task, &plan).await {
                warn!("implement failed: {e}");
                git.hard_rollback().await.ok();
                git.checkout_default().await.ok();
                self.emit(TaskStatus::Retrying { attempt: attempt + 1 });
                continue;
            }

            if let Err(e) = git.check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs).await {
                warn!("diff scope violation: {e}");
                self.emit(TaskStatus::RolledBack);
                git.hard_rollback().await.ok();
                git.checkout_default().await.ok();
                continue;
            }

            self.emit(TaskStatus::Testing);
            let score = scorer.score().await?;
            self.emit(TaskStatus::Scoring);

            if score.passed() {
                git.commit_all(&format!("lopi: {}", self.task.goal)).await.ok();
                let pr_url = git.open_pr(&branch, &self.task.goal).await.ok();
                let status = TaskStatus::Success { branch, pr_url };
                self.emit(status.clone());
                return Ok(status);
            }

            // One in-place fix attempt.
            if let Err(e) = claude.fix(&self.task, &score.errors).await {
                warn!("fix attempt failed: {e}");
            }
            if git.check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs).await.is_ok() {
                let score2 = scorer.score().await?;
                if score2.passed() {
                    git.commit_all(&format!("lopi: {}", self.task.goal)).await.ok();
                    let pr_url = git.open_pr(&branch, &self.task.goal).await.ok();
                    let status = TaskStatus::Success { branch, pr_url };
                    self.emit(status.clone());
                    return Ok(status);
                }
            }

            git.hard_rollback().await.ok();
            git.checkout_default().await.ok();
            self.emit(TaskStatus::Retrying { attempt: attempt + 1 });
        }

        let status = TaskStatus::Failed { reason: "Max retries exceeded".into() };
        self.emit(status.clone());
        Ok(status)
    }
}

