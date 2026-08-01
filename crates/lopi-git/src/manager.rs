use crate::diff::DiffChecker;
use git2::{BranchType, Repository, ResetType};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use thiserror::Error;
use tokio::sync::Mutex;

/// Errors from [`GitManager`] operations (Track C -- the error-taxonomy pass
/// finishing what Sprint S13R, Phase E started in `diff.rs`; also used by
/// `rebase.rs`, which extends `GitManager` from a sibling module).
#[derive(Debug, Error)]
pub enum GitManagerError {
    /// Opening the repository at `path` failed.
    #[error("opening git repo at {path}: {source}")]
    OpenRepo {
        /// The path that failed to open as a git repository.
        path: PathBuf,
        #[source]
        source: git2::Error,
    },
    /// A git2 operation (branch, checkout, commit, reset, diff, status...) failed.
    #[error("git operation failed: {0}")]
    Git2(#[from] git2::Error),
    /// A `tokio::task::spawn_blocking` closure panicked before returning.
    #[error("join error in {context}: {source}")]
    Join {
        /// Which blocking operation this join failure came from.
        context: &'static str,
        #[source]
        source: tokio::task::JoinError,
    },
    /// A subprocess (`git`, `gh`) could not be spawned or its output read.
    #[error("invoking {command}: {source}")]
    Spawn {
        /// The command that failed to spawn, for the error message.
        command: &'static str,
        #[source]
        source: std::io::Error,
    },
    /// A subprocess ran but exited non-zero.
    #[error("{command} failed: {stderr}")]
    CommandFailed {
        /// The command that exited non-zero.
        command: String,
        /// Its captured stderr.
        stderr: String,
    },
    /// The working-tree diff touched a forbidden or out-of-scope path.
    #[error("diff scope: {0}")]
    DiffScope(#[from] crate::diff::DiffScopeError),
}

/// Workspace-level mutex that serialises worktree creation.
///
/// git2's `Repository::branch()` + `checkout_tree()` sequence is not atomic:
/// two concurrent calls racing on the same repo can corrupt the index or HEAD ref.
/// A single process-wide lock is sufficient because lopi agents share one process.
static WORKTREE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Manages git branch creation, checkout, and rollback for isolated agent workspaces.
pub struct GitManager {
    repo_path: PathBuf,
}

impl GitManager {
    /// Create a new `GitManager` for the given repository path.
    ///
    /// # Errors
    /// Returns `Err` if the path is not a valid git repository.
    pub fn new(repo_path: impl AsRef<Path>) -> Result<Self, GitManagerError> {
        let p = repo_path.as_ref().to_path_buf();
        // Sanity-check that this is a real repo.
        Repository::open(&p).map_err(|source| GitManagerError::OpenRepo {
            path: p.clone(),
            source,
        })?;
        Ok(Self { repo_path: p })
    }

    /// Open the underlying git repository.
    ///
    /// # Errors
    /// Returns `Err` if the repository cannot be opened.
    pub fn repo(&self) -> Result<Repository, GitManagerError> {
        Repository::open(&self.repo_path).map_err(|source| GitManagerError::OpenRepo {
            path: self.repo_path.clone(),
            source,
        })
    }

    /// Path to the repository this manager operates on. Used by sibling modules
    /// (e.g. `rebase`) that extend `GitManager` with more git operations.
    pub(crate) fn repo_path(&self) -> &Path {
        &self.repo_path
    }

    /// Snapshot the current HEAD so we can roll back later.
    ///
    /// # Errors
    /// Returns `Err` if HEAD cannot be resolved or the commit cannot be read.
    pub fn head_oid(&self) -> Result<String, GitManagerError> {
        let repo = self.repo()?;
        let head = repo.head()?.peel_to_commit()?;
        Ok(head.id().to_string())
    }

    /// Create + check out a new branch from HEAD.
    ///
    /// Holds the process-wide `WORKTREE_LOCK` for the duration of the operation
    /// so that parallel agents cannot interleave `branch()` + `checkout_tree()` calls
    /// on the same repository.
    ///
    /// # Errors
    /// Returns `Err` if the branch cannot be created or checked out.
    pub async fn checkout_new_branch(&self, name: &str) -> Result<(), GitManagerError> {
        let name = name.to_string();
        let repo_path = self.repo_path.clone();
        let _guard = WORKTREE_LOCK.lock().await;
        tokio::task::spawn_blocking(move || -> Result<(), GitManagerError> {
            let repo = Repository::open(&repo_path)?;
            let head_commit = repo.head()?.peel_to_commit()?;
            // If branch already exists, just check it out.
            if repo.find_branch(&name, BranchType::Local).is_err() {
                repo.branch(&name, &head_commit, false)?;
            }
            let refname = format!("refs/heads/{name}");
            let obj = repo.revparse_single(&refname)?;
            repo.checkout_tree(&obj, None)?;
            repo.set_head(&refname)?;
            Ok(())
        })
        .await
        .map_err(|source| GitManagerError::Join {
            context: "checkout_new_branch",
            source,
        })??;
        Ok(())
    }

    /// Return env-var overrides to set when spawning agent sub-processes in this worktree.
    ///
    /// Setting `CARGO_TARGET_DIR` to a worktree-local path prevents parallel agents from
    /// contending on the shared workspace `target/` directory during `cargo build`/`cargo test`.
    #[must_use]
    pub fn worktree_env(&self) -> Vec<(String, String)> {
        vec![("CARGO_TARGET_DIR".to_string(), ".cargo-target".to_string())]
    }

    /// Verify the working-tree diff vs HEAD only touches allowed dirs.
    ///
    /// # Errors
    /// Returns `Err` if the diff touches forbidden or out-of-scope paths.
    pub async fn check_diff_scope(
        &self,
        allowed: &[String],
        forbidden: &[String],
    ) -> Result<(), GitManagerError> {
        let allowed = allowed.to_vec();
        let forbidden = forbidden.to_vec();
        let repo_path = self.repo_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), GitManagerError> {
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
                None,
                None,
                None,
            )?;
            let checker = DiffChecker::new(allowed, forbidden);
            checker.validate(&paths)?;
            Ok(())
        })
        .await
        .map_err(|source| GitManagerError::Join {
            context: "check_diff_scope",
            source,
        })??;
        Ok(())
    }

    /// Discard all working-tree changes and untracked files, returning to HEAD.
    ///
    /// # Errors
    /// Returns `Err` if the reset operation fails.
    pub async fn hard_rollback(&self) -> Result<(), GitManagerError> {
        let repo_path = self.repo_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), GitManagerError> {
            let repo = Repository::open(&repo_path)?;
            let head = repo.head()?.peel_to_commit()?;
            repo.reset(head.as_object(), ResetType::Hard, None)?;
            // Also clean untracked files.
            let mut opts = git2::StatusOptions::new();
            opts.include_untracked(true).recurse_untracked_dirs(true);
            let statuses = repo.statuses(Some(&mut opts))?;
            for s in statuses.iter() {
                if s.status().contains(git2::Status::WT_NEW) {
                    if let Ok(rel) = s.path() {
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
        .map_err(|source| GitManagerError::Join {
            context: "hard_rollback",
            source,
        })??;
        Ok(())
    }

    /// Switch back to the default branch (main/master if available).
    ///
    /// # Errors
    /// Returns `Err` if the checkout operation fails.
    pub async fn checkout_default(&self) -> Result<(), GitManagerError> {
        let repo_path = self.repo_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), GitManagerError> {
            let repo = Repository::open(&repo_path)?;
            for candidate in ["main", "master"] {
                let refname = format!("refs/heads/{candidate}");
                if let Ok(obj) = repo.revparse_single(&refname) {
                    repo.checkout_tree(&obj, None)?;
                    repo.set_head(&refname)?;
                    return Ok(());
                }
            }
            Ok(())
        })
        .await
        .map_err(|source| GitManagerError::Join {
            context: "checkout_default",
            source,
        })??;
        Ok(())
    }

    /// Commit all current changes on the active branch.
    ///
    /// # Errors
    /// Returns `Err` if staging, tree writing, or committing fails.
    pub async fn commit_all(&self, message: &str) -> Result<String, GitManagerError> {
        let message = message.to_string();
        let repo_path = self.repo_path.clone();
        let oid = tokio::task::spawn_blocking(move || -> Result<String, GitManagerError> {
            let repo = Repository::open(&repo_path)?;
            let mut index = repo.index()?;
            index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)?;
            index.write()?;
            let tree_id = index.write_tree()?;
            let tree = repo.find_tree(tree_id)?;
            let sig = repo
                .signature()
                .or_else(|_| git2::Signature::now("lopi", "lopi@konjoai.dev"))?;
            let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
            let parents: Vec<&git2::Commit> = parent.iter().collect();
            let oid = repo.commit(Some("HEAD"), &sig, &sig, &message, &tree, &parents)?;
            Ok(oid.to_string())
        })
        .await
        .map_err(|source| GitManagerError::Join {
            context: "commit_all",
            source,
        })??;
        Ok(oid)
    }

    /// Push branch to remote without opening a PR.
    ///
    /// # Errors
    /// Returns `Err` if `git push` fails.
    pub async fn push_branch(&self, branch: &str) -> Result<(), GitManagerError> {
        let push = tokio::process::Command::new("git")
            .arg("-C")
            .arg(&self.repo_path)
            .arg("push")
            .arg("-u")
            .arg("origin")
            .arg(branch)
            .output()
            .await
            .map_err(|source| GitManagerError::Spawn {
                command: "git push",
                source,
            })?;
        if !push.status.success() {
            return Err(GitManagerError::CommandFailed {
                command: "git push".to_string(),
                stderr: String::from_utf8_lossy(&push.stderr).into_owned(),
            });
        }
        Ok(())
    }

    /// Push branch and open a PR via the `gh` CLI. Returns the PR URL.
    ///
    /// # Errors
    /// Returns `Err` if `git push` or `gh pr create` fails.
    pub async fn open_pr(&self, branch: &str, title: &str) -> Result<String, GitManagerError> {
        self.create_pr(branch, title, false).await
    }

    /// Push branch and open a **draft** PR via the `gh` CLI. Returns the PR URL.
    ///
    /// Used by the L2 (`draft_pr`) autonomy level: the GitHub review on the
    /// draft is itself the human gate before merge.
    ///
    /// # Errors
    /// Returns `Err` if `git push` or `gh pr create` fails.
    pub async fn open_draft_pr(
        &self,
        branch: &str,
        title: &str,
    ) -> Result<String, GitManagerError> {
        self.create_pr(branch, title, true).await
    }

    /// Push the branch and create a PR. When `draft` is set the PR is opened
    /// as a draft (`gh pr create --draft`).
    async fn create_pr(
        &self,
        branch: &str,
        title: &str,
        draft: bool,
    ) -> Result<String, GitManagerError> {
        self.push_branch(branch).await?;
        let body = format!("Automated PR opened by lopi.\n\nBranch: `{branch}`\n");
        let args = pr_create_args(title, &body, branch, draft);
        let out = tokio::process::Command::new("gh")
            .args(&args)
            .current_dir(&self.repo_path)
            .output()
            .await
            .map_err(|source| GitManagerError::Spawn {
                command: "gh pr create",
                source,
            })?;
        if !out.status.success() {
            return Err(GitManagerError::CommandFailed {
                command: "gh pr create".to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            });
        }
        let url = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(url)
    }

    /// Enable auto-merge (squash) on the PR for `branch` via the `gh` CLI.
    ///
    /// Used by the L4 (`auto_merge`) autonomy level once the verifier has
    /// passed and the score clears the gate. GitHub merges the PR
    /// automatically once its required checks succeed.
    ///
    /// # Errors
    /// Returns `Err` if `gh pr merge` fails.
    pub async fn auto_merge(&self, branch: &str) -> Result<(), GitManagerError> {
        let args = pr_merge_args(branch);
        let out = tokio::process::Command::new("gh")
            .args(&args)
            .current_dir(&self.repo_path)
            .output()
            .await
            .map_err(|source| GitManagerError::Spawn {
                command: "gh pr merge",
                source,
            })?;
        if !out.status.success() {
            return Err(GitManagerError::CommandFailed {
                command: "gh pr merge".to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
            });
        }
        Ok(())
    }
}

/// Build the `gh pr create` argument vector. Appends `--draft` when `draft`
/// is set. Kept pure (returns the args rather than running them) so the
/// flag logic is unit-testable.
fn pr_create_args(title: &str, body: &str, branch: &str, draft: bool) -> Vec<String> {
    let mut args = vec![
        "pr".to_string(),
        "create".to_string(),
        "--title".to_string(),
        title.to_string(),
        "--body".to_string(),
        body.to_string(),
        "--head".to_string(),
        branch.to_string(),
    ];
    if draft {
        args.push("--draft".to_string());
    }
    args
}

/// Build the `gh pr merge --auto --squash` argument vector for `branch`.
/// Kept pure so the flag set is unit-testable.
fn pr_merge_args(branch: &str) -> Vec<String> {
    vec![
        "pr".to_string(),
        "merge".to_string(),
        branch.to_string(),
        "--auto".to_string(),
        "--squash".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{pr_create_args, pr_merge_args};

    #[test]
    fn pr_create_args_normal_omits_draft_flag() {
        let args = pr_create_args("My Title", "body text", "feat/x", false);
        assert_eq!(
            args,
            vec![
                "pr",
                "create",
                "--title",
                "My Title",
                "--body",
                "body text",
                "--head",
                "feat/x",
            ]
        );
        assert!(!args.iter().any(|a| a == "--draft"));
    }

    #[test]
    fn pr_create_args_draft_appends_draft_flag() {
        let args = pr_create_args("T", "b", "feat/y", true);
        // The draft flag is appended last and the core fields are preserved.
        assert_eq!(args.last().map(String::as_str), Some("--draft"));
        assert!(args.iter().any(|a| a == "--title"));
        assert!(args.iter().any(|a| a == "feat/y"));
        // Exactly one extra arg vs the non-draft form.
        assert_eq!(
            args.len(),
            pr_create_args("T", "b", "feat/y", false).len() + 1
        );
    }

    #[test]
    fn pr_merge_args_uses_auto_squash() {
        let args = pr_merge_args("feat/z");
        assert_eq!(args, vec!["pr", "merge", "feat/z", "--auto", "--squash"]);
    }
}
