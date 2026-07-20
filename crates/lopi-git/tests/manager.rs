//! Black-box tests for `GitManager`'s state-mutating operations —
//! `checkout_new_branch`, `check_diff_scope`, `hard_rollback`,
//! `checkout_default`, and `commit_all` — none of which had direct test
//! coverage before this file.
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

/// A fresh repo on `main` with one committed file.
fn init_repo() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let repo = dir.path().to_path_buf();
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["config", "user.email", "t@konjoai.dev"]);
    git(&repo, &["config", "user.name", "tester"]);
    std::fs::write(repo.join("file.txt"), "base\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "base"]);
    (dir, repo)
}

fn current_branch(repo: &Path) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .expect("spawn git");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[tokio::test]
async fn checkout_new_branch_creates_and_switches() {
    let (_d, repo) = init_repo();
    let mgr = GitManager::new(&repo).unwrap();
    mgr.checkout_new_branch("feature/x").await.unwrap();
    assert_eq!(current_branch(&repo), "feature/x");
}

#[tokio::test]
async fn checkout_new_branch_is_idempotent_on_existing_branch() {
    let (_d, repo) = init_repo();
    let mgr = GitManager::new(&repo).unwrap();
    mgr.checkout_new_branch("feature/x").await.unwrap();
    // Calling again for the same branch (now checked out) must not error.
    mgr.checkout_new_branch("feature/x").await.unwrap();
    assert_eq!(current_branch(&repo), "feature/x");
}

#[tokio::test]
async fn commit_all_stages_and_commits_working_tree_changes() {
    let (_d, repo) = init_repo();
    let mgr = GitManager::new(&repo).unwrap();
    let before = mgr.head_oid().unwrap();

    std::fs::write(repo.join("file.txt"), "changed\n").unwrap();
    std::fs::write(repo.join("new.txt"), "new file\n").unwrap();
    let oid = mgr.commit_all("agent change").await.unwrap();

    assert_ne!(oid, before, "a new commit was created");
    assert_eq!(mgr.head_oid().unwrap(), oid);
    // Working tree is clean after the commit — everything was staged.
    let status = Command::new("git")
        .arg("-C")
        .arg(&repo)
        .args(["status", "--porcelain"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&status.stdout).trim().is_empty(),
        "working tree should be clean after commit_all"
    );
}

#[tokio::test]
async fn hard_rollback_discards_tracked_and_untracked_changes() {
    let (_d, repo) = init_repo();
    let mgr = GitManager::new(&repo).unwrap();

    std::fs::write(repo.join("file.txt"), "dirty\n").unwrap();
    std::fs::write(repo.join("untracked.txt"), "scratch\n").unwrap();
    assert!(repo.join("untracked.txt").is_file());

    mgr.hard_rollback().await.unwrap();

    assert_eq!(
        std::fs::read_to_string(repo.join("file.txt")).unwrap(),
        "base\n",
        "tracked change reverted to HEAD"
    );
    assert!(
        !repo.join("untracked.txt").exists(),
        "untracked file removed"
    );
}

#[tokio::test]
async fn checkout_default_returns_to_main() {
    let (_d, repo) = init_repo();
    let mgr = GitManager::new(&repo).unwrap();
    mgr.checkout_new_branch("feature/x").await.unwrap();
    assert_eq!(current_branch(&repo), "feature/x");

    mgr.checkout_default().await.unwrap();
    assert_eq!(current_branch(&repo), "main");
}

#[tokio::test]
async fn check_diff_scope_allows_changes_inside_scope() {
    let (_d, repo) = init_repo();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/lib.rs"), "// change\n").unwrap();
    let mgr = GitManager::new(&repo).unwrap();
    mgr.check_diff_scope(&["src/".to_string()], &[])
        .await
        .unwrap();
}

#[tokio::test]
async fn check_diff_scope_rejects_changes_outside_scope() {
    let (_d, repo) = init_repo();
    std::fs::create_dir_all(repo.join("infra")).unwrap();
    std::fs::write(repo.join("infra/main.tf"), "// change\n").unwrap();
    // diff_tree_to_workdir_with_index diffs against the index, so a brand
    // new file must be staged to appear in the diff at all.
    git(&repo, &["add", "."]);
    let mgr = GitManager::new(&repo).unwrap();
    let err = mgr
        .check_diff_scope(&["src/".to_string()], &[])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("infra/main.tf"), "{err}");
}

#[tokio::test]
async fn check_diff_scope_rejects_forbidden_paths() {
    let (_d, repo) = init_repo();
    std::fs::create_dir_all(repo.join(".github")).unwrap();
    std::fs::write(repo.join(".github/workflows.yml"), "// change\n").unwrap();
    git(&repo, &["add", "."]);
    let mgr = GitManager::new(&repo).unwrap();
    let err = mgr
        .check_diff_scope(&[], &[".github/".to_string()])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("forbidden"), "{err}");
}
