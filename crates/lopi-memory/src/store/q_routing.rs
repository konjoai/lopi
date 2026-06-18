//! Sprint T — persistence for the Q-learning router's value table.
//!
//! The orchestrator's `QRouter` keeps its Q-table in memory; these methods
//! persist it to the `routing_q_values` table so learned estimates survive a
//! restart and can be inspected via `GET /api/routing/q-values`.
use super::MemoryStore;
use anyhow::Result;
use chrono::Utc;

/// A row from the `routing_q_values` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoutingQValueRow {
    /// Task type the estimate is keyed on.
    pub state: String,
    /// Agent-config identifier the estimate is keyed on.
    pub action: String,
    /// Running value estimate in `[0, 1]`.
    pub q: f64,
    /// Number of rewards folded into `q`.
    pub update_count: i64,
    /// ISO-8601 timestamp of the last update.
    pub updated_at: String,
}

impl MemoryStore {
    /// Upsert a single Q-value cell keyed on `(state, action)`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the SQLite write fails.
    pub async fn upsert_q_value(
        &self,
        state: &str,
        action: &str,
        q: f64,
        updates: u64,
    ) -> Result<()> {
        let ts = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO routing_q_values (state, action, q, update_count, updated_at) \
             VALUES (?, ?, ?, ?, ?) \
             ON CONFLICT(state, action) DO UPDATE SET \
             q = excluded.q, update_count = excluded.update_count, updated_at = excluded.updated_at",
        )
        .bind(state)
        .bind(action)
        .bind(q)
        .bind(i64::try_from(updates).unwrap_or(i64::MAX))
        .bind(&ts)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Load the full Q-table, most-recently-updated first.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the SQLite query fails.
    pub async fn load_q_table(&self) -> Result<Vec<RoutingQValueRow>> {
        let rows = sqlx::query_as::<_, RoutingQValueRow>(
            "SELECT state, action, q, update_count, updated_at \
             FROM routing_q_values ORDER BY updated_at DESC",
        )
        .fetch_all(&self.read_pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::MemoryStore;

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn upsert_inserts_then_updates_in_place() {
        let s = store().await;
        s.upsert_q_value("refactor", "fast", 0.4, 1).await.unwrap();
        s.upsert_q_value("refactor", "fast", 0.7, 2).await.unwrap();
        let rows = s.load_q_table().await.unwrap();
        assert_eq!(rows.len(), 1, "same (state, action) upserts in place");
        assert!((rows[0].q - 0.7).abs() < 1e-9);
        assert_eq!(rows[0].update_count, 2);
    }

    #[tokio::test]
    async fn load_returns_all_distinct_pairs() {
        let s = store().await;
        s.upsert_q_value("feature", "deep", 0.9, 5).await.unwrap();
        s.upsert_q_value("feature", "fast", 0.3, 3).await.unwrap();
        s.upsert_q_value("bug", "deep", 0.5, 1).await.unwrap();
        let rows = s.load_q_table().await.unwrap();
        assert_eq!(rows.len(), 3);
    }

    #[tokio::test]
    async fn empty_table_loads_empty() {
        let s = store().await;
        assert!(s.load_q_table().await.unwrap().is_empty());
    }
}
