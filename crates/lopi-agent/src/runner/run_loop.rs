use super::{backoff_secs, AgentRunner};
use crate::claude::{select_model, ClaudeCode};
use crate::scorer::Scorer;
use anyhow::Result;
use lopi_context::Phase;
use lopi_core::{AgentEvent, Attempt, TaskStatus};
use lopi_git::GitManager;
use std::sync::atomic::Ordering;

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

        // Seed planning prompt with patterns from memory.
        // Site 2 (TOON biggest win): PatternRow[] is a uniform tabular array.
        // encode_task_context() in claude.rs renders it as TOON §9.3 tabular,
        // saving ~158 tokens per attempt vs JSON (grows linearly with pattern count).
        let mut extra_constraints: Vec<String> = if let Some(store) = &self.store {
            match store.find_similar_patterns(&self.task.goal).await {
                Ok(patterns) if !patterns.is_empty() => {
                    self.log(format!(
                        "🧠 seeding from {} similar past patterns",
                        patterns.len()
                    ));
                    // Collect (keywords, constraints) pairs for the TOON tabular encoder.
                    // These are passed to encode_task_context() inside ClaudeCode::plan().
                    patterns
                        .iter()
                        .take(5)
                        .filter_map(|p| {
                            let c = p
                                .successful_constraints
                                .as_deref()
                                .unwrap_or("")
                                .to_string();
                            if c.is_empty() {
                                None
                            } else {
                                Some(c)
                            }
                        })
                        .collect()
                }
                _ => vec![],
            }
        } else {
            vec![]
        };

        // Inject repo-level lessons into planning context. Lessons are general
        // strategy/recovery/optimization notes from previous runs on this repo.
        // Appended after task-specific pattern constraints so patterns take priority.
        if let Some(store) = &self.store {
            let repo_str = self.repo_path.to_string_lossy();
            match store.load_lessons(&repo_str, 3).await {
                Ok(lessons) if !lessons.is_empty() => {
                    self.log(format!("📚 injecting {} lessons", lessons.len()));
                    for lesson in &lessons {
                        extra_constraints.push(format!("[{}] {}", lesson.category, lesson.content));
                    }
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "lesson fetch failed"),
            }
        }

        // Load evolved score weights from annotated patterns. Best-effort —
        // falls back to ScoreWeights::default() on any error or missing store.
        self.load_score_weights().await;

        let scorer = Scorer::new(&self.repo_path);

        for attempt in 0..self.task.max_retries {
            // Model routing (4.5): route to cheapest model capable of this task's complexity.
            // Escalates to Opus after the first retry failure.
            let model = select_model(&self.task, attempt);
            let claude = ClaudeCode::new(&self.repo_path)
                .with_extra_constraints(extra_constraints.clone())
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
                // Speculative mode: apply plan steps as they stream from claude.
                let (plan_handle, mut step_rx) = claude.plan_streaming(&self.task);
                self.status(TaskStatus::Implementing, attempt + 1);
                self.context.transition_phase(Phase::Implementation);
                tracing::info!(
                    pressure = self.context.token_pressure(),
                    "context at speculative implementation"
                );
                self.log("⚡ speculative: applying steps as plan streams…");

                while let Some(step) = step_rx.recv().await {
                    self.log(format!("  ↳ {step}"));
                    if let Err(e) = claude.implement_step(&self.task, &step).await {
                        self.warn(format!("speculative step failed: {e}"));
                    }
                }

                // Wait for plan to finish; use the accumulated text for dry-run logging.
                let plan = match plan_handle.await {
                    Ok(Ok(p)) => p,
                    Ok(Err(e)) => {
                        let e = anyhow::anyhow!("{e}");
                        self.warn(format!("plan stream failed: {e}"));
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
                    Err(e) => {
                        let e = anyhow::anyhow!("{e}");
                        self.warn(format!("plan stream failed: {e}"));
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
                };
                self.log(format!(
                    "✅ speculative plan+implement done ({} chars)",
                    plan.len()
                ));
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
                let plan_result = if self.has_direct_api() {
                    match self.plan_via_api(model, attempt + 1).await {
                        Ok(p) => {
                            self.log(format!("✅ plan ready via direct API ({} chars)", p.len()));
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
                                    self.log(format!("✅ plan ready via CLI ({} chars)", p.len()));
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
                };

                let plan = match plan_result {
                    Ok(p) => p,
                    Err(e) => {
                        self.warn(format!("plan failed: {e}"));
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
                };

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

                self.status(TaskStatus::Implementing, attempt + 1);
                self.context.transition_phase(Phase::Implementation);
                tracing::info!(
                    pressure = self.context.token_pressure(),
                    "context at implementation"
                );
                self.log("🔨 implementing…");

                if let Err(e) = claude.implement(&self.task, &plan).await {
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
            let score = scorer.score().await?;

            self.bus.send(AgentEvent::ScoreUpdated {
                task_id: self.id(),
                test_pass_rate: score.test_pass_rate,
                lint_errors: score.lint_errors,
                diff_lines: score.diff_lines,
            });
            let weighted = score.weighted(&self.score_weights);
            self.log(format!(
                "📊 score: pass={:.0}% lint={} diff={}L weighted={:.3}",
                score.test_pass_rate * 100.0,
                score.lint_errors,
                score.diff_lines,
                weighted
            ));

            // Persist attempt with evolved weighted score.
            if let Some(store) = &self.store {
                let mut a = Attempt::new(self.id(), attempt + 1, &branch);
                a.score = Some(score.clone());
                a.weighted_score = Some(weighted);
                a.outcome = if score.passed() {
                    "success".into()
                } else {
                    "retry".into()
                };
                store.save_attempt(&a).await.ok();
            }

            if score.passed() {
                self.log("✅ tests pass — committing…");
                self.context.pin_conclusion(
                    format!(
                        "Sprint succeeded — pass={:.0}% diff={}L",
                        score.test_pass_rate * 100.0,
                        score.diff_lines
                    ),
                    Phase::Conclusion,
                );
                tracing::info!(
                    pressure = self.context.token_pressure(),
                    "context at conclusion"
                );
                git.commit_all(&format!("lopi: {}", self.task.goal))
                    .await
                    .ok();
                let pr_url = git.open_pr(&branch, &self.task.goal).await.ok();
                if let Some(ref url) = pr_url {
                    self.log(format!("🔗 PR opened: {url}"));
                }
                let status = TaskStatus::Success { branch, pr_url };
                self.status(status.clone(), attempt + 1);
                self.run_kcqf_scan().await;
                return Ok(status);
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
                if fixed_score.passed() {
                    self.log("✅ fix worked — committing…");
                    git.commit_all(&format!("lopi: {}", self.task.goal))
                        .await
                        .ok();
                    let pr_url = git.open_pr(&branch, &self.task.goal).await.ok();
                    let status = TaskStatus::Success { branch, pr_url };
                    self.status(status.clone(), attempt + 1);
                    self.run_kcqf_scan().await;
                    return Ok(status);
                }
            }

            // Sprint H — adaptive retry: stash the score's error list so the
            // next attempt's planning prompt can include it. Only stored
            // when adaptive_retry is enabled to avoid pointless work.
            if self.adaptive_retry {
                self.last_error = Some(format!(
                    "Attempt {} failed:\n  test_pass_rate: {:.0}%\n  lint_errors: {}\n  diff_lines: {}\n  errors: {}",
                    attempt + 1,
                    score.test_pass_rate * 100.0,
                    score.lint_errors,
                    score.diff_lines,
                    if score.errors.is_empty() { "(none captured)".into() } else { score.errors.join("\n  - ") }
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
