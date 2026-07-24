//! Store tests — Onboarding-Import-1 Phase 3/5 backfill path.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use lopi_core::TaskId;

fn item<'a>(session_id: &'a str, goal: &'a str) -> OnboardingPattern<'a> {
    OnboardingPattern {
        session_id,
        project_dir: "/home/user/lopi",
        goal,
        toolchain: Some("rust"),
        successful_constraints: Some("ran cargo test before committing"),
    }
}

#[tokio::test]
async fn backfill_inserts_new_pattern_tagged_onboarding_import() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let outcome = store
        .backfill_onboarding_pattern(&item("session-1", "fix the flaky retry test"))
        .await
        .unwrap();
    let BackfillOutcome::Inserted(id) = outcome else {
        panic!("expected Inserted, got {outcome:?}");
    };

    let row = store.find_pattern_by_id_prefix(&id).await.unwrap().unwrap();
    assert_eq!(row.source, "onboarding_import");
    assert_eq!(row.toolchain.as_deref(), Some("rust"));
    assert_eq!(
        row.successful_constraints.as_deref(),
        Some("ran cargo test before committing")
    );
    assert_eq!(row.success_rate, Some(1.0));
}

#[tokio::test]
async fn backfill_is_idempotent_on_session_id() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let first = store
        .backfill_onboarding_pattern(&item("session-2", "add a rate limiter"))
        .await
        .unwrap();
    assert!(matches!(first, BackfillOutcome::Inserted(_)));

    // Re-running the exact same session (reinstall / new machine) must be a
    // no-op, not a second blend into the same pattern row.
    let second = store
        .backfill_onboarding_pattern(&item("session-2", "add a rate limiter"))
        .await
        .unwrap();
    assert_eq!(second, BackfillOutcome::AlreadyImported);

    let patterns = store.load_patterns(10).await.unwrap();
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].avg_attempts, Some(1.0));
}

#[tokio::test]
async fn backfill_skips_empty_fingerprint_goal() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    // Every token is <= 3 chars, so keyword_fingerprint() returns "".
    let outcome = store
        .backfill_onboarding_pattern(&item("session-3", "fix a bug now ok"))
        .await
        .unwrap();
    assert_eq!(outcome, BackfillOutcome::EmptyFingerprint);
    assert!(store.load_patterns(10).await.unwrap().is_empty());
}

#[tokio::test]
async fn onboarding_session_imported_reflects_the_ledger() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    assert!(!store
        .onboarding_session_imported("session-4")
        .await
        .unwrap());
    store
        .backfill_onboarding_pattern(&item("session-4", "migrate the sqlx pool"))
        .await
        .unwrap();
    assert!(store
        .onboarding_session_imported("session-4")
        .await
        .unwrap());
}

/// A backfill sharing a fingerprint with a pattern already mined from a real
/// lopi task run blends into that row rather than creating a duplicate — but
/// must leave `source` as `'lopi_run'` (first-observed provenance) and only
/// fill `toolchain`/`successful_constraints` because the live-mined row
/// never set them.
#[tokio::test]
async fn backfill_blends_into_a_live_mined_pattern_without_stealing_its_source() {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let task_id = TaskId::new();
    store
        .mine_patterns(&task_id, "refactor the retry backoff logic")
        .await
        .unwrap();

    let before = store.load_patterns(10).await.unwrap();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].source, "lopi_run");
    assert!(before[0].toolchain.is_none());

    let outcome = store
        .backfill_onboarding_pattern(&item("session-5", "refactor the retry backoff logic"))
        .await
        .unwrap();
    assert!(matches!(outcome, BackfillOutcome::Inserted(_)));

    let after = store.load_patterns(10).await.unwrap();
    assert_eq!(after.len(), 1, "must blend, not duplicate");
    assert_eq!(after[0].source, "lopi_run");
    assert_eq!(after[0].toolchain.as_deref(), Some("rust"));
}
