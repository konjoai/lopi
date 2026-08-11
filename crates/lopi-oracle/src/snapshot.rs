use anyhow::{Context, Result};
use std::path::Path;

/// One side of a pairwise collision check.
///
/// `commit_ish` is resolved fresh by git on every poll (a branch name, a SHA,
/// or `HEAD`), so the oracle always reads a worktree's current tip rather
/// than a commit cached at registration time.
#[derive(Debug, Clone)]
pub struct WatchedRef {
    /// Human-readable identity for alert text — a task id or branch name.
    pub label: String,
    /// Commit-ish git should snapshot: a branch name, SHA, or `HEAD`.
    pub commit_ish: String,
}

impl WatchedRef {
    /// Build a watched ref from a label and a commit-ish.
    #[must_use]
    pub fn new(label: impl Into<String>, commit_ish: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            commit_ish: commit_ish.into(),
        }
    }
}

/// A textual collision between two watched refs: the files git's three-way
/// merge could not auto-resolve, and the merge-base both sides diverged from.
#[derive(Debug, Clone)]
pub struct Collision {
    /// Files git flagged with a `CONFLICT` marker.
    pub files: Vec<String>,
    /// Merge-base commit of the two refs, at check time.
    pub merge_base: String,
}

/// Snapshot the merge of two refs with `git merge-tree --write-tree` — the
/// exact invocation KT-3 timed (p95 135ms on `lopi` itself; see
/// `KILL_TEST_REGISTER.md`). Never mutates the repo: `--write-tree` writes a
/// loose tree object but touches no ref and no working tree.
///
/// Returns `Ok(None)` when git merges the pair cleanly, `Ok(Some(_))` with
/// the conflicted files otherwise.
///
/// # Errors
/// Returns `Err` if `git` cannot be spawned, or exits with a code other than
/// `0` (clean merge) or `1` (conflict) — e.g. an unresolvable ref.
pub async fn snapshot_pair(
    repo_path: &Path,
    a: &WatchedRef,
    b: &WatchedRef,
) -> Result<Option<Collision>> {
    let merge_base = run_git(
        repo_path,
        &["merge-base", a.commit_ish.as_str(), b.commit_ish.as_str()],
    )
    .await
    .with_context(|| format!("git merge-base {} {}", a.commit_ish, b.commit_ish))?
    .trim()
    .to_string();

    let out = tokio::process::Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args([
            "merge-tree",
            "--write-tree",
            a.commit_ish.as_str(),
            b.commit_ish.as_str(),
        ])
        .output()
        .await
        .context("invoking git merge-tree")?;

    match out.status.code() {
        Some(0) => Ok(None),
        Some(1) => Ok(Some(Collision {
            files: parse_conflicted_files(&String::from_utf8_lossy(&out.stdout)),
            merge_base,
        })),
        _ => anyhow::bail!(
            "git merge-tree {} {} failed: {}",
            a.commit_ish,
            b.commit_ish,
            String::from_utf8_lossy(&out.stderr)
        ),
    }
}

/// Extract conflicted file paths from `git merge-tree --write-tree`'s
/// informational-message section (`CONFLICT (...): ... in <path>` lines).
/// Pure so it is unit-testable against fixed real output without spawning
/// git.
fn parse_conflicted_files(stdout: &str) -> Vec<String> {
    let mut files: Vec<String> = stdout
        .lines()
        .filter(|line| line.starts_with("CONFLICT"))
        .filter_map(|line| line.rsplit(" in ").next())
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Run `git -C <repo> <args>` and return its stdout as a `String`.
///
/// Mirrors `lopi-git`'s own `run_git_stdout` helper
/// (`crates/lopi-git/src/worktree.rs`) — this crate does not depend on
/// `lopi-git` (it needs no worktree lifecycle, only read-only snapshots), so
/// the small helper is kept local rather than pulling in the whole crate for
/// one function.
async fn run_git(repo: &Path, args: &[&str]) -> Result<String> {
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
mod tests {
    use super::*;

    #[test]
    fn parses_a_single_conflict_from_real_output() {
        let stdout = "\
8ebab0410b30f300807f3fba20eac84f853dc9bc
100644 a29bdeb434d874c9b1d8969c40c42161b03fafdc 1\tf.txt
100644 6ef3d0cdf40acdf4e18ea8686f5da7390149ac30 2\tf.txt
100644 bf72511342bd7096b6ad44aa015fd2eea4389543 3\tf.txt

Auto-merging f.txt
CONFLICT (content): Merge conflict in f.txt
";
        assert_eq!(parse_conflicted_files(stdout), vec!["f.txt".to_string()]);
    }

    #[test]
    fn parses_multiple_conflicts_and_dedupes() {
        let stdout = "\
tree-oid
Auto-merging CHANGELOG.md
CONFLICT (content): Merge conflict in CHANGELOG.md
Auto-merging LEDGER.md
CONFLICT (content): Merge conflict in LEDGER.md
CONFLICT (content): Merge conflict in LEDGER.md
";
        assert_eq!(
            parse_conflicted_files(stdout),
            vec!["CHANGELOG.md".to_string(), "LEDGER.md".to_string()]
        );
    }

    #[test]
    fn clean_output_yields_no_conflicts() {
        assert_eq!(
            parse_conflicted_files("tree-oid-only\n"),
            Vec::<String>::new()
        );
    }
}
