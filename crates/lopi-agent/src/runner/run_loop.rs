use super::progress::ProgressGate;
use super::speculative::SpecFlow;
use super::terminal_errors::terminal_failure_reason;
use super::test_phase::TestPhaseOutcome;
use super::AgentRunner;
use crate::claude::{select_model, ClaudeCode};
use crate::scorer::Scorer;
use anyhow::Result;
use lopi_context::Phase;
use lopi_core::{AgentEvent, StopReason, TaskStatus};
use lopi_git::GitManager;
use std::sync::atomic::Ordering;
use tracing::Instrument as _;

/// Tool-use turns allowed within a single `claude -p` session (planning or
/// implementing). Deliberately independent of `AgentRunner::max_turns`
/// (`task.max_iterations`, the card's "loop ×N" repeat-count) — a
/// multi-step research-and-write task can easily need dozens of tool calls
/// in one session, which has nothing to do with how many times the whole
/// card is set to repeat. Generous rather than tight: `CLI_SESSION_TIMEOUT_SECS`
/// and `self.cli_budget_usd` are the real cost/runaway safety nets; this just
/// guards against a session that's genuinely stuck looping tool calls forever.
const CLI_SESSION_MAX_TURNS: u32 = 100;

/// Wall-clock budget for a single `claude -p` session. `ClaudeCode::new`'s
/// own default (300s) is sized for a single-shot plan/implement call and was
/// never a real constraint back when `CLI_SESSION_MAX_TURNS` was
/// accidentally tiny (a session died from `error_max_turns` in seconds) —
/// raising the turn budget surfaced this as the *next* bottleneck, confirmed
/// live: a deep-research task read the repo, launched 6 parallel research
/// sub-agents, and got verified findings back from 5 of them before dying on
/// "claude cli timed out after 300s" — 5 minutes was never enough wall-clock
/// for that much real work either.
const CLI_SESSION_TIMEOUT_SECS: u64 = 1800;

/// Roll back uncommitted changes and return to the default branch — the
/// standard cleanup before recording a retry, cancellation, or terminal
/// failure. Both operations are best-effort: a rollback/checkout failure
/// must not block the status transition that follows.
pub(super) async fn abort_attempt(git: &GitManager) {
    git.hard_rollback().await.ok();
    git.checkout_default().await.ok();
}

impl AgentRunner {
    /// Execute the full agent loop: plan → implement → test → retry.
    ///
    /// # Errors
    ///
    /// Returns an error if git operations fail or if a scorer command fails unexpectedly.
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self), fields(task_id = %self.task.id, goal = %self.task.goal))]
    pub async fn run(&mut self) -> Result<TaskStatus> {
        self.boot_context();

        // Sprint I — Layer 5 stability pre-flight. Runs before git or any
        // implementation work. On Unstable verdict: return early with a
        // Failed status containing the variance score for the ledger.
        if let Some(blocked) = self.run_stability_preflight().await {
            self.status(blocked.clone(), 0);
            return Ok(blocked);
        }

        // Guardrails — `gate`: a precondition that must pass before the
        // very first iteration. Runs once; a failing (or unspawnable) gate
        // blocks the loop entirely rather than burning a retry attempt.
        if let Some(blocked) = self.run_gate_preflight().await {
            self.status(blocked.clone(), 0);
            return Ok(blocked);
        }

        let git = GitManager::new(&self.repo_path)?;

        // Seed the planning prompt with patterns, lessons, and the spec
        // surface from memory (see `seed.rs`).
        let seed = self.gather_seed().await;
        let extra_constraints = seed.extra_constraints;
        let pattern_pairs = seed.pattern_pairs;
        let lessons_data = seed.lessons_data;
        let spec_constraints = seed.spec_constraints;

        let scorer = Scorer::new(&self.repo_path);

        // Progress-Gating (A3) — the live gain gate + termination controls.
        // Keeps an iteration only when it is a genuine gain over best, halts
        // after K non-gaining rounds (no-progress), and caps the loop's token
        // budget. `0` disables either guard. Reuses A1's score (via the gain
        // gate) rather than rebuilding scoring.
        let no_progress_limit = self.no_progress_limit().await;
        let mut gate = ProgressGate::new(no_progress_limit, self.effective_budget_tokens());

        for attempt in 0..self.task.max_retries {
            // Model routing (4.5): route to cheapest model capable of this task's complexity.
            // Escalates to Opus after the first retry failure.
            let model = select_model(&self.task, attempt);
            // Merge pattern constraints + spec constraints for the planning prompt.
            let all_constraints: Vec<String> = extra_constraints
                .iter()
                .chain(spec_constraints.iter())
                .cloned()
                .collect();
            let mut claude = ClaudeCode::new(&self.repo_path)
                .with_extra_constraints(all_constraints)
                .with_patterns(pattern_pairs.clone())
                .with_lessons(lessons_data.clone())
                .with_model(model.clone())
                // Fold the card's `Effort` knob into the worker session.
                // `with_effort` validates against the CLI's accepted levels
                // and drops anything else, so a malformed `Task.effort`
                // can't wedge the spawn. Empty/None leaves the CLI default.
                .with_effort(self.task.effort.clone().unwrap_or_default())
                // Fold the card's `PermissionMode` knob into the worker
                // session. `with_permission_mode` validates against the
                // CLI's four headless-safe values and drops anything else;
                // `Task::permission_mode` defaults to `BypassPermissions`, so
                // an unconfigured task reproduces the pre-existing
                // unconditional `--dangerously-skip-permissions` behavior
                // exactly.
                .with_permission_mode(self.task.permission_mode.as_str())
                // `ClaudeCode::new`'s own default (300s) was sized for a
                // single-shot plan/implement call, not a session that fans
                // out into several parallel research sub-agents (each doing
                // multiple web fetches) — confirmed live: raising
                // `CLI_SESSION_MAX_TURNS` above let a deep-research task
                // actually do the work (read the repo, launch 6 sub-agents,
                // get verified findings back from 5 of them) and then die on
                // "claude cli timed out after 300s" instead, since 5 minutes
                // was never enough wall-clock for that much real work either.
                .with_timeout(CLI_SESSION_TIMEOUT_SECS);
            // `self.max_turns` is `task.max_iterations` — the card's "loop
            // ×N" repeat-count setting (`pool/run_loop.rs`'s `AgentRunner`
            // builder) — and governs the OUTER plan→implement→test attempt
            // loop below (`self.turn_count > self.max_turns`), not this
            // subprocess. It used to also be passed as this CLI session's
            // own `--max-turns` (tool-use turns within ONE `claude -p`
            // call), which meant "loop ×2" silently capped Claude at two
            // tool calls total per attempt — nowhere near enough for a
            // multi-step research/write task, which reliably hit
            // `error_max_turns` before doing any real work regardless of the
            // actual loop-count setting. The two are unrelated axes: use a
            // fixed, generous per-session budget here instead, independent
            // of how many times the user wants the whole card to repeat.
            claude = claude.with_max_turns(CLI_SESSION_MAX_TURNS);
            if let Some(usd) = self.cli_budget_usd {
                claude = claude.with_max_budget_usd(usd);
            }
            if !self.permission_allow.is_empty() || !self.permission_deny.is_empty() {
                claude = claude
                    .with_allowed_tools(self.permission_allow.clone())
                    .with_disallowed_tools(self.permission_deny.clone());
            }
            tracing::info!(model, attempt, "model selected for attempt");
            // Hard stop: prevent runaway agents from looping past the turn cap.
            // `max_turns == 0` is the infinite-loop sentinel (Task::max_iterations,
            // `Some(0)`) — the same "0 = disabled/unbounded" convention already
            // used by `no_progress_limit` and `budget_tokens`, so the cap is
            // skipped entirely rather than firing on the very first turn.
            self.turn_count += 1;
            if self.max_turns > 0 && self.turn_count > self.max_turns {
                let status = TaskStatus::Failed {
                    reason: format!(
                        "TurnLimitExceeded {{ limit: {}, task_id: {} }}",
                        self.max_turns, self.task.id
                    ),
                };
                self.status(status.clone(), self.attempts_made);
                return Ok(status);
            }

            if self.check_cancel() {
                return Ok(TaskStatus::Failed {
                    reason: "Cancelled".into(),
                });
            }

            // Budget gate (A3) — stop before spending more when a prior
            // attempt's streamed tokens have already exhausted the cap.
            if let Some(status) = self.budget_preflight(&gate, &git, attempt).await {
                return Ok(status);
            }

            self.attempts_made = attempt + 1;
            self.attempt_counter
                .store(attempt as usize + 1, Ordering::Relaxed);

            let branch = format!("lopi/{}-attempt-{}", self.task.id.0, attempt + 1);
            self.bus.send(AgentEvent::TaskStarted {
                task_id: self.id(),
                attempt: attempt + 1,
                branch: branch.clone(),
            });
            self.persist_branch(&branch);
            // `●` marks this as synthetic status, not Claude output — the
            // frontend's `reduceLogLine` (web/src/lib/stores/transcript.ts)
            // treats any *unprefixed* log line as real assistant text, so an
            // unmarked line here would render indistinguishably from
            // something Claude actually said.
            self.log(format!("● branch: {branch}"));

            if let Err(e) = git.checkout_new_branch(&branch).await {
                self.warn(format!("checkout failed: {e}"));
                self.status(
                    TaskStatus::Retrying {
                        attempt: attempt + 1,
                    },
                    attempt + 1,
                );
                continue;
            }

            self.status(TaskStatus::Planning, attempt + 1);
            self.context.transition_phase(Phase::Planning);
            tracing::info!(
                pressure = self.context.token_pressure(),
                "context at planning"
            );
            // No hardcoded phase label — the real status (thinking, tool calls,
            // text) streams in live from Claude below. The UI shows a "waiting
            // for Claude" animation until the first event arrives.

            if self.speculative {
                // Speculative mode: apply plan steps as they stream (see
                // `speculative.rs`). On a plan-stream failure the branch is
                // already rolled back — just retry.
                match self
                    .implement_speculative(&claude, &git, &model, attempt)
                    .await
                {
                    SpecFlow::Proceed => {}
                    SpecFlow::Retry => continue,
                }
            } else {
                // Standard mode: wait for full plan, then implement in one pass.
                //
                // Sprint G — direct-API planning path:
                // When the runner has been wired with an AnthropicClient
                // (via `with_api`), try the direct API first. On any
                // failure (rate-limited, breaker open, network error,
                // 4xx/5xx) fall back to the CLI subprocess silently. The
                // CLI path remains the load-bearing default so an API
                // outage cannot stall the agent loop.
                // OTel GenAI-aligned span: think phase. Wraps both the
                // direct-API and CLI planning paths.
                let think_span = tracing::info_span!(
                    "lopi.agent.think",
                    task_id = %self.id(),
                    attempt = attempt + 1,
                );
                let plan_result = async {
                    if self.has_direct_api() {
                        match self.plan_via_api(&model, attempt + 1).await {
                            Ok(p) => {
                                // Direct-API response arrives whole — surface it
                                // line-by-line so the log reads like the stream.
                                for line in p.lines() {
                                    let t = line.trim();
                                    if !t.is_empty() {
                                        self.log(t.to_string());
                                    }
                                }
                                Ok(p)
                            }
                            Err(api_err) => {
                                self.warn(format!(
                                    "direct API plan failed ({api_err}); falling back to CLI"
                                ));
                                self.stream_plan(&claude, &model, attempt + 1).await
                            }
                        }
                    } else {
                        self.stream_plan(&claude, &model, attempt + 1).await
                    }
                }
                .instrument(think_span)
                .await;

                let plan = match plan_result {
                    Ok(p) => p,
                    Err(e) => {
                        let err_chain = format!("{e:#}");
                        self.warn(format!("plan failed: {e}"));
                        abort_attempt(&git).await;
                        // Non-retryable failures (out of API credits, or lopi's
                        // own budget hard-stop) fail identically on any future
                        // attempt — stop now instead of stalling the agent and
                        // flooding the log with retries that can't succeed.
                        if let Some(reason) = terminal_failure_reason(&err_chain) {
                            let status = TaskStatus::Failed {
                                reason: format!("{reason}: {e}"),
                            };
                            self.status(status.clone(), attempt + 1);
                            return Ok(status);
                        }
                        self.status(
                            TaskStatus::Retrying {
                                attempt: attempt + 1,
                            },
                            attempt + 1,
                        );
                        continue;
                    }
                };
                self.last_plan = Some(plan.clone());

                // Dry-run: print plan and exit without touching git or running tests.
                if self.dry_run {
                    self.log("● dry-run — plan generated, no changes applied");
                    for line in plan.lines() {
                        self.log(line.to_string());
                    }
                    git.checkout_default().await.ok();
                    return Ok(TaskStatus::Failed {
                        reason: "dry-run complete".into(),
                    });
                }

                if self.check_cancel() {
                    abort_attempt(&git).await;
                    return Ok(TaskStatus::Failed {
                        reason: "Cancelled".into(),
                    });
                }

                // Phase 11 — plan approval gate (see `plan_gate.rs`). Returns a
                // terminal status when the operator rejects or cancels.
                if let Some(status) = self.gate_plan(&plan, attempt, &git).await {
                    return Ok(status);
                }

                self.status(TaskStatus::Implementing, attempt + 1);
                self.context.transition_phase(Phase::Implementation);
                tracing::info!(
                    pressure = self.context.token_pressure(),
                    "context at implementation"
                );
                // No hardcoded label — Claude's real actions (tool calls, text,
                // status) stream into the log live as it works.

                // OTel GenAI-aligned span: act phase. Uses `.instrument()`
                // (not `.entered()`) so the span guard is not held across
                // .await — the runner's outer future must stay Send.
                let act_span = tracing::info_span!(
                    "lopi.agent.act",
                    task_id = %self.id(),
                    attempt = attempt + 1,
                );
                let act_result = self
                    .stream_implement(&claude, &plan, &model, attempt + 1)
                    .instrument(act_span)
                    .await;
                if let Err(e) = act_result {
                    let err_chain = format!("{e:#}");
                    self.warn(format!("implement failed: {e}"));
                    // Same non-retryable cases as the plan path above.
                    if let Some(reason) = terminal_failure_reason(&err_chain) {
                        abort_attempt(&git).await;
                        let status = TaskStatus::Failed {
                            reason: format!("{reason}: {e}"),
                        };
                        self.status(status.clone(), attempt + 1);
                        return Ok(status);
                    }
                    self.abort_and_mark_retrying(&git, attempt).await;
                    continue;
                }
            }

            match self
                .run_test_phase(&scorer, &claude, &git, &mut gate, &branch, attempt)
                .await?
            {
                TestPhaseOutcome::Terminal(status) => return Ok(status),
                TestPhaseOutcome::Continue => continue,
            }
        }

        // Sprint H — post-mortem on terminal failure. Best-effort; never
        // block task termination. Requires both adaptive_retry AND a
        // configured AnthropicClient (api_client). Persists the derived
        // constraint to the patterns table for future similar tasks.
        if self.adaptive_retry {
            self.run_postmortem_if_configured().await;
        }

        // Max-iteration backstop (A3) — the lowest-precedence stop reason: the
        // loop exhausted its retry budget without meeting the goal or tripping
        // an earlier guard.
        let status = TaskStatus::Failed {
            reason: format!(
                "StopReason::{} {{ Max retries exceeded }}",
                StopReason::MaxIterations.as_str()
            ),
        };
        self.status(status.clone(), self.task.max_retries);
        Ok(status)
    }
}
