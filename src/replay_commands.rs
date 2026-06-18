//! `lopi replay` — inspect a task's DAG-structured execution trace and show
//! the partial-restart plan (Sprint U).
//!
//! Loads the persisted `agent_dag_nodes` for a task, reconstructs the
//! [`AgentDag`], and prints what a replay from the resume point (or an
//! explicit `--from` stage) would re-run, reuse, or skip. Side-effecting
//! stages whose external effect already landed are skipped to stay idempotent
//! (ACRFence, arXiv 2603.20625).
//!
//! Live re-execution is not yet wired — that rides on the runner producer —
//! so the command currently reports the plan (an implicit dry run).

use anyhow::{anyhow, Result};
use lopi_agent::dag::{AgentDag, NodeKind, NodeStatus};
use lopi_memory::MemoryStore;
use std::str::FromStr;

use crate::util::db_path;

pub async fn run(task_id: String, from: Option<String>, dry_run: bool) -> Result<()> {
    let store = MemoryStore::open(db_path()).await?;
    let rows = store.load_dag_nodes(&task_id).await?;
    if rows.is_empty() {
        println!("🕸️  no DAG recorded for task {task_id}.");
        println!("    (the runner producer that records execution DAGs is pending — Sprint U)");
        return Ok(());
    }

    let dag = AgentDag::from_rows(&rows);
    let restart = resolve_restart(&dag, from.as_deref())?;
    print_plan(&task_id, &dag, restart, dry_run);
    Ok(())
}

/// Pick the stage to restart from: an explicit `--from`, else the DAG's resume
/// point (earliest unfinished stage).
fn resolve_restart(dag: &AgentDag, from: Option<&str>) -> Result<NodeKind> {
    match from {
        Some(name) => NodeKind::from_str(name).map_err(|e| anyhow!(e)),
        None => dag.resume_point().ok_or_else(|| {
            anyhow!("task is fully complete — pass --from <stage> to force a replay")
        }),
    }
}

fn print_plan(task_id: &str, dag: &AgentDag, restart: NodeKind, dry_run: bool) {
    println!("🕸️  replay plan for task {task_id}");
    println!("    restart from: {restart}\n");

    for (kind, icon, note) in replay_plan(dag, restart) {
        println!("    {icon} {:<10} {note}", kind.to_string());
    }

    println!();
    if dry_run {
        println!("    (dry run — no changes made)");
    } else {
        println!("    note: live re-execution is not yet wired; showing the plan only.");
        println!("    the recorded idempotency keys above will be reused, not re-issued.");
    }
}

/// Classify every stage for a replay restarting at `restart`. Pure, so the plan
/// can be asserted without capturing stdout.
fn replay_plan(dag: &AgentDag, restart: NodeKind) -> Vec<(NodeKind, &'static str, String)> {
    let mut planned = dag.clone();
    planned.reset_from(restart);
    dag.nodes
        .iter()
        .map(|node| {
            let (icon, note) = classify(dag, &planned, node.kind, restart);
            (node.kind, icon, note)
        })
        .collect()
}

/// Per-stage replay classification: reused upstream, re-run, or skipped because
/// its external effect already committed.
fn classify(
    original: &AgentDag,
    planned: &AgentDag,
    kind: NodeKind,
    restart: NodeKind,
) -> (&'static str, String) {
    if order(kind) < order(restart) {
        return match original.node(kind).map(|n| n.status) {
            Some(NodeStatus::Done) => ("♻️", "reuse — memoized upstream output".into()),
            _ => ("·", "upstream (not done)".into()),
        };
    }
    if !planned.should_execute(kind) {
        let key = original.idempotency_key(kind).unwrap_or("(recorded)");
        return ("⏭️", format!("skip — reuse external effect: {key}"));
    }
    ("▶️", "re-run".into())
}

fn order(kind: NodeKind) -> usize {
    NodeKind::PIPELINE
        .iter()
        .position(|k| *k == kind)
        .unwrap_or(0)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn resolve_restart_uses_resume_point() {
        let mut dag = AgentDag::canonical();
        dag.complete_node(NodeKind::Plan, "h");
        assert_eq!(resolve_restart(&dag, None).unwrap(), NodeKind::Implement);
    }

    #[test]
    fn resolve_restart_parses_explicit_from() {
        let dag = AgentDag::canonical();
        assert_eq!(resolve_restart(&dag, Some("test")).unwrap(), NodeKind::Test);
        assert!(resolve_restart(&dag, Some("bogus")).is_err());
    }

    #[test]
    fn resolve_restart_errors_when_complete() {
        let mut dag = AgentDag::canonical();
        for k in NodeKind::PIPELINE {
            dag.complete_node(k, "h");
        }
        assert!(resolve_restart(&dag, None).is_err());
    }

    #[test]
    fn committed_pr_is_classified_as_skip() {
        let mut dag = AgentDag::canonical();
        dag.record_idempotency_key(NodeKind::Pr, "pr-url");
        let mut planned = dag.clone();
        planned.reset_from(NodeKind::Plan);
        let (icon, note) = classify(&dag, &planned, NodeKind::Pr, NodeKind::Plan);
        assert_eq!(icon, "⏭️");
        assert!(note.contains("pr-url"));
    }

    #[test]
    fn replay_plan_classifies_upstream_restart_and_downstream() {
        let mut dag = AgentDag::canonical();
        dag.complete_node(NodeKind::Plan, "h"); // Done, strictly upstream of Score
        let plan = replay_plan(&dag, NodeKind::Score);
        let icon = |k: NodeKind| plan.iter().find(|(kind, _, _)| *kind == k).unwrap().1;
        // Upstream + Done → reused (kills `order`→const and `<`→`==` mutants,
        // and the `Some(Done)` match-arm deletion).
        assert_eq!(icon(NodeKind::Plan), "♻️");
        // The restart stage itself must re-run, not be treated as upstream
        // (kills the `<`→`<=` mutant).
        assert_eq!(icon(NodeKind::Score), "▶️");
        // A later non-side-effecting stage re-runs.
        assert_eq!(icon(NodeKind::Verify), "▶️");
    }

    #[test]
    fn replay_plan_marks_undone_upstream_with_dot() {
        let dag = AgentDag::canonical(); // all pending
        let plan = replay_plan(&dag, NodeKind::Score);
        let plan_icon = plan
            .iter()
            .find(|(k, _, _)| *k == NodeKind::Plan)
            .unwrap()
            .1;
        assert_eq!(plan_icon, "·");
    }
}
