use anyhow::{Context, Result};
use lopi_core::Task;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

pub struct ClaudeCode {
    repo_path: PathBuf,
    cli_path: String,
    timeout: Duration,
}

impl ClaudeCode {
    pub fn new(repo_path: impl AsRef<Path>) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
            cli_path: "claude".into(),
            timeout: Duration::from_secs(300),
        }
    }

    pub fn with_cli(mut self, cli_path: impl Into<String>) -> Self {
        self.cli_path = cli_path.into();
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    /// Ask Claude Code to produce a plan for `task`. Returns the plan as text.
    pub async fn plan(&self, task: &Task) -> Result<String> {
        let prompt = format!(
            "You are running inside lopi. Produce a concise implementation plan for this task. \
             Output a numbered list of steps only.\n\n\
             Goal: {goal}\n\
             Constraints: {constraints:?}\n\
             Allowed dirs: {allowed:?}\n\
             Forbidden dirs: {forbidden:?}",
            goal = task.goal,
            constraints = task.constraints,
            allowed = task.allowed_dirs,
            forbidden = task.forbidden_dirs,
        );
        self.run(&prompt).await
    }

    /// Ask Claude Code to implement the plan in the repo.
    pub async fn implement(&self, task: &Task, plan: &str) -> Result<String> {
        let prompt = format!(
            "Implement the plan below in the current repository. \
             Edit only files inside allowed dirs ({allowed:?}). Never touch forbidden dirs ({forbidden:?}).\n\n\
             Goal: {goal}\n\nPlan:\n{plan}",
            goal = task.goal,
            allowed = task.allowed_dirs,
            forbidden = task.forbidden_dirs,
        );
        self.run(&prompt).await
    }

    /// Feed a list of failures back into Claude Code and ask for a fix.
    pub async fn fix(&self, task: &Task, errors: &[String]) -> Result<String> {
        let prompt = format!(
            "The previous attempt for this task failed. Fix the failures listed below. \
             Stay inside allowed dirs ({allowed:?}).\n\n\
             Goal: {goal}\n\nFailures:\n{failures}",
            goal = task.goal,
            allowed = task.allowed_dirs,
            failures = errors.join("\n"),
        );
        self.run(&prompt).await
    }

    async fn run(&self, prompt: &str) -> Result<String> {
        let fut = Command::new(&self.cli_path)
            .arg("-p")
            .arg(prompt)
            .current_dir(&self.repo_path)
            .output();
        let out = tokio::time::timeout(self.timeout, fut)
            .await
            .context("claude cli timed out")?
            .context("invoking claude cli")?;
        if !out.status.success() {
            anyhow::bail!("claude cli failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}
