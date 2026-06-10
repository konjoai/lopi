//! Runtime-mutable cron scheduler backing the dashboard's cron UI.
//!
//! [`crate::scheduler::boot`] registers a fixed list of TOML schedules once and
//! hands back a [`JobScheduler`] the caller can never touch again. That is too
//! rigid for an interactive dashboard, which needs to add, edit, enable/disable,
//! delete, and manually fire schedules while the server runs.
//!
//! `ScheduleManager` wraps a live `JobScheduler` plus a map of
//! `schedule_id -> job uuid` so a schedule persisted in `MemoryStore` can be
//! registered, replaced, or removed on demand. Every fire — cron tick or manual
//! `run_now` — appends a row to the schedule's run history.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use dashmap::DashMap;
use lopi_core::{Priority, RepoProfile, Task, TaskSource};
use lopi_memory::{MemoryStore, ScheduleRow};
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{info, warn};
use uuid::Uuid;

use crate::pool::AgentPool;

/// The subset of a schedule needed to fire a task. Decoupled from the storage
/// row so the manager has a single, validated shape to build [`Task`]s from.
#[derive(Debug, Clone)]
pub struct ScheduleSpec {
    /// Stable schedule id — used to attribute run history.
    pub id: String,
    /// Cron expression to register with the scheduler.
    pub cron: String,
    /// Agent goal submitted on each fire.
    pub goal: String,
    /// Target repo path, if any.
    pub repo: Option<PathBuf>,
    /// Priority string (`low` / `normal` / `high` / `critical`).
    pub priority: String,
    /// Allowed directories override.
    pub allowed_dirs: Vec<String>,
    /// Forbidden directories override.
    pub forbidden_dirs: Vec<String>,
}

impl From<ScheduleRow> for ScheduleSpec {
    fn from(r: ScheduleRow) -> Self {
        Self {
            id: r.id,
            cron: r.cron,
            goal: r.goal,
            repo: r.repo.map(PathBuf::from),
            priority: r.priority,
            allowed_dirs: r.allowed_dirs,
            forbidden_dirs: r.forbidden_dirs,
        }
    }
}

/// Build a [`Task`] from a [`ScheduleSpec`], applying priority, dir overrides,
/// and any per-repo `.lopi.toml` profile. Shared by the cron closure and
/// `run_now` so both paths produce identical tasks.
#[must_use]
pub fn build_task(spec: &ScheduleSpec) -> Task {
    let mut task = Task::new(spec.goal.clone());
    task.source = TaskSource::Api;
    task.priority = match spec.priority.as_str() {
        "low" => Priority::Low,
        "high" => Priority::High,
        "critical" => Priority::Critical,
        _ => Priority::Normal,
    };
    if !spec.allowed_dirs.is_empty() {
        task.allowed_dirs.clone_from(&spec.allowed_dirs);
    }
    if !spec.forbidden_dirs.is_empty() {
        task.forbidden_dirs.clone_from(&spec.forbidden_dirs);
    }
    if let Some(repo) = &spec.repo {
        task.repo_path = Some(repo.clone());
        RepoProfile::load_from_repo(repo).apply(&mut task);
    }
    task
}

/// Live, mutable cron scheduler. Cheap to `clone()` — all state is behind `Arc`.
#[derive(Clone)]
pub struct ScheduleManager {
    inner: Arc<Inner>,
}

struct Inner {
    /// `None` until [`ScheduleManager::start`] creates and starts the scheduler.
    sched: Mutex<Option<JobScheduler>>,
    /// schedule_id -> registered job uuid.
    jobs: DashMap<String, Uuid>,
    pool: AgentPool,
    store: MemoryStore,
}

impl ScheduleManager {
    /// Construct an un-started manager. Call [`start`](Self::start) from an
    /// async context to create the underlying scheduler and register schedules.
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

    /// Create + start the `JobScheduler` and register every enabled schedule
    /// currently in the store. Idempotent: a second call is a no-op once started.
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
            .context("creating job scheduler")?;
        sched.start().await.context("starting job scheduler")?;
        *guard = Some(sched);
        drop(guard);

        match self.inner.store.list_schedules().await {
            Ok(rows) => {
                for row in rows.into_iter().filter(|r| r.enabled) {
                    if let Err(e) = self.register(row.into()).await {
                        warn!("failed to register schedule on boot: {e:#}");
                    }
                }
            }
            Err(e) => warn!("could not load schedules on boot: {e:#}"),
        }
        Ok(())
    }

    /// Register (or replace) a live cron job for `spec`. Re-registering the same
    /// id removes the previous job first so edits take effect immediately.
    /// Returns `Ok(false)` (without error) when the cron expression is invalid.
    ///
    /// # Errors
    /// Returns `Err` if the scheduler is not started or job registration fails.
    pub async fn register(&self, spec: ScheduleSpec) -> Result<bool> {
        self.unregister(&spec.id).await?;
        let pool = self.inner.pool.clone();
        let store = self.inner.store.clone();
        let spec_for_job = spec.clone();

        // `tokio-cron-scheduler` (via the `cron` crate) requires a 6-field
        // expression with a leading seconds field. lopi's user-facing
        // convention — matching `next_run_times` — is 5-field with seconds
        // pinned to 0, so prepend it here.
        let six_field = format!("0 {}", spec.cron);
        let job = match Job::new_async(six_field.as_str(), move |_uuid, _lock| {
            let pool = pool.clone();
            let store = store.clone();
            let spec = spec_for_job.clone();
            Box::pin(async move {
                let _ = fire(&pool, &store, &spec).await;
            })
        }) {
            Ok(j) => j,
            Err(e) => {
                warn!(schedule = %spec.id, "invalid cron '{}': {e}", spec.cron);
                return Ok(false);
            }
        };

        let guard = self.inner.sched.lock().await;
        let sched = guard.as_ref().context("scheduler not started")?;
        let uuid = sched.add(job).await.context("adding cron job")?;
        self.inner.jobs.insert(spec.id.clone(), uuid);
        info!(schedule = %spec.id, cron = %spec.cron, "registered schedule");
        Ok(true)
    }

    /// Remove a schedule's live job, if registered. Safe to call for an
    /// unknown id (no-op).
    ///
    /// # Errors
    /// Returns `Err` if the scheduler rejects the removal.
    pub async fn unregister(&self, schedule_id: &str) -> Result<()> {
        let Some((_, uuid)) = self.inner.jobs.remove(schedule_id) else {
            return Ok(());
        };
        let guard = self.inner.sched.lock().await;
        if let Some(sched) = guard.as_ref() {
            sched.remove(&uuid).await.context("removing cron job")?;
            info!(schedule = %schedule_id, "unregistered schedule");
        }
        Ok(())
    }

    /// Fire a schedule immediately, bypassing its cron timing. Used by the
    /// dashboard "run now" button. Returns the new task's id when queued.
    ///
    /// # Errors
    /// Returns `Err` only if recording the run fails irrecoverably.
    pub async fn run_now(&self, spec: &ScheduleSpec) -> Result<Option<String>> {
        Ok(fire(&self.inner.pool, &self.inner.store, spec).await)
    }
}

/// Submit a task for `spec` and append a run-history row. Returns the queued
/// task id, or `None` when the queue deduplicated the submission.
async fn fire(pool: &AgentPool, store: &MemoryStore, spec: &ScheduleSpec) -> Option<String> {
    info!(schedule = %spec.id, "firing scheduled task: {}", spec.goal);
    let task = build_task(spec);
    let new_id = task.id.0.to_string();
    let duplicate = pool.submit(task).await;
    let (task_id, outcome) = match &duplicate {
        Some(existing) => (existing.0.to_string(), "duplicate"),
        None => (new_id.clone(), "queued"),
    };
    if let Err(e) = store
        .record_schedule_run(&spec.id, Some(&task_id), outcome)
        .await
    {
        warn!(schedule = %spec.id, "failed to record schedule run: {e:#}");
    }
    duplicate.map_or(Some(new_id), |_| None)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::queue::TaskQueue;
    use lopi_core::{AgentEvent, EventBus};

    fn spec(cron: &str) -> ScheduleSpec {
        ScheduleSpec {
            id: "sched-1".into(),
            cron: cron.into(),
            goal: "run tests".into(),
            repo: None,
            priority: "high".into(),
            allowed_dirs: vec!["src/".into()],
            forbidden_dirs: vec![],
        }
    }

    async fn manager() -> (ScheduleManager, MemoryStore) {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let queue = TaskQueue::new();
        let bus: EventBus<AgentEvent> = EventBus::new(16);
        let pool = AgentPool::new(1, PathBuf::from("."), queue, bus).with_store(store.clone());
        (ScheduleManager::new(pool, store.clone()), store)
    }

    #[test]
    fn build_task_maps_priority_and_dirs() {
        let task = build_task(&spec("0 2 * * *"));
        assert_eq!(task.priority, Priority::High);
        assert_eq!(task.allowed_dirs, vec!["src/".to_string()]);
        assert_eq!(task.goal, "run tests");
    }

    #[test]
    fn build_task_defaults_unknown_priority_to_normal() {
        let mut s = spec("0 2 * * *");
        s.priority = "weird".into();
        assert_eq!(build_task(&s).priority, Priority::Normal);
    }

    #[test]
    fn spec_from_row_carries_fields() {
        let row = ScheduleRow {
            id: "r1".into(),
            name: "n".into(),
            cron: "0 1 * * *".into(),
            goal: "g".into(),
            repo: Some("/tmp/x".into()),
            priority: "low".into(),
            allowed_dirs: vec!["a/".into()],
            forbidden_dirs: vec!["b/".into()],
            enabled: true,
            created_at: "t".into(),
            updated_at: "t".into(),
        };
        let s: ScheduleSpec = row.into();
        assert_eq!(s.id, "r1");
        assert_eq!(s.repo, Some(PathBuf::from("/tmp/x")));
        assert_eq!(s.forbidden_dirs, vec!["b/".to_string()]);
    }

    #[tokio::test]
    async fn start_then_register_and_unregister() {
        let (mgr, _store) = manager().await;
        mgr.start().await.unwrap();
        assert!(mgr.register(spec("0 2 * * *")).await.unwrap());
        // Invalid cron returns Ok(false), not an error.
        let mut bad = spec("not a cron");
        bad.id = "bad".into();
        assert!(!mgr.register(bad).await.unwrap());
        // Unregister is idempotent.
        mgr.unregister("sched-1").await.unwrap();
        mgr.unregister("sched-1").await.unwrap();
    }

    #[tokio::test]
    async fn start_is_idempotent() {
        let (mgr, _store) = manager().await;
        mgr.start().await.unwrap();
        mgr.start().await.unwrap();
    }

    #[tokio::test]
    async fn run_now_records_history() {
        let (mgr, store) = manager().await;
        let s = spec("0 2 * * *");
        let id = mgr.run_now(&s).await.unwrap();
        assert!(id.is_some());
        let runs = store.list_schedule_runs("sched-1", 10).await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].outcome, "queued");
    }

    #[tokio::test]
    async fn start_registers_enabled_schedules_from_store() {
        let (mgr, store) = manager().await;
        store
            .upsert_schedule(&lopi_memory::ScheduleInput {
                id: None,
                name: "boot".into(),
                cron: "0 3 * * *".into(),
                goal: "g".into(),
                repo: None,
                priority: "normal".into(),
                allowed_dirs: vec![],
                forbidden_dirs: vec![],
                enabled: true,
            })
            .await
            .unwrap();
        // Should not error even though it registers a stored schedule.
        mgr.start().await.unwrap();
    }
}
