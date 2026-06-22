#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use std::process::Command;
use tempfile::TempDir;

// ── Pure helpers ────────────────────────────────────────────────────────────

#[test]
fn slug_flattens_path_separators() {
    assert_eq!(worktree_slug("abc123", 1), "abc123-1");
    assert_eq!(worktree_slug("group/task", 2), "group-task-2");
    assert_eq!(worktree_slug("a\\b", 3), "a-b-3");
}

#[test]
fn add_args_use_new_branch_flag() {
    let args = add_args(Path::new("/wt/x-1"), "lopi/x-attempt-1");
    assert_eq!(
        args,
        vec!["worktree", "add", "/wt/x-1", "-b", "lopi/x-attempt-1"]
    );
}

#[test]
fn remove_args_force_to_discard_throwaway_changes() {
    let args = remove_args(Path::new("/wt/x-1"));
    assert_eq!(args, vec!["worktree", "remove", "--force", "/wt/x-1"]);
}

#[test]
fn parse_worktree_paths_reads_porcelain_records() {
    let porcelain = "\
worktree /repo
HEAD abc
branch refs/heads/main

worktree /repo/.lopi/worktrees/t-1
HEAD def
branch refs/heads/lopi/t-attempt-1
";
    let paths = parse_worktree_paths(porcelain);
    assert_eq!(
        paths,
        vec![
            PathBuf::from("/repo"),
            PathBuf::from("/repo/.lopi/worktrees/t-1"),
        ]
    );
}

// ── Integration: a real throwaway repo ──────────────────────────────────────

/// Run `git -C <repo> <args>` synchronously and assert success (test setup).
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

/// Create a temp repo on `main` with one commit (worktree add needs a HEAD).
fn init_repo() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().to_path_buf();
    git(&path, &["init", "-b", "main"]);
    git(&path, &["config", "user.email", "t@konjoai.dev"]);
    git(&path, &["config", "user.name", "tester"]);
    std::fs::write(path.join("README.md"), "hi").unwrap();
    git(&path, &["add", "."]);
    git(&path, &["commit", "-m", "init"]);
    (dir, path)
}

#[tokio::test]
async fn add_creates_checkout_and_cleanup_removes_it() {
    let (_dir, repo) = init_repo();
    let mgr = WorktreeManager::new(&repo).unwrap();

    let wt = mgr.add("task-a", 1, "lopi/task-a-attempt-1").await.unwrap();
    assert!(wt.path().is_dir(), "checkout dir should exist");
    assert!(wt.path().join("README.md").is_file(), "files checked out");
    assert_eq!(wt.branch(), "lopi/task-a-attempt-1");

    // git tracks both the main worktree and the new one.
    let listed = mgr.list().await.unwrap();
    assert!(listed.iter().any(|p| p == wt.path()));

    wt.cleanup().await.unwrap();
    assert!(!wt.path().exists(), "checkout removed");
    // cleanup is idempotent.
    wt.cleanup().await.unwrap();

    let after = mgr.list().await.unwrap();
    assert!(!after.iter().any(|p| p == wt.path()));
}

#[tokio::test]
async fn env_sets_worktree_local_cargo_target() {
    let (_dir, repo) = init_repo();
    let mgr = WorktreeManager::new(&repo).unwrap();
    let wt = mgr.add("task-env", 1, "lopi/env-1").await.unwrap();
    let env = wt.env();
    assert_eq!(env.len(), 1);
    assert_eq!(env[0].0, "CARGO_TARGET_DIR");
    assert!(env[0].1.ends_with(".cargo-target"));
    assert!(env[0].1.starts_with(wt.path().to_string_lossy().as_ref()));
    wt.cleanup().await.unwrap();
}

#[tokio::test]
async fn drop_without_cleanup_removes_checkout() {
    let (_dir, repo) = init_repo();
    let mgr = WorktreeManager::new(&repo).unwrap();
    let leaked_path;
    {
        let wt = mgr.add("task-drop", 1, "lopi/drop-1").await.unwrap();
        leaked_path = wt.path().to_path_buf();
        assert!(leaked_path.is_dir());
        // wt dropped here without an explicit cleanup() → Drop must reap it.
    }
    assert!(!leaked_path.exists(), "Drop should remove the checkout");
    // Prune any admin entry the synchronous Drop left, then assert no leak.
    mgr.prune().await.unwrap();
    let listed = mgr.list().await.unwrap();
    assert!(!listed.iter().any(|p| p == &leaked_path));
}

#[tokio::test]
async fn add_rejects_duplicate_branch() {
    let (_dir, repo) = init_repo();
    let mgr = WorktreeManager::new(&repo).unwrap();
    let wt = mgr.add("dup", 1, "lopi/dup").await.unwrap();
    // Same branch name in a second worktree must fail (git refuses).
    let err = mgr.add("dup", 2, "lopi/dup").await;
    assert!(err.is_err(), "duplicate branch should be rejected");
    wt.cleanup().await.unwrap();
}

/// The Sprint 1.1 DoD property: N concurrent add/remove cycles leave zero
/// orphan directories and zero `git worktree list` leaks.
#[tokio::test]
async fn concurrent_add_remove_cycles_leave_no_leaks() {
    let (_dir, repo) = init_repo();
    let mgr = Arc::new(WorktreeManager::new(&repo).unwrap());

    let mut handles = Vec::new();
    for i in 0..8u32 {
        let mgr = Arc::clone(&mgr);
        handles.push(tokio::spawn(async move {
            let branch = format!("lopi/conc-{i}");
            let wt = mgr.add(&format!("conc-{i}"), i, &branch).await.unwrap();
            assert!(wt.path().is_dir());
            // Touch a file to mimic real work in the isolated checkout.
            tokio::fs::write(wt.path().join("work.txt"), format!("{i}"))
                .await
                .unwrap();
            wt.cleanup().await.unwrap();
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    mgr.prune().await.unwrap();

    // Only the main working tree should remain tracked.
    let listed = mgr.list().await.unwrap();
    assert_eq!(
        listed.len(),
        1,
        "only the main worktree remains: {listed:?}"
    );

    // The worktree root holds no orphan checkout directories.
    let root = mgr.root();
    if root.exists() {
        let mut entries = tokio::fs::read_dir(&root).await.unwrap();
        let mut leftover = Vec::new();
        while let Some(e) = entries.next_entry().await.unwrap() {
            leftover.push(e.file_name());
        }
        assert!(leftover.is_empty(), "orphan worktree dirs: {leftover:?}");
    }
}
