use anyhow::{Context, Result};
use git2::{Repository, BranchType, ResetType};
use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;
use crate::diff::DiffChecker;

/// Workspace-level mutex that serialises worktree creation.
///
/// git2's `Repository::branch()` + `checkout_tree()` sequence is not atomic:
/// two concurrent calls racing on the same repo can corrupt the index or HEAD ref.
/// A single process-wide lock is sufficient because lopi agents share one process.
static WORKTREE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub struct GitManager {
    repo_path: PathBuf,
}

impl GitManager {
    pub fn new(repo_path: impl AsRef<Path>) -> Result<Self> {
        let p = repo_path.as_ref().to_path_buf();
        // Sanity-check that this is a real repo.
        let _ = Repository::open(&p).with_context(|| format!("opening git repo at {}", p.display()))?;
        Ok(Self { repo_path: p })
    }

    pub fn repo(&self) -> Result<Repository> {
        Repository::open(&self.repo_path).context("opening git repo")
    }

    /// Snapshot the current HEAD so we can roll back later.
    pub fn head_oid(&self) -> Result<String> {
        let repo = self.repo()?;
        let head = repo.head()?.peel_to_commit()?;
        Ok(head.id().to_string())
    }

    /// Create + check out a new branch from HEAD.
    ///
    /// Holds the process-wide `WORKTREE_LOCK` for the duration of the operation
    /// so that parallel agents cannot interleave `branch()` + `checkout_tree()` calls
    /// on the same repository.
    pub async fn checkout_new_branch(&self, name: &str) -> Result<()> {
        let name = name.to_string();
        let repo_path = self.repo_path.clone();
        let _guard = WORKTREE_LOCK.lock().await;
        tokio::task::spawn_blocking(move || -> Result<()> {
            let repo = Repository::open(&repo_path)?;
            let head_commit = repo.head()?.peel_to_commit()?;
            // If branch already exists, just check it out.
            if repo.find_branch(&name, BranchType::Local).is_err() {
                repo.branch(&name, &head_commit, false)?;
            }
            let refname = format!("refs/heads/{}", name);
            let obj = repo.revparse_single(&refname)?;
            repo.checkout_tree(&obj, None)?;
            repo.set_head(&refname)?;
            Ok(())
        })
        .await
        .context("join error in checkout_new_branch")??;
        Ok(())
    }

    /// Return env-var overrides to set when spawning agent sub-processes in this worktree.
    ///
    /// Setting `CARGO_TARGET_DIR` to a worktree-local path prevents parallel agents from
    /// contending on the shared workspace `target/` directory during `cargo build`/`cargo test`.
    pub fn worktree_env(&self) -> Vec<(String, String)> {
        vec![
            ("CARGO_TARGET_DIR".to_string(), ".cargo-target".to_string()),
        ]
    }

    /// Verify the working-tree diff vs HEAD only touches allowed dirs.
    pub async fn check_diff_scope(
        &self,
        allowed: &[String],
        forbidden: &[String],
    ) -> Result<()> {
        let allowed = allowed.to_vec();
        let forbidden = forbidden.to_vec();
        let repo_path = self.repo_path.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let repo = Repository::open(&repo_path)?;
            let mut paths: Vec<String> = vec![];
            // Diff workdir vs HEAD tree; if there's no HEAD yet, treat all index entries as additions.
            let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
            let diff = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), None)?;
            diff.foreach(
                &mut |delta, _| {
                    if let Some(p) = delta.new_file().path().or_else(|| delta.old_file().path()) {
                        paths.push(p.to_string_lossy().into_owned());
                    }
                    true
                },
                None, None, None,
            )?;
            let checker = DiffChecker::new(allowed, forbidden);
            checker.validate(&paths)
        })
        .await
        .context("join error in check_diff_scope")??;
        Ok(())
    }

    /// Discard all working-tree changes and untracked files, returning to HEAD.
    pub async fn hard_rollback(&self) -> Result<()> {
        let repo_path = self.repo_path.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let repo = Repository::open(&repo_path)?;
            let head = repo.head()?.peel_to_commit()?;
            repo.reset(head.as_object(), ResetType::Hard, None)?;
            // Also clean untracked files.
            let mut opts = git2::StatusOptions::new();
            opts.include_untracked(true).recurse_untracked_dirs(true);
            let statuses = repo.statuses(Some(&mut opts))?;
            for s in statuses.iter() {
                if s.status().contains(git2::Status::WT_NEW) {
                    if let Some(rel) = s.path() {
                        let p = repo_path.join(rel);
                        if p.is_file() {
                            let _ = std::fs::remove_file(&p);
                        } else if p.is_dir() {
                            let _ = std::fs::remove_dir_all(&p);
                        }
                    }
                }
            }
            Ok(())
        })
        .await
        .context("join error in hard_rollback")??;
        Ok(())
    }

    /// Switch back to the default branch (main/master if available).
    pub async fn checkout_default(&self) -> Result<()> {
        let repo_path = self.repo_path.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let repo = Repository::open(&repo_path)?;
            for candidate in ["main", "master"] {
                let refname = format!("refs/heads/{}", candidate);
                if let Ok(obj) = repo.revparse_single(&refname) {
                    repo.checkout_tree(&obj, None)?;
                    repo.set_head(&refname)?;
                    return Ok(());
                }
            }
            Ok(())
        })
        .await
        .context("join error in checkout_default")??;
        Ok(())
    }

    /// Commit all current changes on the active branch.
    pub async fn commit_all(&self, message: &str) -> Result<String> {
        let message = message.to_string();
        let repo_path = self.repo_path.clone();
        let oid = tokio::task::spawn_blocking(move || -> Result<String> {
            let repo = Repository::open(&repo_path)?;
            let mut index = repo.index()?;
            index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
            index.write()?;
            let tree_id = index.write_tree()?;
            let tree = repo.find_tree(tree_id)?;
            let sig = repo.signature().or_else(|_| git2::Signature::now("lopi", "lopi@konjoai.dev"))?;
            let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
            let parents: Vec<&git2::Commit> = parent.iter().collect();
            let oid = repo.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)?;
            Ok(oid.to_string())
        })
        .await
        .context("join error in commit_all")??;
        Ok(oid)
    }

    /// Push branch and open a PR via the `gh` CLI. Returns the PR URL.
    pub async fn open_pr(&self, branch: &str, title: &str) -> Result<String> {
        // Push the branch.
        let push = tokio::process::Command::new("git")
            .arg("-C").arg(&self.repo_path)
            .arg("push").arg("-u").arg("origin").arg(branch)
            .output()
            .await
            .context("invoking git push")?;
        if !push.status.success() {
            anyhow::bail!("git push failed: {}", String::from_utf8_lossy(&push.stderr));
        }

        let body = format!("Automated PR opened by lopi.\n\nBranch: `{branch}`\n");
        let out = tokio::process::Command::new("gh")
            .arg("pr").arg("create")
            .arg("--title").arg(title)
            .arg("--body").arg(&body)
            .arg("--head").arg(branch)
            .current_dir(&self.repo_path)
            .output()
            .await
            .context("invoking gh pr create")?;
        if !out.status.success() {
            anyhow::bail!("gh pr create failed: {}", String::from_utf8_lossy(&out.stderr));
        }
        let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(url)
    }

}
