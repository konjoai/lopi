//! Plan artifact — Sprint P1 (review-pipeline plan, Phase 1, section 2).
//! Emitted by a readonly Planner, consumed by the Executor as its system
//! prompt, recorded to the ledger before the Executor spawns (see
//! `lopi-agent`'s `planner_executor` module for the handoff itself).
//!
//! The schema source of truth is `kiban/schemas/plan_artifact.schema.json`,
//! shared with downstream repos. This type enforces the same constraints
//! structurally rather than by convention or comment — most importantly,
//! `scope` can never be empty-but-valid: section 2.4's fail-open fix
//! escalates a future router to Tier 2 on a plan artifact whose scope is
//! absent, empty, or schema-invalid, precisely because a valid-but-empty
//! scope would make the scope-escape rule silently read as "nothing to
//! escape." `TryFrom<RawPlanArtifact>` is the single validation point, and
//! `#[serde(try_from = ..., into = ...)]` routes every deserialization path
//! (JSON, and the TOON round-trip via `serde_json::Value`) through it — a
//! `PlanArtifact` cannot be constructed, by any path, with an empty scope.
//!
//! `predicted_tier` grants zero routing authority (section 7.4) — logged
//! only, so prediction-vs-actual disagreement can be measured as a
//! scope-fidelity signal later. A future router MUST NOT read this field
//! when assigning a tier.
//!
//! Not the same thing as the existing free-form "plan text" the single-agent
//! loop already produces (`lopi-agent`'s `plan_via_api`/`plan_streamed`,
//! gated by the Phase 11 `plan_gate`) — that's the *same* agent's own
//! planning step, ungated by any tool profile, with no structured schema.
//! `PlanArtifact` is the separate, readonly-Planner-produced artifact this
//! sprint adds; confirmed via Sprint P1's PF-4 that no prior implementation
//! of this existed in `lopi-spec` or `lopi-context` (both cover different
//! concerns — spec-surface extraction and KV-cache eviction, respectively).

use serde::{Deserialize, Serialize};

/// A structured plan emitted by a readonly Planner. See the module doc
/// comment for the scope-non-empty invariant this type enforces
/// structurally, at every construction path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawPlanArtifact", into = "RawPlanArtifact")]
pub struct PlanArtifact {
    goal: String,
    scope: Vec<String>,
    invariants: Vec<String>,
    test_strategy: String,
    non_goals: Vec<String>,
    predicted_tier: Option<String>,
    planner_model: String,
    planner_commit: String,
}

/// Wire-format mirror of [`PlanArtifact`] with no invariants enforced. The
/// `TryFrom` impl below is the single place validation happens, so
/// `serde(try_from = ...)` covers every deserialization path with the exact
/// check [`PlanArtifact::new`] itself uses — there is no second, divergent
/// validation path.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawPlanArtifact {
    goal: String,
    scope: Vec<String>,
    #[serde(default)]
    invariants: Vec<String>,
    test_strategy: String,
    #[serde(default)]
    non_goals: Vec<String>,
    #[serde(default)]
    predicted_tier: Option<String>,
    planner_model: String,
    planner_commit: String,
}

/// Why a `PlanArtifact` failed to construct. Mirrors
/// `kiban/schemas/plan_artifact.schema.json`'s `required`/`minItems`
/// constraints (see that file for the two-level schema+fixture validation
/// design, section 7.3).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanArtifactError {
    /// `scope` was empty, or every entry in it was empty/whitespace-only.
    /// Section 2.4's fail-open fix: an empty scope must never be
    /// representable as valid, or a future router's scope-escape rule
    /// silently reads it as "nothing to escape."
    #[error("plan artifact scope must not be empty (section 2.4 fail-open fix)")]
    EmptyScope,
    /// `goal` was empty or whitespace-only.
    #[error("plan artifact goal must not be empty")]
    EmptyGoal,
    /// `test_strategy` was empty or whitespace-only.
    #[error("plan artifact test_strategy must not be empty")]
    EmptyTestStrategy,
    /// `planner_model` was empty.
    #[error("plan artifact planner_model must not be empty")]
    EmptyPlannerModel,
    /// `planner_commit` was empty.
    #[error("plan artifact planner_commit must not be empty")]
    EmptyPlannerCommit,
}

impl TryFrom<RawPlanArtifact> for PlanArtifact {
    type Error = PlanArtifactError;

    fn try_from(raw: RawPlanArtifact) -> Result<Self, Self::Error> {
        if raw.scope.is_empty() || raw.scope.iter().any(|s| s.trim().is_empty()) {
            return Err(PlanArtifactError::EmptyScope);
        }
        if raw.goal.trim().is_empty() {
            return Err(PlanArtifactError::EmptyGoal);
        }
        if raw.test_strategy.trim().is_empty() {
            return Err(PlanArtifactError::EmptyTestStrategy);
        }
        if raw.planner_model.trim().is_empty() {
            return Err(PlanArtifactError::EmptyPlannerModel);
        }
        if raw.planner_commit.trim().is_empty() {
            return Err(PlanArtifactError::EmptyPlannerCommit);
        }
        Ok(Self {
            goal: raw.goal,
            scope: raw.scope,
            invariants: raw.invariants,
            test_strategy: raw.test_strategy,
            non_goals: raw.non_goals,
            predicted_tier: raw.predicted_tier,
            planner_model: raw.planner_model,
            planner_commit: raw.planner_commit,
        })
    }
}

impl From<PlanArtifact> for RawPlanArtifact {
    fn from(p: PlanArtifact) -> Self {
        Self {
            goal: p.goal,
            scope: p.scope,
            invariants: p.invariants,
            test_strategy: p.test_strategy,
            non_goals: p.non_goals,
            predicted_tier: p.predicted_tier,
            planner_model: p.planner_model,
            planner_commit: p.planner_commit,
        }
    }
}

impl PlanArtifact {
    /// Construct directly, running the same validation
    /// [`TryFrom<RawPlanArtifact>`](#impl-TryFrom<RawPlanArtifact>-for-PlanArtifact)
    /// applies on deserialize.
    ///
    /// # Errors
    /// Returns [`PlanArtifactError`] if `scope`/`goal`/`test_strategy`/
    /// `planner_model`/`planner_commit` fail their non-empty constraint.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        goal: impl Into<String>,
        scope: Vec<String>,
        invariants: Vec<String>,
        test_strategy: impl Into<String>,
        non_goals: Vec<String>,
        predicted_tier: Option<String>,
        planner_model: impl Into<String>,
        planner_commit: impl Into<String>,
    ) -> Result<Self, PlanArtifactError> {
        RawPlanArtifact {
            goal: goal.into(),
            scope,
            invariants,
            test_strategy: test_strategy.into(),
            non_goals,
            predicted_tier,
            planner_model: planner_model.into(),
            planner_commit: planner_commit.into(),
        }
        .try_into()
    }

    /// The task goal this plan addresses, in the Planner's own words.
    #[must_use]
    pub fn goal(&self) -> &str {
        &self.goal
    }

    /// Explicit file/glob list the Executor may touch. Never empty — see
    /// the module doc comment.
    #[must_use]
    pub fn scope(&self) -> &[String] {
        &self.scope
    }

    /// Hard constraints the Executor's diff must preserve.
    #[must_use]
    pub fn invariants(&self) -> &[String] {
        &self.invariants
    }

    /// How the Executor is expected to verify its own work.
    #[must_use]
    pub fn test_strategy(&self) -> &str {
        &self.test_strategy
    }

    /// Explicitly out of scope for this plan.
    #[must_use]
    pub fn non_goals(&self) -> &[String] {
        &self.non_goals
    }

    /// The Planner's predicted review tier. Zero routing authority (section
    /// 7.4) — logged only. A future router MUST NOT read this when
    /// assigning a tier.
    #[must_use]
    pub fn predicted_tier(&self) -> Option<&str> {
        self.predicted_tier.as_deref()
    }

    /// The exact model version string that produced this plan.
    #[must_use]
    pub fn planner_model(&self) -> &str {
        &self.planner_model
    }

    /// The repo commit SHA the Planner read against.
    #[must_use]
    pub fn planner_commit(&self) -> &str {
        &self.planner_commit
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn valid() -> PlanArtifact {
        PlanArtifact::new(
            "Add a readonly Planner tool profile",
            vec!["crates/lopi-core/src/tool_profile.rs".to_string()],
            vec!["Mutating stays the default".to_string()],
            "Live-spawn under Readonly, confirm write denial",
            vec!["No critic, no router, no gate".to_string()],
            Some("1".to_string()),
            "claude-sonnet-5",
            "6b57438",
        )
        .unwrap()
    }

    #[test]
    fn valid_plan_constructs() {
        let p = valid();
        assert_eq!(p.scope(), ["crates/lopi-core/src/tool_profile.rs"]);
        assert_eq!(p.predicted_tier(), Some("1"));
    }

    #[test]
    fn rejects_empty_scope() {
        let err = PlanArtifact::new(
            "goal",
            vec![],
            vec![],
            "strategy",
            vec![],
            None,
            "model",
            "commit",
        )
        .unwrap_err();
        assert_eq!(err, PlanArtifactError::EmptyScope);
    }

    #[test]
    fn rejects_whitespace_only_scope_entry() {
        let err = PlanArtifact::new(
            "goal",
            vec!["   ".to_string()],
            vec![],
            "strategy",
            vec![],
            None,
            "model",
            "commit",
        )
        .unwrap_err();
        assert_eq!(err, PlanArtifactError::EmptyScope);
    }

    #[test]
    fn rejects_empty_goal() {
        let err = PlanArtifact::new(
            "",
            vec!["src/".to_string()],
            vec![],
            "strategy",
            vec![],
            None,
            "model",
            "commit",
        )
        .unwrap_err();
        assert_eq!(err, PlanArtifactError::EmptyGoal);
    }

    #[test]
    fn rejects_empty_test_strategy() {
        let err = PlanArtifact::new(
            "goal",
            vec!["src/".to_string()],
            vec![],
            "",
            vec![],
            None,
            "model",
            "commit",
        )
        .unwrap_err();
        assert_eq!(err, PlanArtifactError::EmptyTestStrategy);
    }

    #[test]
    fn deserializing_json_with_empty_scope_fails() {
        let json = serde_json::json!({
            "goal": "g",
            "scope": [],
            "invariants": [],
            "test_strategy": "t",
            "non_goals": [],
            "predicted_tier": null,
            "planner_model": "m",
            "planner_commit": "c",
        });
        let result: Result<PlanArtifact, _> = serde_json::from_value(json);
        assert!(
            result.is_err(),
            "empty scope must fail deserialization, not just PlanArtifact::new"
        );
    }

    #[test]
    fn deserializing_json_with_missing_scope_fails() {
        let json = serde_json::json!({
            "goal": "g",
            "test_strategy": "t",
            "planner_model": "m",
            "planner_commit": "c",
        });
        let result: Result<PlanArtifact, _> = serde_json::from_value(json);
        assert!(result.is_err(), "missing scope must fail deserialization");
    }

    #[test]
    fn json_round_trip_preserves_every_field() {
        let p = valid();
        let json = serde_json::to_value(&p).unwrap();
        let back: PlanArtifact = serde_json::from_value(json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn predicted_tier_defaults_to_none_when_absent() {
        let json = serde_json::json!({
            "goal": "g",
            "scope": ["src/"],
            "test_strategy": "t",
            "planner_model": "m",
            "planner_commit": "c",
        });
        let p: PlanArtifact = serde_json::from_value(json).unwrap();
        assert_eq!(p.predicted_tier(), None);
        assert!(p.invariants().is_empty());
        assert!(p.non_goals().is_empty());
    }
}
