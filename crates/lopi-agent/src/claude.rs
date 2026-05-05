// TOON integration sites (from token analysis):
//   plan()      — constraints, allowed_dirs, forbidden_dirs arrays + pattern memory table
//   implement() — allowed_dirs, forbidden_dirs arrays
//   fix()       — allowed_dirs only (error text is free-form prose; TOON skipped)
//
// Token savings: ~17/prompt for dir/constraint arrays; ~158/attempt for pattern table.

use anyhow::{Context, Result};
use lopi_core::Task;
use lopi_toon::encode_task_context;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

/// Structured output from `claude --output-format json`.
#[derive(Debug, Deserialize)]
pub struct ClaudeOutput {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub result: Option<String>,
    pub is_error: Option<bool>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    #[serde(skip)]
    pub raw: String,
}

impl ClaudeOutput {
    pub fn text(&self) -> &str { self.result.as_deref().unwrap_or(&self.raw) }
    pub fn succeeded(&self) -> bool { !self.is_error.unwrap_or(false) }
}

pub struct ClaudeCode {
    repo_path: PathBuf,
    cli_path: String,
    timeout: Duration,
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

    pub fn with_extra_constraints(mut self, constraints: Vec<String>) -> Self {
        self.extra_constraints = constraints;
        self
    }

    /// Plan the task. Uses TOON for constraints/dirs/pattern memory context.
    ///
    /// Site 1 (struct arrays, §9.1) — ~17 tokens/prompt saved.
    /// Site 2 (pattern memory table, §9.3 tabular) — ~158 tokens/attempt saved (grows with memory).
    pub async fn plan(&self, task: &Task) -> Result<String> {
        let all_constraints: Vec<&str> = task.constraints.iter()
            .chain(self.extra_constraints.iter())
            .map(|s| s.as_str())
            .collect();

        let allowed: Vec<&str> = task.allowed_dirs.iter().map(|s| s.as_str()).collect();
        let forbidden: Vec<&str> = task.forbidden_dirs.iter().map(|s| s.as_str()).collect();

        let ctx = encode_task_context(
            &task.goal,
            &allowed,
            &forbidden,
            &all_constraints,
            &[], // patterns already folded into extra_constraints by runner.rs
        );

        let prompt = format!(
            "You are running inside lopi. \
             Produce a concise implementation plan. \
             Output a numbered list of steps only.\n\n\
             ## Task context (TOON)\n\
             {ctx}"
        );
        let out = self.run(&prompt).await?;
        Ok(out.text().to_string())
    }

    /// Implement the plan. Uses TOON for dir arrays in the constraint block.
    ///
    /// Site 1 (struct arrays) — ~17 tokens/prompt saved.
    pub async fn implement(&self, task: &Task, plan: &str) -> Result<String> {
        let allowed: Vec<&str> = task.allowed_dirs.iter().map(|s| s.as_str()).collect();
        let forbidden: Vec<&str> = task.forbidden_dirs.iter().map(|s| s.as_str()).collect();

        let scope = encode_task_context(&task.goal, &allowed, &forbidden, &[], &[]);

        let prompt = format!(
            "Implement the plan below in the current repository.\n\n\
             ## Scope (TOON)\n\
             {scope}\n\
             ## Plan\n\
             {plan}"
        );
        let out = self.run(&prompt).await?;
        if !out.succeeded() {
            anyhow::bail!("claude implement failed: {}", out.text());
        }
        Ok(out.text().to_string())
    }

    /// Fix the failing tests. Error text is free-form prose — TOON not applied here (no gain).
    /// Only the allowed_dirs scope is encoded as TOON.
    pub async fn fix(&self, task: &Task, errors: &[String]) -> Result<String> {
        let allowed: Vec<&str> = task.allowed_dirs.iter().map(|s| s.as_str()).collect();
        // Inline primitive array: site 1 partial (dirs only).
        let allowed_str = if allowed.is_empty() {
            String::new()
        } else {
            format!("allowed[{}]: {}\n", allowed.len(), allowed.join(","))
        };

        let failures = compress_errors(errors);
        let prompt = format!(
            "The previous attempt failed. Fix the failures below.\n\
             {allowed_str}\n\
             Goal: {goal}\n\n\
             ## Failures\n\
             {failures}",
            goal = task.goal,
        );
        let out = self.run(&prompt).await?;
        Ok(out.text().to_string())
    }

    /// Stream plan steps as they are generated. Returns a channel receiver that emits
    /// numbered plan steps (lines matching `^\d+\.`) and a join handle that resolves to
    /// the full plan text when the claude process exits.
    pub fn plan_streaming(
        &self,
        task: &Task,
    ) -> (tokio::task::JoinHandle<Result<String>>, tokio::sync::mpsc::Receiver<String>) {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let (tx, rx) = tokio::sync::mpsc::channel::<String>(64);

        let all_constraints: Vec<String> = task.constraints.iter()
            .chain(self.extra_constraints.iter())
            .cloned()
            .collect();
        let allowed: Vec<String> = task.allowed_dirs.clone();
        let forbidden: Vec<String> = task.forbidden_dirs.clone();
        let goal = task.goal.clone();
        let cli_path = self.cli_path.clone();
        let repo_path = self.repo_path.clone();
        let timeout = self.timeout;

        let handle = tokio::spawn(async move {
            let ctx = lopi_toon::encode_task_context(
                &goal,
                &allowed.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                &forbidden.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                &all_constraints.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                &[],
            );
            let prompt = format!(
                "You are running inside lopi. \
                 Produce a concise implementation plan. \
                 Output a numbered list of steps only.\n\n\
                 ## Task context (TOON)\n{ctx}"
            );

            let mut child = tokio::process::Command::new(&cli_path)
                .arg("-p").arg(&prompt)
                .current_dir(&repo_path)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .spawn()
                .context("spawning claude for streaming plan")?;

            let stdout = child.stdout.take().context("claude stdout unavailable")?;
            let mut reader = BufReader::new(stdout).lines();
            let mut full_text = String::new();

            let deadline = tokio::time::sleep(timeout);
            tokio::pin!(deadline);

            loop {
                tokio::select! {
                    line = reader.next_line() => {
                        match line? {
                            Some(l) => {
                                // Emit numbered plan steps immediately so the implement
                                // worker can begin applying them speculatively.
                                if l.trim_start().chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                                    let _ = tx.send(l.clone()).await;
                                }
                                full_text.push_str(&l);
                                full_text.push('\n');
                            }
                            None => break,
                        }
                    }
                    _ = &mut deadline => {
                        child.kill().await.ok();
                        anyhow::bail!("claude plan stream timed out");
                    }
                }
            }
            child.wait().await.ok();
            Ok(full_text)
        });

        (handle, rx)
    }

    /// Apply a single plan step to the repository.
    pub async fn implement_step(&self, task: &Task, step: &str) -> Result<()> {
        let allowed: Vec<&str> = task.allowed_dirs.iter().map(|s| s.as_str()).collect();
        let scope = lopi_toon::encode_task_context(&task.goal, &allowed, &[], &[], &[]);
        let prompt = format!(
            "Apply this single implementation step to the repository. Make only the changes described.\n\n\
             ## Scope\n{scope}\n\n\
             ## Step\n{step}"
        );
        let out = self.run(&prompt).await?;
        if !out.succeeded() {
            anyhow::bail!("step failed: {}", out.text());
        }
        Ok(())
    }

    async fn run(&self, prompt: &str) -> Result<ClaudeOutput> {

        let mut cmd = Command::new(&self.cli_path);
        cmd.arg("-p").arg(prompt);
        if self.json_output {
            cmd.arg("--output-format").arg("json");
        }
        cmd.current_dir(&self.repo_path);

        let raw_out = tokio::time::timeout(self.timeout, cmd.output())
            .await
            .context("claude cli timed out")?
            .context("invoking claude cli")?;

        if !raw_out.status.success() {
            anyhow::bail!("claude cli exited {}: {}",
                raw_out.status, String::from_utf8_lossy(&raw_out.stderr));
        }

        let stdout = String::from_utf8_lossy(&raw_out.stdout).into_owned();
        if self.json_output {
            match serde_json::from_str::<ClaudeOutput>(&stdout) {
                Ok(mut o) => { o.raw = stdout; Ok(o) }
                Err(_) => Ok(ClaudeOutput {
                    kind: None, result: Some(stdout.clone()), is_error: None,
                    cost_usd: None, duration_ms: None, raw: stdout,
                }),
            }
        } else {
            Ok(ClaudeOutput {
                kind: None, result: Some(stdout.clone()), is_error: None,
                cost_usd: None, duration_ms: None, raw: stdout,
            })
        }
    }
}

/// Strip Rust backtrace noise and deduplicate repeated error blocks to reduce fix-prompt token count.
/// Removes lines matching `at src/`, `note: run with RUST_BACKTRACE`, and limits each error to
/// 30 lines. Identical adjacent blocks are collapsed to one copy.
fn compress_errors(errors: &[String]) -> String {
    let mut seen: Vec<String> = Vec::with_capacity(errors.len());
    for err in errors {
        let compressed: String = err
            .lines()
            .filter(|line| {
                let t = line.trim();
                !t.starts_with("note: run with RUST_BACKTRACE")
                    && !t.starts_with("stack backtrace:")
                    && !(t.starts_with("at ") && (t.contains("src/") || t.contains(".rs:")))
            })
            .take(30)
            .collect::<Vec<_>>()
            .join("\n");
        if !seen.contains(&compressed) {
            seen.push(compressed);
        }
    }
    seen.join("\n---\n")
}
