//! A2 (reflection) — durable, rollback-safe learnings: store-level integration
//! tests. Exercises the public `MemoryStore` learning API against an in-memory
//! SQLite db. Kept out of `store/tests.rs` (already at the 500-line ceiling) as
//! its own integration file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use lopi_memory::MemoryStore;

#[tokio::test]
async fn save_learning_persists_regardless_of_score() {
    // The whole point of A2: a rejected/rolled-back attempt (which by definition
    // scored low) must still yield its lesson — no quality gate drops it.
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .save_learning(
            "/repo",
            "fix the auth timeout",
            "token TTL was set to 0",
            "shortened the retry window",
            "eval_rejected",
            Some("t1"),
        )
        .await
        .unwrap();
    let rows = store.load_learnings("/repo", 10).await.unwrap();
    assert_eq!(
        rows.len(),
        1,
        "a failure learning must persist unconditionally"
    );
    assert_eq!(rows[0].critique, "token TTL was set to 0");
    assert_eq!(rows[0].outcome, "eval_rejected");
}

#[tokio::test]
async fn learning_survives_a_simulated_rollback() {
    // git rollback discards the working tree, not SQLite. Capture-before-rollback
    // is proven by writing the learning, then doing work that mimics the rollback
    // path (no store mutation), and reading it back intact.
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .save_learning(
            "/repo",
            "add caching",
            "cache key collided",
            "",
            "non_gaining",
            None,
        )
        .await
        .unwrap();
    // ... attempt is rolled back here (git-only; the store is untouched) ...
    let rows = store.load_learnings("/repo", 10).await.unwrap();
    assert_eq!(rows.len(), 1, "the lesson outlives the rolled-back attempt");
}

#[tokio::test]
async fn save_learning_dedups_identical_critique() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    for _ in 0..3 {
        store
            .save_learning(
                "/repo",
                "same goal",
                "same critique",
                "",
                "non_gaining",
                None,
            )
            .await
            .unwrap();
    }
    let rows = store.load_learnings("/repo", 10).await.unwrap();
    assert_eq!(
        rows.len(),
        1,
        "repeated identical critiques collapse to one row"
    );
}

#[tokio::test]
async fn empty_critique_is_not_stored() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .save_learning("/repo", "goal", "   ", "tried x", "eval_rejected", None)
        .await
        .unwrap();
    assert!(store.load_learnings("/repo", 10).await.unwrap().is_empty());
}

#[tokio::test]
async fn find_relevant_learnings_matches_topic_and_skips_unrelated() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    store
        .save_learning(
            "/repo",
            "fix authentication token timeout",
            "auth token expired too early",
            "",
            "eval_rejected",
            None,
        )
        .await
        .unwrap();
    store
        .save_learning(
            "/repo",
            "optimize image rendering pipeline",
            "image cache was undersized",
            "",
            "non_gaining",
            None,
        )
        .await
        .unwrap();

    let hits = store
        .find_relevant_learnings("/repo", "authentication token timeout bug", 5)
        .await
        .unwrap();
    assert_eq!(hits.len(), 1, "only the on-topic learning is retrieved");
    assert_eq!(hits[0].critique, "auth token expired too early");

    let miss = store
        .find_relevant_learnings("/repo", "render svg charts faster", 5)
        .await
        .unwrap();
    assert!(
        miss.iter()
            .all(|r| r.critique != "auth token expired too early"),
        "an unrelated goal must not surface the auth learning"
    );
}

#[tokio::test]
async fn find_relevant_learnings_caps_injection() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    for i in 0..6 {
        store
            .save_learning(
                "/repo",
                "refactor the database migration runner",
                &format!("distinct critique {i}"),
                "",
                "non_gaining",
                None,
            )
            .await
            .unwrap();
    }
    let hits = store
        .find_relevant_learnings("/repo", "refactor the database migration runner", 2)
        .await
        .unwrap();
    assert_eq!(hits.len(), 2, "the injection cap is a hard bound");
}
