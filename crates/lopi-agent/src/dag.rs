//! Sprint U — DAG-structured agent execution trace.
//!
//! Each agent attempt is a small directed acyclic graph of pipeline stages:
//! `Plan → Implement → Test → Score → Verify → Diff → PR`. Recording the run
//! as a DAG (rather than a flat attempt counter) unlocks *partial restart*:
//! on retry we resume from the earliest unfinished node instead of replaying
//! the whole pipeline, reusing the memoized output of every node still marked
//! `Done`.
//!
//! Grounded in the Scheduler-Theoretic Framework (arXiv 2604.11378): partial
//! restart from failed nodes dominates linear retry. This module is the pure
//! data structure; persistence and the `lopi replay` CLI build on top of it.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A single stage in the agent pipeline. Each kind runs at most once per
/// attempt; together they form a linear dependency chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Generate the implementation plan.
    Plan,
    /// Apply code changes.
    Implement,
    /// Run the test suite.
    Test,
    /// Heuristic scoring (pass rate, lint, diff size).
    Score,
    /// Konjo Verifier rubric pass (Sprint S).
    Verify,
    /// Compute the final diff.
    Diff,
    /// Open the pull request.
    Pr,
}

impl NodeKind {
    /// The canonical pipeline order, earliest stage first.
    pub const PIPELINE: [NodeKind; 7] = [
        NodeKind::Plan,
        NodeKind::Implement,
        NodeKind::Test,
        NodeKind::Score,
        NodeKind::Verify,
        NodeKind::Diff,
        NodeKind::Pr,
    ];

    /// Lowercase wire/display name.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::Plan => "plan",
            NodeKind::Implement => "implement",
            NodeKind::Test => "test",
            NodeKind::Score => "score",
            NodeKind::Verify => "verify",
            NodeKind::Diff => "diff",
            NodeKind::Pr => "pr",
        }
    }

    /// Index of this kind in [`Self::PIPELINE`].
    fn order(&self) -> usize {
        Self::PIPELINE.iter().position(|k| k == self).unwrap_or(0)
    }

    /// The stage this one directly depends on (its predecessor), if any.
    #[must_use]
    pub fn predecessor(&self) -> Option<NodeKind> {
        let idx = self.order();
        (idx > 0).then(|| Self::PIPELINE[idx - 1])
    }

    /// True when running this stage writes state *outside* the agent's
    /// git-isolated sandbox — currently only `Pr`, which opens a pull request.
    ///
    /// Such nodes must be idempotent across replays: re-running them would
    /// duplicate the external effect (a second PR). See ACRFence
    /// (arXiv 2603.20625) on semantic rollback hazards in agent retry.
    #[must_use]
    pub fn is_side_effecting(&self) -> bool {
        matches!(self, NodeKind::Pr)
    }
}

impl fmt::Display for NodeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for NodeKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::PIPELINE
            .into_iter()
            .find(|k| k.as_str() == s)
            .ok_or_else(|| format!("unknown node kind: {s}"))
    }
}

/// Execution status of a single DAG node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NodeStatus {
    /// Not yet started.
    #[default]
    Pending,
    /// Currently executing.
    Running,
    /// Finished successfully; `output_hash` is populated.
    Done,
    /// Finished with a failure; retry resumes here.
    Failed,
}

impl std::str::FromStr for NodeStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(NodeStatus::Pending),
            "running" => Ok(NodeStatus::Running),
            "done" => Ok(NodeStatus::Done),
            "failed" => Ok(NodeStatus::Failed),
            other => Err(format!("unknown node status: {other}")),
        }
    }
}

/// One node in the agent execution DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    /// Which pipeline stage this node represents (also its unique id).
    pub kind: NodeKind,
    /// Current execution status.
    pub status: NodeStatus,
    /// Stages that must complete before this one may run.
    pub depends_on: Vec<NodeKind>,
    /// Hash of this node's output once `Done` — the memoization key that lets
    /// a retry reuse the result without re-running the stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_hash: Option<String>,
    /// Idempotency key for a side-effecting node (e.g. the opened PR URL),
    /// recorded once the external effect lands. Unlike `output_hash` this is
    /// *preserved* across [`AgentDag::reset_from`] so a replay reuses the
    /// committed effect instead of duplicating it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub idempotency_key: Option<String>,
}

/// The execution DAG for one agent run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDag {
    /// Nodes in canonical pipeline order.
    pub nodes: Vec<DagNode>,
}

impl Default for AgentDag {
    fn default() -> Self {
        Self::canonical()
    }
}

impl AgentDag {
    /// Build the canonical linear pipeline with every node `Pending`.
    #[must_use]
    pub fn canonical() -> Self {
        let nodes = NodeKind::PIPELINE
            .iter()
            .map(|kind| DagNode {
                kind: *kind,
                status: NodeStatus::Pending,
                depends_on: kind.predecessor().into_iter().collect(),
                output_hash: None,
                idempotency_key: None,
            })
            .collect();
        Self { nodes }
    }

    /// Borrow the node for `kind`, if present.
    #[must_use]
    pub fn node(&self, kind: NodeKind) -> Option<&DagNode> {
        self.nodes.iter().find(|n| n.kind == kind)
    }

    fn node_mut(&mut self, kind: NodeKind) -> Option<&mut DagNode> {
        self.nodes.iter_mut().find(|n| n.kind == kind)
    }

    /// Set a node's status. No-op for an unknown kind.
    pub fn set_status(&mut self, kind: NodeKind, status: NodeStatus) {
        if let Some(node) = self.node_mut(kind) {
            node.status = status;
        }
    }

    /// Mark a node `Done` and record its output hash (the memoization key).
    pub fn complete_node(&mut self, kind: NodeKind, output_hash: impl Into<String>) {
        if let Some(node) = self.node_mut(kind) {
            node.status = NodeStatus::Done;
            node.output_hash = Some(output_hash.into());
        }
    }

    /// Mark a node `Failed`.
    pub fn fail_node(&mut self, kind: NodeKind) {
        self.set_status(kind, NodeStatus::Failed);
    }

    /// Record that a side-effecting node committed its external effect, keyed
    /// by `key` (e.g. the opened PR URL). The key survives `reset_from`.
    pub fn record_idempotency_key(&mut self, kind: NodeKind, key: impl Into<String>) {
        if let Some(node) = self.node_mut(kind) {
            node.idempotency_key = Some(key.into());
        }
    }

    /// The idempotency key of `kind`, if its external effect already landed.
    #[must_use]
    pub fn idempotency_key(&self, kind: NodeKind) -> Option<&str> {
        self.node(kind).and_then(|n| n.idempotency_key.as_deref())
    }

    /// Whether `kind` may be (re-)executed. A side-effecting node whose effect
    /// already landed must be skipped to stay idempotent across replays — the
    /// caller reuses the recorded [`Self::idempotency_key`] instead.
    #[must_use]
    pub fn should_execute(&self, kind: NodeKind) -> bool {
        !(kind.is_side_effecting() && self.idempotency_key(kind).is_some())
    }

    /// The earliest node not yet `Done` — where execution should (re)start.
    /// Returns `None` when the whole pipeline is complete.
    ///
    /// After a failure this is the failed node itself, since every upstream
    /// stage is still `Done` and reusable (memoized on `output_hash`).
    #[must_use]
    pub fn resume_point(&self) -> Option<NodeKind> {
        NodeKind::PIPELINE.iter().copied().find(|kind| {
            self.node(*kind)
                .is_none_or(|n| n.status != NodeStatus::Done)
        })
    }

    /// Reset `from` and every downstream node to `Pending`, clearing their
    /// output hashes. Upstream `Done` nodes are preserved so their memoized
    /// output is reused — this is the `lopi replay --from <node>` primitive.
    ///
    /// Idempotency keys are deliberately *not* cleared: a side-effecting node
    /// re-runs its compute on replay but reuses the already-committed external
    /// effect rather than duplicating it.
    pub fn reset_from(&mut self, from: NodeKind) {
        let cutoff = from.order();
        for node in &mut self.nodes {
            if node.kind.order() >= cutoff {
                node.status = NodeStatus::Pending;
                node.output_hash = None;
            }
        }
    }

    /// True when every node is `Done`.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.nodes.iter().all(|n| n.status == NodeStatus::Done)
    }

    /// Directed edges `(from, to)` derived from each node's `depends_on`.
    #[must_use]
    pub fn edges(&self) -> Vec<(NodeKind, NodeKind)> {
        self.nodes
            .iter()
            .flat_map(|n| n.depends_on.iter().map(move |dep| (*dep, n.kind)))
            .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn canonical_has_seven_nodes_in_pipeline_order() {
        let dag = AgentDag::canonical();
        assert_eq!(dag.nodes.len(), 7);
        let kinds: Vec<_> = dag.nodes.iter().map(|n| n.kind).collect();
        assert_eq!(kinds, NodeKind::PIPELINE.to_vec());
    }

    #[test]
    fn canonical_dependencies_are_linear() {
        let dag = AgentDag::canonical();
        assert!(dag.node(NodeKind::Plan).unwrap().depends_on.is_empty());
        assert_eq!(
            dag.node(NodeKind::Implement).unwrap().depends_on,
            vec![NodeKind::Plan]
        );
        assert_eq!(
            dag.node(NodeKind::Pr).unwrap().depends_on,
            vec![NodeKind::Diff]
        );
    }

    #[test]
    fn resume_point_is_plan_when_all_pending() {
        let dag = AgentDag::canonical();
        assert_eq!(dag.resume_point(), Some(NodeKind::Plan));
    }

    #[test]
    fn resume_point_skips_done_prefix() {
        let mut dag = AgentDag::canonical();
        dag.complete_node(NodeKind::Plan, "h1");
        dag.complete_node(NodeKind::Implement, "h2");
        assert_eq!(dag.resume_point(), Some(NodeKind::Test));
    }

    #[test]
    fn resume_point_returns_failed_node() {
        let mut dag = AgentDag::canonical();
        dag.complete_node(NodeKind::Plan, "h1");
        dag.complete_node(NodeKind::Implement, "h2");
        dag.fail_node(NodeKind::Test);
        // Upstream stays Done and reusable; resume lands on the failed Test.
        assert_eq!(dag.resume_point(), Some(NodeKind::Test));
        assert_eq!(
            dag.node(NodeKind::Plan).unwrap().output_hash.as_deref(),
            Some("h1")
        );
    }

    #[test]
    fn resume_point_none_when_complete() {
        let mut dag = AgentDag::canonical();
        for kind in NodeKind::PIPELINE {
            dag.complete_node(kind, "h");
        }
        assert!(dag.is_complete());
        assert_eq!(dag.resume_point(), None);
    }

    #[test]
    fn reset_from_preserves_upstream_clears_downstream() {
        let mut dag = AgentDag::canonical();
        for kind in NodeKind::PIPELINE {
            dag.complete_node(kind, "h");
        }
        dag.reset_from(NodeKind::Score);
        // Plan..Test preserved (memoized).
        assert_eq!(dag.node(NodeKind::Test).unwrap().status, NodeStatus::Done);
        assert!(dag.node(NodeKind::Test).unwrap().output_hash.is_some());
        // Score..Pr reset.
        assert_eq!(
            dag.node(NodeKind::Score).unwrap().status,
            NodeStatus::Pending
        );
        assert!(dag.node(NodeKind::Pr).unwrap().output_hash.is_none());
        assert_eq!(dag.resume_point(), Some(NodeKind::Score));
    }

    #[test]
    fn edges_form_six_link_chain() {
        let dag = AgentDag::canonical();
        let edges = dag.edges();
        assert_eq!(edges.len(), 6);
        assert!(edges.contains(&(NodeKind::Plan, NodeKind::Implement)));
        assert!(edges.contains(&(NodeKind::Diff, NodeKind::Pr)));
    }

    #[test]
    fn node_kind_serialises_snake_case() {
        assert_eq!(serde_json::to_string(&NodeKind::Pr).unwrap(), "\"pr\"");
        assert_eq!(NodeKind::Verify.to_string(), "verify");
    }

    #[test]
    fn dag_round_trips_through_json() {
        let mut dag = AgentDag::canonical();
        dag.complete_node(NodeKind::Plan, "abc");
        dag.fail_node(NodeKind::Implement);
        let json = serde_json::to_string(&dag).unwrap();
        let back: AgentDag = serde_json::from_str(&json).unwrap();
        assert_eq!(back.nodes.len(), 7);
        assert_eq!(
            back.node(NodeKind::Plan).unwrap().output_hash.as_deref(),
            Some("abc")
        );
        assert_eq!(
            back.node(NodeKind::Implement).unwrap().status,
            NodeStatus::Failed
        );
    }

    #[test]
    fn predecessor_chain_is_correct() {
        assert_eq!(NodeKind::Plan.predecessor(), None);
        assert_eq!(NodeKind::Pr.predecessor(), Some(NodeKind::Diff));
    }

    #[test]
    fn node_kind_and_status_from_str_round_trip() {
        use std::str::FromStr;
        for kind in NodeKind::PIPELINE {
            assert_eq!(NodeKind::from_str(kind.as_str()).unwrap(), kind);
        }
        assert!(NodeKind::from_str("nope").is_err());
        assert_eq!(NodeStatus::from_str("done").unwrap(), NodeStatus::Done);
        assert_eq!(NodeStatus::from_str("failed").unwrap(), NodeStatus::Failed);
        assert!(NodeStatus::from_str("weird").is_err());
    }

    #[test]
    fn only_pr_is_side_effecting() {
        assert!(NodeKind::Pr.is_side_effecting());
        for kind in [
            NodeKind::Plan,
            NodeKind::Implement,
            NodeKind::Test,
            NodeKind::Score,
            NodeKind::Verify,
            NodeKind::Diff,
        ] {
            assert!(
                !kind.is_side_effecting(),
                "{kind} must not be side-effecting"
            );
        }
    }

    #[test]
    fn committed_side_effect_blocks_re_execution() {
        let mut dag = AgentDag::canonical();
        // A non-committed Pr is freely executable.
        assert!(dag.should_execute(NodeKind::Pr));
        dag.record_idempotency_key(NodeKind::Pr, "https://github.com/org/repo/pull/7");
        // Once the PR exists, re-running Pr must be skipped (reuse the key).
        assert!(!dag.should_execute(NodeKind::Pr));
        assert_eq!(
            dag.idempotency_key(NodeKind::Pr),
            Some("https://github.com/org/repo/pull/7")
        );
        // Non-side-effecting nodes are always executable.
        assert!(dag.should_execute(NodeKind::Test));
    }

    #[test]
    fn reset_from_preserves_idempotency_key() {
        let mut dag = AgentDag::canonical();
        dag.complete_node(NodeKind::Pr, "hash");
        dag.record_idempotency_key(NodeKind::Pr, "pr-url");
        dag.reset_from(NodeKind::Plan);
        // Compute state is rewound …
        assert_eq!(dag.node(NodeKind::Pr).unwrap().status, NodeStatus::Pending);
        assert!(dag.node(NodeKind::Pr).unwrap().output_hash.is_none());
        // … but the external effect's key survives, so replay won't duplicate it.
        assert_eq!(dag.idempotency_key(NodeKind::Pr), Some("pr-url"));
        assert!(!dag.should_execute(NodeKind::Pr));
    }
}
