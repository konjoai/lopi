//! Per-task worktree isolation for the agent pool (Pentad M1.2).
//!
//! When a repo's `.lopi/loop.toml` selects `isolation = "worktree"`, each
//! running task gets its own detached `git worktree` checkout instead of sharing
//! the repo's working directory. The agent runner then operates entirely inside
//! that checkout — its `target/` follows the cwd — so parallel tasks on the same
//! repo never collide on files or contend on one build directory.
//!
//! Both helpers are deliberately failure-tolerant: a worktree that can't be
//! created or removed must never stall or fail a task, so errors are logged and
//! the run falls back to (or leaves behind, for `prune` to reclaim) the shared
//! checkout.

use lopi_core::TaskId;
use lopi_git::{Worktree, WorktreeManager};
use std::path::Path;
use tracing::{info, warn};

/// Create a per-task **detached** worktree when `enabled`.
///
/// Returns `None` — the shared-repo fallback — when isolation is off *or* on any
/// setup failure. The returned [`Worktree`] reaps its checkout on drop (the
/// panic / early-return safety net); callers should still [`cleanup_worktree`]
/// explicitly on the normal path.
pub(super) async fn setup_worktree(
    repo: &Path,
    enabled: bool,
    task_id: &TaskId,
) -> Option<Worktree> {
    if !enabled {
        return None;
    }
    let mgr = match WorktreeManager::new(repo) {
        Ok(m) => m,
        Err(e) => {
            warn!(task_id = %task_id, "worktree init failed ({e}); using shared repo");
            return None;
        }
    };
    match mgr.add_detached(&task_id.0.to_string()).await {
        Ok(wt) => {
            info!(task_id = %task_id, path = %wt.path().display(), "task isolated in worktree");
            Some(wt)
        }
        Err(e) => {
            warn!(task_id = %task_id, "worktree add failed ({e}); using shared repo");
            None
        }
    }
}

/// Explicitly reap a task's worktree, logging (never propagating) any failure —
/// a stale checkout is reclaimed by the next `prune`, so cleanup must not fail
/// the task's terminal handling.
pub(super) async fn cleanup_worktree(worktree: Option<Worktree>) {
    if let Some(wt) = worktree {
        if let Err(e) = wt.cleanup().await {
            warn!(path = %wt.path().display(), "worktree cleanup failed ({e}); prune will reclaim it");
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::{cleanup_worktree, setup_worktree};
    use lopi_core::TaskId;
    use std::process::Command;
    use tempfile::TempDir;

    fn run_git(repo: &std::path::Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(repo)
                .args(args)
                .status()
                .unwrap()
                .success(),
            "git {args:?} failed"
        );
    }

    fn init_repo() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let p = dir.path().to_path_buf();
        run_git(&p, &["init", "-b", "main"]);
        run_git(&p, &["config", "user.email", "t@konjoai.dev"]);
        run_git(&p, &["config", "user.name", "t"]);
        std::fs::write(p.join("README.md"), "hi").unwrap();
        run_git(&p, &["add", "."]);
        run_git(&p, &["commit", "-m", "init"]);
        (dir, p)
    }

    #[tokio::test]
    async fn disabled_yields_shared_repo_fallback() {
        let (_d, repo) = init_repo();
        assert!(setup_worktree(&repo, false, &TaskId::new()).await.is_none());
    }

    #[tokio::test]
    async fn non_repo_falls_back_instead_of_failing() {
        let dir = TempDir::new().unwrap();
        // Enabled, but the path is not a git repo → must degrade to None.
        assert!(setup_worktree(dir.path(), true, &TaskId::new())
            .await
            .is_none());
    }

    #[tokio::test]
    async fn enabled_creates_isolated_checkout_then_cleans_up() {
        let (_d, repo) = init_repo();
        let wt = setup_worktree(&repo, true, &TaskId::new()).await;
        let path = wt.as_ref().map(|w| w.path().to_path_buf());
        assert!(path.as_ref().is_some_and(|p| p.is_dir()), "checkout exists");
        cleanup_worktree(wt).await;
        assert!(path.is_some_and(|p| !p.exists()), "checkout reaped");
    }

    #[tokio::test]
    async fn cleanup_none_is_a_noop() {
        cleanup_worktree(None).await;
    }
}
