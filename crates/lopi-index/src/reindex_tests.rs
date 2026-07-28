#![allow(clippy::unwrap_used)]

use super::reindex;
use crate::store::IndexStore;
use std::process::Command;

    fn init_git_repo(dir: &std::path::Path) {
        let run = |args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(dir)
                .status()
                .unwrap();
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@example.com"]);
        run(&["config", "user.name", "t"]);
    }

    fn commit_all(dir: &std::path::Path, msg: &str) {
        Command::new("git").args(["add", "-A"]).current_dir(dir).status().unwrap();
        Command::new("git")
            .args(["commit", "-q", "-m", msg])
            .current_dir(dir)
            .status()
            .unwrap();
    }

    #[tokio::test]
    async fn full_reindex_finds_a_fixture_symbol() {
        let dir = tempfile::TempDir::new().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("lib.rs"), "pub fn run() {}\n").unwrap();
        commit_all(dir.path(), "init");

        let store = IndexStore::open_in_memory().await.unwrap();
        let delta = reindex(&store, "repo", dir.path()).await.unwrap();
        assert_eq!(delta.files_indexed, 1);
        assert_eq!(delta.symbols_added, 1);
        assert_eq!(store.symbol_count("repo").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn adding_a_symbol_produces_a_positive_delta_on_reindex() {
        let dir = tempfile::TempDir::new().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("lib.rs"), "pub fn run() {}\n").unwrap();
        commit_all(dir.path(), "init");
        let store = IndexStore::open_in_memory().await.unwrap();
        reindex(&store, "repo", dir.path()).await.unwrap();

        std::fs::write(
            dir.path().join("lib.rs"),
            "pub fn run() {}\npub fn stop() {}\n",
        )
        .unwrap();
        commit_all(dir.path(), "add stop");

        let delta = reindex(&store, "repo", dir.path()).await.unwrap();
        assert_eq!(delta.files_indexed, 1, "only the changed file is reparsed");
        assert_eq!(store.symbol_count("repo").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn renaming_a_symbol_replaces_the_old_row() {
        let dir = tempfile::TempDir::new().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("lib.rs"), "pub fn old_name() {}\n").unwrap();
        commit_all(dir.path(), "init");
        let store = IndexStore::open_in_memory().await.unwrap();
        reindex(&store, "repo", dir.path()).await.unwrap();

        std::fs::write(dir.path().join("lib.rs"), "pub fn new_name() {}\n").unwrap();
        commit_all(dir.path(), "rename");
        reindex(&store, "repo", dir.path()).await.unwrap();

        let all = store
            .list("repo", &crate::store::SymbolFilter::default())
            .await
            .unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "new_name");
    }

    #[tokio::test]
    async fn deleting_a_file_removes_its_symbols() {
        let dir = tempfile::TempDir::new().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("a.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(dir.path().join("b.rs"), "pub fn b() {}\n").unwrap();
        commit_all(dir.path(), "init");
        let store = IndexStore::open_in_memory().await.unwrap();
        reindex(&store, "repo", dir.path()).await.unwrap();
        assert_eq!(store.symbol_count("repo").await.unwrap(), 2);

        std::fs::remove_file(dir.path().join("b.rs")).unwrap();
        commit_all(dir.path(), "delete b");
        let delta = reindex(&store, "repo", dir.path()).await.unwrap();
        assert_eq!(delta.files_removed, 1);
        assert_eq!(store.symbol_count("repo").await.unwrap(), 1);
    }

    #[tokio::test]
    async fn dirty_working_tree_is_picked_up_by_hash_mismatch() {
        let dir = tempfile::TempDir::new().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("lib.rs"), "pub fn run() {}\n").unwrap();
        commit_all(dir.path(), "init");
        let store = IndexStore::open_in_memory().await.unwrap();
        reindex(&store, "repo", dir.path()).await.unwrap();

        // Uncommitted change — no new commit, so the git-diff path alone
        // would see nothing; the hash sweep must catch it.
        std::fs::write(
            dir.path().join("lib.rs"),
            "pub fn run() {}\npub fn uncommitted() {}\n",
        )
        .unwrap();
        let delta = reindex(&store, "repo", dir.path()).await.unwrap();
        assert_eq!(delta.files_indexed, 1);
        assert_eq!(store.symbol_count("repo").await.unwrap(), 2);
    }

    #[tokio::test]
    async fn second_reindex_with_no_changes_touches_no_files() {
        let dir = tempfile::TempDir::new().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("lib.rs"), "pub fn run() {}\n").unwrap();
        commit_all(dir.path(), "init");
        let store = IndexStore::open_in_memory().await.unwrap();
        reindex(&store, "repo", dir.path()).await.unwrap();

        let delta = reindex(&store, "repo", dir.path()).await.unwrap();
        assert_eq!(delta.files_indexed, 0, "nothing changed, nothing reparsed");
    }

    /// Regression: a file with zero extracted symbols (a re-export-only
    /// module here) has no `symbols` row to carry a `file_hash`, so before
    /// `files` existed as its own table, this file looked "changed" on
    /// every single incremental pass — reparsed forever, never converging.
    #[tokio::test]
    async fn zero_symbol_file_does_not_get_reindexed_forever() {
        let dir = tempfile::TempDir::new().unwrap();
        init_git_repo(dir.path());
        std::fs::write(dir.path().join("reexport.rs"), "// just a comment, no symbols\n").unwrap();
        std::fs::write(dir.path().join("real.rs"), "pub fn run() {}\n").unwrap();
        commit_all(dir.path(), "init");
        let store = IndexStore::open_in_memory().await.unwrap();
        let first = reindex(&store, "repo", dir.path()).await.unwrap();
        assert_eq!(first.files_indexed, 2, "both files parsed on the first pass");

        // Nothing changed and the tree is clean — a second pass must not
        // re-touch the zero-symbol file (or anything else).
        let second = reindex(&store, "repo", dir.path()).await.unwrap();
        assert_eq!(second.files_indexed, 0, "clean tree: nothing to reparse");

        // Now dirty the tree via an unrelated uncommitted edit — the
        // zero-symbol file still must not spuriously reappear as changed.
        std::fs::write(dir.path().join("real.rs"), "pub fn run() {}\npub fn run2() {}\n").unwrap();
        let third = reindex(&store, "repo", dir.path()).await.unwrap();
        assert_eq!(third.files_indexed, 1, "only the genuinely-edited file is reparsed");
    }

    /// A one-file change on an otherwise-clean, committed tree must not pay
    /// to hash every other file in the repo — that's what `git diff
    /// --name-status` is for. This is the shape the LEDGER's <150ms target
    /// is measured against.
    #[tokio::test]
    async fn clean_tree_one_file_commit_reindexes_only_that_file() {
        let dir = tempfile::TempDir::new().unwrap();
        init_git_repo(dir.path());
        for i in 0..20 {
            std::fs::write(dir.path().join(format!("f{i}.rs")), format!("pub fn f{i}() {{}}\n")).unwrap();
        }
        commit_all(dir.path(), "init");
        let store = IndexStore::open_in_memory().await.unwrap();
        reindex(&store, "repo", dir.path()).await.unwrap();

        std::fs::write(dir.path().join("f0.rs"), "pub fn f0() {}\npub fn f0b() {}\n").unwrap();
        commit_all(dir.path(), "touch f0");
        let delta = reindex(&store, "repo", dir.path()).await.unwrap();
        assert_eq!(delta.files_indexed, 1, "committed one-file change: no repo-wide hash sweep");
    }
