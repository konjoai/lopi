//! Sprint S — Konjo Verifier verdict persistence.

use super::MemoryStore;
use anyhow::Result;
use chrono::Utc;
use lopi_core::VerifierVerdict;
use uuid::Uuid;

/// A row from the `verifier_verdicts` table.
#[derive(Debug, sqlx::FromRow)]
pub struct VerifierVerdictRow {
    /// UUID primary key.
    pub id: String,
    /// Task that was evaluated.
    pub task_id: String,
    /// Attempt number (1-based).
    pub attempt: i64,
    /// 1 when the output passed the rubric, 0 when it was rejected.
    pub passed: i64,
    /// JSON array of unmet-criteria strings.
    pub gaps_json: String,
    /// JSON array of fix-hint strings.
    pub fix_hints_json: String,
    /// Verifier confidence `[0.0, 1.0]`.
    pub confidence: f64,
    /// Model used for this verdict call.
    pub model_used: String,
    /// ISO-8601 timestamp when the verdict was recorded.
    pub ts: String,
}

impl MemoryStore {
    /// Persist a verifier verdict for a task attempt.
    ///
    /// # Errors
    ///
    /// Returns `Err` if JSON serialisation or the SQLite write fails.
    pub async fn save_verifier_verdict(
        &self,
        task_id: &str,
        attempt: u8,
        verdict: &VerifierVerdict,
        model: &str,
    ) -> Result<()> {
        let id = Uuid::new_v4().to_string();
        let gaps = serde_json::to_string(&verdict.gaps)?;
        let hints = serde_json::to_string(&verdict.fix_hints)?;
        let passed: i64 = if verdict.passed { 1 } else { 0 };
        let ts = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO verifier_verdicts \
             (id, task_id, attempt, passed, gaps_json, fix_hints_json, confidence, model_used, ts) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(task_id)
        .bind(attempt as i64)
        .bind(passed)
        .bind(&gaps)
        .bind(&hints)
        .bind(verdict.confidence)
        .bind(model)
        .bind(&ts)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Load all verifier verdicts for a task, ordered by timestamp ascending.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the SQLite query fails.
    pub async fn load_verifier_verdicts(&self, task_id: &str) -> Result<Vec<VerifierVerdictRow>> {
        let rows = sqlx::query_as::<_, VerifierVerdictRow>(
            "SELECT * FROM verifier_verdicts WHERE task_id = ? ORDER BY ts ASC",
        )
        .bind(task_id)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use lopi_core::VerifierVerdict;

    #[tokio::test]
    async fn save_and_load_verifier_verdict() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let task_id = "task-abc-001";
        let verdict = VerifierVerdict {
            passed: false,
            gaps: vec!["Missing test for error branch".into()],
            fix_hints: vec!["Add a test asserting Err is returned".into()],
            confidence: 0.85,
        };
        store
            .save_verifier_verdict(task_id, 1, &verdict, "claude-opus-4-7")
            .await
            .unwrap();
        let rows = store.load_verifier_verdicts(task_id).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].passed, 0);
        assert_eq!(rows[0].attempt, 1);
        assert_eq!(rows[0].model_used, "claude-opus-4-7");
        assert!((rows[0].confidence - 0.85).abs() < 1e-6);
    }

    #[tokio::test]
    async fn save_passed_verdict() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let verdict = VerifierVerdict {
            passed: true,
            gaps: vec![],
            fix_hints: vec![],
            confidence: 0.97,
        };
        store
            .save_verifier_verdict("task-xyz", 2, &verdict, "claude-opus-4-7")
            .await
            .unwrap();
        let rows = store.load_verifier_verdicts("task-xyz").await.unwrap();
        assert_eq!(rows[0].passed, 1);
        assert_eq!(rows[0].attempt, 2);
    }

    #[tokio::test]
    async fn load_empty_returns_empty_vec() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let rows = store.load_verifier_verdicts("nonexistent").await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn multiple_attempts_ordered_by_ts() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let tid = "multi-attempt-task";
        for attempt in [1u8, 2, 3] {
            let v = VerifierVerdict {
                passed: attempt == 3,
                gaps: vec![],
                fix_hints: vec![],
                confidence: f64::from(attempt) * 0.3,
            };
            store.save_verifier_verdict(tid, attempt, &v, "claude-opus-4-7").await.unwrap();
        }
        let rows = store.load_verifier_verdicts(tid).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2].passed, 1);
    }
}
