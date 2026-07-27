//! The low-level `claude -p` spawn engine — split out of `claude.rs` purely
//! to keep that file under the 500-line CI file-size gate. Sprint F4's
//! resume-fallback wrapping (the establishment-failure detection plus the
//! cold-retry path) roughly doubled these four methods' combined length; no
//! behavioral difference from being in a sibling module. `run_streamed`/
//! `run` are the two call-in points [`super::ClaudeCode`]'s public API
//! (`plan_streamed`, `implement_streamed`, `fix`, `implement_step`) uses;
//! `run_streamed_once`/`run_once` are their `session`-parameterized bodies,
//! called twice on a resume-establishment failure (once with the real
//! session, once cold).

use crate::claude::ClaudeCode;
use crate::claude_events::{parse_line, StreamEvent};
use crate::claude_model::{ClaudeOutput, ERR_BUDGET_HARD_STOP};
use crate::claude_support::{
    apply_cli_caps, apply_env_allowlist, scrub_inherited_anthropic_env, SessionMode,
};
use anyhow::{Context, Result};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command;

impl ClaudeCode {
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
    pub(crate) async fn run_streamed<F>(&self, prompt: &str, on_event: F) -> Result<String>
    where
        F: Fn(&StreamEvent) -> bool + Send,
    {
        let establishment_failed = AtomicBool::new(false);
        match self
            .run_streamed_once(
                prompt,
                self.session.as_mode(),
                &on_event,
                &establishment_failed,
            )
            .await
        {
            // Sprint F4 Phase 2 — "fall back silently on any resume
            // failure. Cold spawn is the current behaviour and is always
            // correct." Only a resumed session's own establishment failure
            // (KT-4.1's live signature) triggers this, not any failure a
            // resumed call happens to hit — see
            // `claude_support::looks_like_session_establishment_failure`.
            Err(e) if self.session.is_resume() && establishment_failed.load(Ordering::Relaxed) => {
                tracing::warn!(
                    error = %e,
                    "resumed claude session could not be established; falling back to a cold spawn"
                );
                self.session_fell_back.store(true, Ordering::Relaxed);
                let cold = AtomicBool::new(false);
                self.run_streamed_once(prompt, SessionMode::None, &on_event, &cold)
                    .await
            }
            other => other,
        }
    }

    /// The actual spawn, parameterized on `session` so [`run_streamed`] can
    /// retry cold after a resume-establishment failure without duplicating
    /// this body. `establishment_failed` is set (never cleared here) if a
    /// terminal `result` looks like the session itself never established
    /// (see [`claude_support::looks_like_session_establishment_failure`]).
    async fn run_streamed_once<F>(
        &self,
        prompt: &str,
        session: SessionMode<'_>,
        on_event: &F,
        establishment_failed: &AtomicBool,
    ) -> Result<String>
    where
        F: Fn(&StreamEvent) -> bool + Send,
    {
        let mut cmd = Command::new(&self.cli_path);
        // Sprint S10, Phase 1 — must run before any other `.env()`/`.arg()`
        // call touches `cmd`: replaces the whole inherited environment with
        // the explicit allowlist (see `apply_env_allowlist`'s doc comment).
        apply_env_allowlist(&mut cmd);
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
            // Sprint F2 Phase 6 — a worker session; must load the target
            // repo's own CLAUDE.md/skills, so explicitly not `--bare`.
            false,
            session,
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
                        if let StreamEvent::Result {
                            subtype, num_turns, ..
                        } = &ev
                        {
                            if crate::claude_support::looks_like_session_establishment_failure(
                                subtype != "success",
                                *num_turns,
                            ) {
                                establishment_failed.store(true, Ordering::Relaxed);
                            }
                        }
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

    pub(crate) async fn run(&self, prompt: &str) -> Result<ClaudeOutput> {
        match self.run_once(prompt, self.session.as_mode()).await {
            // Sprint F4 Phase 2 — same "fall back silently on any resume
            // failure" contract as `run_streamed`'s, only triggered by a
            // resumed session's own establishment failure (KT-4.1: a bad
            // `--resume` exits non-zero, so `run_once` below surfaces this
            // as `Ok` with an error-shaped `ClaudeOutput` rather than
            // bubbling `build_cli_error`'s generic `Err` — that's what lets
            // this match distinguish "resume never established" from any
            // other non-zero exit).
            Ok(out)
                if self.session.is_resume() && out.looks_like_session_establishment_failure() =>
            {
                tracing::warn!(
                    "resumed claude session could not be established; falling back to a cold spawn"
                );
                self.session_fell_back.store(true, Ordering::Relaxed);
                self.run_once(prompt, SessionMode::None).await
            }
            other => other,
        }
    }

    /// The actual spawn, parameterized on `session` so [`run`] can retry
    /// cold after a resume-establishment failure without duplicating this
    /// body. On a non-zero exit, eagerly parses stdout to check for that
    /// specific signature *before* falling through to the generic
    /// `build_cli_error` path — a resume-establishment failure is returned
    /// as `Ok` (an error-shaped `ClaudeOutput`) precisely so [`run`] can
    /// inspect it and decide to retry cold, instead of that signal being
    /// erased into an opaque `anyhow::Error` string.
    async fn run_once(&self, prompt: &str, session: SessionMode<'_>) -> Result<ClaudeOutput> {
        let mut cmd = Command::new(&self.cli_path);
        apply_env_allowlist(&mut cmd);
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
            // Sprint F2 Phase 6 — a worker session (backs `fix()` and
            // `implement_step()`); must load the target repo's own
            // CLAUDE.md/skills, so explicitly not `--bare`.
            false,
            session,
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
            // Sprint F4 — parse eagerly (regardless of `self.json_output`;
            // the CLI emits this envelope shape on a resume failure
            // whatever output format was requested) so a resumed session's
            // own establishment failure can be returned as `Ok` for `run`
            // to inspect and retry cold, rather than erased into
            // `build_cli_error`'s opaque string.
            let parsed =
                crate::claude_model::parse_claude_output(stdout.clone().into_owned(), true);
            if matches!(session, SessionMode::Resume(_))
                && parsed.looks_like_session_establishment_failure()
            {
                return Ok(parsed);
            }
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
