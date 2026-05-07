use crate::claude::{select_model, ClaudeCode};
use crate::scorer::Scorer;
use anyhow::Result;
use lopi_context::{ContentBlock, ContextWindow, Phase, PinPolicy, Role, TaggedMessage};
use lopi_core::{AgentEvent, Attempt, EventBus, Task, TaskId, TaskStatus};
use lopi_git::GitManager;
use lopi_memory::MemoryStore;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Full-jitter exponential backoff for transient failures.
///
/// Computes: sleep = Uniform(0, min(cap, base * 2^attempt))
///
/// This is the "Full Jitter" strategy from the AWS Architecture blog:
/// avoids thundering-herd by randomising the wait uniformly over [0, ceiling].
fn backoff_secs(attempt: u8, base_ms: u64) -> Duration {
    let cap_ms: u64 = 30_000;
    let ceiling = (base_ms * (1u64 << attempt.min(10))).min(cap_ms);
    // rand::random is seeded from OS entropy — safe and lock-free.
    let jitter = rand::random::<u64>() % ceiling.max(1);
    Duration::from_millis(jitter)
}

pub struct AgentRunner {
    pub task: Task,
    pub repo_path: PathBuf,
    pub bus: EventBus<AgentEvent>,
    pub store: Option<MemoryStore>,
    /// When true: generate and print the plan, then exit without touching git.
    pub dry_run: bool,
    /// When true: apply plan steps speculatively as they stream instead of waiting for the full plan.
    pub speculative: bool,
    /// Session context window — tracks phase transitions and token pressure across the agent run.
    pub context: ContextWindow,
    /// Hard upper bound on total attempt iterations before the runner gives up.
    /// Prevents runaway agents from looping indefinitely when `task.max_retries` is very high.
    pub max_turns: u32,
    cancel_rx: Option<oneshot::Receiver<()>>,
    /// Second cancellation mechanism — compatible with `tokio_util::sync::CancellationToken`
    /// for structured cancellation from the pool `JoinSet`.
    cancel_token: CancellationToken,
    attempt_counter: Arc<AtomicUsize>,
    attempts_made: u8,
    turn_count: u32,
}

impl AgentRunner {
    /// Token budget for the context window — 75% of Claude claude-sonnet-4-6's 200K context.
    const CONTEXT_BUDGET: usize = 150_000;

    pub fn new(
        task: Task,
        repo_path: PathBuf,
        bus: EventBus<AgentEvent>,
        store: Option<MemoryStore>,
        cancel_rx: oneshot::Receiver<()>,
        attempt_counter: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            task,
            repo_path,
            bus,
            store,
            dry_run: false,
            speculative: false,
            context: ContextWindow::new(Self::CONTEXT_BUDGET),
            max_turns: 25,
            cancel_rx: Some(cancel_rx),
            cancel_token: CancellationToken::new(),
            attempt_counter,
            attempts_made: 0,
            turn_count: 0,
        }
    }

    /// One-shot constructor — creates a standalone bus for `lopi run`.
    #[must_use]
    pub fn standalone(task: Task, repo_path: PathBuf) -> (Self, EventBus<AgentEvent>) {
        let bus: EventBus<AgentEvent> = EventBus::new(128);
        let (_cancel_tx, cancel_rx) = oneshot::channel();
        let runner = Self {
            bus: bus.clone(),
            task,
            repo_path,
            store: None,
            dry_run: false,
            speculative: false,
            context: ContextWindow::new(Self::CONTEXT_BUDGET),
            max_turns: 25,
            cancel_rx: Some(cancel_rx),
            cancel_token: CancellationToken::new(),
            attempt_counter: Arc::new(AtomicUsize::new(0)),
            attempts_made: 0,
            turn_count: 0,
        };
        (runner, bus)
    }

    /// Return a child token derived from this runner's `CancellationToken`.
    /// The pool can cancel this token to abort the runner from a `JoinSet` teardown.
    #[must_use]
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Return the number of attempts made by this runner.
    #[must_use]
    pub fn attempts_made(&self) -> u8 {
        self.attempts_made
    }

    fn id(&self) -> TaskId {
        self.task.id
    }

    fn log(&self, msg: impl Into<String>) {
        self.bus.send(AgentEvent::info(self.id(), msg));
    }

    fn warn(&self, msg: impl Into<String>) {
        self.bus.send(AgentEvent::warn(self.id(), msg));
    }

    fn status(&self, s: TaskStatus, attempt: u8) {
        // Emit phase-aligned cognition metrics alongside the status change so the
        // Forge UI animates in lockstep with the agent's lifecycle. Activity is a
        // phase heuristic until Sprint G wires real tokens/sec from the SDK.
        let activity = match &s {
            TaskStatus::Planning => 0.45_f32,
            TaskStatus::Implementing => 0.85_f32,
            TaskStatus::Testing => 0.55_f32,
            TaskStatus::Scoring => 0.30_f32,
            TaskStatus::Retrying { .. } => 0.40_f32,
            TaskStatus::Success { .. } | TaskStatus::Failed { .. } | TaskStatus::RolledBack => 0.0_f32,
            TaskStatus::Queued => 0.10_f32,
        };
        self.emit_turn_metrics(activity);
        self.bus.send(AgentEvent::StatusChanged {
            task_id: self.id(),
            status: s,
            attempt,
        });
    }

    /// Emit a TurnMetrics event for the live UI. Pressure comes from the
    /// ContextWindow; activity is supplied by the caller (phase-derived for now).
    /// Cost is 0.0 until Sprint G — the CLI subprocess path doesn't expose
    /// per-turn token accounting.
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

    fn check_cancel(&mut self) -> bool {
        // Check the structured CancellationToken first (pool JoinSet teardown path).
        if self.cancel_token.is_cancelled() {
            self.log("⛔ cancelled via token");
            return true;
        }
        // Then check the legacy oneshot cancel channel (web API / CLI path).
        if let Some(mut rx) = self.cancel_rx.take() {
            match rx.try_recv() {
                Ok(()) | Err(oneshot::error::TryRecvError::Closed) => {
                    self.log("⛔ cancelled by user");
                    return true;
                }
                Err(oneshot::error::TryRecvError::Empty) => {
                    self.cancel_rx = Some(rx);
                }
            }
        }
        false
    }

    /// Pin the task goal as a Boot-phase turn so it's always visible across evictions.
    fn boot_context(&mut self) {
        let content = vec![ContentBlock::Text(format!("Task goal: {}", self.task.goal))];
        let msg = TaggedMessage {
            id: Uuid::new_v4(),
            role: Role::User,
            content,
            tokens: 0,
            pin: PinPolicy::Always,
            phase: Phase::Boot,
            evict_after: None,
            tool_pair_id: None,
            is_conclusion: false,
        };
        self.context.push(msg).ok();
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

        // Seed planning prompt with patterns from memory.
        // Site 2 (TOON biggest win): PatternRow[] is a uniform tabular array.
        // encode_task_context() in claude.rs renders it as TOON §9.3 tabular,
        // saving ~158 tokens per attempt vs JSON (grows linearly with pattern count).
        let extra_constraints = if let Some(store) = &self.store {
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
                let plan = match claude.plan(&self.task).await {
                    Ok(p) => {
                        self.log(format!("✅ plan ready ({} chars)", p.len()));
                        p
                    }
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
            self.log(format!(
                "📊 score: pass={:.0}% lint={} diff={}L",
                score.test_pass_rate * 100.0,
                score.lint_errors,
                score.diff_lines
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

        let status = TaskStatus::Failed {
            reason: "Max retries exceeded".into(),
        };
        self.status(status.clone(), self.task.max_retries);
        Ok(status)
    }
}
