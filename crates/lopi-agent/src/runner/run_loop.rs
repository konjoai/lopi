use super::progress::ProgressGate;
use super::speculative::SpecFlow;
use super::{schema_gate, AgentRunner};
use crate::claude::{select_model, ClaudeCode, ERR_CREDIT_EXHAUSTED};
use crate::scorer::Scorer;
use anyhow::Result;
use lopi_context::Phase;
use lopi_core::{AgentEvent, Attempt, StopReason, TaskStatus};
use lopi_git::GitManager;
use std::sync::atomic::Ordering;
use tracing::Instrument as _;

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
                .with_model(model.clone());
            // Cost ceiling: the CLI session must not out-run the runner's own
            // turn cap. `max_turns == 0` is the infinite-loop sentinel — omit
            // `--max-turns` entirely rather than pass a literal 0, which would
            // cap the subprocess at zero turns instead of leaving it unbounded.
            if self.max_turns > 0 {
                claude = claude.with_max_turns(self.max_turns);
            }
            if let Some(usd) = self.cli_budget_usd {
                claude = claude.with_max_budget_usd(usd);
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
            self.log(format!("🔀 branch: {branch}"));

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
                match self.implement_speculative(&claude, &git, attempt).await {
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
                                self.stream_plan(&claude).await
                            }
                        }
                    } else {
                        self.stream_plan(&claude).await
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
                        // Non-retryable: out of API credits. Retrying just stalls
                        // the agent and floods the log — fail fast with a clear
                        // terminal status so the operator sees the billing issue.
                        if err_chain.contains(ERR_CREDIT_EXHAUSTED) {
                            let status = TaskStatus::Failed {
                                reason: format!("CreditExhausted: {e}"),
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
                    self.log("🔍 dry-run — plan generated, no changes applied");
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
                    .stream_implement(&claude, &plan)
                    .instrument(act_span)
                    .await;
                if let Err(e) = act_result {
                    self.warn(format!("implement failed: {e}"));
                    self.abort_and_mark_retrying(&git, attempt).await;
                    continue;
                }
            }

            if let Err(e) = git
                .check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs)
                .await
            {
                self.warn(format!("diff scope violation: {e}"));
                self.status(TaskStatus::RolledBack, attempt + 1);
                abort_attempt(&git).await;
                continue;
            }

            self.status(TaskStatus::Testing, attempt + 1);
            self.context.transition_phase(Phase::Testing);
            tracing::info!(
                pressure = self.context.token_pressure(),
                "context at testing"
            );
            self.log("🧪 running tests…");
            // OTel GenAI-aligned span: score phase.
            let score_span = tracing::info_span!(
                "lopi.agent.score",
                task_id = %self.id(),
                attempt = attempt + 1,
            );
            let score = scorer.score().instrument(score_span).await?;

            // P1.4 — Optional structured-output schema validation (see
            // `schema_gate.rs`). On any violation the agent stashes the
            // messages as `last_error` (so the next planning prompt sees them
            // via adaptive retry) and rolls into the next attempt.
            if let Some(ref schema) = self.task.output_schema {
                if let Some((count, summary)) = schema_gate::violation_summary(schema, &score) {
                    self.warn(format!(
                        "📐 output_schema validation failed ({count} issue(s)):\n{summary}"
                    ));
                    if self.adaptive_retry {
                        self.last_error = Some(format!(
                            "Attempt {} output failed schema validation:\n{summary}",
                            attempt + 1
                        ));
                    }
                    self.abort_and_mark_retrying(&git, attempt).await;
                    continue;
                }
            }

            self.bus.send(AgentEvent::ScoreUpdated {
                task_id: self.id(),
                test_pass_rate: score.test_pass_rate,
                lint_errors: score.lint_errors,
                diff_lines: score.diff_lines,
            });
            let weighted = score.weighted(&self.score_weights);
            self.log(format!(
                "📊 score: pass={:.0}% lint={} diff={}L (weighted={:.3})",
                score.test_pass_rate * 100.0,
                score.lint_errors,
                score.diff_lines,
                weighted
            ));
            // Best weighted score seen this attempt — updated if an in-place
            // fix lifts it. Drives the no-progress stall guard below.
            let mut attempt_weighted = weighted;

            // Persist attempt.
            if let Some(store) = &self.store {
                let mut a = Attempt::new(self.id(), attempt + 1, &branch);
                a.score = Some(score.clone());
                a.outcome = if score.passed() {
                    "success".into()
                } else {
                    "retry".into()
                };
                store.save_attempt(&a).await.ok();
            }

            // Guardrails — `until`: an independent exit-condition checked
            // every iteration. A pass ends the loop early as a success
            // regardless of the iteration's own test score; `None`
            // configured leaves `score.passed()` as the sole condition,
            // unchanged from before this field existed.
            let until_satisfied = self.check_until().await;
            if score.passed() || until_satisfied {
                if until_satisfied && !score.passed() {
                    self.log("🏁 until condition met — concluding the loop early");
                }
                // Phase 16.3 — finalize per the L1–L4 autonomy ladder. `finalize`
                // forces the verifier on for L3/L4, commits, rebases onto the
                // advanced default, then opens (or skips) the PR. `None` ⇒
                // verifier rejected (already rolled back, marked Retrying); a
                // `Conflict` ⇒ the rebase collided and the loop stops here.
                if let Some(status) = self.finalize(&branch, &git, &score, attempt + 1).await {
                    // Goal-met (A3) — the highest-precedence terminal: the loop
                    // satisfied its acceptance/until goal and finalized.
                    self.log(format!("🎯 stop reason: {}", StopReason::GoalMet.as_str()));
                    return Ok(self.conclude_finalized(status, &score, attempt + 1));
                }
                continue;
            }

            // In-place fix attempt.
            self.log(format!("🔧 fixing {} error(s)…", score.errors.len()));
            if let Err(e) = claude.fix(&self.task, &score.errors).await {
                self.warn(format!("fix failed: {e}"));
            }

            if git
                .check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs)
                .await
                .is_ok()
            {
                self.status(TaskStatus::Testing, attempt + 1);
                let fixed_score = scorer.score().await?;
                self.bus.send(AgentEvent::ScoreUpdated {
                    task_id: self.id(),
                    test_pass_rate: fixed_score.test_pass_rate,
                    lint_errors: fixed_score.lint_errors,
                    diff_lines: fixed_score.diff_lines,
                });
                let weighted = fixed_score.weighted(&self.score_weights);
                self.log(format!(
                    "📊 fixed score: pass={:.0}% lint={} diff={}L (weighted={:.3})",
                    fixed_score.test_pass_rate * 100.0,
                    fixed_score.lint_errors,
                    fixed_score.diff_lines,
                    weighted
                ));
                // The fix lifted (or lowered) the score — track the better of
                // the two for the stall guard.
                attempt_weighted = attempt_weighted.max(weighted);
                if fixed_score.passed() {
                    self.log("✅ fix worked — finalizing…");
                    // Same L1–L4 finalize path as the primary success branch.
                    if let Some(status) = self
                        .finalize(&branch, &git, &fixed_score, attempt + 1)
                        .await
                    {
                        self.status(status.clone(), attempt + 1);
                        return Ok(status);
                    }
                    continue;
                }
            }

            // Sprint H — adaptive retry: stash the score's error list so the
            // next attempt's planning prompt can include it. Only stored
            // when adaptive_retry is enabled to avoid pointless work.
            if self.adaptive_retry {
                let base_failure = format!(
                    "Attempt {} failed:\n  test_pass_rate: {:.0}%\n  lint_errors: {}\n  diff_lines: {}\n  errors: {}",
                    attempt + 1,
                    score.test_pass_rate * 100.0,
                    score.lint_errors,
                    score.diff_lines,
                    if score.errors.is_empty() { "(none captured)".into() } else { score.errors.join("\n  - ") }
                );
                // Phase 16.4/16.5 — reframe the raw failure per the self-prompting
                // strategy. `Direct` returns it unchanged (legacy behaviour);
                // richer strategies prepend a Reflexion / Self-Refine /
                // Plan-Then-Act preamble. With escalation enabled the strategy
                // climbs one S-rung per failed attempt (see `effective_strategy`).
                let strategy = self.effective_strategy(attempt + 1);
                self.last_error = Some(strategy.frame(&base_failure, attempt + 1));
            }

            // Gain gate + termination (A3) — feed this attempt's best objective
            // score to the gate (a gain locks best + resets the streak; a
            // non-gain keeps the prior best and grows it) and stop with a
            // specific `StopReason` when budget or no-progress trips. The
            // rejected (non-gaining) iteration's work is discarded by
            // `abort_and_mark_retrying` below — A1's rollback path, unchanged.
            if let Some(status) = self
                .observe_and_check_stop(&mut gate, attempt_weighted, &git, attempt + 1)
                .await
            {
                return Ok(status);
            }

            self.abort_and_mark_retrying(&git, attempt).await;
            self.apply_on_fail_delay(attempt).await;
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
