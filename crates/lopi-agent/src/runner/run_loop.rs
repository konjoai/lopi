use super::{backoff_secs, AgentRunner};
use crate::claude::{select_model, ClaudeCode};
use crate::scorer::Scorer;
use anyhow::Result;
use lopi_context::Phase;
use lopi_core::{AgentEvent, Attempt, TaskStatus};
use lopi_git::GitManager;
use lopi_spec::SpecSurface;
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
                            p.successful_constraints.as_deref().and_then(|c| {
                                if c.is_empty() {
                                    None
                                } else {
                                    Some(c.to_string())
                                }
                            })
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

                    let lessons = match store
                        .load_lessons(self.repo_path.to_string_lossy().as_ref(), 10)
                        .await
                    {
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
        self.task_lessons = lessons_data
            .iter()
            .map(|(_, content)| content.clone())
            .collect();

        // Load spec surface if cached — inject top 10 items as planning constraints.
        let spec_constraints: Vec<String> = match SpecSurface::load(&self.repo_path) {
            Ok(Some(surface)) if !surface.is_empty() => {
                self.log(format!("📋 spec surface: {} items loaded", surface.len()));
                surface.top_descriptions(10)
            }
            _ => vec![],
        };

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
                self.last_plan = Some(plan.clone());
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
                // Sprint S — Konjo Verifier: rubric-guided second-score pass.
                // Runs only when verifier_enabled; best-effort (errors log + continue).
                if self.verifier_enabled
                    && !self.run_verifier_pass(attempt + 1, &score.errors).await
                {
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
                // OTel GenAI-aligned span: task completion event. The span
                // body is empty — it exists to mark the task boundary in
                // any attached trace exporter (OTLP via `otel` feature).
                let _ = tracing::info_span!(
                    "lopi.agent.task.complete",
                    task_id = %self.id(),
                    outcome = "success",
                    attempts = attempt + 1,
                )
                .entered();
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
