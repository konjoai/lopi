use anyhow::Result;
use lopi_core::{Task, TaskStatus};
use lopi_git::GitManager;
use crate::claude::ClaudeCode;
use crate::scorer::Scorer;
use std::path::PathBuf;
use tokio::sync::broadcast;
use tracing::{info, warn};

pub struct AgentRunner {
    pub task: Task,
    pub repo_path: PathBuf,
    pub status_tx: broadcast::Sender<TaskStatus>,
}

impl AgentRunner {
    pub fn new(task: Task, repo_path: PathBuf) -> (Self, broadcast::Receiver<TaskStatus>) {
        let (tx, rx) = broadcast::channel(64);
        (Self { task, repo_path, status_tx: tx }, rx)
    }

    pub async fn run(&self) -> Result<TaskStatus> {
        let git = GitManager::new(&self.repo_path)?;
        let claude = ClaudeCode::new(&self.repo_path);
        let scorer = Scorer::new(&self.repo_path);

        for attempt in 0..self.task.max_retries {
            let branch = format!("lopi/{}-attempt-{}", self.task.id.0, attempt + 1);
            info!(task_id = %self.task.id, attempt = attempt + 1, "isolating in branch {branch}");

            git.checkout_new_branch(&branch).await?;
            let _ = self.status_tx.send(TaskStatus::Planning);

            let plan = match claude.plan(&self.task).await {
                Ok(p) => p,
                Err(e) => {
                    warn!("plan failed: {e}");
                    git.hard_rollback().await.ok();
                    git.checkout_default().await.ok();
                    let _ = self.status_tx.send(TaskStatus::Retrying { attempt: attempt + 1 });
                    continue;
                }
            };

            let _ = self.status_tx.send(TaskStatus::Implementing);
            if let Err(e) = claude.implement(&self.task, &plan).await {
                warn!("implement failed: {e}");
                git.hard_rollback().await.ok();
                git.checkout_default().await.ok();
                let _ = self.status_tx.send(TaskStatus::Retrying { attempt: attempt + 1 });
                continue;
            }

            // Diff safety.
            if let Err(e) = git.check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs).await {
                warn!("diff scope violation: {e}");
                let _ = self.status_tx.send(TaskStatus::RolledBack);
                git.hard_rollback().await.ok();
                git.checkout_default().await.ok();
                continue;
            }

            // Test + score.
            let _ = self.status_tx.send(TaskStatus::Testing);
            let score = scorer.score().await?;
            let _ = self.status_tx.send(TaskStatus::Scoring);

            if score.passed() {
                git.commit_all(&format!("lopi: {}", self.task.goal)).await.ok();
                let pr_url = git.open_pr(&branch, &self.task.goal).await.ok();
                return Ok(TaskStatus::Success { branch, pr_url });
            }

            // One in-place fix attempt before rolling back.
            if let Err(e) = claude.fix(&self.task, &score.errors).await {
                warn!("fix failed: {e}");
            }
            if let Err(e) = git.check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs).await {
                warn!("diff scope violation after fix: {e}");
                let _ = self.status_tx.send(TaskStatus::RolledBack);
                git.hard_rollback().await.ok();
                git.checkout_default().await.ok();
                continue;
            }
            let score2 = scorer.score().await?;
            if score2.passed() {
                git.commit_all(&format!("lopi: {}", self.task.goal)).await.ok();
                let pr_url = git.open_pr(&branch, &self.task.goal).await.ok();
                return Ok(TaskStatus::Success { branch, pr_url });
            }

            git.hard_rollback().await.ok();
            git.checkout_default().await.ok();
            let _ = self.status_tx.send(TaskStatus::Retrying { attempt: attempt + 1 });
        }

        Ok(TaskStatus::Failed { reason: "Max retries exceeded".into() })
    }
}
