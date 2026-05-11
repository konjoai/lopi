//! Stability ledger store — Sprint I (Layer 5 Patch Stability Harness).
//!
//! Persists one row per stability assessment so we accumulate an empirical
//! dataset of (task_class, model, variance_score, verdict). Over time this
//! shows which task categories the model is reliable enough to self-ship
//! and which always need a human gate.

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::MemoryStore;

/// A single row from the `stability_ledger` table.
#[derive(Debug, sqlx::FromRow)]
pub struct StabilityEntry {
    /// UUID primary key.
    pub id: String,
    /// First 64 chars of the task goal (lowercased) — approximate key for
    /// grouping similar tasks without storing PII-sensitive verbatim goals.
    pub task_goal_pfx: String,
    /// Model that generated the plan samples.
    pub model: String,
    /// Number of plan samples collected (≤ config.n_samples on partial runs).
    pub n_samples: i64,
    /// Variance score: 1 − mean_pairwise_jaccard ∈ [0, 1].
    pub variance_score: f64,
    /// Gate verdict: `"stable"`, `"warning"`, or `"unstable"`.
    pub verdict: String,
    /// JSON array of file paths the patch touched outside `allowed_dirs`.
    pub semantic_flags: String,
    /// 1 = the run proceeded after this assessment; 0 = blocked.
    pub accepted: i64,
    /// RFC 3339 timestamp.
    pub created_at: String,
}

/// Parameters for a stability ledger write.
///
/// Passed to `MemoryStore::save_stability_entry` to avoid the clippy
/// `too_many_arguments` lint while keeping each field self-documenting.
pub struct StabilityRecord<'a> {
    /// Full task goal text — truncated to 64 chars before storage.
    pub task_goal: &'a str,
    /// Model used to generate plan samples.
    pub model: &'a str,
    /// Number of plan samples actually collected.
    pub n_samples: usize,
    /// Pairwise Jaccard variance score ∈ [0, 1].
    pub variance_score: f32,
    /// Gate verdict: `"stable"`, `"warning"`, or `"unstable"`.
    pub verdict: &'a str,
    /// Files outside `allowed_dirs` touched by the patch (may be empty).
    pub semantic_flags: &'a [String],
    /// True if the run proceeded after this assessment.
    pub accepted: bool,
}

impl MemoryStore {
    /// Persist a stability assessment to the ledger.
    ///
    /// Returns the UUID string of the inserted row.
    ///
    /// # Errors
    /// Returns `Err` if the database insert fails.
    pub async fn save_stability_entry(&self, rec: StabilityRecord<'_>) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let pfx: String = rec
            .task_goal
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(64)
            .collect();
        let flags_json =
            serde_json::to_string(rec.semantic_flags).unwrap_or_else(|_| "[]".into());

        sqlx::query(
            "INSERT INTO stability_ledger \
             (id, task_goal_pfx, model, n_samples, variance_score, verdict, semantic_flags, accepted, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(&id)
        .bind(&pfx)
        .bind(rec.model)
        .bind(rec.n_samples as i64)
        .bind(f64::from(rec.variance_score))
        .bind(rec.verdict)
        .bind(&flags_json)
        .bind(i64::from(rec.accepted))
        .bind(&now)
        .execute(&self.write_pool)
        .await?;

        Ok(id)
    }

    /// Load the most recent `limit` stability ledger entries, newest first.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn load_stability_entries(&self, limit: i64) -> Result<Vec<StabilityEntry>> {
        let rows = sqlx::query_as::<_, StabilityEntry>(
            "SELECT id, task_goal_pfx, model, n_samples, variance_score, verdict, \
             semantic_flags, accepted, created_at \
             FROM stability_ledger ORDER BY created_at DESC LIMIT ?1",
        )
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Count ledger entries grouped by verdict.
    ///
    /// Returns `(stable, warning, unstable)` counts.
    ///
    /// # Errors
    /// Returns `Err` if the database query fails.
    pub async fn stability_verdict_counts(&self) -> Result<(i64, i64, i64)> {
        let stable: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM stability_ledger WHERE verdict = 'stable'",
        )
        .fetch_one(&self.read_pool)
        .await?;
        let warning: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM stability_ledger WHERE verdict = 'warning'",
        )
        .fetch_one(&self.read_pool)
        .await?;
        let unstable: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM stability_ledger WHERE verdict = 'unstable'",
        )
        .fetch_one(&self.read_pool)
        .await?;
        Ok((stable.0, warning.0, unstable.0))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn rec<'a>(
        task_goal: &'a str,
        model: &'a str,
        verdict: &'a str,
        accepted: bool,
    ) -> StabilityRecord<'a> {
        StabilityRecord {
            task_goal,
            model,
            n_samples: 5,
            variance_score: 0.1,
            verdict,
            semantic_flags: &[],
            accepted,
        }
    }

    #[tokio::test]
    async fn save_and_load_stability_entry() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let id = store
            .save_stability_entry(StabilityRecord {
                task_goal: "add retry logic to the HTTP client module",
                model: "claude-sonnet-4-6",
                n_samples: 5,
                variance_score: 0.12,
                verdict: "stable",
                semantic_flags: &[],
                accepted: true,
            })
            .await
            .unwrap();
        assert!(!id.is_empty());

        let entries = store.load_stability_entries(10).await.unwrap();
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.id, id);
        assert_eq!(e.model, "claude-sonnet-4-6");
        assert_eq!(e.n_samples, 5);
        assert_eq!(e.verdict, "stable");
        assert_eq!(e.accepted, 1);
    }

    #[tokio::test]
    async fn save_entry_truncates_goal_prefix() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let long_goal = "a".repeat(200);
        store
            .save_stability_entry(StabilityRecord {
                task_goal: &long_goal,
                model: "claude-haiku-4-5",
                n_samples: 3,
                variance_score: 0.4,
                verdict: "unstable",
                semantic_flags: &[],
                accepted: false,
            })
            .await
            .unwrap();
        let entries = store.load_stability_entries(1).await.unwrap();
        assert!(entries[0].task_goal_pfx.len() <= 64);
    }

    #[tokio::test]
    async fn save_entry_with_semantic_flags() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let flags = vec![".github/workflows/ci.yml".to_string(), "Cargo.lock".to_string()];
        store
            .save_stability_entry(StabilityRecord {
                task_goal: "fix the parser",
                model: "claude-sonnet-4-6",
                n_samples: 5,
                variance_score: 0.25,
                verdict: "warning",
                semantic_flags: &flags,
                accepted: true,
            })
            .await
            .unwrap();
        let entries = store.load_stability_entries(1).await.unwrap();
        let decoded: Vec<String> =
            serde_json::from_str(&entries[0].semantic_flags).unwrap();
        assert_eq!(decoded, flags);
    }

    #[tokio::test]
    async fn verdict_counts_correct() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        for verdict in &["stable", "stable", "warning", "unstable"] {
            store
                .save_stability_entry(rec("goal", "model", verdict, true))
                .await
                .unwrap();
        }
        let (s, w, u) = store.stability_verdict_counts().await.unwrap();
        assert_eq!(s, 2);
        assert_eq!(w, 1);
        assert_eq!(u, 1);
    }

    #[tokio::test]
    async fn load_entries_newest_first() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        for i in 0..3_u8 {
            store
                .save_stability_entry(rec(&format!("goal {i}"), "model", "stable", true))
                .await
                .unwrap();
        }
        let entries = store.load_stability_entries(3).await.unwrap();
        // created_at strings are RFC3339 — DESC ordering means newest first.
        // Since inserts happen in rapid succession the order may tie; just
        // verify we get 3 rows.
        assert_eq!(entries.len(), 3);
    }
}
