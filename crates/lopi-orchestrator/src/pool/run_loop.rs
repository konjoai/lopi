//! The dispatch loop and per-task execution: `submit`, `run`, and the
//! helpers that drive a single agent attempt to completion.

use super::types::AgentHandle;
use super::worktree::{cleanup_worktree, setup_worktree};
use super::AgentPool;
use anyhow::Result;
use lopi_agent::AgentRunner;
use lopi_core::topology::TopologyHint;
use lopi_core::{AgentEvent, EventBus, ScoreWeights, Task, TaskId, TaskSource, TaskStatus};
use lopi_memory::{AuditInput, DeadLetterInput, MemoryStore};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{error, info, warn};

/// The task's effective topology: its explicit hint, or the heuristic
/// classification of its goal when unset (Sprint T). Pure — no side effects.
#[must_use]
pub fn effective_topology(task: &Task) -> TopologyHint {
    task.topology
        .unwrap_or_else(|| crate::topology::classify(&task.goal).hint)
}

impl AgentPool {
    /// Enqueue a task and broadcast `TaskQueued`.
    ///
    /// Sprint T — when the task carries no explicit topology, the keyword
    /// classifier fills one in so downstream consumers (and the dashboard)
    /// see a concrete hint. The classification is advisory and never blocks.
    pub async fn submit(&self, mut task: Task) -> Option<TaskId> {
        if task.topology.is_none() {
            let verdict = crate::topology::classify(&task.goal);
            info!(
                task_id = %task.id,
                topology = %verdict.hint,
                confidence = verdict.confidence,
                "classified task topology"
            );
            task.topology = Some(verdict.hint);
        }
        self.bus.send(AgentEvent::TaskQueued {
            task_id: task.id,
            goal: task.goal.clone(),
            priority: task.priority,
        });
        if let Some(store) = &self.store {
            store.save_task(&task, "queued").await.ok();
            // P2 — audit every dispatch so the operator can trace task
            // flow without recomputing from PoolStats deltas.
            let actor = task_source_label(&task);
            // Hand-build the payload — lopi-orchestrator already pulls in
            // chrono + thiserror via lopi-core, but not serde_json, so
            // staying string-only keeps the dep graph thin. The shape is
            // fixed enough that escape risk is bounded.
            let caps = task
                .required_capabilities
                .iter()
                .map(|c| format!("\"{}\"", c.replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(",");
            let payload = format!(
                "{{\"priority\":\"{:?}\",\"required_capabilities\":[{caps}]}}",
                task.priority,
            );
            let _ = store
                .record_audit(
                    &AuditInput::new("task.dispatch")
                        .subject("task", task.id.0.to_string())
                        .actor(actor)
                        .payload_json(payload),
                )
                .await;
        }
        self.queue.push(task).await
    }

    /// Dispatch loop — pops tasks from the queue and spawns bounded workers.
    ///
    /// # Errors
    ///
    /// Returns an error if a semaphore is closed (only happens on shutdown).
    pub async fn run(self) -> Result<()> {
        let bus_stats = self.bus.clone();
        let counters_stats = self.counters.clone();
        let queue_stats = self.queue.clone();
        let started_at = self.started_at.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                let running = counters_stats.running.load(Ordering::Relaxed);
                let queued = queue_stats.len();
                let succeeded = counters_stats.succeeded.load(Ordering::Relaxed);
                let failed = counters_stats.failed.load(Ordering::Relaxed);
                let uptime_secs = started_at.elapsed().as_secs();
                bus_stats.send(AgentEvent::PoolStats {
                    running,
                    queued,
                    succeeded,
                    failed,
                    uptime_secs,
                });
            }
        });

        loop {
            let task = self.queue.pop().await;

            // Resolve the repo for this task (task-level override or pool default).
            let repo = task
                .repo_path
                .clone()
                .unwrap_or_else(|| self.repo_path.clone());

            // Acquire global concurrency permit.
            let permit = self.permits.clone().acquire_owned().await?;

            // Acquire per-repo permit — caps concurrency on any single repo to max_agents.
            let repo_sem = self
                .repo_permits
                .entry(repo.clone())
                .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(self.max_agents)))
                .clone();
            let repo_permit = repo_sem.acquire_owned().await?;

            let task_id = task.id;
            let goal = task.goal.clone();
            // Snapshot the few fields the DLQ writer needs before `task`
            // moves into the spawned future. The goal/repo/source are
            // cheap String + Option clones; everything else is by value.
            let dlq_goal = task.goal.clone();
            let dlq_repo = task.repo_path.clone();
            let dlq_source = task_source_label(&task);

            let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
            // Phase 11 — plan-approval decision channel. Always created; the
            // runner only awaits it when the task is gated, so an ungated run
            // simply drops the receiver.
            let (plan_decision_tx, plan_decision_rx) =
                oneshot::channel::<lopi_core::PlanDecision>();
            let attempt = Arc::new(AtomicUsize::new(0));

            let handle = Arc::new(tokio::sync::RwLock::new(AgentHandle {
                goal: goal.clone(),
                cancel_tx: Some(cancel_tx),
                plan_decision_tx: Some(plan_decision_tx),
                attempt: attempt.clone(),
                started_at: std::time::Instant::now(),
            }));
            self.handles.insert(task_id, handle);
            self.counters.running.fetch_add(1, Ordering::Relaxed);

            let bus = self.bus.clone();
            let store = self.store.clone();
            let handles = self.handles.clone();
            let counters = self.counters.clone();
            let join_set = self.join_set.clone();

            let mut js = join_set.lock().await;
            // Drain any completed tasks to keep the JoinSet from growing unboundedly.
            while js.try_join_next().is_some() {}
            js.spawn(async move {
                let _permit = permit;
                let _repo_permit = repo_permit;
                let max_retries = attempt.load(Ordering::Relaxed); // 0 here, updated in runner
                let outcome = run_one(
                    task,
                    repo,
                    bus.clone(),
                    store.clone(),
                    cancel_rx,
                    plan_decision_rx,
                    attempt.clone(),
                )
                .await;
                handles.remove(&task_id);
                counters.running.fetch_sub(1, Ordering::Relaxed);
                let _ = max_retries; // hint for future per-task cap reporting

                // Was this a terminal failure (vs. a runner error or a
                // success)? If so, capture enough state to push to the DLQ.
                let dlq_payload: Option<(u8, String)> = match &outcome {
                    Ok(TaskStatus::Success { .. }) => {
                        counters.succeeded.fetch_add(1, Ordering::Relaxed);
                        None
                    }
                    Ok(TaskStatus::Failed { reason }) => {
                        counters.failed.fetch_add(1, Ordering::Relaxed);
                        Some((
                            u8::try_from(attempt.load(Ordering::Relaxed)).unwrap_or(u8::MAX),
                            reason.clone(),
                        ))
                    }
                    Ok(TaskStatus::RolledBack) => {
                        counters.failed.fetch_add(1, Ordering::Relaxed);
                        Some((
                            u8::try_from(attempt.load(Ordering::Relaxed)).unwrap_or(u8::MAX),
                            "rolled back".into(),
                        ))
                    }
                    // A rebase conflict isn't a code failure — main simply moved
                    // under the task. Count it and DLQ it so it can be retried
                    // fresh on the new base.
                    Ok(TaskStatus::Conflict { paths }) => {
                        counters.failed.fetch_add(1, Ordering::Relaxed);
                        Some((
                            u8::try_from(attempt.load(Ordering::Relaxed)).unwrap_or(u8::MAX),
                            format!("rebase conflict: {}", paths.join(", ")),
                        ))
                    }
                    Err(_) => {
                        counters.failed.fetch_add(1, Ordering::Relaxed);
                        None // handled by the explicit error branch below
                    }
                    _ => None,
                };

                if let Err(e) = &outcome {
                    error!(task_id = %task_id, "agent run error: {e}");
                    let reason = format!("{e}");
                    bus.send(AgentEvent::TaskCompleted {
                        task_id,
                        outcome: TaskStatus::Failed {
                            reason: reason.clone(),
                        },
                        total_attempts: 1,
                    });
                    if let Some(store) = &store {
                        let _ = store.mark_completed(&task_id, "failed").await;
                        push_dlq(
                            store,
                            task_id,
                            &dlq_goal,
                            dlq_repo.as_deref(),
                            1,
                            Some(reason),
                            &dlq_source,
                        )
                        .await;
                    }
                } else if let (Some((attempts, reason)), Some(store)) = (dlq_payload, &store) {
                    push_dlq(
                        store,
                        task_id,
                        &dlq_goal,
                        dlq_repo.as_deref(),
                        attempts,
                        Some(reason),
                        &dlq_source,
                    )
                    .await;
                }
            });
        }
    }
}

/// Stable wire label for `TaskSource` — used in audit log + DLQ `source`
/// column so dashboards can group by origin without re-parsing.
fn task_source_label(task: &Task) -> String {
    match &task.source {
        TaskSource::Cli => "cli".into(),
        TaskSource::Api => "api".into(),
        TaskSource::Telegram { .. } => "telegram".into(),
        TaskSource::Webhook { .. } => "webhook".into(),
        TaskSource::SelfModify { .. } => "self-modify".into(),
    }
}

/// Best-effort DLQ write — logs but does not propagate failure. The
/// agent loop is already in its terminal state; failing to record the
/// DLQ row must not panic the worker.
async fn push_dlq(
    store: &MemoryStore,
    task_id: TaskId,
    goal: &str,
    repo_path: Option<&std::path::Path>,
    total_attempts: u8,
    last_error: Option<String>,
    source: &str,
) {
    let mut input = DeadLetterInput::new(task_id, goal);
    input.repo_path = repo_path.map(|p| p.display().to_string());
    input.total_attempts = total_attempts;
    input.last_error = last_error;
    input.source = source.to_string();
    if let Err(e) = store.push_dead_letter(&input).await {
        warn!(task_id = %task_id, "dead-letter write failed: {e}");
        return;
    }
    let payload = format!(
        "{{\"total_attempts\":{},\"source\":\"{}\"}}",
        total_attempts, source
    );
    let _ = store
        .record_audit(
            &AuditInput::new("task.dead_letter")
                .subject("task", task_id.0.to_string())
                .actor("pool")
                .payload_json(payload),
        )
        .await;
}

/// Phase 5b / Sprint N — Compute score weights from user-annotated pattern history.
///
/// Approved patterns (human marked "approved") that required fewer attempts signal
/// that the current quality bar is right or too loose → tighten penalties slightly.
/// Rejected patterns that required many attempts → loosen penalties.
/// Falls back to defaults when no annotations exist or the store is absent.
async fn compute_weight_adjustments(_goal: &str, store: Option<&MemoryStore>) -> ScoreWeights {
    let Some(store) = store else {
        return ScoreWeights::default();
    };
    match store.compute_weight_adjustments().await {
        Ok(weights) => weights,
        Err(e) => {
            tracing::warn!("weight calibration query failed ({e}); using defaults");
            ScoreWeights::default()
        }
    }
}

/// Build the configured [`AgentRunner`] for one task's attempt-loop.
///
/// Pure assembly of an already-resolved builder chain — no I/O happens here,
/// so the Verifier-as-Explicit-Gate wiring can be proven at this seam without
/// exercising a real agent run: `.with_verifier()` is called when the task
/// carries `verifier_required` or an explicit `verifier_model`, the first
/// real call site this path has ever had.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_runner(
    task: Task,
    work_repo: PathBuf,
    bus: EventBus<AgentEvent>,
    store: Option<MemoryStore>,
    cancel_rx: oneshot::Receiver<()>,
    attempt_counter: Arc<AtomicUsize>,
    weights: ScoreWeights,
    self_prompt: lopi_core::SelfPromptStrategy,
    escalate: bool,
    skills: lopi_skill::SkillRegistry,
    budget_tokens: u64,
    repo_max_iterations: u8,
    repo_guardrails: RepoGuardrails,
    reflect_cross_run: bool,
    plan_decision_rx: oneshot::Receiver<lopi_core::PlanDecision>,
) -> AgentRunner {
    let verifier_needed = task.verifier_required || task.verifier_model.is_some();
    // Loop-as-code: a task-level override always wins over the repo's
    // `.lopi/loop.toml` ceiling when set — mirrors verifier_model's "explicit
    // wins over default" precedent, falling back to the repo config.
    let max_turns = u32::from(task.max_iterations.unwrap_or(repo_max_iterations));
    // Guardrails — same "explicit task override wins over repo default"
    // precedent as `max_turns` above.
    let gate = task.gate.clone().or(repo_guardrails.gate);
    let until = task.until.clone().or(repo_guardrails.until);
    let on_fail = task.on_fail.unwrap_or(repo_guardrails.on_fail);
    // Progress-Gating (A3) — the repo budget seeds `task_budget`; a positive
    // per-task `budget_tokens` overrides it as the loop's hard cap in the runner
    // (`AgentRunner::effective_budget_tokens`), so no extra folding is needed.
    let mut runner = AgentRunner::new(task, work_repo, bus, store, cancel_rx, attempt_counter)
        .with_score_weights(weights)
        .with_self_prompt(self_prompt)
        .with_strategy_escalation(escalate)
        .with_skills(skills)
        .with_task_budget(budget_tokens)
        .with_cross_run_reflection(reflect_cross_run)
        .with_plan_gate(plan_decision_rx);
    runner.max_turns = max_turns;
    runner.gate = gate;
    runner.until = until;
    runner.on_fail = on_fail;
    if verifier_needed {
        runner.with_verifier()
    } else {
        runner
    }
}

/// The repo-level (`.lopi/loop.toml`) guardrail defaults a task's own
/// `gate`/`until`/`on_fail` may override. Bundled into one struct — rather
/// than three more positional args on [`build_runner`] — since they're
/// always loaded and passed together.
#[derive(Debug, Clone, Default)]
pub(super) struct RepoGuardrails {
    pub(super) gate: Option<String>,
    pub(super) until: Option<String>,
    pub(super) on_fail: lopi_core::loop_config::OnFail,
}

#[tracing::instrument(skip(bus, store, cancel_rx, plan_decision_rx, attempt_counter), fields(task_id = %task.id, goal = %task.goal))]
async fn run_one(
    task: Task,
    repo: PathBuf,
    bus: EventBus<AgentEvent>,
    store: Option<MemoryStore>,
    cancel_rx: oneshot::Receiver<()>,
    plan_decision_rx: oneshot::Receiver<lopi_core::PlanDecision>,
    attempt_counter: Arc<AtomicUsize>,
) -> Result<TaskStatus> {
    info!(task_id = %task.id, "starting agent");
    let task_id = task.id;
    let goal = task.goal.clone();

    // Transition the durable row out of "queued" the moment execution begins,
    // so `GET /api/tasks/:id` and the WebSocket snapshot reflect the running
    // state promptly rather than lagging at "queued" until the terminal write
    // (Ops-2 bug #4). Fine-grained phases still ride the live event stream;
    // this is the single persisted "in flight" marker a fresh page load reads.
    if let Some(store) = &store {
        if let Err(e) = store.mark_running(&task_id).await {
            warn!(task_id = %task_id, "mark_running failed: {e}");
        }
    }

    let weights = compute_weight_adjustments(&goal, store.as_ref()).await;
    // Loop-as-code: read the repo's whole `.lopi/loop.toml` (self-prompting,
    // isolation, skills, budget, max-iterations, guardrails) off the reactor
    // in one blocking load. A missing/malformed config yields the
    // conservative `LoopConfig::default()` (Direct self-prompt,
    // shared-checkout Branch isolation, no skills, inherited budget, the
    // default iteration ceiling, no gate/until, `on_fail: Stop`).
    let (cfg, skills) = {
        let repo = repo.clone();
        tokio::task::spawn_blocking(move || {
            let cfg = lopi_core::LoopConfig::load_from_repo(&repo).unwrap_or_default();
            let skills = super::skills::load_skills(&repo);
            (cfg, skills)
        })
        .await
        .unwrap_or_else(|e| {
            // The blocking task itself panicked/was cancelled — not a config
            // parse failure (that's already handled by `unwrap_or_default()`
            // above). Fall back to `LoopConfig::default()` explicitly rather
            // than assuming any particular field's zero value, since `0` is
            // the infinite-loop sentinel for `max_iterations` but not a safe
            // default on its own.
            tracing::warn!(task_id = %task_id, "loop-config blocking load task failed: {e}; using LoopConfig defaults");
            (
                lopi_core::LoopConfig::default(),
                lopi_skill::SkillRegistry::default(),
            )
        })
    };
    let isolation = cfg.isolation;
    let repo_guardrails = RepoGuardrails {
        gate: cfg.gate.clone(),
        until: cfg.until.clone(),
        on_fail: cfg.on_fail,
    };

    // Worktree isolation (Pentad M1.2): when enabled, give this task its own
    // physical checkout so its build/test/branch ops cannot collide with peers
    // on the same repo. The runner then operates entirely inside the worktree —
    // its own `target/` follows the cwd, so parallel cargo runs never contend.
    // On any setup failure we log and fall back to the shared repo (a missing
    // worktree must never stall a task). The handle's RAII drop reaps the
    // checkout even if the runner panics; we also clean up explicitly below.
    let worktree = setup_worktree(&repo, isolation.is_worktree(), &task_id).await;
    let work_repo = worktree
        .as_ref()
        .map_or_else(|| repo.clone(), |w| w.path().to_path_buf());

    let mut runner = build_runner(
        task,
        work_repo,
        bus.clone(),
        store.clone(),
        cancel_rx,
        attempt_counter,
        weights,
        cfg.self_prompt,
        cfg.escalate_strategy,
        skills,
        cfg.budget_tokens,
        cfg.max_iterations,
        repo_guardrails,
        cfg.reflect_cross_run,
        plan_decision_rx,
    );
    let outcome = runner.run().await?;
    // Reap the throwaway worktree now the run is done. The RAII drop is the
    // panic / early-return safety net; this is the clean, observable path.
    cleanup_worktree(worktree).await;

    let total_attempts = runner.attempts_made();

    bus.send(AgentEvent::TaskCompleted {
        task_id,
        outcome: outcome.clone(),
        total_attempts,
    });

    if let Some(store) = store {
        // Canonical status token — one vocabulary shared with the API and the
        // web snapshot bucketing. `db_status` covers every variant, so there's
        // no `"unknown"` fallthrough to mis-bucket.
        store
            .mark_completed(&task_id, outcome.db_status())
            .await
            .ok();
        if let Err(e) = store.mine_patterns(&task_id, &goal).await {
            warn!("pattern mining failed: {e}");
        }
    }

    Ok(outcome)
}
