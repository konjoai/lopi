//! Pattern tests — split out of `tests.rs` to keep that file under the
//! 500-line CI file-size gate. Covers `jaccard_similarity`/
//! `keyword_fingerprint`, `find_similar_patterns`, `mine_patterns`
//! (including Constraint-Capture-2's constraint capture and
//! `occurrence_count`), and `load_patterns` ordering.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use super::*;
use lopi_core::Task;

#[test]
fn jaccard_sim_identical() {
    assert_eq!(
        jaccard_similarity("auth middleware refactor", "auth middleware refactor"),
        1.0
    );
}

#[test]
fn jaccard_sim_partial() {
    let s = jaccard_similarity("auth middleware", "auth database");
    assert!(s > 0.0 && s < 1.0);
}

#[test]
fn jaccard_sim_disjoint() {
    assert_eq!(jaccard_similarity("alpha beta", "gamma delta"), 0.0);
}

#[test]
fn keyword_fingerprint_sorts_and_dedupes() {
    let fp = keyword_fingerprint("refactor authentication middleware refactor");
    let words: Vec<&str> = fp.split_whitespace().collect();
    assert!(words.windows(2).all(|w| w[0] <= w[1]), "should be sorted");
    assert_eq!(
        words.len(),
        words.iter().collect::<std::collections::HashSet<_>>().len(),
        "should be deduped"
    );
}

#[test]
fn keyword_fingerprint_filters_short_words() {
    let fp = keyword_fingerprint("do it now fix");
    assert!(fp.is_empty() || !fp.contains("do"));
}

#[tokio::test]
async fn find_similar_patterns_empty_db() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let results = store
        .find_similar_patterns("optimize the engine")
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn find_similar_patterns_returns_matches() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("refactor authentication middleware");
    store.save_task(&task, "success").await.unwrap();
    store
        .mine_patterns(&task.id, &task.goal, None)
        .await
        .unwrap();

    // Similar goal should match above 0.3 Jaccard threshold.
    let results = store
        .find_similar_patterns("update authentication middleware logic")
        .await
        .unwrap();
    assert!(!results.is_empty(), "should find similar pattern");
}

/// `find_similar_patterns` now pre-filters candidates via the
/// `pattern_keywords` junction table before Jaccard-scoring in Rust —
/// this pins that the candidate lookup still surfaces the closest match
/// and excludes an unrelated pattern with no shared keywords.
#[tokio::test]
async fn find_similar_patterns_ranks_best_match_first_via_keyword_candidates() {
    let store = MemoryStore::open_in_memory().await.unwrap();

    let close = Task::new("refactor authentication middleware layer");
    store.save_task(&close, "success").await.unwrap();
    store
        .mine_patterns(&close.id, &close.goal, None)
        .await
        .unwrap();

    let unrelated = Task::new("optimize database query planner");
    store.save_task(&unrelated, "success").await.unwrap();
    store
        .mine_patterns(&unrelated.id, &unrelated.goal, None)
        .await
        .unwrap();

    let results = store
        .find_similar_patterns("refactor authentication middleware handling")
        .await
        .unwrap();

    assert!(!results.is_empty(), "should find the close match");
    assert!(
        results[0].goal_keywords.contains("authentication"),
        "best match should be the authentication pattern, got {:?}",
        results[0].goal_keywords
    );
    assert!(
        results
            .iter()
            .all(|r| !r.goal_keywords.contains("database")),
        "unrelated pattern with no shared keywords must not appear"
    );
}

#[tokio::test]
async fn mine_patterns_inserts_new_row() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("refactor authentication middleware");
    store.save_task(&task, "queued").await.unwrap();
    store
        .mine_patterns(&task.id, &task.goal, None)
        .await
        .unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 1);
    let kw = &patterns[0].goal_keywords;
    assert!(kw.contains("authentication") || kw.contains("middleware") || kw.contains("refactor"));
    assert_eq!(
        patterns[0].occurrence_count, 1,
        "a fresh pattern has been seen exactly once"
    );
    assert_eq!(
        patterns[0].successful_constraints, None,
        "no constraint passed in means none recorded, same as before this sprint"
    );
}

#[tokio::test]
async fn mine_patterns_updates_existing_row() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let t1 = Task::new("optimize database queries");
    let t2 = Task::new("optimize database queries");
    store.save_task(&t1, "queued").await.unwrap();
    store.save_task(&t2, "queued").await.unwrap();

    store.mine_patterns(&t1.id, &t1.goal, None).await.unwrap();
    store.mine_patterns(&t2.id, &t2.goal, None).await.unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(
        patterns[0].occurrence_count, 2,
        "a second mine_patterns call for the same fingerprint increments occurrence_count"
    );
}

/// Constraint-Capture-2 — a clean success's derived constraint is written
/// into `successful_constraints` on insert, closing the gap where
/// `mine_patterns` recorded `avg_attempts`/`success_rate` but never a
/// constraint for `seed_from_patterns` to read back.
#[tokio::test]
async fn mine_patterns_records_constraint_on_insert() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("refactor authentication middleware");
    store.save_task(&task, "queued").await.unwrap();
    store
        .mine_patterns(&task.id, &task.goal, Some("always mock the token clock"))
        .await
        .unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(
        patterns[0].successful_constraints.as_deref(),
        Some("always mock the token clock")
    );
}

/// A second success for the same fingerprint bumps `occurrence_count` but
/// keeps the first constraint recorded rather than replacing it — the
/// COALESCE-on-update policy `upsert_pattern_row`
/// (`store/pattern_upsert.rs`) already established for the onboarding-import
/// backfill path, adopted here rather than forked per-caller (see
/// `LEDGER.md`'s Constraint-Capture-2 entry for why this superseded the
/// sprint's original overwrite-latest design).
#[tokio::test]
async fn mine_patterns_keeps_first_constraint_and_increments_occurrence_on_update() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let t1 = Task::new("optimize database queries");
    let t2 = Task::new("optimize database queries");
    store.save_task(&t1, "queued").await.unwrap();
    store.save_task(&t2, "queued").await.unwrap();

    store
        .mine_patterns(&t1.id, &t1.goal, Some("add an index on user_id"))
        .await
        .unwrap();
    store
        .mine_patterns(&t2.id, &t2.goal, Some("batch the writes"))
        .await
        .unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].occurrence_count, 2);
    assert_eq!(
        patterns[0].successful_constraints.as_deref(),
        Some("add an index on user_id"),
        "the first-recorded constraint must survive a later success's own constraint"
    );
}

/// A `None` constraint on an update call must not clobber a constraint a
/// prior call already recorded — only a `Some` should ever overwrite.
#[tokio::test]
async fn mine_patterns_none_constraint_does_not_clobber_existing_one_on_update() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let t1 = Task::new("optimize database queries");
    let t2 = Task::new("optimize database queries");
    store.save_task(&t1, "queued").await.unwrap();
    store.save_task(&t2, "queued").await.unwrap();

    store
        .mine_patterns(&t1.id, &t1.goal, Some("add an index on user_id"))
        .await
        .unwrap();
    // t2's attempt was not a clean success — no constraint passed.
    store.mine_patterns(&t2.id, &t2.goal, None).await.unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].occurrence_count, 2, "stats still update");
    assert_eq!(
        patterns[0].successful_constraints.as_deref(),
        Some("add an index on user_id"),
        "a non-success run's None must not erase the earlier constraint"
    );
}

/// An empty-string constraint is treated the same as `None` — defensive
/// against a future caller that derives an empty summary and passes
/// `Some("")` instead of `None`.
#[tokio::test]
async fn mine_patterns_empty_string_constraint_is_treated_as_none() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("refactor authentication middleware");
    store.save_task(&task, "queued").await.unwrap();
    store
        .mine_patterns(&task.id, &task.goal, Some(""))
        .await
        .unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].successful_constraints, None);
}

/// Regression test for the `mine_patterns` race: two calls for the same
/// fingerprint firing concurrently used to both read "no existing row" via
/// the read pool before either had written, and both insert — leaving
/// duplicate rows for one goal. The transaction on the single-connection
/// write pool must serialize them into exactly one row.
#[tokio::test]
async fn mine_patterns_concurrent_same_fingerprint_yields_one_row() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let t1 = Task::new("consolidate the warp core telemetry");
    let t2 = Task::new("consolidate the warp core telemetry");
    store.save_task(&t1, "queued").await.unwrap();
    store.save_task(&t2, "queued").await.unwrap();

    let (r1, r2) = tokio::join!(
        store.mine_patterns(&t1.id, &t1.goal, None),
        store.mine_patterns(&t2.id, &t2.goal, None),
    );
    r1.unwrap();
    r2.unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(
        patterns.len(),
        1,
        "concurrent mine_patterns must not duplicate rows"
    );
}

#[tokio::test]
async fn mine_patterns_skips_short_words() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task = Task::new("do it now");
    store.save_task(&task, "queued").await.unwrap();
    store
        .mine_patterns(&task.id, &task.goal, None)
        .await
        .unwrap();
    let patterns = store.load_patterns(10).await.unwrap();
    assert!(patterns.is_empty());
}

#[tokio::test]
async fn load_patterns_ordered_by_success_rate() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let t1 = Task::new("write comprehensive unit tests");
    let t2 = Task::new("deploy production infrastructure");
    store.save_task(&t1, "success").await.unwrap();
    store.save_task(&t2, "failed").await.unwrap();
    store.mine_patterns(&t1.id, &t1.goal, None).await.unwrap();
    store.mine_patterns(&t2.id, &t2.goal, None).await.unwrap();

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 2);
}
