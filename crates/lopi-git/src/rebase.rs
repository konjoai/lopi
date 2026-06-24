//! Pre-PR rebase support for [`GitManager`] (Pentad M1.3b).
//!
//! When a task finishes, the default branch may have advanced underneath it.
//! Rebasing the task's branch onto the new tip before opening a PR keeps history
//! linear and surfaces real conflicts as *paths* — mapped to
//! [`TaskStatus::Conflict`](lopi_core::TaskStatus::Conflict) — instead of
//! failing silently or force-merging an unsafe result.
//!
//! These methods extend `GitManager` from a sibling module, reaching the repo
//! path through its `repo_path()` accessor. They shell out to `git` (consistent
//! with the manager's push/PR helpers) rather than using libgit2's rebase API.

use crate::manager::GitManager;
use anyhow::{Context, Result};

impl GitManager {
    /// Rebase the current branch onto `base` (e.g. `"origin/main"`).
    ///
    /// Returns the conflicting repo-relative paths when the rebase hits
    /// conflicts — **aborting** the rebase so the worktree is left clean — or an
    /// empty vec on a clean rebase. The empty/non-empty split lets the caller map
    /// a conflict to a structured status instead of silently failing.
    ///
    /// # Errors
    /// Returns `Err` if the rebase fails for a non-conflict reason (e.g. `base`
    /// does not exist), or if git cannot be invoked.
    pub async fn rebase_onto(&self, base: &str) -> Result<Vec<String>> {
        let out = tokio::process::Command::new("git")
            .arg("-C")
            .arg(self.repo_path())
            .args(rebase_args(base))
            .output()
            .await
            .context("invoking git rebase")?;
        if out.status.success() {
            return Ok(Vec::new());
        }
        let conflicts = self.unmerged_paths().await?;
        // Restore a clean worktree regardless of why the rebase stopped.
        if let Err(e) = self.rebase_abort().await {
            tracing::warn!("git rebase --abort failed: {e}");
        }
        if conflicts.is_empty() {
            anyhow::bail!(
                "git rebase {base} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        Ok(conflicts)
    }

    /// Fetch `origin` (best-effort) and rebase the current branch onto the latest
    /// default branch — `origin/main`/`origin/master`, falling back to the local
    /// branch. The runner calls this before opening a PR so a task that ran while
    /// `main` advanced lands cleanly on the new tip.
    ///
    /// Returns conflicting paths (rebase aborted) or an empty vec on a clean or
    /// no-op rebase. A missing remote or default branch is **not** an error: the
    /// rebase is simply skipped, so single-branch repos and offline runs proceed.
    ///
    /// # Errors
    /// Returns `Err` only if a resolvable rebase fails for a non-conflict reason.
    pub async fn rebase_onto_default(&self) -> Result<Vec<String>> {
        // Best-effort refresh; offline / no-remote repos just skip it.
        let _ = tokio::process::Command::new("git")
            .arg("-C")
            .arg(self.repo_path())
            .args(["fetch", "origin"])
            .output()
            .await;
        let Some(base) = self.resolve_default_base().await else {
            return Ok(Vec::new()); // nothing to rebase onto
        };
        self.rebase_onto(&base).await
    }

    /// First existing ref among the conventional default branches, preferring a
    /// fetched remote tip. `None` when none exist (a fresh single-branch repo).
    async fn resolve_default_base(&self) -> Option<String> {
        for cand in ["origin/main", "origin/master", "main", "master"] {
            if self.ref_exists(cand).await {
                return Some(cand.to_string());
            }
        }
        None
    }

    /// Whether `refname` resolves to a commit in this repo.
    async fn ref_exists(&self, refname: &str) -> bool {
        tokio::process::Command::new("git")
            .arg("-C")
            .arg(self.repo_path())
            .args(["rev-parse", "--verify", "--quiet", refname])
            .output()
            .await
            .is_ok_and(|o| o.status.success())
    }

    /// Repo-relative paths currently in an unmerged (conflicted) state.
    async fn unmerged_paths(&self) -> Result<Vec<String>> {
        let out = tokio::process::Command::new("git")
            .arg("-C")
            .arg(self.repo_path())
            .args(["diff", "--name-only", "--diff-filter=U"])
            .output()
            .await
            .context("invoking git diff for conflicts")?;
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// Abort an in-progress rebase, restoring the pre-rebase worktree state.
    async fn rebase_abort(&self) -> Result<()> {
        let out = tokio::process::Command::new("git")
            .arg("-C")
            .arg(self.repo_path())
            .args(["rebase", "--abort"])
            .output()
            .await
            .context("invoking git rebase --abort")?;
        if !out.status.success() {
            anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr));
        }
        Ok(())
    }
}

/// Build the `git rebase <base>` argument vector. Kept pure for testability.
fn rebase_args(base: &str) -> Vec<String> {
    vec!["rebase".to_string(), base.to_string()]
}

#[cfg(test)]
mod tests {
    #[test]
    fn rebase_args_targets_base() {
        assert_eq!(
            super::rebase_args("origin/main"),
            vec!["rebase", "origin/main"]
        );
    }
}
