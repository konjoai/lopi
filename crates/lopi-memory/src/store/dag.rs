//! Sprint U — persistence for the DAG-structured execution trace.
//!
//! Stores one row per pipeline stage of a task attempt. The `AgentDag` type
//! lives in `lopi-agent` (a higher layer), so this module is row-level only —
//! callers map between `AgentDag` and these rows. The edge list is derived
//! from `depends_on_json`, so there is no separate edges table.
use super::MemoryStore;
use anyhow::Result;
use chrono::Utc;
use serde_json::{json, Value};

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

/// Shape DAG node rows into the `{ task_id, nodes, edges }` JSON graph. Edges
/// are derived from each node's `depends_on` list (`dep → kind`). Shared by
/// the REST route (`lopi-ui`'s `GET /api/agents/:id/dag`) and the
/// `lopi_get_agent_dag` MCP tool so both return the identical shape.
#[must_use]
pub fn dag_graph_json(task_id: &str, rows: &[DagNodeRow]) -> Value {
    let mut edges = Vec::new();
    let nodes: Vec<Value> = rows
        .iter()
        .map(|r| {
            let deps: Vec<String> = serde_json::from_str(&r.depends_on_json).unwrap_or_default();
            for dep in &deps {
                edges.push(json!({ "from": dep, "to": r.kind }));
            }
            json!({
                "kind": r.kind,
                "status": r.status,
                "depends_on": deps,
                "output_hash": r.output_hash,
                "idempotency_key": r.idempotency_key,
                "updated_at": r.updated_at,
            })
        })
        .collect();
    json!({ "task_id": task_id, "nodes": nodes, "edges": edges })
}

/// The pipeline stages `AgentRunner::record_dag_transition`
/// (`crates/lopi-agent/src/runner/lifecycle.rs`) ever actually writes, in
/// execution order. `Verify`/`Diff`/`Pr` exist in `lopi_agent::dag::NodeKind`
/// but that transition match arm never reaches them, so they're excluded
/// here rather than silently ranked as "more advanced than test/score."
const RECORDED_PIPELINE: [&str; 4] = ["plan", "implement", "test", "score"];

/// Derive a human-readable "current stage" for the `lopi_get_stack_status`
/// MCP tool (MCPB-App-1 KT-B1/KT-B2) from a task's DAG nodes: the stage
/// currently `running`, or else the most advanced stage marked `done`, or
/// `"queued"` when no DAG node has been recorded yet (the task hasn't
/// started).
#[must_use]
pub fn current_stage(rows: &[DagNodeRow]) -> String {
    if let Some(running) = rows.iter().find(|r| r.status == "running") {
        return running.kind.clone();
    }
    RECORDED_PIPELINE
        .iter()
        .rev()
        .find(|kind| rows.iter().any(|r| &r.kind == *kind && r.status == "done"))
        .map(|s| (*s).to_string())
        .unwrap_or_else(|| "queued".to_string())
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
    use super::{dag_graph_json, DagNodeRow};
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

    fn row(kind: &str, depends_on_json: &str) -> DagNodeRow {
        DagNodeRow {
            task_id: "t".into(),
            kind: kind.into(),
            status: "pending".into(),
            depends_on_json: depends_on_json.into(),
            output_hash: None,
            idempotency_key: None,
            updated_at: "now".into(),
        }
    }

    #[test]
    fn dag_graph_derives_edges_from_depends_on() {
        let rows = vec![row("plan", "[]"), row("implement", "[\"plan\"]")];
        let g = dag_graph_json("t1", &rows);
        assert_eq!(g["task_id"], "t1");
        assert_eq!(g["nodes"].as_array().unwrap().len(), 2);
        let edges = g["edges"].as_array().unwrap();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0]["from"], "plan");
        assert_eq!(edges[0]["to"], "implement");
    }

    #[test]
    fn dag_graph_empty_for_no_rows() {
        let g = dag_graph_json("t1", &[]);
        assert!(g["nodes"].as_array().unwrap().is_empty());
        assert!(g["edges"].as_array().unwrap().is_empty());
    }

    #[test]
    fn current_stage_prefers_the_running_node() {
        use super::current_stage;
        let mut plan = row("plan", "[]");
        plan.status = "done".into();
        let mut implement = row("implement", "[\"plan\"]");
        implement.status = "running".into();
        assert_eq!(current_stage(&[plan, implement]), "implement");
    }

    #[test]
    fn current_stage_falls_back_to_most_advanced_done() {
        use super::current_stage;
        let mut plan = row("plan", "[]");
        plan.status = "done".into();
        let mut implement = row("implement", "[\"plan\"]");
        implement.status = "done".into();
        assert_eq!(current_stage(&[implement, plan]), "implement");
    }

    #[test]
    fn current_stage_is_queued_with_no_nodes() {
        use super::current_stage;
        assert_eq!(current_stage(&[]), "queued");
    }

    #[test]
    fn current_stage_ignores_pending_nodes() {
        use super::current_stage;
        let pending = row("test", "[]");
        assert_eq!(current_stage(&[pending]), "queued");
    }
}
