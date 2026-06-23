//! Black-box tests for [`GitManager::rebase_onto`] — the pre-PR rebase that
//! lands a task on an advanced default branch, surfacing conflicts as paths
//! rather than failing silently (Pentad M1.3b).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lopi_git::GitManager;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Repo on `main` with `file.txt`, plus a `feature` branch off the first commit.
/// `main` then advances. The feature branch is left checked out.
fn repo_with_diverged_branches(
    feature_line: &str,
    main_line: &str,
) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let p = dir.path().to_path_buf();
    git(&p, &["init", "-b", "main"]);
    git(&p, &["config", "user.email", "t@konjoai.dev"]);
    git(&p, &["config", "user.name", "tester"]);
    std::fs::write(p.join("file.txt"), "base\n").unwrap();
    git(&p, &["add", "."]);
    git(&p, &["commit", "-m", "base"]);

    // feature: change file.txt off the base commit.
    git(&p, &["checkout", "-b", "feature"]);
    std::fs::write(p.join("file.txt"), feature_line).unwrap();
    git(&p, &["commit", "-am", "feature change"]);

    // main advances with its own change to the same file.
    git(&p, &["checkout", "main"]);
    std::fs::write(p.join("file.txt"), main_line).unwrap();
    git(&p, &["commit", "-am", "main change"]);

    // Back on feature, ready to rebase onto the advanced main.
    git(&p, &["checkout", "feature"]);
    (dir, p)
}

#[tokio::test]
async fn clean_rebase_returns_no_conflicts() {
    // feature edits a *different* file than main → rebases cleanly.
    let dir = TempDir::new().unwrap();
    let repo = dir.path().to_path_buf();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "t@konjoai.dev"]);
    git(&repo, &["config", "user.name", "tester"]);
    std::fs::write(repo.join("file.txt"), "base\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "base"]);

    git(&repo, &["checkout", "-b", "feature"]);
    std::fs::write(repo.join("other.txt"), "feature-only\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "feature other file"]);

    git(&repo, &["checkout", "main"]);
    std::fs::write(repo.join("file.txt"), "main-advanced\n").unwrap();
    git(&repo, &["commit", "-am", "main change"]);
    git(&repo, &["checkout", "feature"]);

    let mgr = GitManager::new(&repo).unwrap();
    let conflicts = mgr.rebase_onto("main").await.unwrap();
    assert!(
        conflicts.is_empty(),
        "expected clean rebase, got {conflicts:?}"
    );
    // feature now sits on top of main's advance: both changes coexist.
    assert_eq!(
        std::fs::read_to_string(repo.join("file.txt")).unwrap(),
        "main-advanced\n"
    );
    assert!(repo.join("other.txt").is_file(), "feature change preserved");
}

#[tokio::test]
async fn conflicting_rebase_reports_paths_and_aborts() {
    // Both branches edit file.txt differently → conflict on rebase.
    let (_d, repo) = repo_with_diverged_branches("feature-change\n", "main-change\n");
    let mgr = GitManager::new(&repo).unwrap();

    let conflicts = mgr.rebase_onto("main").await.unwrap();
    assert_eq!(
        conflicts,
        vec!["file.txt".to_string()],
        "conflict path reported"
    );

    // The rebase was aborted: worktree is clean and back on feature's content.
    let status = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&status.stdout).trim().is_empty(),
        "worktree should be clean after abort"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("file.txt")).unwrap(),
        "feature-change\n"
    );
}

#[tokio::test]
async fn rebase_onto_default_resolves_local_main() {
    // No remote: the best-effort fetch is a no-op and resolve falls back to the
    // local `main`, so a conflicting feature still reports the colliding path.
    let (_d, repo) = repo_with_diverged_branches("feature-change\n", "main-change\n");
    let mgr = GitManager::new(&repo).unwrap();
    let conflicts = mgr.rebase_onto_default().await.unwrap();
    assert_eq!(conflicts, vec!["file.txt".to_string()]);
}

#[tokio::test]
async fn rebase_onto_missing_base_errors() {
    let (_d, repo) = repo_with_diverged_branches("feature-change\n", "main-change\n");
    let mgr = GitManager::new(&repo).unwrap();
    assert!(mgr.rebase_onto("does-not-exist").await.is_err());
}
