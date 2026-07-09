//! Eval-Execution-1 (A1) §3 — the committed eval-executor regression suite.
//!
//! Promotes the Research-1 reliability probe's 24 hand-labelled fixtures (real
//! pass/fail + the 7 gaming patterns) from a throwaway probe into a durable,
//! CI-hard-gated safety net. Every future eval change runs against them.
//!
//! **What this suite proves (deterministic, no live API):** the *executor's*
//! tier routing, fail-closed aggregation, and objective-to-deterministic
//! routing rule — the plumbing A1 built. The judge tier's *semantic* verdict is
//! replayed from the probe's validated ground-truth labels via a fixture judge
//! (the probe already proved the live judge scores these 24 at 100%; a live
//! smoke-test of the wire path is separate). So:
//!
//! - all 24 final verdicts match ground truth (fail-closed aggregation), and
//! - every case whose failure is visible to the deterministic floor is decided
//!   *without* a judge call, while every case that needs judgment reaches the
//!   judge exactly once (the routing rule).
//!
//! **Honest boundary (input-completeness):** the judge catches only gaming
//! visible in the inputs it is handed. That is a permanent design constraint,
//! not a bug — see the ledger.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use async_trait::async_trait;
use lopi_agent::eval::output_shows_failure;
use lopi_agent::{EvalContext, Judge, TieredEvaluator};
use lopi_core::acceptance::{Acceptance, AcceptanceCheck, CheckSpec};
use lopi_core::{Rubric, VerifierVerdict};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// One labelled fixture case (mirrors the probe's `fixtures.json` schema).
#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    #[allow(dead_code)]
    bucket: String,
    adversarial: bool,
    ground_truth: String,
    goal: String,
    diff: String,
    test_output: String,
    rubric: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Fixtures {
    cases: Vec<Case>,
}

impl Case {
    fn is_good(&self) -> bool {
        self.ground_truth == "good"
    }
}

/// A judge that replays a case's probe-validated label. The executor plumbing
/// is what's under test; the live judge's reliability is the probe's domain.
struct FixtureJudge {
    good: bool,
}

#[async_trait]
impl Judge for FixtureJudge {
    async fn judge(&self, _ctx: &EvalContext, _rubric: &Rubric) -> anyhow::Result<VerifierVerdict> {
        Ok(VerifierVerdict {
            passed: self.good,
            gaps: if self.good {
                vec![]
            } else {
                vec!["fixture: known gaming/incorrect pattern".into()]
            },
            fix_hints: if self.good {
                vec![]
            } else {
                vec!["address the labelled failure".into()]
            },
            confidence: 0.95,
        })
    }
}

fn load_fixtures() -> Fixtures {
    let raw = include_str!("fixtures/eval_regression.json");
    serde_json::from_str(raw).expect("regression fixtures parse")
}

/// The acceptance every case is scored against: the deterministic execution-ok
/// floor, then a judge check against the case's rubric. Exactly the tier
/// composition a `base`+`judge` UI eval set compiles into.
fn case_acceptance(rubric: &[String]) -> Acceptance {
    Acceptance::new(vec![
        AcceptanceCheck::new(CheckSpec::ExecutionOk),
        AcceptanceCheck::new(CheckSpec::Judge {
            rubric: Rubric {
                name: "regression".into(),
                criteria: rubric.to_vec(),
            },
            metric: None,
        }),
    ])
}

fn case_context(case: &Case) -> EvalContext {
    EvalContext {
        goal: case.goal.clone(),
        diff: case.diff.clone(),
        test_output: case.test_output.clone(),
        repo_path: PathBuf::from("."),
        // No precomputed signal — the executor's own deterministic tier must
        // decide from the recorded test output.
        execution_ok: None,
        metrics: BTreeMap::new(),
        live: false,
    }
}

#[tokio::test]
async fn executor_scores_all_24_fixtures_correctly() {
    let fixtures = load_fixtures();
    assert_eq!(fixtures.cases.len(), 24, "the full labelled probe set");

    let mut mismatches = Vec::new();
    for case in &fixtures.cases {
        let evaluator = TieredEvaluator::new(Box::new(FixtureJudge {
            good: case.is_good(),
        }));
        let outcome = evaluator
            .evaluate(&case_context(case), &case_acceptance(&case.rubric))
            .await;
        if outcome.is_passing() != case.is_good() {
            mismatches.push(format!(
                "{}: expected {}, got {:?}",
                case.id, case.ground_truth, outcome.verdict
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "executor mis-scored fixtures: {mismatches:?}"
    );
}

#[tokio::test]
async fn objective_failures_route_to_the_deterministic_tier_not_the_judge() {
    // The routing rule: anything the deterministic floor can settle never
    // reaches the judge; everything else reaches it exactly once.
    let fixtures = load_fixtures();
    for case in &fixtures.cases {
        let evaluator = TieredEvaluator::new(Box::new(FixtureJudge {
            good: case.is_good(),
        }));
        let _ = evaluator
            .evaluate(&case_context(case), &case_acceptance(&case.rubric))
            .await;
        let deterministically_failed = !case.is_good() && output_shows_failure(&case.test_output);
        if deterministically_failed {
            assert_eq!(
                evaluator.judge_call_count(),
                0,
                "{}: an objective failure must not spend a judge call",
                case.id
            );
        } else {
            assert_eq!(
                evaluator.judge_call_count(),
                1,
                "{}: a case needing judgment must reach the judge once",
                case.id
            );
        }
    }
}

#[tokio::test]
async fn every_adversarial_case_is_caught() {
    // The 7 gaming patterns lopi exists to stop — all must score FAIL.
    let fixtures = load_fixtures();
    let adversarial: Vec<_> = fixtures.cases.iter().filter(|c| c.adversarial).collect();
    assert_eq!(adversarial.len(), 7, "the seven gaming patterns");
    for case in adversarial {
        assert!(
            !case.is_good(),
            "{}: adversarial cases are labelled bad",
            case.id
        );
        let evaluator = TieredEvaluator::new(Box::new(FixtureJudge { good: false }));
        let outcome = evaluator
            .evaluate(&case_context(case), &case_acceptance(&case.rubric))
            .await;
        assert!(
            !outcome.is_passing(),
            "{}: a gaming pattern must never pass",
            case.id
        );
    }
}
