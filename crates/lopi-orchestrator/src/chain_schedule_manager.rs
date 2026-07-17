//! Stack-Chain-1 — server-side whole-stack cron scheduling.
//!
//! [`crate::schedule_manager::ScheduleManager`] fires exactly one goal per
//! cron tick. A stack is an ORDERED SEQUENCE of independent goals — each its
//! own full agent run, not a pipeline stage of one run (that's what
//! `lopi-agent`'s `AgentDag` already models; it doesn't fit here). This
//! module fires step 0 on the cron tick, subscribes to the event bus for
//! that task's terminal status, and on completion submits the next step —
//! entirely server-side, so the chain keeps advancing with no browser tab
//! open (unlike today's client-side `stackRun.ts::advance()`).
//!
//! Restart safety: `AgentPool`'s `TaskQueue` is purely in-memory (see
//! `crate::queue`) — nothing survives a process restart, so a task that was
//! mid-flight when the process died will never emit the `TaskCompleted` this
//! manager is waiting for. [`ChainScheduleManager::resume_orphaned`] scans
//! `schedule_chain_runs` rows still marked `running` on boot, checks the
//! in-flight step's task against its *last known* durable status, and either
//! advances (if it actually finished before the restart) or resubmits the
//! same step (if it was orphaned) — never step 0, never silently dropped.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use dashmap::DashMap;
use lopi_core::{AgentEvent, AutonomyLevel, TaskId, TaskStatus};
use lopi_memory::{ChainRunRow, MemoryStore, ScheduleChainRow};
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, warn};
use uuid::Uuid;

use crate::pool::AgentPool;
use crate::task_build::build_task_from_fields;

/// On-fail policy for a chain step, mirroring the client's `OnFail` union
/// (`web/src/lib/stores/stack.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnFail {
    /// Stop the chain; the run ends `failed`.
    Stop,
    /// Submit the next step anyway.
    Continue,
    /// Submit the next step after a short backoff delay.
    Backoff,
}

impl OnFail {
    fn parse(s: &str) -> Self {
        match s {
            "continue" => Self::Continue,
            "backoff" => Self::Backoff,
            _ => Self::Stop,
        }
    }
}

/// One ordered step, decoupled from the storage row shape.
#[derive(Debug, Clone)]
struct StepSpec {
    goal: String,
    allowed_dirs: Vec<String>,
    forbidden_dirs: Vec<String>,
}

/// The subset of a chain needed to drive execution.
#[derive(Debug, Clone)]
pub struct ChainSpec {
    id: String,
    cron: String,
    repo: Option<PathBuf>,
    priority: String,
    autonomy_level: AutonomyLevel,
    on_fail: OnFail,
    steps: Vec<StepSpec>,
}

impl From<ScheduleChainRow> for ChainSpec {
    fn from(r: ScheduleChainRow) -> Self {
        Self {
            id: r.id,
            cron: r.cron,
            repo: r.repo.map(PathBuf::from),
            priority: r.priority,
            autonomy_level: AutonomyLevel::parse(&r.autonomy_level).unwrap_or_default(),
            on_fail: OnFail::parse(&r.on_fail),
            steps: r
                .steps
                .into_iter()
                .map(|s| StepSpec {
                    goal: s.goal,
                    allowed_dirs: s.allowed_dirs,
                    forbidden_dirs: s.forbidden_dirs,
                })
                .collect(),
        }
    }
}

/// Live, mutable chain-cron scheduler. Cheap to `clone()` — all state is
/// behind `Arc`.
#[derive(Clone)]
pub struct ChainScheduleManager {
    inner: Arc<Inner>,
}

struct Inner {
    sched: Mutex<Option<JobScheduler>>,
    /// chain_id -> registered job uuid.
    jobs: DashMap<String, Uuid>,
    pool: AgentPool,
    store: MemoryStore,
}

impl ChainScheduleManager {
    /// Construct an un-started manager.
    #[must_use]
    pub fn new(pool: AgentPool, store: MemoryStore) -> Self {
        Self {
            inner: Arc::new(Inner {
                sched: Mutex::new(None),
                jobs: DashMap::new(),
                pool,
                store,
            }),
        }
    }

    /// Create + start the `JobScheduler`, register every enabled chain, and
    /// resume any run orphaned by a prior restart. Idempotent.
    ///
    /// # Errors
    /// Returns `Err` if the scheduler cannot be created or started.
    pub async fn start(&self) -> Result<()> {
        let mut guard = self.inner.sched.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        let sched = JobScheduler::new()
            .await
            .context("creating chain job scheduler")?;
        sched
            .start()
            .await
            .context("starting chain job scheduler")?;
        *guard = Some(sched);
        drop(guard);

        match self.inner.store.list_schedule_chains().await {
            Ok(rows) => {
                for row in rows.into_iter().filter(|r| r.enabled) {
                    if let Err(e) = self.register(row.into()).await {
                        warn!("failed to register schedule chain on boot: {e:#}");
                    }
                }
            }
            Err(e) => warn!("could not load schedule chains on boot: {e:#}"),
        }
        self.resume_orphaned().await;
        Ok(())
    }

    /// Register (or replace) a live cron job for `chain`.
    /// Returns `Ok(false)` (without error) when the cron expression is invalid.
    ///
    /// # Errors
    /// Returns `Err` if the scheduler is not started or job registration fails.
    pub async fn register(&self, chain: ChainSpec) -> Result<bool> {
        self.unregister(&chain.id).await?;
        let manager = self.clone();
        let chain_for_job = chain.clone();

        let six_field = format!("0 {}", chain.cron);
        let job = match Job::new_async(six_field.as_str(), move |_uuid, _lock| {
            let manager = manager.clone();
            let chain = chain_for_job.clone();
            Box::pin(async move {
                manager.fire(&chain).await;
            })
        }) {
            Ok(j) => j,
            Err(e) => {
                warn!(chain = %chain.id, "invalid cron: {e}");
                return Ok(false);
            }
        };

        let guard = self.inner.sched.lock().await;
        let sched = guard.as_ref().context("chain scheduler not started")?;
        let uuid = sched.add(job).await.context("adding chain cron job")?;
        self.inner.jobs.insert(chain.id.clone(), uuid);
        info!(chain = %chain.id, "registered schedule chain");
        Ok(true)
    }

    /// Remove a chain's live job, if registered. Safe to call for an unknown id.
    ///
    /// # Errors
    /// Returns `Err` if the scheduler rejects the removal.
    pub async fn unregister(&self, chain_id: &str) -> Result<()> {
        let Some((_, uuid)) = self.inner.jobs.remove(chain_id) else {
            return Ok(());
        };
        let guard = self.inner.sched.lock().await;
        if let Some(sched) = guard.as_ref() {
            sched
                .remove(&uuid)
                .await
                .context("removing chain cron job")?;
            info!(chain = %chain_id, "unregistered schedule chain");
        }
        Ok(())
    }

    /// Fire a chain immediately, bypassing its cron timing. Used by the
    /// dashboard "run now" button. Returns the new run's id, if the chain
    /// has at least one step.
    ///
    /// # Errors
    /// Returns `Err` only if starting the run row fails irrecoverably.
    pub async fn run_now(&self, chain: ChainSpec) -> Result<Option<String>> {
        if chain.steps.is_empty() {
            return Ok(None);
        }
        let run = self.inner.store.start_chain_run(&chain.id).await?;
        let run_id = run.id.clone();
        self.submit_step(&chain, &run, 0).await;
        Ok(Some(run_id))
    }

    /// Cron-tick entrypoint: start a fresh run at step 0.
    async fn fire(&self, chain: &ChainSpec) {
        if chain.steps.is_empty() {
            warn!(chain = %chain.id, "cron fired but chain has no steps");
            return;
        }
        let run = match self.inner.store.start_chain_run(&chain.id).await {
            Ok(r) => r,
            Err(e) => {
                warn!(chain = %chain.id, "failed to start chain run: {e:#}");
                return;
            }
        };
        self.submit_step(chain, &run, 0).await;
    }

    /// Submit `step_order`'s task, record it on the run row, and spawn a
    /// listener that advances the chain when that task reaches a terminal
    /// state.
    async fn submit_step(&self, chain: &ChainSpec, run: &ChainRunRow, step_order: usize) {
        let Some(step) = chain.steps.get(step_order) else {
            self.finish_run_completed(chain, run).await;
            return;
        };
        let task = build_task_from_fields(
            &step.goal,
            chain.repo.as_deref(),
            &chain.priority,
            &step.allowed_dirs,
            &step.forbidden_dirs,
            chain.autonomy_level,
        );
        let task_id = task.id;
        self.record_step_advance(chain, run, step_order, task_id)
            .await;
        // Duplicate goals within a running repo are deduplicated by the pool
        // — `submit` returns the existing id in that case, and we still
        // listen on it, so the chain advances off the real in-flight task.
        let effective_id = self.inner.pool.submit(task).await.unwrap_or(task_id);
        info!(chain = %chain.id, run = %run.id, step = step_order, task = %effective_id.0, "submitted chain step");
        self.spawn_listener(chain.clone(), run.id.clone(), step_order, effective_id);
    }

    /// Mark a run `completed` — reached when `submit_step` is called past the
    /// last step.
    async fn finish_run_completed(&self, chain: &ChainSpec, run: &ChainRunRow) {
        let _ = self
            .inner
            .store
            .finish_chain_run(&run.id, "completed")
            .await;
        info!(chain = %chain.id, run = %run.id, "chain run completed");
    }

    /// Persist which task is now driving `step_order`, best-effort (a failure
    /// here means resume-on-restart may resubmit a step that already has a
    /// task in flight — logged, not fatal).
    async fn record_step_advance(
        &self,
        chain: &ChainSpec,
        run: &ChainRunRow,
        step_order: usize,
        task_id: TaskId,
    ) {
        if let Err(e) = self
            .inner
            .store
            .advance_chain_run(
                &run.id,
                i64::try_from(step_order).unwrap_or(i64::MAX),
                &task_id.0.to_string(),
            )
            .await
        {
            warn!(chain = %chain.id, run = %run.id, "failed to record chain step advance: {e:#}");
        }
    }

    /// Spawn a task that waits for `task_id`'s terminal `AgentEvent` and
    /// advances the chain (or stops it, per `on_fail`) when it arrives.
    fn spawn_listener(&self, chain: ChainSpec, run_id: String, step_order: usize, task_id: TaskId) {
        let manager = self.clone();
        let mut rx = self.inner.pool.bus().subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(AgentEvent::TaskCompleted {
                        task_id: completed_id,
                        outcome,
                        ..
                    }) if completed_id == task_id => {
                        manager
                            .on_step_terminal(&chain, &run_id, step_order, &outcome)
                            .await;
                        return;
                    }
                    Ok(AgentEvent::TaskCancelled {
                        task_id: cancelled_id,
                    }) if cancelled_id == task_id => {
                        manager
                            .on_step_terminal(
                                &chain,
                                &run_id,
                                step_order,
                                &TaskStatus::Failed {
                                    reason: "cancelled".into(),
                                },
                            )
                            .await;
                        return;
                    }
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        });
    }

    /// A step reached a terminal state: advance to the next step, or stop
    /// the run, per `on_fail`.
    async fn on_step_terminal(
        &self,
        chain: &ChainSpec,
        run_id: &str,
        step_order: usize,
        outcome: &TaskStatus,
    ) {
        let succeeded = matches!(outcome, TaskStatus::Success { .. });
        if !succeeded {
            match chain.on_fail {
                OnFail::Stop => {
                    self.stop_run_failed(chain, run_id, step_order).await;
                    return;
                }
                OnFail::Backoff => {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                }
                OnFail::Continue => {}
            }
        }
        self.advance_to_next_step(chain, run_id, step_order).await;
    }

    /// Mark a run `failed` — reached when a step fails and `on_fail == Stop`.
    async fn stop_run_failed(&self, chain: &ChainSpec, run_id: &str, step_order: usize) {
        let _ = self.inner.store.finish_chain_run(run_id, "failed").await;
        warn!(chain = %chain.id, run = %run_id, step = step_order, "chain step failed, stopping (on_fail=stop)");
    }

    /// Reload the run row and submit the next step off its current state.
    async fn advance_to_next_step(&self, chain: &ChainSpec, run_id: &str, step_order: usize) {
        match self.inner.store.get_chain_run(run_id).await {
            Ok(Some(run)) => self.submit_step(chain, &run, step_order + 1).await,
            Ok(None) => warn!(chain = %chain.id, run = %run_id, "chain run vanished mid-flight"),
            Err(e) => warn!(chain = %chain.id, run = %run_id, "failed to reload chain run: {e:#}"),
        }
    }

    /// Boot-time resume: for every run still marked `running`, check its
    /// in-flight step's task against the durable store. If the task actually
    /// reached a terminal state before the restart (its `TaskCompleted` event
    /// was lost with the old process, but `mark_completed` already wrote the
    /// durable row), advance normally. Otherwise the task was orphaned by the
    /// restart — resubmit the *same* step, never step 0.
    async fn resume_orphaned(&self) {
        let running = match self.inner.store.list_running_chain_runs().await {
            Ok(r) => r,
            Err(e) => {
                warn!("failed to list running chain runs for resume: {e:#}");
                return;
            }
        };
        for run in running {
            self.resume_one_run(run).await;
        }
    }

    /// Resume a single `running` chain run found on boot — see
    /// [`resume_orphaned`](Self::resume_orphaned)'s doc comment for the
    /// completed-vs-orphaned distinction this makes.
    async fn resume_one_run(&self, run: ChainRunRow) {
        let Ok(Some(chain_row)) = self.inner.store.get_schedule_chain(&run.chain_id).await else {
            warn!(run = %run.id, chain = %run.chain_id, "orphaned run references missing chain; leaving as-is");
            return;
        };
        let chain: ChainSpec = chain_row.into();
        let step_order = usize::try_from(run.current_step).unwrap_or(0);

        let terminal_status = match &run.current_task_id {
            Some(tid) => task_terminal_status(&self.inner.store, tid).await,
            None => None,
        };
        self.resume_run_by_status(chain, run, step_order, terminal_status)
            .await;
    }

    /// Either replay the step's already-known outcome (it finished before the
    /// restart) or resubmit the same step (it was orphaned by the restart),
    /// depending on whether `terminal_status` resolved to something.
    async fn resume_run_by_status(
        &self,
        chain: ChainSpec,
        run: ChainRunRow,
        step_order: usize,
        terminal_status: Option<TaskStatus>,
    ) {
        match terminal_status {
            Some(outcome) => {
                info!(run = %run.id, chain = %chain.id, step = step_order, "resuming chain run: step already completed before restart");
                self.on_step_terminal(&chain, &run.id, step_order, &outcome)
                    .await;
            }
            None => {
                info!(run = %run.id, chain = %chain.id, step = step_order, "resuming chain run: step orphaned by restart, resubmitting");
                self.submit_step(&chain, &run, step_order).await;
            }
        }
    }
}

/// Best-effort terminal-status lookup for a task by its durable row, mapped
/// back to an `AgentEvent`-shaped `TaskStatus`. Returns `None` when the task
/// is missing or still in a non-terminal status — i.e. truly orphaned.
async fn task_terminal_status(store: &MemoryStore, task_id: &str) -> Option<TaskStatus> {
    let id = TaskId(Uuid::parse_str(task_id).ok()?);
    let row = store.get_task(&id).await.ok().flatten()?;
    match row.status.as_str() {
        "success" => Some(TaskStatus::Success {
            branch: String::new(),
            pr_url: None,
        }),
        "failed" => Some(TaskStatus::Failed {
            reason: "unknown (resumed after restart)".into(),
        }),
        "rolled_back" => Some(TaskStatus::RolledBack),
        "conflict" => Some(TaskStatus::Failed {
            reason: "unresolved rebase conflict (resumed after restart)".into(),
        }),
        _ => None,
    }
}

#[cfg(test)]
#[path = "chain_schedule_manager_tests.rs"]
mod tests;
