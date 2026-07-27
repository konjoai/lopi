// TOON integration sites (from token analysis):
//   plan_streamed()      — constraints, allowed_dirs, forbidden_dirs arrays + pattern memory table
//   implement_streamed() — allowed_dirs, forbidden_dirs arrays
//   fix()                — allowed_dirs only (error text is free-form prose; TOON skipped)
//
// Measured token savings (cl100k_base, vs. compact JSON, on real corpus —
// see crates/lopi-toon/benches/results/2026-07-26_token_savings.md):
// adding the constraint array to a dirs-only prompt costs ~2.0 tokens/prompt
// (a small loss, not a saving); adding the pattern table saves ~5.0
// tokens/attempt. Both replace unsourced "~17/prompt" and "~158/attempt"
// figures that did not trace to any committed measurement.

use crate::claude_events::StreamEvent;
use anyhow::Result;
use lopi_core::Task;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Re-exported so every existing `crate::claude::select_model`/
/// `ClaudeOutput`/`ERR_CREDIT_EXHAUSTED` path stays valid — these moved to
/// `claude_model.rs` purely to keep this file under the 500-line CI
/// file-size gate; see that module's doc comment.
pub use crate::claude_model::{
    select_model, ClaudeOutput, ERR_BUDGET_HARD_STOP, ERR_CREDIT_EXHAUSTED,
};
use crate::claude_support::compress_errors;
/// Re-exported so `crate::claude::scrub_inherited_anthropic_env` stays valid
/// for `claude_stream.rs`'s call site — moved to `claude_support.rs` for the
/// same file-size reason.
pub(crate) use crate::claude_support::scrub_inherited_anthropic_env;
/// Re-exported so every existing `crate::claude::model_*` path stays valid —
/// Sprint F2 Phase 4 moved these from hardcoded consts here to
/// `crate::model_config`'s runtime-read, operator-overridable config.
pub use crate::model_config::{model_haiku, model_opus, model_sonnet};

/// Session-continuity state carried on a `ClaudeCode` instance across the
/// plan → implement → fix calls of a single attempt (Sprint F4 Phase 2 —
/// "one session per attempt, not per phase"). Owned (unlike
/// [`claude_support::SessionMode`](crate::claude_support::SessionMode),
/// which borrows) because a `ClaudeCode` outlives any one spawned command.
#[derive(Debug, Clone, Default)]
pub(crate) enum SessionState {
    /// No session-continuity flag — the default before this sprint, and
    /// still what every checker/post-mortem spawn site uses unconditionally
    /// (Phase 3) and what a `--speculative` run's per-step spawns use (out
    /// of scope this sprint — see `run_loop.rs`'s speculative branch).
    #[default]
    None,
    /// Start a fresh session under this caller-chosen id (`--session-id`).
    New(String),
    /// Continue the session with this id (`--resume`).
    Resume(String),
}

impl SessionState {
    pub(crate) fn as_mode(&self) -> crate::claude_support::SessionMode<'_> {
        match self {
            SessionState::None => crate::claude_support::SessionMode::None,
            SessionState::New(id) => crate::claude_support::SessionMode::New(id),
            SessionState::Resume(id) => crate::claude_support::SessionMode::Resume(id),
        }
    }

    pub(crate) fn is_resume(&self) -> bool {
        matches!(self, SessionState::Resume(_))
    }
}

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
    /// Sprint F4 — session-continuity state for this attempt's spawns.
    /// `None` (the default) reproduces every spawn site's pre-F4 behavior.
    pub(crate) session: SessionState,
    /// Sprint F4 Phase 2 — set when a `Resume` spawn failed to establish and
    /// this instance silently fell back to a cold spawn (Phase 2's "fall
    /// back silently on any resume failure" constraint). `&self`-mutable so
    /// `run`/`run_streamed` can set it without a `&mut self` receiver;
    /// callers check it once after a call returns to log the fallback as a
    /// visible event rather than a silent one. `pub(crate)` (not private) so
    /// `claude_spawn.rs`'s sibling-module `impl ClaudeCode` block can set it.
    pub(crate) session_fell_back: AtomicBool,
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
            session: SessionState::None,
            session_fell_back: AtomicBool::new(false),
        }
    }

    /// Sprint F4 Phase 2 — whether the most recent call on this instance
    /// silently fell back from a resumed session to a cold spawn. Callers
    /// (the runner) check this once after a call returns and log the
    /// fallback as a visible event, per Phase 2's "recorded as an event, not
    /// a silent one" verify criterion.
    #[must_use]
    pub(crate) fn session_fell_back(&self) -> bool {
        self.session_fell_back.load(Ordering::Relaxed)
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
            // Sprint F4 — speculative mode is out of scope this sprint; see
            // `claude_stream::plan_streaming`'s doc comment.
            crate::claude_support::SessionMode::None,
        )
    }

    /// Apply a single plan step to the repository. Returns the full CLI
    /// output envelope (not just success) so the caller can meter the
    /// step's real token usage and cost — see [`ClaudeOutput::usage`].
    ///
    /// # Errors
    ///
    /// Returns an error if the claude CLI process fails or times out.
    pub async fn implement_step(&self, task: &Task, step: &str) -> Result<ClaudeOutput> {
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
        Ok(out)
    }
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;
