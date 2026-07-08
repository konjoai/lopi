//! Orchestration topology hints (Sprint T).
//!
//! A [`crate::topology::TopologyHint`] describes how a task's work should be decomposed across
//! agents. It is an *advisory* signal attached to a [`crate::Task`]: the
//! orchestrator's classifier proposes one, and the dispatcher may branch on it.
//! Inspired by AdaptOrch (arXiv 2602.16873), which shows topology-aware routing
//! beats any single static topology by 12–23% on identical models.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// How a task should be spread across agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyHint {
    /// Independent sub-goals fan out across N isolated worktree branches.
    Parallel,
    /// Strictly ordered steps run on a single branch with checkpoint-resume.
    Sequential,
    /// A planner agent decomposes the goal and spawns child tasks.
    Hierarchical,
    /// A mix — some steps parallel, others ordered. Default when unsure.
    Hybrid,
}

impl TopologyHint {
    /// Lowercase wire/display name (`"parallel"`, `"sequential"`, …).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Parallel => "parallel",
            Self::Sequential => "sequential",
            Self::Hierarchical => "hierarchical",
            Self::Hybrid => "hybrid",
        }
    }
}

impl fmt::Display for TopologyHint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TopologyHint {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "parallel" => Ok(Self::Parallel),
            "sequential" | "serial" => Ok(Self::Sequential),
            "hierarchical" | "hierarchy" => Ok(Self::Hierarchical),
            "hybrid" | "mixed" => Ok(Self::Hybrid),
            other => Err(format!("unknown topology hint: {other}")),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn display_and_from_str_round_trip() {
        for hint in [
            TopologyHint::Parallel,
            TopologyHint::Sequential,
            TopologyHint::Hierarchical,
            TopologyHint::Hybrid,
        ] {
            let s = hint.to_string();
            assert_eq!(TopologyHint::from_str(&s).unwrap(), hint);
        }
    }

    #[test]
    fn from_str_accepts_aliases_and_case() {
        assert_eq!(
            TopologyHint::from_str("SERIAL").unwrap(),
            TopologyHint::Sequential
        );
        assert_eq!(
            TopologyHint::from_str("  Mixed ").unwrap(),
            TopologyHint::Hybrid
        );
    }

    #[test]
    fn from_str_rejects_unknown() {
        assert!(TopologyHint::from_str("ring").is_err());
    }

    #[test]
    fn serde_uses_snake_case() {
        let json = serde_json::to_string(&TopologyHint::Hierarchical).unwrap();
        assert_eq!(json, "\"hierarchical\"");
        let back: TopologyHint = serde_json::from_str(&json).unwrap();
        assert_eq!(back, TopologyHint::Hierarchical);
    }
}
