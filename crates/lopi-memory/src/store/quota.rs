//! Quota headroom observations (MAXX Phase 0).
//!
//! Every `AgentEvent::ApiRetry` seen system-wide is upserted here, keyed by
//! `limit_type` so a `five_hour` observation never clobbers a `seven_day`
//! one — they arrive through the same event variant. `lopi-orchestrator`'s
//! `QuotaTracker` is the only writer; `GET /api/quota` and the `maxx_loop`
//! tick are the readers.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::Row as _;

use super::MemoryStore;

/// One persisted quota window observation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QuotaObservationRow {
    /// Window type — `five_hour` or `seven_day`. Primary key.
    pub limit_type: String,
    /// Status string from the CLI, e.g. `allowed_warning`.
    pub status: String,
    /// Window utilization in `[0.0, 1.0]`.
    pub utilization: f32,
    /// Unix seconds the window resets, if the CLI reported it.
    pub resets_at: Option<i64>,
    /// ISO-8601 timestamp this observation was recorded.
    pub observed_at: String,
}

impl MemoryStore {
    /// Upsert the observation for `limit_type`. Independent rows per window —
    /// never a shared "last event wins" scalar.
    ///
    /// # Errors
    /// Returns `Err` if the write fails.
    pub async fn upsert_quota_observation(
        &self,
        limit_type: &str,
        status: &str,
        utilization: f32,
        resets_at: Option<i64>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO quota_observations (limit_type, status, utilization, resets_at, observed_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(limit_type) DO UPDATE SET
               status = excluded.status,
               utilization = excluded.utilization,
               resets_at = excluded.resets_at,
               observed_at = excluded.observed_at",
        )
        .bind(limit_type)
        .bind(status)
        .bind(f64::from(utilization))
        .bind(resets_at)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.write_pool)
        .await
        .context("upserting quota observation")?;
        Ok(())
    }

    /// Fetch the current observation for `limit_type`, if any has been recorded.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn get_quota_observation(
        &self,
        limit_type: &str,
    ) -> Result<Option<QuotaObservationRow>> {
        let row = sqlx::query(
            "SELECT limit_type, status, utilization, resets_at, observed_at
             FROM quota_observations WHERE limit_type = ?",
        )
        .bind(limit_type)
        .fetch_optional(&self.read_pool)
        .await
        .context("fetching quota observation")?;
        Ok(row.map(observation_from_row))
    }

    /// List every recorded quota observation.
    ///
    /// # Errors
    /// Returns `Err` if the query fails.
    pub async fn list_quota_observations(&self) -> Result<Vec<QuotaObservationRow>> {
        let rows = sqlx::query(
            "SELECT limit_type, status, utilization, resets_at, observed_at FROM quota_observations",
        )
        .fetch_all(&self.read_pool)
        .await
        .context("listing quota observations")?;
        Ok(rows.into_iter().map(observation_from_row).collect())
    }
}

fn observation_from_row(row: sqlx::sqlite::SqliteRow) -> QuotaObservationRow {
    QuotaObservationRow {
        limit_type: row.get("limit_type"),
        status: row.get("status"),
        utilization: row.get::<f64, _>("utilization") as f32,
        resets_at: row.get("resets_at"),
        observed_at: row.get("observed_at"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn upsert_then_get_round_trips() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .upsert_quota_observation("seven_day", "allowed_warning", 0.92, Some(1_782_691_200))
            .await
            .unwrap();
        let row = store
            .get_quota_observation("seven_day")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.status, "allowed_warning");
        assert!((row.utilization - 0.92).abs() < 1e-6);
        assert_eq!(row.resets_at, Some(1_782_691_200));
    }

    #[tokio::test]
    async fn five_hour_and_seven_day_are_independent_rows() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .upsert_quota_observation("seven_day", "allowed_warning", 0.92, Some(1_782_691_200))
            .await
            .unwrap();
        store
            .upsert_quota_observation("five_hour", "allowed", 0.10, Some(1_700_000_000))
            .await
            .unwrap();
        let seven_day = store
            .get_quota_observation("seven_day")
            .await
            .unwrap()
            .unwrap();
        let five_hour = store
            .get_quota_observation("five_hour")
            .await
            .unwrap()
            .unwrap();
        // The five_hour write must not have clobbered the seven_day row.
        assert!((seven_day.utilization - 0.92).abs() < 1e-6);
        assert!((five_hour.utilization - 0.10).abs() < 1e-6);
        assert_eq!(store.list_quota_observations().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn unknown_window_returns_none() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        assert!(store
            .get_quota_observation("five_hour")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn repeated_upsert_updates_in_place() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .upsert_quota_observation("seven_day", "allowed", 0.10, None)
            .await
            .unwrap();
        store
            .upsert_quota_observation("seven_day", "allowed_warning", 0.80, Some(42))
            .await
            .unwrap();
        let row = store
            .get_quota_observation("seven_day")
            .await
            .unwrap()
            .unwrap();
        assert!((row.utilization - 0.80).abs() < 1e-6);
        assert_eq!(row.resets_at, Some(42));
        assert_eq!(store.list_quota_observations().await.unwrap().len(), 1);
    }
}
