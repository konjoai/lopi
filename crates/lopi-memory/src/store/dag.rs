//! Sprint U — persistence for the DAG-structured execution trace.
//!
//! Stores one row per pipeline stage of a task attempt. The `AgentDag` type
//! lives in `lopi-agent` (a higher layer), so this module is row-level only —
//! callers map between `AgentDag` and these rows. The edge list is derived
//! from `depends_on_json`, so there is no separate edges table.
use super::MemoryStore;
use anyhow::Result;
use chrono::Utc;

/// A row from the `agent_dag_nodes` table.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DagNodeRow {
    /// Task this node belongs to.
    pub task_id: String,
    /// Pipeline stage name (`plan`, `implement`, …).
    pub kind: String,
    /// Execution status (`pending` / `running` / `done` / `failed`).
    pub status: String,
    /// JSON array of upstream stage names this node depends on.
    pub depends_on_json: String,
    /// Memoised output hash, present once the node is `done`.
    pub output_hash: Option<String>,
    /// Recorded external effect of a side-effecting node (e.g. the PR URL).
    pub idempotency_key: Option<String>,
    /// ISO-8601 timestamp of the last update.
    pub updated_at: String,
}

impl MemoryStore {
    /// Upsert a single DAG node, keyed on `(task_id, kind)`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the SQLite write fails.
    pub async fn upsert_dag_node(
        &self,
        task_id: &str,
        kind: &str,
        status: &str,
        depends_on_json: &str,
        output_hash: Option<&str>,
        idempotency_key: Option<&str>,
    ) -> Result<()> {
        let ts = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO agent_dag_nodes \
             (task_id, kind, status, depends_on_json, output_hash, idempotency_key, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(task_id, kind) DO UPDATE SET \
             status = excluded.status, depends_on_json = excluded.depends_on_json, \
             output_hash = excluded.output_hash, idempotency_key = excluded.idempotency_key, \
             updated_at = excluded.updated_at",
        )
        .bind(task_id)
        .bind(kind)
        .bind(status)
        .bind(depends_on_json)
        .bind(output_hash)
        .bind(idempotency_key)
        .bind(&ts)
        .execute(&self.write_pool)
        .await?;
        Ok(())
    }

    /// Load a task's DAG nodes. Ordering is left to the caller (the canonical
    /// pipeline order is fixed and small).
    ///
    /// # Errors
    ///
    /// Returns `Err` if the SQLite query fails.
    pub async fn load_dag_nodes(&self, task_id: &str) -> Result<Vec<DagNodeRow>> {
        let rows = sqlx::query_as::<_, DagNodeRow>(
            "SELECT task_id, kind, status, depends_on_json, output_hash, idempotency_key, \
             updated_at FROM agent_dag_nodes WHERE task_id = ?",
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
    use crate::MemoryStore;

    async fn store() -> MemoryStore {
        MemoryStore::open_in_memory().await.unwrap()
    }

    #[tokio::test]
    async fn upsert_then_load_round_trips() {
        let s = store().await;
        s.upsert_dag_node("t1", "plan", "done", "[]", Some("h1"), None)
            .await
            .unwrap();
        s.upsert_dag_node(
            "t1",
            "pr",
            "pending",
            "[\"diff\"]",
            None,
            Some("https://example/pull/1"),
        )
        .await
        .unwrap();
        let rows = s.load_dag_nodes("t1").await.unwrap();
        assert_eq!(rows.len(), 2);
        let pr = rows.iter().find(|r| r.kind == "pr").unwrap();
        assert_eq!(
            pr.idempotency_key.as_deref(),
            Some("https://example/pull/1")
        );
        assert_eq!(pr.depends_on_json, "[\"diff\"]");
    }

    #[tokio::test]
    async fn upsert_updates_in_place() {
        let s = store().await;
        s.upsert_dag_node("t2", "test", "running", "[]", None, None)
            .await
            .unwrap();
        s.upsert_dag_node("t2", "test", "done", "[]", Some("h"), None)
            .await
            .unwrap();
        let rows = s.load_dag_nodes("t2").await.unwrap();
        assert_eq!(rows.len(), 1, "same (task, kind) upserts in place");
        assert_eq!(rows[0].status, "done");
        assert_eq!(rows[0].output_hash.as_deref(), Some("h"));
    }

    #[tokio::test]
    async fn load_unknown_task_is_empty() {
        let s = store().await;
        assert!(s.load_dag_nodes("nope").await.unwrap().is_empty());
    }
}
