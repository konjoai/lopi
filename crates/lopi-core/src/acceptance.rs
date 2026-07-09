//! Eval-Execution-1 (A1) — the goal/acceptance object (cross-cutting seam #1).
//!
//! A single, machine-checkable success condition used at **both** loop scope
//! ([`crate::Task::acceptance`]) and stack scope (B1 will reuse it verbatim).
//! It is a small, composable set of tier-tagged criteria the tiered eval
//! executor scores a loop against — the authoring surface for the UI's inert
//! `EvalRef{name,tier}` tags, which now compile into real [`AcceptanceCheck`]s.
//!
//! This module owns only the *schema*. The pluggable evaluator interface and
//! the tiered executor live in `lopi-agent`; the result object lives in
//! [`crate::eval_outcome`] so `lopi-memory` can persist it without depending on
//! `lopi-agent`.

use crate::Rubric;
use serde::{Deserialize, Serialize};

/// Which evaluation tier a [`AcceptanceCheck`] runs at, ordered cheapest →
/// most expensive. The executor decides at the cheapest tier that can decide:
/// objective, machine-checkable criteria route to [`EvalTier::ExecutionOk`] /
/// [`EvalTier::ShellTest`] (deterministic, un-gameable, free) and the judge is
/// reserved for genuine judgment.
///
/// The serde representation is the UI's `EvalTier` string union
/// (`base`/`test`/`judge`/`suite`, `web/src/lib/stores/stack.ts`) so one schema
/// round-trips through the API and the store unchanged (cross-cutting seam #1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EvalTier {
    /// Tier 0 — the code builds/runs clean (deterministic). UI: `base`.
    #[serde(rename = "base")]
    ExecutionOk,
    /// Tier 1 — a shell/test command exits `0` (deterministic). UI: `test`.
    #[serde(rename = "test")]
    ShellTest,
    /// Tier 2 — a separate-model judge grades against a rubric. UI: `judge`.
    #[serde(rename = "judge")]
    Judge,
    /// Tier 3 — a named quality suite (KCQF). UI: `suite`.
    #[serde(rename = "suite")]
    Suite,
}

impl EvalTier {
    /// The UI `EvalTier` string this tier maps to (`base`/`test`/`judge`/`suite`).
    #[must_use]
    pub const fn as_ui_str(self) -> &'static str {
        match self {
            Self::ExecutionOk => "base",
            Self::ShellTest => "test",
            Self::Judge => "judge",
            Self::Suite => "suite",
        }
    }

    /// Parse a UI `EvalTier` string back into a tier. Returns `None` for any
    /// unrecognised string rather than guessing a default.
    #[must_use]
    pub fn from_ui_str(s: &str) -> Option<Self> {
        match s {
            "base" => Some(Self::ExecutionOk),
            "test" => Some(Self::ShellTest),
            "judge" => Some(Self::Judge),
            "suite" => Some(Self::Suite),
            _ => None,
        }
    }

    /// Whether this tier is deterministic (objective, un-gameable, no model
    /// call). The routing rule prefers a deterministic tier for any criterion
    /// that can be made objective, so gaming can't be argued around a judge.
    #[must_use]
    pub const fn is_deterministic(self) -> bool {
        matches!(self, Self::ExecutionOk | Self::ShellTest)
    }
}

/// Comparison operator for a [`MetricGate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    /// `reading >= threshold`.
    Gte,
    /// `reading > threshold`.
    Gt,
    /// `reading <= threshold`.
    Lte,
    /// `reading < threshold`.
    Lt,
}

impl Op {
    /// Apply the operator: does `reading <op> threshold` hold?
    #[must_use]
    pub fn holds(self, reading: f64, threshold: f64) -> bool {
        match self {
            Self::Gte => reading >= threshold,
            Self::Gt => reading > threshold,
            Self::Lte => reading <= threshold,
            Self::Lt => reading < threshold,
        }
    }
}

/// A machine-checkable numeric gate, e.g. `coverage >= 0.8`. Prefer this over a
/// judge check whenever the criterion is objective (input-completeness rule):
/// a metric read from raw tool output can't be argued around by a model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricGate {
    /// Metric name the reading is looked up by (e.g. `"coverage"`).
    pub name: String,
    /// Comparison operator.
    pub op: Op,
    /// Threshold the reading is compared against.
    pub threshold: f64,
}

impl MetricGate {
    /// Whether a `reading` for this metric satisfies the gate.
    #[must_use]
    pub fn satisfied_by(&self, reading: f64) -> bool {
        self.op.holds(reading, self.threshold)
    }
}

/// The tier-specific payload of an [`AcceptanceCheck`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CheckSpec {
    /// The code builds/runs clean — reuses the heuristic `Scorer`.
    ExecutionOk,
    /// A shell command; exit `0` = pass — reuses `run_guard_command`.
    Shell {
        /// The shell command string, run via `sh -c` in the repo root.
        cmd: String,
    },
    /// A separate-model verdict against a rubric, optionally gated on a metric.
    Judge {
        /// Criteria the judge grades against.
        rubric: Rubric,
        /// Optional objective metric gate layered on top of the judgment.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        metric: Option<MetricGate>,
    },
    /// A named quality suite (KCQF etc.).
    Suite {
        /// Suite name (e.g. `"kcqf"`).
        name: String,
    },
}

impl CheckSpec {
    /// The [`EvalTier`] this spec must run at. Keeps `tier` and `spec`
    /// consistent — an `AcceptanceCheck` built via [`AcceptanceCheck::new`]
    /// can never claim a tier its spec doesn't belong to.
    #[must_use]
    pub const fn tier(&self) -> EvalTier {
        match self {
            Self::ExecutionOk => EvalTier::ExecutionOk,
            Self::Shell { .. } => EvalTier::ShellTest,
            Self::Judge { .. } => EvalTier::Judge,
            Self::Suite { .. } => EvalTier::Suite,
        }
    }
}

/// One tier-tagged criterion of an [`Acceptance`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcceptanceCheck {
    /// The tier this check runs at (always equals `spec.tier()`).
    pub tier: EvalTier,
    /// The tier-specific payload.
    pub spec: CheckSpec,
    /// Weight of this check in the scalar score A3's ratchet reads. `[0, ∞)`;
    /// checks with weight `0` contribute critique/verdict but not score.
    pub weight: f32,
    /// `true` — a hard gate: a fail/error here fails the whole acceptance.
    /// `false` — a soft signal that feeds the score/critique only.
    pub required: bool,
}

impl AcceptanceCheck {
    /// Build a required, unit-weighted check from a spec, deriving `tier` from
    /// the spec so the two can never disagree.
    #[must_use]
    pub fn new(spec: CheckSpec) -> Self {
        Self {
            tier: spec.tier(),
            spec,
            weight: 1.0,
            required: true,
        }
    }

    /// Set the check's weight (builder).
    #[must_use]
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    /// Mark the check soft (non-`required`) — a score/critique signal that
    /// never blocks the gate (builder).
    #[must_use]
    pub fn soft(mut self) -> Self {
        self.required = false;
        self
    }
}

/// A machine-checkable success condition for a loop or a stack.
///
/// AND semantics across `required` checks (all must pass); non-`required`
/// checks feed only the scalar score and critique. `checks` is evaluated in
/// tier order so the executor can short-circuit on the first required failure
/// at a cheap tier before paying for the judge.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Acceptance {
    /// The tier-tagged criteria that define the goal.
    pub checks: Vec<AcceptanceCheck>,
}

impl Acceptance {
    /// An acceptance with no checks — vacuously met. Callers should treat an
    /// empty acceptance as "no explicit goal set" and fall back to the legacy
    /// `score.passed()` gate.
    #[must_use]
    pub fn empty() -> Self {
        Self { checks: Vec::new() }
    }

    /// Build from an explicit list of checks.
    #[must_use]
    pub fn new(checks: Vec<AcceptanceCheck>) -> Self {
        Self { checks }
    }

    /// `true` when there is nothing to evaluate.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.checks.is_empty()
    }

    /// The checks in tier order (cheapest first) — the order the tiered
    /// executor runs and short-circuits in. Returns owned clones so the caller
    /// can drive them without borrowing `self`.
    #[must_use]
    pub fn ordered(&self) -> Vec<AcceptanceCheck> {
        let mut out = self.checks.clone();
        out.sort_by_key(|c| c.tier);
        out
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn tier_ordering_is_cheapest_first() {
        assert!(EvalTier::ExecutionOk < EvalTier::ShellTest);
        assert!(EvalTier::ShellTest < EvalTier::Judge);
        assert!(EvalTier::Judge < EvalTier::Suite);
    }

    #[test]
    fn tier_ui_string_round_trips() {
        for t in [
            EvalTier::ExecutionOk,
            EvalTier::ShellTest,
            EvalTier::Judge,
            EvalTier::Suite,
        ] {
            assert_eq!(EvalTier::from_ui_str(t.as_ui_str()), Some(t));
        }
        assert_eq!(EvalTier::from_ui_str("nonsense"), None);
    }

    #[test]
    fn only_deterministic_tiers_report_deterministic() {
        assert!(EvalTier::ExecutionOk.is_deterministic());
        assert!(EvalTier::ShellTest.is_deterministic());
        assert!(!EvalTier::Judge.is_deterministic());
        assert!(!EvalTier::Suite.is_deterministic());
    }

    #[test]
    fn tier_serde_uses_the_ui_union() {
        let json = serde_json::to_string(&EvalTier::Judge).unwrap();
        assert_eq!(json, "\"judge\"");
        let back: EvalTier = serde_json::from_str("\"base\"").unwrap();
        assert_eq!(back, EvalTier::ExecutionOk);
    }

    #[test]
    fn op_holds_covers_all_operators() {
        assert!(Op::Gte.holds(0.8, 0.8));
        assert!(!Op::Gt.holds(0.8, 0.8));
        assert!(Op::Gt.holds(0.9, 0.8));
        assert!(Op::Lte.holds(0.8, 0.8));
        assert!(Op::Lt.holds(0.7, 0.8));
        assert!(!Op::Lt.holds(0.8, 0.8));
    }

    #[test]
    fn metric_gate_satisfied_by_reading() {
        let gate = MetricGate {
            name: "coverage".into(),
            op: Op::Gte,
            threshold: 0.8,
        };
        assert!(gate.satisfied_by(0.85));
        assert!(!gate.satisfied_by(0.79));
    }

    #[test]
    fn check_spec_tier_matches_variant() {
        assert_eq!(CheckSpec::ExecutionOk.tier(), EvalTier::ExecutionOk);
        assert_eq!(
            CheckSpec::Shell { cmd: "x".into() }.tier(),
            EvalTier::ShellTest
        );
        assert_eq!(
            CheckSpec::Judge {
                rubric: Rubric::default(),
                metric: None,
            }
            .tier(),
            EvalTier::Judge
        );
        assert_eq!(
            CheckSpec::Suite {
                name: "kcqf".into()
            }
            .tier(),
            EvalTier::Suite
        );
    }

    #[test]
    fn new_check_derives_tier_and_defaults_required_unit_weight() {
        let c = AcceptanceCheck::new(CheckSpec::ExecutionOk);
        assert_eq!(c.tier, EvalTier::ExecutionOk);
        assert!(c.required);
        assert_eq!(c.weight, 1.0);
    }

    #[test]
    fn check_builders_set_weight_and_soft() {
        let c = AcceptanceCheck::new(CheckSpec::ExecutionOk)
            .with_weight(2.5)
            .soft();
        assert_eq!(c.weight, 2.5);
        assert!(!c.required);
    }

    #[test]
    fn ordered_sorts_by_tier_regardless_of_input_order() {
        let acc = Acceptance::new(vec![
            AcceptanceCheck::new(CheckSpec::Suite { name: "k".into() }),
            AcceptanceCheck::new(CheckSpec::ExecutionOk),
            AcceptanceCheck::new(CheckSpec::Judge {
                rubric: Rubric::default(),
                metric: None,
            }),
            AcceptanceCheck::new(CheckSpec::Shell { cmd: "t".into() }),
        ]);
        let tiers: Vec<_> = acc.ordered().into_iter().map(|c| c.tier).collect();
        assert_eq!(
            tiers,
            vec![
                EvalTier::ExecutionOk,
                EvalTier::ShellTest,
                EvalTier::Judge,
                EvalTier::Suite,
            ]
        );
    }

    #[test]
    fn empty_acceptance_is_empty() {
        assert!(Acceptance::empty().is_empty());
        assert!(!Acceptance::new(vec![AcceptanceCheck::new(CheckSpec::ExecutionOk)]).is_empty());
    }

    #[test]
    fn acceptance_round_trips_through_json() {
        let acc = Acceptance::new(vec![
            AcceptanceCheck::new(CheckSpec::ExecutionOk),
            AcceptanceCheck::new(CheckSpec::Shell {
                cmd: "cargo test".into(),
            })
            .soft()
            .with_weight(0.0),
        ]);
        let json = serde_json::to_string(&acc).unwrap();
        let back: Acceptance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, acc);
    }
}
