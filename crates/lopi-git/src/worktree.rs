//! True git-worktree isolation for parallel agent runs.
//!
//! Where [`GitManager`](crate::GitManager) checks out a branch in the *shared*
//! working directory (forcing concurrent runs to serialize on a process-wide
//! lock), a [`WorktreeManager`] gives every run its own physical checkout under
//! `<repo>/.lopi/worktrees/`. Two agents in two worktrees can build, test, and
//! commit simultaneously without touching each other's files — the isolation
//! loop-engineering actually asks for.
//!
//! Lifecycle is driven through the `git` CLI (consistent with
//! [`GitManager::push_branch`](crate::GitManager::push_branch)): `git worktree
//! add`, `remove`, and `prune`. Each [`Worktree`] is an RAII handle: dropping it
//! removes its checkout, so a panicking attempt cannot leak a checkout or a
//! dangling `git worktree list` entry.

use anyhow::{Context, Result};
use git2::Repository;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;

/// Directory, relative to the repo root, that holds all lopi worktrees.
const WORKTREE_ROOT: &str = ".lopi/worktrees";

/// Serializes the *metadata-mutating* worktree ops (`add`/`remove`/`prune`).
///
/// `git worktree add` and `prune` both touch the shared `.git/worktrees`
/// admin area; running them concurrently can race (a `prune` reaping an entry a
/// parallel `add` is still writing). The lock is held only for these short
/// git invocations — the agent *work* inside each worktree runs fully parallel,
/// which is the whole point of worktrees.
static WT_META_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// Manages the lifecycle of `git worktree` checkouts for isolated agent runs.
pub struct WorktreeManager {
    repo_path: PathBuf,
}

impl WorktreeManager {
    /// Create a manager for the repository at `repo_path`.
    ///
    /// # Errors
    /// Returns `Err` if the path is not a valid git repository.
    pub fn new(repo_path: impl AsRef<Path>) -> Result<Self> {
        let p = repo_path.as_ref().to_path_buf();
        let _ =
            Repository::open(&p).with_context(|| format!("opening git repo at {}", p.display()))?;
        Ok(Self { repo_path: p })
    }

    /// Absolute path of the worktree root (`<repo>/.lopi/worktrees`).
    #[must_use]
    pub fn root(&self) -> PathBuf {
        self.repo_path.join(WORKTREE_ROOT)
    }

    /// Add a worktree for `task_id` / `attempt`, checked out on a fresh `branch`.
    ///
    /// The checkout lives at `<root>/<task_id>-<attempt>`. The returned
    /// [`Worktree`] removes it on drop; call [`Worktree::cleanup`] for the
    /// async, error-surfacing path.
    ///
    /// # Errors
    /// Returns `Err` if `git worktree add` fails (e.g. the branch already exists
    /// at a different commit, or the path is occupied).
    pub async fn add(&self, task_id: &str, attempt: u32, branch: &str) -> Result<Worktree> {
        let slug = worktree_slug(task_id, attempt);
        let path = self.root().join(&slug);
        self.ensure_parent(&path).await?;
        let args = add_args(&path, branch);
        {
            let _guard = WT_META_LOCK.lock().await;
            run_git(&self.repo_path, &args)
                .await
                .with_context(|| format!("git worktree add for branch {branch}"))?;
        }
        Ok(Worktree::new(
            self.repo_path.clone(),
            path,
            branch.to_string(),
        ))
    }

    /// Add a **detached** worktree for `task_id` at the repo's current `HEAD`.
    ///
    /// Unlike [`add`](WorktreeManager::add) this checks out no named branch: the
    /// caller (the agent runner) creates its own per-attempt branches *inside*
    /// the worktree. This is the per-task isolation used at the pool layer — one
    /// physical checkout per running task. The returned [`Worktree`] has an empty
    /// [`branch`](Worktree::branch) and is removed on drop.
    ///
    /// # Errors
    /// Returns `Err` if `git worktree add --detach` fails (e.g. the path is
    /// occupied by a stale checkout).
    pub async fn add_detached(&self, task_id: &str) -> Result<Worktree> {
        let path = self.root().join(sanitize(task_id));
        self.ensure_parent(&path).await?;
        let args = add_detached_args(&path);
        {
            let _guard = WT_META_LOCK.lock().await;
            run_git(&self.repo_path, &args)
                .await
                .with_context(|| format!("git worktree add --detach for {task_id}"))?;
        }
        Ok(Worktree::new(self.repo_path.clone(), path, String::new()))
    }

    /// Create the worktree's parent directory if needed (`git worktree add`
    /// requires the parent to exist but the leaf not to).
    async fn ensure_parent(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("creating worktree root {}", parent.display()))?;
        }
        Ok(())
    }

    /// Prune administrative entries for worktrees whose directories are gone.
    ///
    /// # Errors
    /// Returns `Err` if `git worktree prune` fails.
    pub async fn prune(&self) -> Result<()> {
        let _guard = WT_META_LOCK.lock().await;
        run_git(&self.repo_path, &["worktree".into(), "prune".into()]).await
    }

    /// List the absolute paths of all worktrees git currently tracks, including
    /// the repo's own main working tree.
    ///
    /// # Errors
    /// Returns `Err` if `git worktree list` fails.
    pub async fn list(&self) -> Result<Vec<PathBuf>> {
        let out = run_git_stdout(
            &self.repo_path,
            &["worktree".into(), "list".into(), "--porcelain".into()],
        )
        .await?;
        Ok(parse_worktree_paths(&out))
    }

    /// Reclaim throwaway worktrees and branches left behind by finished tasks.
    ///
    /// Prunes git's worktree admin entries, removes any orphan checkout dir
    /// under the root that git no longer tracks (a cleanup that died mid-flight),
    /// and deletes local branches matching `branch_prefix` that are not checked
    /// out in any live worktree — lopi's per-attempt branches are ephemeral, the
    /// pushed PR is the durable artifact, so an idle one is garbage. Backs
    /// `lopi worktree gc`.
    ///
    /// Individual stuck entries are logged and skipped, never failing the whole
    /// sweep; the returned [`GcReport`] counts what was reclaimed.
    ///
    /// # Errors
    /// Returns `Err` only if the initial `git worktree prune`/`list` fails.
    pub async fn gc(&self, branch_prefix: &str) -> Result<GcReport> {
        let _guard = WT_META_LOCK.lock().await;
        run_git(&self.repo_path, &["worktree".into(), "prune".into()]).await?;
        let porcelain = run_git_stdout(
            &self.repo_path,
            &["worktree".into(), "list".into(), "--porcelain".into()],
        )
        .await?;
        let live_paths = parse_worktree_paths(&porcelain);
        let live_branches = parse_worktree_branches(&porcelain);
        let worktrees_removed = self.remove_orphan_dirs(&live_paths).await;
        let branches_removed = self
            .delete_stale_branches(branch_prefix, &live_branches)
            .await?;
        Ok(GcReport {
            worktrees_removed,
            branches_removed,
        })
    }

    /// Remove dirs under the worktree root that git no longer tracks. Matches by
    /// leaf name (git reports absolute paths; ours may be relative). Returns the
    /// count removed; a dir that won't delete is logged and left for next time.
    async fn remove_orphan_dirs(&self, live: &[PathBuf]) -> usize {
        use std::collections::HashSet;
        let live_names: HashSet<_> = live.iter().filter_map(|p| p.file_name()).collect();
        let Ok(mut entries) = tokio::fs::read_dir(self.root()).await else {
            return 0; // no root yet → nothing to reclaim
        };
        let mut removed = 0;
        while let Ok(Some(entry)) = entries.next_entry().await {
            if live_names.contains(entry.file_name().as_os_str()) {
                continue;
            }
            if let Err(e) = tokio::fs::remove_dir_all(entry.path()).await {
                tracing::warn!(path = %entry.path().display(), "orphan worktree remove failed: {e}");
            } else {
                removed += 1;
            }
        }
        removed
    }

    /// Delete local branches matching `prefix` that are not checked out in any
    /// live worktree. Returns the count deleted.
    async fn delete_stale_branches(&self, prefix: &str, live: &[String]) -> Result<usize> {
        let listed = run_git_stdout(&self.repo_path, &for_each_ref_args()).await?;
        let mut removed = 0;
        for name in listed
            .lines()
            .map(str::trim)
            .filter(|n| n.starts_with(prefix) && !live.iter().any(|b| b == n))
        {
            match run_git(&self.repo_path, &branch_delete_args(name)).await {
                Ok(()) => removed += 1,
                Err(e) => tracing::warn!(branch = name, "branch gc delete failed: {e}"),
            }
        }
        Ok(removed)
    }
}

/// What a [`WorktreeManager::gc`] sweep reclaimed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct GcReport {
    /// Orphaned worktree directories removed from under the root.
    pub worktrees_removed: usize,
    /// Stale `lopi/*` branches deleted from the local ref store.
    pub branches_removed: usize,
}

/// An RAII handle to a live `git worktree` checkout.
///
/// Dropping a `Worktree` removes its checkout best-effort (synchronously, since
/// `Drop` cannot be async) so a panicking run never leaks a directory. Prefer
/// [`Worktree::cleanup`] when you can `await`: it removes the checkout, prunes,
/// and surfaces any error instead of only logging it.
pub struct Worktree {
    repo_path: PathBuf,
    path: PathBuf,
    branch: String,
    /// Cleared once the checkout has been removed, so `Drop` is a no-op after an
    /// explicit [`cleanup`](Worktree::cleanup) and double-removal can't happen.
    armed: Arc<AtomicBool>,
}

impl Worktree {
    fn new(repo_path: PathBuf, path: PathBuf, branch: String) -> Self {
        Self {
            repo_path,
            path,
            branch,
            armed: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Absolute path of this worktree's checkout.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The branch checked out in this worktree, or empty for a detached
    /// worktree created via [`WorktreeManager::add_detached`].
    #[must_use]
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Env-var overrides for sub-processes run inside this worktree.
    ///
    /// Each worktree gets its own absolute `CARGO_TARGET_DIR` so parallel
    /// `cargo build`/`cargo test` invocations never contend on one `target/`.
    #[must_use]
    pub fn env(&self) -> Vec<(String, String)> {
        let target = self.path.join(".cargo-target");
        vec![(
            "CARGO_TARGET_DIR".to_string(),
            target.to_string_lossy().into_owned(),
        )]
    }

    /// Remove this worktree's checkout and prune the admin entry.
    ///
    /// Disarms the `Drop` fallback on success, so calling this is the clean,
    /// observable path. Idempotent: a second call is a no-op.
    ///
    /// # Errors
    /// Returns `Err` if `git worktree remove` fails for a reason other than the
    /// checkout already being gone.
    pub async fn cleanup(&self) -> Result<()> {
        if !self.armed.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        let _guard = WT_META_LOCK.lock().await;
        let args = remove_args(&self.path);
        if let Err(e) = run_git(&self.repo_path, &args).await {
            // Re-arm so a later drop still attempts cleanup if this was transient.
            self.armed.store(true, Ordering::SeqCst);
            return Err(e).context("git worktree remove");
        }
        run_git(&self.repo_path, &["worktree".into(), "prune".into()]).await
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        if !self.armed.swap(false, Ordering::SeqCst) {
            return;
        }
        // Best-effort synchronous cleanup — `Drop` cannot await. Removal is a
        // fast local filesystem + git-metadata op. A failure here is logged,
        // never silently swallowed (per the no-silent-failures rule); `prune`
        // on the next run reclaims anything left behind.
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.repo_path)
            .args(remove_args(&self.path))
            .output();
        match out {
            Ok(o) if o.status.success() => {}
            Ok(o) => tracing::warn!(
                path = %self.path.display(),
                stderr = %String::from_utf8_lossy(&o.stderr),
                "worktree drop cleanup failed; prune will reclaim it"
            ),
            Err(e) => tracing::warn!(
                path = %self.path.display(),
                error = %e,
                "worktree drop cleanup could not spawn git"
            ),
        }
    }
}

/// Flatten path separators in a task id so it is safe as a single dir name.
fn sanitize(task_id: &str) -> String {
    task_id
        .chars()
        .map(|c| if c == '/' || c == '\\' { '-' } else { c })
        .collect()
}

/// Filesystem-safe leaf name for a per-attempt worktree: `<task_id>-<attempt>`.
fn worktree_slug(task_id: &str, attempt: u32) -> String {
    format!("{}-{attempt}", sanitize(task_id))
}

/// Build the `git worktree add <path> -b <branch>` argument vector. Kept pure
/// so the flag set is unit-testable.
fn add_args(path: &Path, branch: &str) -> Vec<String> {
    vec![
        "worktree".to_string(),
        "add".to_string(),
        path.to_string_lossy().into_owned(),
        "-b".to_string(),
        branch.to_string(),
    ]
}

/// Build the `git worktree add --detach <path>` argument vector — a worktree at
/// `HEAD` with no named branch. Kept pure so the flag set is unit-testable.
fn add_detached_args(path: &Path) -> Vec<String> {
    vec![
        "worktree".to_string(),
        "add".to_string(),
        "--detach".to_string(),
        path.to_string_lossy().into_owned(),
    ]
}

/// Build the `git worktree remove --force <path>` argument vector. `--force`
/// discards any uncommitted changes in the throwaway checkout. Kept pure.
fn remove_args(path: &Path) -> Vec<String> {
    vec![
        "worktree".to_string(),
        "remove".to_string(),
        "--force".to_string(),
        path.to_string_lossy().into_owned(),
    ]
}

/// Parse `git worktree list --porcelain` output into worktree paths. Each
/// record begins with a `worktree <abs-path>` line.
fn parse_worktree_paths(porcelain: &str) -> Vec<PathBuf> {
    porcelain
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect()
}

/// Parse the short branch names currently checked out across worktrees from
/// `git worktree list --porcelain` (records carry `branch refs/heads/<name>`).
fn parse_worktree_branches(porcelain: &str) -> Vec<String> {
    porcelain
        .lines()
        .filter_map(|l| l.strip_prefix("branch refs/heads/"))
        .map(str::to_string)
        .collect()
}

/// Build `git for-each-ref --format=%(refname:short) refs/heads/` — lists local
/// branch short names, one per line. Kept pure for unit-testability.
fn for_each_ref_args() -> Vec<String> {
    vec![
        "for-each-ref".to_string(),
        "--format=%(refname:short)".to_string(),
        "refs/heads/".to_string(),
    ]
}

/// Build the `git branch -D <name>` (force-delete) argument vector. Kept pure.
fn branch_delete_args(name: &str) -> Vec<String> {
    vec!["branch".to_string(), "-D".to_string(), name.to_string()]
}

/// Run `git -C <repo> <args>`, returning `Err` with stderr on a non-zero exit.
async fn run_git(repo: &Path, args: &[String]) -> Result<()> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await
        .context("invoking git")?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

/// Run `git -C <repo> <args>` and return its stdout as a `String`.
async fn run_git_stdout(repo: &Path, args: &[String]) -> Result<String> {
    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .await
        .context("invoking git")?;
    if !out.status.success() {
        anyhow::bail!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests;
