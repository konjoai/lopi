//! Phase 16.7 — earned-trust ledger persistence.
//!
//! Persists one [`EarnedTrust`] state per scope (a schedule id or repo path)
//! and applies the pure transitions from `lopi-core::earned_trust` as runs land:
//! a clean verifier-passed run advances the streak (and may promote), a failed
//! run breaks the streak, and a post-merge revert demotes. The autonomy level a
//! scope earns is the value callers read back to seed the next run.

use super::MemoryStore;
use anyhow::{Context, Result};
use chrono::Utc;
use lopi_core::{AutonomyLevel, EarnedTrust};

/// A row from the `trust_ledger` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TrustLedgerRow {
    /// Scope key — a schedule id or repo path.
    pub scope: String,
    /// Earned autonomy level tag (`report_only` … `auto_merge`).
    pub level: String,
    /// Consecutive clean, verifier-passed runs since the last promotion/reset.
    pub clean_streak: i64,
    /// RFC3339 timestamp of the last update.
    pub updated_at: String,
}

impl TrustLedgerRow {
    /// Decode this row into the typed [`EarnedTrust`] state, falling back to
    /// `base` for an unparseable level tag.
    #[must_use]
    pub fn to_state(&self, base: AutonomyLevel) -> EarnedTrust {
        EarnedTrust {
            level: AutonomyLevel::parse(&self.level).unwrap_or(base),
            clean_streak: u32::try_from(self.clean_streak.max(0)).unwrap_or(0),
        }
    }
}

impl MemoryStore {
    /// Load the earned-trust state for `scope`, or a fresh state pinned at `base`
    /// when the scope has no ledger row yet.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite read fails.
    pub async fn load_trust(&self, scope: &str, base: AutonomyLevel) -> Result<EarnedTrust> {
        let row = sqlx::query_as::<_, TrustLedgerRow>(
            "SELECT scope, level, clean_streak, updated_at FROM trust_ledger WHERE scope = ?",
        )
        .bind(scope)
        .fetch_optional(&self.read_pool)
        .await
        .context("loading trust ledger")?;
        Ok(row.map_or_else(|| EarnedTrust::new(base), |r| r.to_state(base)))
    }

    /// Upsert the earned-trust `state` for `scope`.
    ///
    /// # Errors
    /// Returns `Err` if the SQLite write fails.
    async fn save_trust(&self, scope: &str, state: EarnedTrust) -> Result<()> {
        sqlx::query(
            "INSERT INTO trust_ledger (scope, level, clean_streak, updated_at) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(scope) DO UPDATE SET \
             level = excluded.level, clean_streak = excluded.clean_streak, \
             updated_at = excluded.updated_at",
        )
        .bind(scope)
        .bind(state.level.tag_snake())
        .bind(i64::from(state.clean_streak))
        .bind(Utc::now().to_rfc3339())
        .execute(&self.write_pool)
        .await
        .context("saving trust ledger")?;
        Ok(())
    }

    /// Record a clean, verifier-passed run for `scope` and return the resulting
    /// (possibly promoted) earned-trust state. `base` seeds a first-seen scope.
    ///
    /// # Errors
    /// Returns `Err` if the ledger read or write fails.
    pub async fn record_clean_run(
        &self,
        scope: &str,
        base: AutonomyLevel,
        promote_after: u32,
        ceiling: AutonomyLevel,
    ) -> Result<EarnedTrust> {
        let next = self
            .load_trust(scope, base)
            .await?
            .on_clean_run(promote_after, ceiling);
        self.save_trust(scope, next).await?;
        Ok(next)
    }

    /// Record a failed run for `scope` (breaks the streak, no demotion) and
    /// return the resulting state.
    ///
    /// # Errors
    /// Returns `Err` if the ledger read or write fails.
    pub async fn record_failed_run(&self, scope: &str, base: AutonomyLevel) -> Result<EarnedTrust> {
        let next = self.load_trust(scope, base).await?.on_failed_run();
        self.save_trust(scope, next).await?;
        Ok(next)
    }

    /// Record a post-merge revert for `scope` — demote one rung toward `floor` —
    /// and return the resulting state.
    ///
    /// # Errors
    /// Returns `Err` if the ledger read or write fails.
    pub async fn record_revert(
        &self,
        scope: &str,
        base: AutonomyLevel,
        floor: AutonomyLevel,
    ) -> Result<EarnedTrust> {
        let next = self.load_trust(scope, base).await?.on_revert(floor);
        self.save_trust(scope, next).await?;
        Ok(next)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fresh_scope_loads_base() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let t = store
            .load_trust("repo-x", AutonomyLevel::DraftPr)
            .await
            .unwrap();
        assert_eq!(t.level, AutonomyLevel::DraftPr);
        assert_eq!(t.clean_streak, 0);
    }

    #[tokio::test]
    async fn clean_runs_promote_and_persist() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let scope = "sched-1";
        let mut last = EarnedTrust::new(AutonomyLevel::DraftPr);
        for _ in 0..3 {
            last = store
                .record_clean_run(scope, AutonomyLevel::DraftPr, 3, AutonomyLevel::VerifiedPr)
                .await
                .unwrap();
        }
        assert_eq!(last.level, AutonomyLevel::VerifiedPr);
        // Reloading sees the promoted level persisted.
        let reloaded = store
            .load_trust(scope, AutonomyLevel::DraftPr)
            .await
            .unwrap();
        assert_eq!(reloaded.level, AutonomyLevel::VerifiedPr);
        assert_eq!(reloaded.clean_streak, 0);
    }

    #[tokio::test]
    async fn failed_run_resets_streak_without_demotion() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let scope = "sched-2";
        store
            .record_clean_run(scope, AutonomyLevel::DraftPr, 5, AutonomyLevel::VerifiedPr)
            .await
            .unwrap();
        let after = store
            .record_failed_run(scope, AutonomyLevel::DraftPr)
            .await
            .unwrap();
        assert_eq!(after.level, AutonomyLevel::DraftPr);
        assert_eq!(after.clean_streak, 0);
    }

    #[tokio::test]
    async fn revert_demotes_toward_floor() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let scope = "sched-3";
        // Climb to L3 first.
        for _ in 0..2 {
            store
                .record_clean_run(scope, AutonomyLevel::DraftPr, 2, AutonomyLevel::VerifiedPr)
                .await
                .unwrap();
        }
        let promoted = store
            .load_trust(scope, AutonomyLevel::DraftPr)
            .await
            .unwrap();
        assert_eq!(promoted.level, AutonomyLevel::VerifiedPr);
        // A revert drops one rung toward the L2 floor.
        let after = store
            .record_revert(scope, AutonomyLevel::DraftPr, AutonomyLevel::DraftPr)
            .await
            .unwrap();
        assert_eq!(after.level, AutonomyLevel::DraftPr);
        assert_eq!(after.clean_streak, 0);
    }
}
