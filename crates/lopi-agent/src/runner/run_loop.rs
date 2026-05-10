use super::{backoff_secs, postmortem, AgentRunner};
use crate::claude::{select_model, ClaudeCode, MODEL_HAIKU};
use crate::scorer::Scorer;
use anyhow::Result;
use lopi_context::Phase;
use lopi_core::{AgentEvent, Attempt, LifecyclePhase, TaskStatus};
use lopi_git::GitManager;
use std::sync::atomic::Ordering;

impl AgentRunner {
    pub(super) fn status(&self, s: TaskStatus, attempt: u8) {
        let activity = match &s {
            TaskStatus::Planning => 0.45_f32,
            TaskStatus::Implementing => 0.85_f32,
            TaskStatus::Testing => 0.55_f32,
            TaskStatus::Scoring => 0.30_f32,
            TaskStatus::Retrying { .. } => 0.40_f32,
            TaskStatus::Success { .. } | TaskStatus::Failed { .. } | TaskStatus::RolledBack => {
                0.0_f32
            }
            TaskStatus::Queued => 0.10_f32,
        };
        self.emit_turn_metrics(activity);
        self.bus.send(AgentEvent::StatusChanged {
            task_id: self.id(),
            status: s,
            attempt,
        });
    }

    fn emit_turn_metrics(&self, activity: f32) {
        let pressure = self.context.token_pressure();
        self.bus.send(AgentEvent::TurnMetrics {
            task_id: self.id(),
            pressure,
            activity,
            tokens_per_sec: 0.0,
            cost_usd: 0.0,
        });
    }

    /// Emit a `Lifecycle` event with the runner's current goal + branch attached.
    pub(super) fn emit_lifecycle(
        &self,
        phase: LifecyclePhase,
        branch: Option<String>,
        score: Option<f32>,
    ) {
        self.bus.send(AgentEvent::Lifecycle {
            task_id: self.id(),
            phase,
            goal_snippet: self.task.goal.chars().take(80).collect(),
            branch,
            score,
            duration_ms: None,
        });
    }

    /// Sprint H — run the failure post-mortem if both adaptive retry and a
    /// direct-API client are configured. Best-effort; on any error we log
    /// a warning and continue. The derived constraint is persisted to the
    /// patterns table with `derived_from_postmortem = 1`.
    async fn run_postmortem_if_configured(&self) {
        let Some(client) = self.api_client.as_ref() else {
            // Adaptive retry without API access — postmortem can't run.
            // Future: a CLI fallback could pipe the error log to `claude -p`.
            return;
        };
        let Some(error_log) = self.last_error.as_deref() else {
            return;
        };

        self.log("🧠 running failure post-mortem…");
        let outcome = postmortem::run_postmortem_quiet(
            client,
            self.limiter.as_ref(),
            self.breaker.as_ref(),
            // Haiku is the right cost/quality trade-off for a single-line constraint
            MODEL_HAIKU,
            &self.task.goal,
            error_log,
        )
        .await;

        let Some(outcome) = outcome else {
            return;
        };

        // Persist the derived constraint as a pattern. Use the goal text as
        // the keywords so future similar tasks pick it up via Jaccard.
        if let Some(store) = &self.store {
            match store
                .insert_postmortem_pattern(&self.task.goal, &outcome.constraint)
                .await
            {
                Ok(id) => {
                    self.log(format!("🧠 post-mortem pattern saved [{}]", &id[..8]));
                    self.log(format!("    constraint: {}", outcome.constraint));

                    // Phase 5b — also save as a lesson for future reference.
                    let repo_str = self.repo_path.to_string_lossy();
                    let task_id_str = self.task.id.0.to_string();
                    if let Err(e) = store
                        .save_lesson(
                            &repo_str,
                            "recovery",
                            &outcome.constraint,
                            Some(&task_id_str),
                            1.0,
                        )
                        .await
                    {
                        self.warn(format!("failed to save post-mortem lesson: {e}"));
                    }
                }
                Err(e) => {
                    self.warn(format!("post-mortem persist failed: {e}"));
                }
            }
        } else {
            // No store — log the constraint anyway so it's not lost
            self.log(format!("🧠 post-mortem constraint: {}", outcome.constraint));
        }
    }

    /// Execute the full agent loop: plan → implement → test → retry.
    ///
    /// # Errors
    ///
    /// Returns an error if git operations fail or if a scorer command fails unexpectedly.
    #[allow(clippy::too_many_lines)]
    #[tracing::instrument(skip(self), fields(task_id = %self.task.id, goal = %self.task.goal))]
    pub async fn run(&mut self) -> Result<TaskStatus> {
        self.boot_context();

        let git = GitManager::new(&self.repo_path)?;

        // Seed planning prompt with patterns and lessons from memory.
        // Site 2 (TOON biggest win): PatternRow[] is a uniform tabular array.
        // encode_task_context() in claude.rs renders it as TOON §9.3 tabular,
        // saving ~158 tokens per attempt vs JSON (grows linearly with pattern count).
        let (extra_constraints, pattern_pairs, lessons_data) = if let Some(store) = &self.store {
            match store.find_similar_patterns(&self.task.goal).await {
                Ok(patterns) if !patterns.is_empty() => {
                    self.log(format!(
                        "🧠 seeding from {} similar past patterns",
                        patterns.len()
                    ));
                    // Build three outputs:
                    // 1. extra_constraints: string constraints (legacy flat list)
                    // 2. pattern_pairs: (keywords, constraints) tuples for TOON
                    // 3. lessons: (category, content) from lessons table
                    let constraints: Vec<String> = patterns
                        .iter()
                        .take(5)
                        .filter_map(|p| {
                            p.successful_constraints
                                .as_deref()
                                .and_then(|c| if c.is_empty() { None } else { Some(c.to_string()) })
                        })
                        .collect();

                    let pairs: Vec<(String, String)> = patterns
                        .iter()
                        .take(5)
                        .filter_map(|p| {
                            p.successful_constraints.as_deref().and_then(|c| {
                                if c.is_empty() {
                                    None
                                } else {
                                    Some((p.goal_keywords.clone(), c.to_string()))
                                }
                            })
                        })
                        .collect();

                    let lessons = match store.load_lessons(
                        self.repo_path.to_string_lossy().as_ref(),
                        10,
                    ).await {
                        Ok(rows) => rows
                            .into_iter()
                            .map(|row| (row.category, row.content))
                            .collect(),
                        Err(e) => {
                            self.warn(format!("failed to load lessons: {e}"));
                            vec![]
                        }
                    };

                    (constraints, pairs, lessons)
                }
                _ => (vec![], vec![], vec![]),
            }
        } else {
            (vec![], vec![], vec![])
        };

        // Store lessons for use in the API planning path.
        self.task_lessons = lessons_data.iter().map(|(_, content)| content.clone()).collect();

        let scorer = Scorer::new(&self.repo_path);

        for attempt in 0..self.task.max_retries {
            // Model routing (4.5): route to cheapest model capable of this task's complexity.
            // Escalates to Opus after the first retry failure.
            let model = select_model(&self.task, attempt);
            let claude = ClaudeCode::new(&self.repo_path)
                .with_extra_constraints(extra_constraints.clone())
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
            self.emit_lifecycle(LifecyclePhase::Thinking, Some(branch.clone()), None);

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
                self.emit_lifecycle(LifecyclePhase::Acting, Some(branch.clone()), None);

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
                            claude.plan(&self.task, self.last_error.as_deref()).await.inspect(|p| {
                                self.log(format!("✅ plan ready via CLI ({} chars)", p.len()));
                            })
                        }
                    }
                } else {
                    claude.plan(&self.task, self.last_error.as_deref()).await.inspect(|p| {
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
                self.emit_lifecycle(LifecyclePhase::Acting, Some(branch.clone()), None);

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
            self.emit_lifecycle(
                LifecyclePhase::Scored,
                Some(branch.clone()),
                Some(weighted),
            );
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
                let weighted = fixed_score.weighted(&self.score_weights);
                self.emit_lifecycle(
                    LifecyclePhase::Scored,
                    Some(branch.clone()),
                    Some(weighted),
                );
                self.log(format!(
                    "📊 fixed score: pass={:.0}% lint={} diff={}L (weighted={:.3})",
                    fixed_score.test_pass_rate * 100.0,
                    fixed_score.lint_errors,
                    fixed_score.diff_lines,
                    weighted
                ));
                if fixed_score.passed() {
                    self.log("✅ fix worked — committing…");
                    git.commit_all(&format!("lopi: {}", self.task.goal))
                        .await
                        .ok();
                    let pr_url = git.open_pr(&branch, &self.task.goal).await.ok();
                    let status = TaskStatus::Success { branch, pr_url };
                    self.status(status.clone(), attempt + 1);
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
