use super::speculative::SpecFlow;
use super::{backoff_secs, AgentRunner};
use crate::claude::{select_model, ClaudeCode, ERR_CREDIT_EXHAUSTED};
use crate::scorer::Scorer;
use anyhow::Result;
use lopi_context::Phase;
use lopi_core::{AgentEvent, Attempt, TaskStatus};
use lopi_git::GitManager;
use std::sync::atomic::Ordering;
use tracing::Instrument as _;

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

        let git = GitManager::new(&self.repo_path)?;

        // Seed the planning prompt with patterns, lessons, and the spec
        // surface from memory (see `seed.rs`).
        let seed = self.gather_seed().await;
        let extra_constraints = seed.extra_constraints;
        let pattern_pairs = seed.pattern_pairs;
        let lessons_data = seed.lessons_data;
        let spec_constraints = seed.spec_constraints;

        let scorer = Scorer::new(&self.repo_path);

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
            let claude = ClaudeCode::new(&self.repo_path)
                .with_extra_constraints(all_constraints)
                .with_patterns(pattern_pairs.clone())
                .with_lessons(lessons_data.clone())
                .with_model(model);
            tracing::info!(model, attempt, "model selected for attempt");
            // Hard stop: prevent runaway agents from looping past the turn cap.
            self.turn_count += 1;
            if self.turn_count > self.max_turns {
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
            self.log("📋 planning…");

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
                        match self.plan_via_api(model, attempt + 1).await {
                            Ok(p) => {
                                self.log(format!(
                                    "✅ plan ready via direct API ({} chars)",
                                    p.len()
                                ));
                                Ok(p)
                            }
                            Err(api_err) => {
                                self.warn(format!(
                                    "direct API plan failed ({api_err}); falling back to CLI"
                                ));
                                claude
                                    .plan(&self.task, self.last_error.as_deref())
                                    .await
                                    .inspect(|p| {
                                        self.log(format!(
                                            "✅ plan ready via CLI ({} chars)",
                                            p.len()
                                        ));
                                    })
                            }
                        }
                    } else {
                        claude
                            .plan(&self.task, self.last_error.as_deref())
                            .await
                            .inspect(|p| {
                                self.log(format!("✅ plan ready ({} chars)", p.len()));
                            })
                    }
                }
                .instrument(think_span)
                .await;

                let plan = match plan_result {
                    Ok(p) => p,
                    Err(e) => {
                        let err_chain = format!("{e:#}");
                        self.warn(format!("plan failed: {e}"));
                        git.hard_rollback().await.ok();
                        git.checkout_default().await.ok();
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
                    git.hard_rollback().await.ok();
                    git.checkout_default().await.ok();
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
                self.log("🔨 implementing…");

                // OTel GenAI-aligned span: act phase. Uses `.instrument()`
                // (not `.entered()`) so the span guard is not held across
                // .await — the runner's outer future must stay Send.
                let act_span = tracing::info_span!(
                    "lopi.agent.act",
                    task_id = %self.id(),
                    attempt = attempt + 1,
                );
                let act_result = claude
                    .implement(&self.task, &plan)
                    .instrument(act_span)
                    .await;
                if let Err(e) = act_result {
                    self.warn(format!("implement failed: {e}"));
                    git.hard_rollback().await.ok();
                    git.checkout_default().await.ok();
                    self.status(
                        TaskStatus::Retrying {
                            attempt: attempt + 1,
                        },
                        attempt + 1,
                    );
                    continue;
                }
            }

            if let Err(e) = git
                .check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs)
                .await
            {
                self.warn(format!("diff scope violation: {e}"));
                self.status(TaskStatus::RolledBack, attempt + 1);
                git.hard_rollback().await.ok();
                git.checkout_default().await.ok();
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

            // P1.4 — Optional structured-output schema validation. When the
            // task carries `output_schema`, validate the scorer's JSON
            // projection against it. Each failure increments the
            // process-wide `lopi_schema_violations_total{kind=…}` counter
            // surfaced via `/metrics`. On any violation the agent stashes
            // the messages as `last_error` (so the next planning prompt
            // sees them via adaptive retry) and rolls into the next attempt.
            if let Some(ref schema) = self.task.output_schema {
                let score_json = serde_json::json!({
                    "test_pass_rate": score.test_pass_rate,
                    "lint_errors": score.lint_errors,
                    "diff_lines": score.diff_lines,
                });
                let violations = lopi_core::validate_schema(&score_json, schema);
                if !violations.is_empty() {
                    for v in &violations {
                        lopi_core::schema_violations_inc(v.kind.clone());
                    }
                    let summary = violations
                        .iter()
                        .map(|v| format!("- {}@{}: {}", v.kind.as_str(), v.path, v.message))
                        .collect::<Vec<_>>()
                        .join("\n");
                    self.warn(format!(
                        "📐 output_schema validation failed ({} issue(s)):\n{summary}",
                        violations.len()
                    ));
                    if self.adaptive_retry {
                        self.last_error = Some(format!(
                            "Attempt {} output failed schema validation:\n{summary}",
                            attempt + 1
                        ));
                    }
                    git.hard_rollback().await.ok();
                    git.checkout_default().await.ok();
                    self.status(
                        TaskStatus::Retrying {
                            attempt: attempt + 1,
                        },
                        attempt + 1,
                    );
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

            if score.passed() {
                // Phase 16.3 — finalize per the L1–L4 autonomy ladder. `finalize`
                // forces the verifier on for L3/L4, commits, rebases onto the
                // advanced default, then opens (or skips) the PR. `None` ⇒
                // verifier rejected (already rolled back, marked Retrying); a
                // `Conflict` ⇒ the rebase collided and the loop stops here.
                if let Some(status) = self.finalize(&branch, &git, &score, attempt + 1).await {
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

            git.hard_rollback().await.ok();
            git.checkout_default().await.ok();
            self.status(
                TaskStatus::Retrying {
                    attempt: attempt + 1,
                },
                attempt + 1,
            );
            // Full-jitter exponential backoff before the next attempt.
            // Base 500 ms — caps at 30 s. Prevents thundering-herd on transient failures.
            let wait = backoff_secs(attempt, 500);
            self.log(format!(
                "♻️ retry {}/{} (backoff {}ms)",
                attempt + 1,
                self.task.max_retries,
                wait.as_millis()
            ));
            tokio::time::sleep(wait).await;
        }

        // Sprint H — post-mortem on terminal failure. Best-effort; never
        // block task termination. Requires both adaptive_retry AND a
        // configured AnthropicClient (api_client). Persists the derived
        // constraint to the patterns table for future similar tasks.
        if self.adaptive_retry {
            self.run_postmortem_if_configured().await;
        }

        let status = TaskStatus::Failed {
            reason: "Max retries exceeded".into(),
        };
        self.status(status.clone(), self.task.max_retries);
        Ok(status)
    }
}
