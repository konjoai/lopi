//! The one terminal choke point for a dispatched task.
//!
//! Split out of `run_loop.rs` purely to keep that file under the 500-line CI
//! gate, matching the `run_loop_builder.rs` precedent (see that file's own
//! doc comment). Every path a task can leave `run_one` by — success,
//! failure, rollback, conflict, or an outright error — passes through
//! [`finish_task`], which makes it the correct and only place to release
//! per-task pool state.

use super::types::{AgentHandle, PoolCounters};
use super::AgentPool;
use anyhow::Result;
use dashmap::DashMap;
use lopi_core::{AgentEvent, EventBus, TaskId, TaskStatus};
use lopi_memory::MemoryStore;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, warn};

/// Everything [`finish_task`] needs to retire one dispatched task.
///
/// Bundled into a struct rather than passed as eight positional arguments,
/// the same remedy `run_loop_builder.rs` already applies with
/// `RepoGuardrails`.
pub(super) struct TerminalContext {
    pub(super) task_id: TaskId,
    /// The repo this task actually ran against (its own override, or the
    /// pool's default) — the key its collision-oracle peer entry lives under.
    pub(super) repo: PathBuf,
    pub(super) handles: Arc<DashMap<TaskId, Arc<RwLock<AgentHandle>>>>,
    pub(super) counters: Arc<PoolCounters>,
    pub(super) bus: EventBus<AgentEvent>,
    pub(super) store: Option<MemoryStore>,
    /// A clone taken before `run_one` consumed the pool, so the error path
    /// below still reaches `finish_economics_reservation`.
    pub(super) pool: AgentPool,
}

/// Retire one dispatched task: drop its live handle, deregister it from the
/// collision-oracle peer list, advance the pool counters, and — on an
/// outright error — publish the terminal event, persist `failed`, and
/// release the economics reservation.
///
/// `run_one` has already persisted the task's own final status on every
/// non-error path; this only advances in-memory pool state.
pub(super) async fn finish_task(ctx: TerminalContext, outcome: &Result<TaskStatus>) {
    let TerminalContext {
        task_id,
        repo,
        handles,
        counters,
        bus,
        store,
        pool,
    } = ctx;

    handles.remove(&task_id);
    // The peer list is process-lifetime and shared across every task on this
    // repo, so a finished task that stays registered is not inert: the oracle
    // keeps `merge-tree`-ing its dead branch against every live one, which
    // grows poll cost quadratically and alerts on work that already merged.
    pool.deregister_collision_peer(&repo, &task_id).await;
    counters.running.fetch_sub(1, Ordering::Relaxed);

    match outcome {
        Ok(TaskStatus::Success { .. }) => {
            counters.succeeded.fetch_add(1, Ordering::Relaxed);
        }
        Ok(TaskStatus::Failed { .. } | TaskStatus::RolledBack | TaskStatus::Conflict { .. })
        | Err(_) => {
            counters.failed.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }

    if let Err(e) = outcome {
        error!(task_id = %task_id, "agent run error: {e}");
        let reason = format!("{e}");
        bus.send(AgentEvent::TaskCompleted {
            task_id,
            outcome: TaskStatus::Failed { reason },
            total_attempts: 1,
            successor: None,
        });
        if let Some(store) = &store {
            if let Err(e) = store.mark_completed(&task_id, "failed").await {
                warn!(task_id = %task_id, "mark_completed(failed) failed: {e}");
            }
        }
        // No cost was ever recorded for this run — release, not reconcile, so
        // the reservation's hold vanishes without attributing spend it never
        // actually incurred.
        pool.finish_economics_reservation(task_id, None).await;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::queue::TaskQueue;
    use lopi_oracle::WatchedRef;
    use std::path::Path;

    /// Retire a task through the real terminal path — the same `finish_task`
    /// `run_loop`'s dispatch calls — rather than by poking
    /// `deregister_collision_peer` directly. Driving a full agent run needs a
    /// live `claude`, which no sandbox has; this is the closest seam that
    /// still proves the deregistration is *inside* the retirement path and
    /// not merely a method that exists.
    async fn retire(pool: &AgentPool, repo: &Path, task_id: TaskId) {
        finish_task(
            TerminalContext {
                task_id,
                repo: repo.to_path_buf(),
                handles: Arc::new(DashMap::new()),
                counters: Arc::new(PoolCounters::default()),
                bus: EventBus::new(16),
                store: None,
                pool: pool.clone(),
            },
            &Ok(TaskStatus::Success {
                branch: "lopi/finishing".to_string(),
                pr_url: None,
            }),
        )
        .await;
    }

    #[tokio::test]
    async fn retiring_a_task_drops_it_from_the_repo_peer_roster() {
        let repo = PathBuf::from("/tmp/lopi-terminal-test-a");
        let pool = AgentPool::new(
            2,
            repo.clone(),
            TaskQueue::new(),
            EventBus::<AgentEvent>::new(16),
        )
        .with_collision_oracle();
        let wiring = pool.collision_wiring(&repo).unwrap();

        let running = TaskId::new();
        let finishing = TaskId::new();
        {
            let mut guard = wiring.peers.lock().await;
            guard.push(WatchedRef::new(running.0.to_string(), "lopi/running"));
            guard.push(WatchedRef::new(finishing.0.to_string(), "lopi/finishing"));
        }

        retire(&pool, &repo, finishing).await;

        let remaining = wiring.peers.lock().await;
        assert_eq!(
            remaining.len(),
            1,
            "a retired task must leave the roster, or the oracle keeps \
             merge-tree-ing its dead branch against every live one forever"
        );
        assert_eq!(remaining.first().unwrap().label, running.0.to_string());
    }

    #[tokio::test]
    async fn retiring_a_task_on_an_unwired_pool_is_a_no_op() {
        let repo = PathBuf::from("/tmp/lopi-terminal-test-b");
        let pool = AgentPool::new(
            2,
            repo.clone(),
            TaskQueue::new(),
            EventBus::<AgentEvent>::new(16),
        );
        // No `with_collision_oracle` — the terminal path must not construct
        // oracle state as a side effect of retiring a task.
        retire(&pool, &repo, TaskId::new()).await;
        assert!(pool.collision_wiring(&repo).is_none());
    }
}
