//! `lopi worktree` — inspect and garbage-collect per-task git worktrees.
//!
//! When a repo runs with `isolation = "worktree"` (see `.lopi/loop.toml`), each
//! task gets a throwaway checkout under `.lopi/worktrees/`. These commands let an
//! operator see what's live and reclaim what finished tasks left behind — the
//! cleanup half of loop-engineering's worktree isolation.
//!
//! Both entry points are pure return-value functions so the `main.rs` dispatch
//! only prints; the behavior here is unit-testable without capturing stdout.

use anyhow::Result;
use lopi_git::WorktreeManager;
use std::path::Path;

/// Prefix for lopi's ephemeral per-attempt branches.
const LOPI_BRANCH_PREFIX: &str = "lopi/";

/// Reclaim orphaned worktrees and stale `lopi/*` branches for `repo`, returning
/// a printable summary. Backs `lopi worktree gc`.
///
/// # Errors
/// Returns `Err` if `repo` is not a git repository or the initial git
/// prune/list fails.
pub async fn gc(repo: &Path) -> Result<String> {
    let mgr = WorktreeManager::new(repo)?;
    let report = mgr.gc(LOPI_BRANCH_PREFIX).await?;
    Ok(format!(
        "⟲ lopi worktree gc — {}\n  worktrees reclaimed: {}\n  branches reclaimed:  {}\n",
        repo.display(),
        report.worktrees_removed,
        report.branches_removed,
    ))
}

/// Render the worktrees git currently tracks for `repo`. Backs
/// `lopi worktree list`.
///
/// # Errors
/// Returns `Err` if `repo` is not a git repository or `git worktree list` fails.
pub async fn list(repo: &Path) -> Result<String> {
    let mgr = WorktreeManager::new(repo)?;
    let paths = mgr.list().await?;
    let mut out = format!("⟲ lopi worktree list — {}\n", repo.display());
    for p in &paths {
        out.push_str(&format!("  {}\n", p.display()));
    }
    out.push_str(&format!("  ({} worktree(s))\n", paths.len()));
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::{gc, list};
    use std::process::Command;
    use tempfile::TempDir;

    fn init_repo() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new().unwrap();
        let p = dir.path().to_path_buf();
        for a in [
            vec!["init", "-b", "main"],
            vec!["config", "user.email", "t@konjoai.dev"],
            vec!["config", "user.name", "t"],
        ] {
            assert!(Command::new("git")
                .arg("-C")
                .arg(&p)
                .args(&a)
                .status()
                .unwrap()
                .success());
        }
        std::fs::write(p.join("README.md"), "hi").unwrap();
        for a in [vec!["add", "."], vec!["commit", "-m", "init"]] {
            assert!(Command::new("git")
                .arg("-C")
                .arg(&p)
                .args(&a)
                .status()
                .unwrap()
                .success());
        }
        (dir, p)
    }

    #[tokio::test]
    async fn list_shows_main_worktree() {
        let (_d, repo) = init_repo();
        let out = list(&repo).await.unwrap();
        assert!(out.contains("lopi worktree list"));
        assert!(out.contains("(1 worktree(s))"));
    }

    #[tokio::test]
    async fn gc_on_clean_repo_reclaims_nothing() {
        let (_d, repo) = init_repo();
        let out = gc(&repo).await.unwrap();
        assert!(out.contains("worktrees reclaimed: 0"));
        assert!(out.contains("branches reclaimed:  0"));
    }

    #[tokio::test]
    async fn commands_error_on_non_repo() {
        let dir = TempDir::new().unwrap();
        assert!(gc(dir.path()).await.is_err());
        assert!(list(dir.path()).await.is_err());
    }
}
