// TOON integration sites (from token analysis):
//   plan()      — constraints, allowed_dirs, forbidden_dirs arrays + pattern memory table
//   implement() — allowed_dirs, forbidden_dirs arrays
//   fix()       — allowed_dirs only (error text is free-form prose; TOON skipped)
//
// Token savings: ~17/prompt for dir/constraint arrays; ~158/attempt for pattern table.

use crate::claude_events::{parse_line, StreamEvent};
use anyhow::{Context, Result};
use lopi_core::Task;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command;

/// Re-exported so every existing `crate::claude::MODEL_*`/`select_model`/
/// `ClaudeOutput`/`ERR_CREDIT_EXHAUSTED` path stays valid — these moved to
/// `claude_model.rs` purely to keep this file under the 500-line CI
/// file-size gate; see that module's doc comment.
pub use crate::claude_model::{
    select_model, ClaudeOutput, ERR_BUDGET_HARD_STOP, ERR_CREDIT_EXHAUSTED, MODEL_HAIKU,
    MODEL_OPUS, MODEL_SONNET,
};
/// Re-exported so `crate::claude::scrub_inherited_anthropic_env` stays valid
/// for `claude_stream.rs`'s call site — moved to `claude_support.rs` for the
/// same file-size reason.
pub(crate) use crate::claude_support::scrub_inherited_anthropic_env;
use crate::claude_support::{apply_cli_caps, compress_errors};

/// Wrapper around the `claude` CLI — drives plan, implement, fix, and streaming calls.
pub struct ClaudeCode {
    // Fields are `pub(crate)` so the `with_*` builders can live in
    // `claude_builders.rs` (keeping this file under the 500-line CI gate)
    // while still setting them directly. Not part of the public API.
    pub(crate) repo_path: PathBuf,
    pub(crate) cli_path: String,
    pub(crate) timeout: Duration,
    pub(crate) json_output: bool,
    /// Constraints seeded from pattern memory — injected into the planning prompt.
    pub(crate) extra_constraints: Vec<String>,
    /// Model to use for CLI calls. None = let the CLI pick its default.
    pub(crate) model: Option<String>,
    /// Reasoning-effort level (`--effort`) for the worker session. Stored
    /// only after validation against the CLI's accepted levels (see
    /// `with_effort`). None = let the CLI pick its default.
    pub(crate) effort: Option<String>,
    /// Permission mode (`--permission-mode`) for the worker session. Stored
    /// only after validation against `PermissionMode`'s four headless-safe
    /// values (see `with_permission_mode`). None = `apply_cli_caps` falls
    /// back to `PermissionMode::default()` (`bypassPermissions`), reproducing
    /// the pre-existing unconditional `--dangerously-skip-permissions`
    /// behavior exactly.
    pub(crate) permission_mode: Option<String>,
    /// Phase 5b — tabular pattern pairs (keywords, constraints) for TOON encoding.
    pub(crate) patterns: Vec<(String, String)>,
    /// Phase 5b — lessons learned from past patterns or post-mortems (category, content).
    pub(crate) lessons: Vec<(String, String)>,
    /// Per-session `--max-turns` cap passed to `claude -p`. None = CLI default.
    pub(crate) max_turns: Option<u32>,
    /// Per-session `--max-budget-usd` cap passed to `claude -p`. None = no cap.
    pub(crate) max_budget_usd: Option<f64>,
    /// `--allowedTools` — tool names explicitly permitted (e.g. `"Bash(git *)"`).
    /// Wired from `LoopConfig::permission_allow`. Empty = no additions beyond
    /// the CLI's own defaults.
    pub(crate) allowed_tools: Vec<String>,
    /// `--disallowedTools` — tool names explicitly denied. Wired from
    /// `LoopConfig::permission_deny`. Empty = nothing denied.
    pub(crate) disallowed_tools: Vec<String>,
}

impl ClaudeCode {
    /// Create a new `ClaudeCode` wrapper rooted at `repo_path`.
    pub fn new(repo_path: impl AsRef<Path>) -> Self {
        Self {
            repo_path: repo_path.as_ref().to_path_buf(),
            cli_path: "claude".into(),
            timeout: Duration::from_secs(300),
            json_output: true,
            extra_constraints: vec![],
            model: None,
            effort: None,
            permission_mode: None,
            patterns: vec![],
            lessons: vec![],
            max_turns: None,
            max_budget_usd: None,
            allowed_tools: vec![],
            disallowed_tools: vec![],
        }
    }

    /// Plan the task. Uses TOON for constraints/dirs/pattern memory context.
    ///
    /// Site 1 (struct arrays, §9.1) — ~17 tokens/prompt saved.
    /// Site 2 (pattern memory table, §9.3 tabular) — ~158 tokens/attempt saved (grows with memory).
    ///
    /// # Errors
    ///
    /// Returns an error if the claude CLI process fails or times out.
    pub async fn plan(&self, task: &Task, last_error: Option<&str>) -> Result<String> {
        let prompt = self.build_plan_prompt(task, last_error);
        let out = self.run(&prompt).await?;
        Ok(out.text().to_string())
    }

    /// See [`claude_support::build_plan_prompt`](crate::claude_support::build_plan_prompt).
    fn build_plan_prompt(&self, task: &Task, last_error: Option<&str>) -> String {
        crate::claude_support::build_plan_prompt(
            task,
            last_error,
            &self.extra_constraints,
            &self.patterns,
            &self.lessons,
        )
    }

    /// Stream the CLI output to `on_line` as Claude generates it, surfacing the
    /// *real* status of the response rather than any hardcoded phase label.
    ///
    /// Uses `--output-format stream-json --verbose --include-partial-messages`,
    /// which emits one NDJSON event per line: assistant text/thinking blocks,
    /// `tool_use` calls, tool results, partial-message token usage,
    /// `rate_limit_event`s, and the terminal `result`. Each line is decoded by
    /// [`parse_line`] and every [`StreamEvent`] is handed to `on_event` the
    /// moment it arrives, so the caller can derive both the log line and the
    /// structured pane events. `on_event` returns `false` to hard-stop the
    /// session immediately (the subprocess is killed and this bails with
    /// [`ERR_BUDGET_HARD_STOP`]) — the caller's own budget accrual is the
    /// only thing that can request this; a `--max-budget-usd` cap alone only
    /// stops the CLI's *own* internal accounting, which is checked between
    /// turns and can let one expensive turn overshoot the cap before it
    /// fires. Returns the canonical final response text.
    async fn run_streamed<F>(&self, prompt: &str, on_event: F) -> Result<String>
    where
        F: Fn(&StreamEvent) -> bool + Send,
    {
        let mut cmd = Command::new(&self.cli_path);
        cmd.arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--include-partial-messages");
        // `apply_cli_caps` always emits `--permission-mode` (falling back to
        // `PermissionMode::default()`, `bypassPermissions`, when
        // `self.permission_mode` is unset) — without a headless-safe mode, a
        // tool call needing approval (e.g. a multi-part Bash command) stalls
        // the session waiting on a prompt nothing in this pipeline can
        // answer, burning turns until `--max-turns` cuts it off
        // (`error_max_turns`) with the actual work half-done — see
        // run_loop.rs's Planning/Implementing phases. The default preserves
        // that unconditional bypass exactly; a task may now opt into a
        // tighter mode via `Task::permission_mode`.
        apply_cli_caps(
            &mut cmd,
            self.model.as_deref(),
            self.effort.as_deref(),
            self.permission_mode.as_deref(),
            self.max_turns,
            self.max_budget_usd,
            &self.allowed_tools,
            &self.disallowed_tools,
        );
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());
        cmd.current_dir(&self.repo_path);
        scrub_inherited_anthropic_env(&mut cmd);

        let mut child = cmd.spawn().context("spawning claude cli for streaming")?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("claude cli: no stdout handle"))?;
        let mut lines = AsyncBufReader::new(stdout).lines();
        let mut final_text = String::new();
        let mut fallback = String::new();

        let deadline = tokio::time::Instant::now() + self.timeout;
        loop {
            match tokio::time::timeout_at(deadline, lines.next_line()).await {
                Ok(Ok(Some(line))) => {
                    let mut hard_stop = false;
                    for ev in parse_line(&line) {
                        if let Some(t) = ev.final_text() {
                            final_text = t.to_string();
                        } else if let Some(l) = ev.log_line() {
                            fallback.push_str(&l);
                            fallback.push('\n');
                        }
                        if !on_event(&ev) {
                            hard_stop = true;
                            break;
                        }
                    }
                    if hard_stop {
                        child.kill().await.ok();
                        anyhow::bail!("{ERR_BUDGET_HARD_STOP}");
                    }
                }
                Ok(Ok(None)) => break,
                Ok(Err(e)) => anyhow::bail!("reading claude stdout: {e}"),
                Err(_) => {
                    child.kill().await.ok();
                    anyhow::bail!("claude cli timed out after {:?}", self.timeout);
                }
            }
        }

        let status = child.wait().await.context("waiting for claude cli")?;
        let text = if final_text.trim().is_empty() {
            fallback
        } else {
            final_text
        };
        if !status.success() && text.trim().is_empty() {
            anyhow::bail!("claude cli exited {status} with no output");
        }
        Ok(text)
    }

    /// Plan the task with live streaming — each decoded [`StreamEvent`] (text,
    /// thinking, tool calls, token usage, status) is passed to `on_event` as it
    /// arrives, so the caller can emit both log lines and structured events.
    ///
    /// # Errors
    ///
    /// Returns an error if the claude CLI process fails or times out.
    pub async fn plan_streamed<F>(
        &self,
        task: &Task,
        last_error: Option<&str>,
        on_event: F,
    ) -> Result<String>
    where
        F: Fn(&StreamEvent) -> bool + Send,
    {
        let prompt = self.build_plan_prompt(task, last_error);
        self.run_streamed(&prompt, on_event).await
    }

    /// Implement the plan with live streaming output (real status, not a label).
    ///
    /// # Errors
    ///
    /// Returns an error if the claude CLI process fails or times out.
    pub async fn implement_streamed<F>(
        &self,
        task: &Task,
        plan: &str,
        on_event: F,
    ) -> Result<String>
    where
        F: Fn(&StreamEvent) -> bool + Send,
    {
        let prompt = crate::claude_support::build_implement_prompt(task, plan);
        self.run_streamed(&prompt, on_event).await
    }

    /// Implement the plan. Uses TOON for dir arrays in the constraint block.
    ///
    /// Site 1 (struct arrays) — ~17 tokens/prompt saved.
    ///
    /// # Errors
    ///
    /// Returns an error if the claude CLI process fails or times out.
    pub async fn implement(&self, task: &Task, plan: &str) -> Result<String> {
        let prompt = crate::claude_support::build_implement_prompt(task, plan);
        let out = self.run(&prompt).await?;
        if !out.succeeded() {
            anyhow::bail!("claude implement failed: {}", out.text());
        }
        Ok(out.text().to_string())
    }

    /// Fix the failing tests. Error text is free-form prose — TOON not applied here (no gain).
    /// Only the `allowed_dirs` scope is encoded as TOON.
    ///
    /// # Errors
    ///
    /// Returns an error if the claude CLI process fails or times out.
    pub async fn fix(&self, task: &Task, errors: &[String]) -> Result<String> {
        let allowed: Vec<&str> = task.allowed_dirs.iter().map(String::as_str).collect();
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
    ///
    /// Forwards `self.model`/`effort`/`permission_mode`/`max_budget_usd`/
    /// `max_turns`/`allowed_tools`/`disallowed_tools` to
    /// [`claude_stream::plan_streaming`](crate::claude_stream::plan_streaming) —
    /// the same caps [`run`](Self::run) and [`run_streamed`](Self::run_streamed) apply,
    /// so a `--speculative` session can never spawn `claude -p` uncapped just
    /// because it took this third spawn path instead of the other two.
    #[must_use]
    pub fn plan_streaming(
        &self,
        task: &Task,
    ) -> (
        tokio::task::JoinHandle<Result<String>>,
        tokio::sync::mpsc::Receiver<String>,
    ) {
        let all_constraints: Vec<String> = task
            .constraints
            .iter()
            .chain(self.extra_constraints.iter())
            .cloned()
            .collect();
        crate::claude_stream::plan_streaming(
            &self.repo_path,
            &self.cli_path,
            self.timeout,
            task,
            all_constraints,
            self.model.as_deref(),
            self.effort.as_deref(),
            self.permission_mode.as_deref(),
            self.max_budget_usd,
            self.max_turns,
            &self.allowed_tools,
            &self.disallowed_tools,
        )
    }

    /// Apply a single plan step to the repository.
    ///
    /// # Errors
    ///
    /// Returns an error if the claude CLI process fails or times out.
    pub async fn implement_step(&self, task: &Task, step: &str) -> Result<()> {
        let allowed: Vec<&str> = task.allowed_dirs.iter().map(String::as_str).collect();
        let scope = lopi_toon::encode_task_context(&task.goal, &allowed, &[], &[], &[], &[]);
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
        // Same caps as `run_streamed` — this one-shot path backs `fix()` and
        // `implement_step()` (speculative mode), both real spend that was
        // previously uncapped here regardless of what `run_streamed`'s caller
        // configured. `apply_cli_caps` emits `--permission-mode`, falling
        // back to `PermissionMode::default()` (`bypassPermissions`) when
        // unset — the same unconditional bypass this site always used.
        apply_cli_caps(
            &mut cmd,
            self.model.as_deref(),
            self.effort.as_deref(),
            self.permission_mode.as_deref(),
            self.max_turns,
            self.max_budget_usd,
            &self.allowed_tools,
            &self.disallowed_tools,
        );
        cmd.current_dir(&self.repo_path);
        scrub_inherited_anthropic_env(&mut cmd);

        let raw_out = tokio::time::timeout(self.timeout, cmd.output())
            .await
            .context("claude cli timed out")?
            .context("invoking claude cli")?;

        if !raw_out.status.success() {
            let stderr = String::from_utf8_lossy(&raw_out.stderr);
            let stdout = String::from_utf8_lossy(&raw_out.stdout);
            tracing::error!(
                cwd = %self.repo_path.display(),
                model = self.model.as_deref().unwrap_or("<default>"),
                prompt_bytes = prompt.len(),
                status = %raw_out.status,
                stderr = %stderr,
                stdout = %stdout,
                "claude cli failed"
            );
            return Err(crate::claude_support::build_cli_error(
                &stdout,
                &stderr,
                raw_out.status,
                &self.repo_path,
                prompt.len(),
            ));
        }

        let stdout = String::from_utf8_lossy(&raw_out.stdout).into_owned();
        Ok(crate::claude_model::parse_claude_output(
            stdout,
            self.json_output,
        ))
    }
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
