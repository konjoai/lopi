use anyhow::Result;
use lopi_context::{ContentBlock, ContextWindow, Phase, PinPolicy, Role, TaggedMessage};
use lopi_core::{AgentEvent, Attempt, EventBus, Task, TaskId, TaskStatus};
use lopi_git::GitManager;
use lopi_memory::MemoryStore;
use crate::claude::ClaudeCode;
use crate::scorer::Scorer;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::oneshot;
use uuid::Uuid;

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
    cancel_rx: Option<oneshot::Receiver<()>>,
    attempt_counter: Arc<AtomicUsize>,
    attempts_made: u8,
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
            cancel_rx: Some(cancel_rx),
            attempt_counter,
            attempts_made: 0,
        }
    }

    /// One-shot constructor — creates a standalone bus for `lopi run`.
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
            cancel_rx: Some(cancel_rx),
            attempt_counter: Arc::new(AtomicUsize::new(0)),
            attempts_made: 0,
        };
        (runner, bus)
    }

    pub fn attempts_made(&self) -> u8 {
        self.attempts_made
    }

    fn id(&self) -> TaskId { self.task.id }

    fn log(&self, msg: impl Into<String>) {
        self.bus.send(AgentEvent::info(self.id(), msg));
    }

    fn warn(&self, msg: impl Into<String>) {
        self.bus.send(AgentEvent::warn(self.id(), msg));
    }

    fn status(&self, s: TaskStatus, attempt: u8) {
        self.bus.send(AgentEvent::StatusChanged {
            task_id: self.id(),
            status: s,
            attempt,
        });
    }

    fn check_cancel(&mut self) -> bool {
        if let Some(mut rx) = self.cancel_rx.take() {
            match rx.try_recv() {
                Ok(_) | Err(oneshot::error::TryRecvError::Closed) => {
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
                    self.log(format!("🧠 seeding from {} similar past patterns", patterns.len()));
                    // Collect (keywords, constraints) pairs for the TOON tabular encoder.
                    // These are passed to encode_task_context() inside ClaudeCode::plan().
                    patterns.iter().take(5)
                        .filter_map(|p| {
                            let c = p.successful_constraints.as_deref().unwrap_or("").to_string();
                            if c.is_empty() { None } else { Some(c) }
                        })
                        .collect()
                }
                _ => vec![],
            }
        } else {
            vec![]
        };

        let claude = ClaudeCode::new(&self.repo_path).with_extra_constraints(extra_constraints);
        let scorer = Scorer::new(&self.repo_path);

        for attempt in 0..self.task.max_retries {
            if self.check_cancel() {
                return Ok(TaskStatus::Failed { reason: "Cancelled".into() });
            }

            self.attempts_made = attempt + 1;
            self.attempt_counter.store(attempt as usize + 1, Ordering::Relaxed);

            let branch = format!("lopi/{}-attempt-{}", self.task.id.0, attempt + 1);
            self.bus.send(AgentEvent::TaskStarted {
                task_id: self.id(),
                attempt: attempt + 1,
                branch: branch.clone(),
            });
            self.log(format!("🔀 branch: {branch}"));

            if let Err(e) = git.checkout_new_branch(&branch).await {
                self.warn(format!("checkout failed: {e}"));
                self.status(TaskStatus::Retrying { attempt: attempt + 1 }, attempt + 1);
                continue;
            }

            self.status(TaskStatus::Planning, attempt + 1);
            self.context.transition_phase(Phase::Planning);
            tracing::info!(pressure = self.context.token_pressure(), "context at planning");
            self.log("📋 planning…");

            if self.speculative {
                // Speculative mode: apply plan steps as they stream from claude.
                let (plan_handle, mut step_rx) = claude.plan_streaming(&self.task);
                self.status(TaskStatus::Implementing, attempt + 1);
                self.context.transition_phase(Phase::Implementation);
                tracing::info!(pressure = self.context.token_pressure(), "context at speculative implementation");
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
                        self.status(TaskStatus::Retrying { attempt: attempt + 1 }, attempt + 1);
                        continue;
                    }
                    Err(e) => {
                        let e = anyhow::anyhow!("{e}");
                        self.warn(format!("plan stream failed: {e}"));
                        git.hard_rollback().await.ok();
                        git.checkout_default().await.ok();
                        self.status(TaskStatus::Retrying { attempt: attempt + 1 }, attempt + 1);
                        continue;
                    }
                };
                self.log(format!("✅ speculative plan+implement done ({} chars)", plan.len()));
            } else {
                // Standard mode: wait for full plan, then implement in one pass.
                let plan = match claude.plan(&self.task).await {
                    Ok(p) => { self.log(format!("✅ plan ready ({} chars)", p.len())); p }
                    Err(e) => {
                        self.warn(format!("plan failed: {e}"));
                        git.hard_rollback().await.ok();
                        git.checkout_default().await.ok();
                        self.status(TaskStatus::Retrying { attempt: attempt + 1 }, attempt + 1);
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
                    return Ok(TaskStatus::Failed { reason: "dry-run complete".into() });
                }

                if self.check_cancel() {
                    git.hard_rollback().await.ok();
                    git.checkout_default().await.ok();
                    return Ok(TaskStatus::Failed { reason: "Cancelled".into() });
                }

                self.status(TaskStatus::Implementing, attempt + 1);
                self.context.transition_phase(Phase::Implementation);
                tracing::info!(pressure = self.context.token_pressure(), "context at implementation");
                self.log("🔨 implementing…");

                if let Err(e) = claude.implement(&self.task, &plan).await {
                    self.warn(format!("implement failed: {e}"));
                    git.hard_rollback().await.ok();
                    git.checkout_default().await.ok();
                    self.status(TaskStatus::Retrying { attempt: attempt + 1 }, attempt + 1);
                    continue;
                }
            }

            if let Err(e) = git.check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs).await {
                self.warn(format!("diff scope violation: {e}"));
                self.status(TaskStatus::RolledBack, attempt + 1);
                git.hard_rollback().await.ok();
                git.checkout_default().await.ok();
                continue;
            }

            self.status(TaskStatus::Testing, attempt + 1);
            self.context.transition_phase(Phase::Testing);
            tracing::info!(pressure = self.context.token_pressure(), "context at testing");
            self.log("🧪 running tests…");
            let score = scorer.score().await?;

            self.bus.send(AgentEvent::ScoreUpdated {
                task_id: self.id(),
                test_pass_rate: score.test_pass_rate,
                lint_errors: score.lint_errors,
                diff_lines: score.diff_lines,
            });
            self.log(format!("📊 score: pass={:.0}% lint={} diff={}L",
                score.test_pass_rate * 100.0, score.lint_errors, score.diff_lines));

            // Persist attempt.
            if let Some(store) = &self.store {
                let mut a = Attempt::new(self.id(), attempt + 1, &branch);
                a.score = Some(score.clone());
                a.outcome = if score.passed() { "success".into() } else { "retry".into() };
                store.save_attempt(&a).await.ok();
            }

            if score.passed() {
                self.log("✅ tests pass — committing…");
                self.context.pin_conclusion(
                    format!("Sprint succeeded — pass={:.0}% diff={}L", score.test_pass_rate * 100.0, score.diff_lines),
                    Phase::Conclusion,
                );
                tracing::info!(pressure = self.context.token_pressure(), "context at conclusion");
                git.commit_all(&format!("lopi: {}", self.task.goal)).await.ok();
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

            if git.check_diff_scope(&self.task.allowed_dirs, &self.task.forbidden_dirs).await.is_ok() {
                self.status(TaskStatus::Testing, attempt + 1);
                let score2 = scorer.score().await?;
                self.bus.send(AgentEvent::ScoreUpdated {
                    task_id: self.id(),
                    test_pass_rate: score2.test_pass_rate,
                    lint_errors: score2.lint_errors,
                    diff_lines: score2.diff_lines,
                });
                if score2.passed() {
                    self.log("✅ fix worked — committing…");
                    git.commit_all(&format!("lopi: {}", self.task.goal)).await.ok();
                    let pr_url = git.open_pr(&branch, &self.task.goal).await.ok();
                    let status = TaskStatus::Success { branch, pr_url };
                    self.status(status.clone(), attempt + 1);
                    return Ok(status);
                }
            }

            git.hard_rollback().await.ok();
            git.checkout_default().await.ok();
            self.status(TaskStatus::Retrying { attempt: attempt + 1 }, attempt + 1);
            self.log(format!("♻️ retry {}/{}", attempt + 1, self.task.max_retries));
        }

        let status = TaskStatus::Failed { reason: "Max retries exceeded".into() };
        self.status(status.clone(), self.task.max_retries);
        Ok(status)
    }
}
