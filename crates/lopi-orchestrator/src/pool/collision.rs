//! Pool-level `lopi-oracle` wiring — the production call site
//! `AgentRunner::with_collision_oracle` never had.
//!
//! `lopi-oracle` shipped with its runner-side integration built and tested
//! but nothing in the orchestrator constructing it, so cross-agent collision
//! detection could not run for a real dispatched task. These methods close
//! that gap.
//!
//! **Per repo, not per pool.** A pool dispatches against several repos (a
//! task's own `repo_path` overrides the pool's default, and `repo_permits`
//! is keyed the same way). Two tasks only ever collide when they run against
//! the same repo, and `git merge-tree` needs both refs resolvable in one
//! object store, so oracle and peer list are keyed by repo path. The key is
//! always the *shared* repo, never a per-task worktree checkout: worktrees
//! share the repo's refs and object database, so every task's branch
//! resolves there whichever checkout created it.
//!
//! **Opt-in, and inert until opted in.** Without
//! [`AgentPool::with_collision_oracle`], [`AgentPool::collision_wiring`]
//! returns `None` and the runner is built exactly as it was before this
//! module existed — the behavior-identical-when-unset invariant
//! `lopi-oracle`'s own docs state.

use super::AgentPool;
use lopi_core::TaskId;
use lopi_oracle::{CollisionOracle, WatchedRef};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// One repo's shared oracle and peer list, handed to a runner as a pair.
///
/// Bundled rather than passed as two more positional arguments to
/// `build_runner`, which is already at its argument ceiling — the same
/// remedy `run_loop_builder.rs` applies with `RepoGuardrails`.
#[derive(Clone)]
pub(super) struct CollisionWiring {
    pub(super) oracle: Arc<Mutex<CollisionOracle>>,
    pub(super) peers: Arc<Mutex<Vec<WatchedRef>>>,
}

impl AgentPool {
    /// Opt this pool into cross-agent collision detection.
    ///
    /// Every task dispatched afterwards registers its branch into a peer
    /// list shared by all tasks on the same repo, and receives any textual
    /// collision with a sibling as advisory planning context. Detection
    /// only: an alert never blocks, cancels, or retries a task.
    ///
    /// Unset (the default) is behavior-identical to before this existed.
    #[must_use]
    pub fn with_collision_oracle(mut self) -> Self {
        self.collision_enabled = true;
        self
    }

    /// The shared oracle and peer list for `repo`, creating them on first
    /// use. `None` when the pool was never opted in.
    ///
    /// `repo` must be the shared repo path, not a task's worktree checkout.
    pub(super) fn collision_wiring(&self, repo: &Path) -> Option<CollisionWiring> {
        if !self.collision_enabled {
            return None;
        }
        let oracle = self
            .collision_oracles
            .entry(repo.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(CollisionOracle::new(repo))))
            .clone();
        let peers = self
            .collision_peers
            .entry(repo.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(Vec::new())))
            .clone();
        Some(CollisionWiring { oracle, peers })
    }

    /// Drop a finished task's entry from `repo`'s peer list.
    ///
    /// The peer list outlives every task on the repo, and the runner side
    /// only ever registers — it has no completion hook to deregister from.
    /// Left in, a finished task's branch is checked against every live one
    /// on every poll forever: poll cost grows with tasks *ever* run rather
    /// than tasks running, and agents are warned about collisions with work
    /// that already merged. Retiring the entry here, at the pool's single
    /// terminal choke point, is what makes the shared list a live roster.
    ///
    /// No-op when the pool was never opted in, or when the task never
    /// registered (it opted out, or failed before its first planning pass).
    pub(super) async fn deregister_collision_peer(&self, repo: &Path, task_id: &TaskId) {
        let Some(peers) = self.collision_peers.get(repo).map(|e| e.clone()) else {
            return;
        };
        // The runner labels its own ref with the bare task UUID
        // (`collision_seed.rs`); match that exactly.
        let label = task_id.0.to_string();
        peers.lock().await.retain(|r| r.label != label);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::queue::TaskQueue;
    use lopi_core::{AgentEvent, EventBus};
    use std::path::PathBuf;

    fn pool(repo: &Path) -> AgentPool {
        AgentPool::new(
            2,
            repo.to_path_buf(),
            TaskQueue::new(),
            EventBus::<AgentEvent>::new(16),
        )
    }

    #[tokio::test]
    async fn an_unwired_pool_hands_out_no_wiring() {
        let repo = PathBuf::from("/tmp/lopi-collision-test-a");
        assert!(
            pool(&repo).collision_wiring(&repo).is_none(),
            "unset must stay behavior-identical: no oracle, no peer list"
        );
    }

    #[tokio::test]
    async fn one_repo_shares_a_single_oracle_and_peer_list() {
        let repo = PathBuf::from("/tmp/lopi-collision-test-b");
        let pool = pool(&repo).with_collision_oracle();

        let first = pool.collision_wiring(&repo).unwrap();
        let second = pool.collision_wiring(&repo).unwrap();

        assert!(
            Arc::ptr_eq(&first.oracle, &second.oracle),
            "sibling tasks on one repo must share one oracle, or each keeps \
             its own delivery ledger and nothing de-duplicates"
        );
        assert!(
            Arc::ptr_eq(&first.peers, &second.peers),
            "and one peer list, or neither task can see the other"
        );
    }

    #[tokio::test]
    async fn two_repos_get_independent_oracles() {
        let a = PathBuf::from("/tmp/lopi-collision-test-c");
        let b = PathBuf::from("/tmp/lopi-collision-test-d");
        let pool = pool(&a).with_collision_oracle();

        let wiring_a = pool.collision_wiring(&a).unwrap();
        let wiring_b = pool.collision_wiring(&b).unwrap();

        assert!(
            !Arc::ptr_eq(&wiring_a.oracle, &wiring_b.oracle),
            "tasks on different repos never collide and must not share state"
        );
        assert!(!Arc::ptr_eq(&wiring_a.peers, &wiring_b.peers));
    }

    #[tokio::test]
    async fn deregistering_removes_only_that_task() {
        let repo = PathBuf::from("/tmp/lopi-collision-test-e");
        let pool = pool(&repo).with_collision_oracle();
        let wiring = pool.collision_wiring(&repo).unwrap();

        let keep = TaskId::new();
        let retire = TaskId::new();
        {
            let mut guard = wiring.peers.lock().await;
            guard.push(WatchedRef::new(keep.0.to_string(), "lopi/keep"));
            guard.push(WatchedRef::new(retire.0.to_string(), "lopi/retire"));
        }

        pool.deregister_collision_peer(&repo, &retire).await;

        let remaining = wiring.peers.lock().await;
        assert_eq!(remaining.len(), 1, "exactly one entry retired");
        assert_eq!(
            remaining.first().unwrap().label,
            keep.0.to_string(),
            "the still-running task's entry must survive"
        );
    }

    #[tokio::test]
    async fn deregistering_an_unregistered_task_is_a_no_op() {
        let repo = PathBuf::from("/tmp/lopi-collision-test-f");
        let pool = pool(&repo).with_collision_oracle();
        // Never called `collision_wiring`, so no peer list exists for this
        // repo at all — a task that failed before its first planning pass.
        pool.deregister_collision_peer(&repo, &TaskId::new()).await;
    }
}
