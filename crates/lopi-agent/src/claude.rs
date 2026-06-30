// TOON integration sites (from token analysis):
//   plan()      — constraints, allowed_dirs, forbidden_dirs arrays + pattern memory table
//   implement() — allowed_dirs, forbidden_dirs arrays
//   fix()       — allowed_dirs only (error text is free-form prose; TOON skipped)
//
// Token savings: ~17/prompt for dir/constraint arrays; ~158/attempt for pattern table.

use crate::claude_events::{parse_line, StreamEvent};
use anyhow::{Context, Result};
use lopi_core::Task;
use lopi_toon::encode_task_context;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command;

// ── Model identifiers ─────────────────────────────────────────────────────────

/// Claude Haiku model identifier — lowest cost, fast latency.
pub const MODEL_HAIKU: &str = "claude-haiku-4-5-20251001";
/// Claude Sonnet model identifier — default balanced model.
pub const MODEL_SONNET: &str = "claude-sonnet-4-6";
/// Claude Opus model identifier — highest capability, used for complex or retried tasks.
pub const MODEL_OPUS: &str = "claude-opus-4-7";

/// Sentinel substring used by the run loop to detect a non-retryable billing
/// failure from the Anthropic API. Matched against the error chain so we don't
/// burn the retry budget looping on a credit-exhausted account.
pub const ERR_CREDIT_EXHAUSTED: &str = "anthropic credits exhausted";

/// Route a task to the cheapest model capable of handling its complexity.
///
/// Heuristic: task size = constraints + `allowed_dirs` count.
/// - ≤ 2: Haiku (read-only discovery, simple rewrites) — ~20× cheaper than Opus
/// - 3–6: Sonnet (default — implementation, test writing)
/// - > 6 or retry ≥ 2: Opus (complex multi-file changes, repeated failures)
#[must_use]
pub fn select_model(task: &Task, attempt: u8) -> &'static str {
    if attempt >= 2 {
        return MODEL_OPUS; // escalate on repeated failure
    }
    let size = task.constraints.len() + task.allowed_dirs.len();
    match size {
        0..=2 => MODEL_HAIKU,
        3..=6 => MODEL_SONNET,
        _ => MODEL_OPUS,
    }
}

/// Structured output from `claude --output-format json`.
#[derive(Debug, Deserialize)]
pub struct ClaudeOutput {
    /// JSON `type` field from the CLI response envelope.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// The assistant's text response, if present.
    pub result: Option<String>,
    /// `true` when the CLI reports an error outcome.
    pub is_error: Option<bool>,
    /// Estimated cost in USD as reported by the CLI.
    pub cost_usd: Option<f64>,
    /// Wall-clock duration of the CLI invocation in milliseconds.
    pub duration_ms: Option<u64>,
    /// Raw stdout from the CLI process — fallback when JSON parsing fails.
    #[serde(skip)]
    pub raw: String,
}

impl ClaudeOutput {
    /// Return the response text, falling back to raw stdout when `result` is absent.
    #[must_use]
    pub fn text(&self) -> &str {
        self.result.as_deref().unwrap_or(&self.raw)
    }
    /// Return `true` when the CLI did not report an error.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        !self.is_error.unwrap_or(false)
    }
}

/// Wrapper around the `claude` CLI — drives plan, implement, fix, and streaming calls.
pub struct ClaudeCode {
    repo_path: PathBuf,
    cli_path: String,
    timeout: Duration,
    json_output: bool,
    /// Constraints seeded from pattern memory — injected into the planning prompt.
    extra_constraints: Vec<String>,
    /// Model to use for CLI calls. None = let the CLI pick its default.
    model: Option<String>,
    /// Phase 5b — tabular pattern pairs (keywords, constraints) for TOON encoding.
    patterns: Vec<(String, String)>,
    /// Phase 5b — lessons learned from past patterns or post-mortems (category, content).
    lessons: Vec<(String, String)>,
    /// Per-session `--max-turns` cap passed to `claude -p`. None = CLI default.
    max_turns: Option<u32>,
    /// Per-session `--max-budget-usd` cap passed to `claude -p`. None = no cap.
    max_budget_usd: Option<f64>,
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
            patterns: vec![],
            lessons: vec![],
            max_turns: None,
            max_budget_usd: None,
        }
    }

    /// Override the Claude model used for CLI invocations.
    #[must_use]
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    /// Set the per-session `--max-turns` cap. The CLI halts cleanly at the cap
    /// and emits a terminal `result`, rather than running on.
    #[must_use]
    pub fn with_max_turns(mut self, turns: u32) -> Self {
        self.max_turns = Some(turns);
        self
    }

    /// Set the per-session `--max-budget-usd` cap. The CLI halts cleanly once
    /// cumulative cost reaches the cap.
    #[must_use]
    pub fn with_max_budget_usd(mut self, usd: f64) -> Self {
        self.max_budget_usd = Some(usd);
        self
    }

    /// Override the path to the `claude` CLI binary.
    #[must_use]
    pub fn with_cli(mut self, cli_path: impl Into<String>) -> Self {
        self.cli_path = cli_path.into();
        self
    }

    /// Set the per-invocation timeout in seconds.
    #[must_use]
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout = Duration::from_secs(secs);
        self
    }

    /// Enable or disable `--output-format json` on CLI calls.
    #[must_use]
    pub fn with_json_output(mut self, enabled: bool) -> Self {
        self.json_output = enabled;
        self
    }

    /// Inject additional constraints from pattern memory into the planning prompt.
    #[must_use]
    pub fn with_extra_constraints(mut self, constraints: Vec<String>) -> Self {
        self.extra_constraints = constraints;
        self
    }

    /// Attach TOON-encoded keyword/constraint pattern pairs for the planning prompt.
    #[must_use]
    pub fn with_patterns(mut self, patterns: Vec<(String, String)>) -> Self {
        self.patterns = patterns;
        self
    }

    /// Attach lessons learned from past post-mortems for the planning prompt.
    #[must_use]
    pub fn with_lessons(mut self, lessons: Vec<(String, String)>) -> Self {
        self.lessons = lessons;
        self
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

    /// Build the planning prompt: a TOON-encoded task context (goal, dirs,
    /// constraints, pattern memory, lessons) plus the optional previous-failure
    /// addendum. Shared by the one-shot [`plan`](Self::plan) and streaming
    /// [`plan_streamed`](Self::plan_streamed) paths so the prompt stays identical.
    fn build_plan_prompt(&self, task: &Task, last_error: Option<&str>) -> String {
        let all_constraints: Vec<&str> = task
            .constraints
            .iter()
            .chain(self.extra_constraints.iter())
            .map(String::as_str)
            .collect();
        let allowed: Vec<&str> = task.allowed_dirs.iter().map(String::as_str).collect();
        let forbidden: Vec<&str> = task.forbidden_dirs.iter().map(String::as_str).collect();
        // Convert lessons from Vec<(String, String)> to Vec<(&str, &str)> for TOON.
        let lesson_refs: Vec<(&str, &str)> = self
            .lessons
            .iter()
            .map(|(cat, content)| (cat.as_str(), content.as_str()))
            .collect();
        let ctx = encode_task_context(
            &task.goal,
            &allowed,
            &forbidden,
            &all_constraints,
            &self.patterns,
            &lesson_refs,
        );
        let mut prompt = format!(
            "You are running inside lopi. \
             Produce a concise implementation plan. \
             Output a numbered list of steps only.\n\n\
             ## Task context (TOON)\n\
             {ctx}"
        );
        if let Some(err) = last_error {
            prompt.push_str(&format!(
                "\n\n## Previous attempt failed\nAnalyze this error and adjust your approach:\n{err}"
            ));
        }
        prompt
    }

    /// Build the implementation prompt: a TOON-encoded scope plus the plan.
    /// Shared by [`implement`](Self::implement) and
    /// [`implement_streamed`](Self::implement_streamed).
    fn build_implement_prompt(&self, task: &Task, plan: &str) -> String {
        let allowed: Vec<&str> = task.allowed_dirs.iter().map(String::as_str).collect();
        let forbidden: Vec<&str> = task.forbidden_dirs.iter().map(String::as_str).collect();
        let scope = encode_task_context(&task.goal, &allowed, &forbidden, &[], &[], &[]);
        format!(
            "Implement the plan below in the current repository.\n\n\
             ## Scope (TOON)\n\
             {scope}\n\
             ## Plan\n\
             {plan}"
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
    /// structured pane events. Returns the canonical final response text.
    async fn run_streamed<F>(&self, prompt: &str, on_event: F) -> Result<String>
    where
        F: Fn(&StreamEvent) + Send,
    {
        let mut cmd = Command::new(&self.cli_path);
        cmd.arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("stream-json")
            .arg("--verbose")
            .arg("--include-partial-messages");
        if let Some(model) = &self.model {
            cmd.arg("--model").arg(model);
        }
        if let Some(turns) = self.max_turns {
            cmd.arg("--max-turns").arg(turns.to_string());
        }
        if let Some(usd) = self.max_budget_usd {
            cmd.arg("--max-budget-usd").arg(format!("{usd}"));
        }
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
                    for ev in parse_line(&line) {
                        if let Some(t) = ev.final_text() {
                            final_text = t.to_string();
                        } else if let Some(l) = ev.log_line() {
                            fallback.push_str(&l);
                            fallback.push('\n');
                        }
                        on_event(&ev);
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
        F: Fn(&StreamEvent) + Send,
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
        F: Fn(&StreamEvent) + Send,
    {
        let prompt = self.build_implement_prompt(task, plan);
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
        let prompt = self.build_implement_prompt(task, plan);
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
        if let Some(model) = &self.model {
            cmd.arg("--model").arg(model);
        }
        cmd.current_dir(&self.repo_path);
        scrub_inherited_anthropic_env(&mut cmd);

        let raw_out = tokio::time::timeout(self.timeout, cmd.output())
            .await
            .context("claude cli timed out")?
            .context("invoking claude cli")?;

        if !raw_out.status.success() {
            let stderr = String::from_utf8_lossy(&raw_out.stderr);
            let stdout = String::from_utf8_lossy(&raw_out.stdout);

            // Claude CLI writes structured failure payloads to stdout (rate-limit
            // JSON, auth errors, billing errors) while exiting non-zero. Parse
            // the JSON envelope when present so we surface the human-readable
            // `result` field plus the API status code, instead of a wall of
            // JSON noise. Falls back to raw streams when the envelope is absent
            // or unparseable.
            let parsed_msg: Option<(String, Option<u16>)> =
                serde_json::from_str::<serde_json::Value>(&stdout)
                    .ok()
                    .and_then(|v| {
                        let result = v.get("result")?.as_str()?.to_string();
                        let status = v
                            .get("api_error_status")
                            .and_then(serde_json::Value::as_u64)
                            .map(|s| s as u16);
                        Some((result, status))
                    });

            tracing::error!(
                cwd = %self.repo_path.display(),
                model = self.model.as_deref().unwrap_or("<default>"),
                prompt_bytes = prompt.len(),
                status = %raw_out.status,
                stderr = %stderr,
                stdout = %stdout,
                "claude cli failed"
            );

            if let Some((msg, api_status)) = parsed_msg {
                // Hard stop for billing failure — retrying just stalls the
                // agent. The run loop matches on ERR_CREDIT_EXHAUSTED to
                // short-circuit instead of burning the retry budget.
                if msg.to_lowercase().contains("credit balance") || api_status == Some(402) {
                    anyhow::bail!(
                        "{ERR_CREDIT_EXHAUSTED}: {msg}. \
                         Add credits at https://console.anthropic.com/settings/billing"
                    );
                }
                let api = api_status
                    .map(|s| format!(" (api_error_status={s})"))
                    .unwrap_or_default();
                anyhow::bail!("claude api error{api}: {msg}");
            }

            let detail = match (stderr.trim().is_empty(), stdout.trim().is_empty()) {
                (false, false) => format!("stderr={stderr}; stdout={stdout}"),
                (false, true) => format!("stderr={stderr}"),
                (true, false) => format!("stdout={stdout}"),
                (true, true) => "no output on stderr or stdout".to_string(),
            };
            anyhow::bail!(
                "claude cli exited {} (cwd={}, prompt={}B): {detail}",
                raw_out.status,
                self.repo_path.display(),
                prompt.len(),
            );
        }

        let stdout = String::from_utf8_lossy(&raw_out.stdout).into_owned();
        if self.json_output {
            match serde_json::from_str::<ClaudeOutput>(&stdout) {
                Ok(mut o) => {
                    o.raw = stdout;
                    Ok(o)
                }
                Err(_) => Ok(ClaudeOutput {
                    kind: None,
                    result: Some(stdout.clone()),
                    is_error: None,
                    cost_usd: None,
                    duration_ms: None,
                    raw: stdout,
                }),
            }
        } else {
            Ok(ClaudeOutput {
                kind: None,
                result: Some(stdout.clone()),
                is_error: None,
                cost_usd: None,
                duration_ms: None,
                raw: stdout,
            })
        }
    }
}

/// Names of environment variables that, when inherited from the parent
/// process, cause the spawned `claude` CLI to bypass the user's interactive
/// subscription auth and route through the per-token billed API (or a custom
/// gateway). lopi must NOT silently bill against the user's API balance —
/// the design intent is to drive their Claude Code subscription. We strip
/// these from the child process env so the CLI falls back to its on-disk
/// credentials at `~/.claude/`.
const ANTHROPIC_ROUTING_ENV: &[&str] = &[
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "ANTHROPIC_BEDROCK_BASE_URL",
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
];

/// Remove inherited Anthropic routing/auth env vars from a spawned-process
/// command. Used for both the one-shot `run()` path and the streaming plan
/// path so neither accidentally bills against a user's API credits.
pub(crate) fn scrub_inherited_anthropic_env(cmd: &mut Command) {
    for var in ANTHROPIC_ROUTING_ENV {
        cmd.env_remove(var);
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

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lopi_core::Task;

    #[test]
    fn select_model_haiku_for_minimal_task() {
        // 0 constraints + 2 default allowed_dirs = size 2 → Haiku
        let t = Task::new("fix a typo");
        assert_eq!(select_model(&t, 0), MODEL_HAIKU);
    }

    #[test]
    fn select_model_sonnet_for_medium_task() {
        let mut t = Task::new("implement feature");
        t.constraints = vec!["no new deps".into(), "keep API stable".into()];
        // 2 constraints + 2 default dirs = size 4 → Sonnet
        assert_eq!(select_model(&t, 0), MODEL_SONNET);
    }

    #[test]
    fn select_model_opus_for_large_task() {
        let mut t = Task::new("big refactor");
        t.constraints = vec![
            "c1".into(),
            "c2".into(),
            "c3".into(),
            "c4".into(),
            "c5".into(),
        ];
        // 5 constraints + 2 dirs = size 7 → Opus
        assert_eq!(select_model(&t, 0), MODEL_OPUS);
    }

    #[test]
    fn select_model_escalates_to_opus_at_attempt_2() {
        let t = Task::new("simple task");
        assert_eq!(select_model(&t, 2), MODEL_OPUS);
    }

    #[test]
    fn select_model_escalates_to_opus_at_attempt_3() {
        let t = Task::new("simple task");
        assert_eq!(select_model(&t, 3), MODEL_OPUS);
    }

    #[test]
    fn compress_errors_removes_backtrace_noise() {
        let errors = vec![
            "error[E0308]: mismatched types\n  at src/main.rs:10\nnote: run with RUST_BACKTRACE=1\nstack backtrace:\n  at src/foo.rs:5".to_string(),
        ];
        let out = compress_errors(&errors);
        assert!(!out.contains("RUST_BACKTRACE"));
        assert!(!out.contains("stack backtrace:"));
        assert!(!out.contains("at src/"));
        assert!(out.contains("mismatched types"));
    }

    #[test]
    fn compress_errors_deduplicates_identical_blocks() {
        let block = "error: cannot borrow as mutable".to_string();
        let errors = vec![block.clone(), block.clone(), block.clone()];
        let out = compress_errors(&errors);
        // Only one copy should survive deduplication
        assert_eq!(out.matches("cannot borrow").count(), 1);
    }
}
