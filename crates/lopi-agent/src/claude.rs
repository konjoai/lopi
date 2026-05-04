use anyhow::{Context, Result};
use lopi_core::Task;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

/// Structured output from `claude --output-format json`.
/// Claude Code emits a JSON object on stdout when invoked with that flag.
#[derive(Debug, Deserialize)]
pub struct ClaudeOutput {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub result: Option<String>,
    pub is_error: Option<bool>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    // Fallback: raw text for non-JSON-mode calls.
    #[serde(skip)]
    pub raw: String,
}

impl ClaudeOutput {
    pub fn text(&self) -> &str {
        self.result.as_deref().unwrap_or(&self.raw)
    }

    pub fn succeeded(&self) -> bool {
        !self.is_error.unwrap_or(false)
    }
}

pub struct ClaudeCode {
    repo_path: PathBuf,
    cli_path: String,
    timeout: Duration,
    /// Use `--output-format json` for structured output (requires Claude Code ≥ 1.x).
    json_output: bool,
    /// Constraints seeded from pattern memory — injected into the planning prompt.
    extra_constraints: Vec<String>,
}

impl ClaudeCode {
    pub fn new(repo_path: impl AsRef<Path>) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
            cli_path: "claude".into(),
            timeout: Duration::from_secs(300),
            json_output: true,
            extra_constraints: vec![],
        }
    }

    pub fn with_extra_constraints(mut self, constraints: Vec<String>) -> Self {
        self.extra_constraints = constraints;
        self
    }

    pub fn with_cli(mut self, cli_path: impl Into<String>) -> Self {
        self.cli_path = cli_path.into();
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    pub fn with_json_output(mut self, enabled: bool) -> Self {
        self.json_output = enabled;
        self
    }

    /// Ask Claude Code to produce a plan for `task`. Returns the plan text.
    pub async fn plan(&self, task: &Task) -> Result<String> {
        let memory_hint = if self.extra_constraints.is_empty() {
            String::new()
        } else {
            format!("\nSuccessful patterns from memory:\n{}", self.extra_constraints.join("\n"))
        };
        let prompt = format!(
            "You are running inside lopi. Produce a concise implementation plan for this task. \
             Output a numbered list of steps only.\n\n\
             Goal: {goal}\n\
             Constraints: {constraints:?}\n\
             Allowed dirs: {allowed:?}\n\
             Forbidden dirs: {forbidden:?}{memory_hint}",
            goal = task.goal,
            constraints = task.constraints,
            allowed = task.allowed_dirs,
            forbidden = task.forbidden_dirs,
        );
        let out = self.run(&prompt).await?;
        Ok(out.text().to_string())
    }

    /// Implement the plan in the repo.
    pub async fn implement(&self, task: &Task, plan: &str) -> Result<String> {
        let prompt = format!(
            "Implement the plan below in the current repository. \
             Edit only files inside allowed dirs ({allowed:?}). Never touch forbidden dirs ({forbidden:?}).\n\n\
             Goal: {goal}\n\nPlan:\n{plan}",
            goal = task.goal,
            allowed = task.allowed_dirs,
            forbidden = task.forbidden_dirs,
        );
        let out = self.run(&prompt).await?;
        if !out.succeeded() {
            anyhow::bail!("claude implement failed: {}", out.text());
        }
        Ok(out.text().to_string())
    }

    /// Feed failures back into Claude Code and ask for a fix.
    pub async fn fix(&self, task: &Task, errors: &[String]) -> Result<String> {
        let prompt = format!(
            "The previous attempt for this task failed. Fix the failures listed below. \
             Stay inside allowed dirs ({allowed:?}).\n\n\
             Goal: {goal}\n\nFailures:\n{failures}",
            goal = task.goal,
            allowed = task.allowed_dirs,
            failures = errors.join("\n"),
        );
        let out = self.run(&prompt).await?;
        Ok(out.text().to_string())
    }

    async fn run(&self, prompt: &str) -> Result<ClaudeOutput> {
        let mut cmd = Command::new(&self.cli_path);
        cmd.arg("-p").arg(prompt);
        if self.json_output {
            cmd.arg("--output-format").arg("json");
        }
        cmd.current_dir(&self.repo_path);

        let fut = cmd.output();
        let raw_out = tokio::time::timeout(self.timeout, fut)
            .await
            .context("claude cli timed out")?
            .context("invoking claude cli")?;

        if !raw_out.status.success() {
            anyhow::bail!("claude cli exited {}: {}", raw_out.status,
                String::from_utf8_lossy(&raw_out.stderr));
        }

        let stdout = String::from_utf8_lossy(&raw_out.stdout).into_owned();
        if self.json_output {
            match serde_json::from_str::<ClaudeOutput>(&stdout) {
                Ok(mut o) => { o.raw = stdout; Ok(o) }
                Err(_) => {
                    // Claude Code may emit plain text if --output-format json isn't supported.
                    Ok(ClaudeOutput { kind: None, result: Some(stdout.clone()), is_error: None,
                        cost_usd: None, duration_ms: None, raw: stdout })
                }
            }
        } else {
            Ok(ClaudeOutput { kind: None, result: Some(stdout.clone()), is_error: None,
                cost_usd: None, duration_ms: None, raw: stdout })
        }
    }
}
