//! Quality check run ledger — Sprint M.
//!
//! Persists the result of each `lopi gap-fill` / `lopi check` invocation
//! so coverage trend can be tracked over time. One row per run.

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use super::MemoryStore;

/// A single quality check run retrieved from the ledger.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct QualityRunRow {
    pub id: String,
    pub repo_path: String,
    pub spec_items: i64,
    pub passing: i64,
    pub failing: i64,
    pub gaps: i64,
    /// Pass rate 0.0–1.0: `passing / spec_items` (0.0 when spec_items == 0).
    pub score: f64,
    pub run_at: String,
}

impl QualityRunRow {
    /// Coverage trend direction vs a previous run.
    #[must_use]
    pub fn improved_vs(&self, prev: &Self) -> bool {
        self.score > prev.score
    }
}

/// Arguments for a single quality run record.
pub struct QualityRunRecord {
    pub repo_path: String,
    pub spec_items: usize,
    pub passing: usize,
    pub failing: usize,
    pub gaps: usize,
}

impl MemoryStore {
    /// Persist a quality check run. Idempotent on the generated UUID.
    ///
    /// # Errors
    ///
    /// Returns an error if the database write fails.
    pub async fn save_quality_run(&self, rec: QualityRunRecord) -> Result<String> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let score = if rec.spec_items == 0 {
            0.0_f64
        } else {
            rec.passing as f64 / rec.spec_items as f64
        };
        sqlx::query(
            "INSERT INTO quality_check_runs \
             (id, repo_path, spec_items, passing, failing, gaps, score, run_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(id) DO NOTHING",
        )
        .bind(&id)
        .bind(&rec.repo_path)
        .bind(rec.spec_items as i64)
        .bind(rec.passing as i64)
        .bind(rec.failing as i64)
        .bind(rec.gaps as i64)
        .bind(score)
        .bind(&now)
        .execute(&self.write_pool)
        .await?;
        Ok(id)
    }

    /// Load the most recent quality check runs for a repo.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn load_quality_trend(
        &self,
        repo_path: &str,
        limit: i64,
    ) -> Result<Vec<QualityRunRow>> {
        let rows = sqlx::query_as::<_, QualityRunRow>(
            "SELECT id, repo_path, spec_items, passing, failing, gaps, score, run_at \
             FROM quality_check_runs \
             WHERE repo_path = ?1 \
             ORDER BY run_at DESC \
             LIMIT ?2",
        )
        .bind(repo_path)
        .bind(limit)
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }

    /// Return the two most recent runs to compute a trend arrow.
    ///
    /// # Errors
    ///
    /// Returns an error if the database query fails.
    pub async fn quality_trend_delta(&self, repo_path: &str) -> Result<Option<(f64, f64)>> {
        let rows = self.load_quality_trend(repo_path, 2).await?;
        match rows.as_slice() {
            [latest, prev] => Ok(Some((latest.score, prev.score))),
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn save_and_load_quality_run() {
        let s = store().await;
        let rec = QualityRunRecord {
            repo_path: "/repo/a".into(),
            spec_items: 10,
            passing: 8,
            failing: 1,
            gaps: 2,
        };
        s.save_quality_run(rec).await.unwrap();
        let trend = s.load_quality_trend("/repo/a", 10).await.unwrap();
        assert_eq!(trend.len(), 1);
        assert_eq!(trend[0].spec_items, 10);
        assert_eq!(trend[0].passing, 8);
        assert!((trend[0].score - 0.8).abs() < 0.001);
    }

    #[tokio::test]
    async fn trend_delta_returns_none_for_single_run() {
        let s = store().await;
        s.save_quality_run(QualityRunRecord {
            repo_path: "/repo/b".into(),
            spec_items: 5,
            passing: 5,
            failing: 0,
            gaps: 0,
        })
        .await
        .unwrap();
        let delta = s.quality_trend_delta("/repo/b").await.unwrap();
        assert!(delta.is_none());
    }

    #[tokio::test]
    async fn trend_delta_returns_pair() {
        let s = store().await;
        for (p, f) in [(3, 2), (4, 1)] {
            s.save_quality_run(QualityRunRecord {
                repo_path: "/repo/c".into(),
                spec_items: 5,
                passing: p,
                failing: f,
                gaps: f,
            })
            .await
            .unwrap();
        }
        let (latest, prev) = s.quality_trend_delta("/repo/c").await.unwrap().unwrap();
        // Second run (4 passing) should be latest
        assert!(latest >= prev);
    }

    #[tokio::test]
    async fn score_zero_when_no_spec_items() {
        let s = store().await;
        s.save_quality_run(QualityRunRecord {
            repo_path: "/repo/d".into(),
            spec_items: 0,
            passing: 0,
            failing: 0,
            gaps: 0,
        })
        .await
        .unwrap();
        let rows = s.load_quality_trend("/repo/d", 1).await.unwrap();
        assert!((rows[0].score - 0.0).abs() < 0.001);
    }

    #[tokio::test]
    async fn improved_vs_comparison() {
        let a = QualityRunRow {
            id: "a".into(),
            repo_path: "/r".into(),
            spec_items: 10,
            passing: 7,
            failing: 3,
            gaps: 3,
            score: 0.7,
            run_at: "2026-01-01".into(),
        };
        let b = QualityRunRow {
            id: "b".into(),
            repo_path: "/r".into(),
            spec_items: 10,
            passing: 9,
            failing: 1,
            gaps: 1,
            score: 0.9,
            run_at: "2026-01-02".into(),
        };
        assert!(b.improved_vs(&a));
        assert!(!a.improved_vs(&b));
    }
}
