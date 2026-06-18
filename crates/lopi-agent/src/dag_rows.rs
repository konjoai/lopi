//! Sprint U — reconstruct an [`AgentDag`] from persisted `agent_dag_nodes` rows.
//!
//! Kept out of `dag.rs` so the pure data structure stays free of the
//! `lopi-memory` coupling and within the file-size budget.

use crate::dag::{AgentDag, DagNode, NodeKind, NodeStatus};
use lopi_memory::DagNodeRow;
use std::str::FromStr;

impl AgentDag {
    /// Rebuild a DAG from stored rows. Rows are matched into the canonical
    /// pipeline by `kind`; unparseable kinds/statuses are skipped (the
    /// canonical default — `Pending`, no hash — stands in), so a partially
    /// written trace still yields a usable graph rather than an error.
    #[must_use]
    pub fn from_rows(rows: &[DagNodeRow]) -> Self {
        let mut dag = Self::canonical();
        for row in rows {
            let Ok(kind) = NodeKind::from_str(&row.kind) else {
                continue;
            };
            let Some(node) = dag.nodes.iter_mut().find(|n| n.kind == kind) else {
                continue;
            };
            apply_row(node, row);
        }
        dag
    }
}

/// Overlay a stored row onto its canonical node.
fn apply_row(node: &mut DagNode, row: &DagNodeRow) {
    if let Ok(status) = NodeStatus::from_str(&row.status) {
        node.status = status;
    }
    node.output_hash = row.output_hash.clone();
    node.idempotency_key = row.idempotency_key.clone();
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn row(kind: &str, status: &str, hash: Option<&str>, key: Option<&str>) -> DagNodeRow {
        DagNodeRow {
            task_id: "t".into(),
            kind: kind.into(),
            status: status.into(),
            depends_on_json: "[]".into(),
            output_hash: hash.map(str::to_string),
            idempotency_key: key.map(str::to_string),
            updated_at: "now".into(),
        }
    }

    #[test]
    fn from_rows_overlays_status_hash_and_key() {
        let rows = vec![
            row("plan", "done", Some("h1"), None),
            row("pr", "pending", None, Some("https://pr/1")),
        ];
        let dag = AgentDag::from_rows(&rows);
        assert_eq!(dag.nodes.len(), 7, "canonical pipeline preserved");
        assert_eq!(dag.node(NodeKind::Plan).unwrap().status, NodeStatus::Done);
        assert_eq!(
            dag.node(NodeKind::Plan).unwrap().output_hash.as_deref(),
            Some("h1")
        );
        assert_eq!(dag.idempotency_key(NodeKind::Pr), Some("https://pr/1"));
        // Untouched stage stays at the canonical default.
        assert_eq!(
            dag.node(NodeKind::Test).unwrap().status,
            NodeStatus::Pending
        );
    }

    #[test]
    fn from_rows_skips_unknown_kind() {
        let rows = vec![row("teleport", "done", None, None)];
        let dag = AgentDag::from_rows(&rows);
        assert!(dag.nodes.iter().all(|n| n.status == NodeStatus::Pending));
    }

    #[test]
    fn from_empty_rows_is_canonical() {
        let dag = AgentDag::from_rows(&[]);
        assert_eq!(dag.resume_point(), Some(NodeKind::Plan));
    }
}
