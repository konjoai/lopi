//! Per-run drill-down endpoints — the backend for the Loop run-trace view.
//!
//! - `GET /api/loop-engineering/runs` — recent runs for the picker.
//! - `GET /api/loop-engineering/runs/:id` — one run's attempt-by-attempt trace,
//!   stitching `attempts` + `verifier_verdicts` + per-attempt `turn_metrics`
//!   token/cost totals into a single timeline.
//!
//! Both routes sit behind the shared Bearer-auth + rate-limit middleware.

use super::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_memory::{RunAttemptRow, RunTurnAgg};
use serde_json::{json, Value};

/// How many recent runs the picker lists.
const RUN_WINDOW: i64 = 40;

/// `GET /api/loop-engineering/runs` — recent runs summarised for the picker.
pub(super) async fn list_runs(State(s): State<AppState>) -> impl IntoResponse {
    let runs = s.store.recent_runs(RUN_WINDOW).await.unwrap_or_default();
    let runs: Vec<Value> = runs
        .into_iter()
        .map(|r| {
            json!({
                "task_id": r.task_id,
                "goal": r.goal,
                "status": r.status,
                "attempts": r.attempts,
                "best_score": r.best_score.unwrap_or(0.0),
                "final_outcome": r.final_outcome,
                "last_at": r.last_at,
            })
        })
        .collect();
    (StatusCode::OK, Json(json!({ "runs": runs }))).into_response()
}

/// `GET /api/loop-engineering/runs/:id` — the attempt-by-attempt trace for a run.
pub(super) async fn get_run_trace(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let meta = s.store.run_task_meta(&id).await.ok().flatten();
    let Some((goal, status)) = meta else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "run not found" })),
        )
            .into_response();
    };

    let attempts = s.store.run_attempts(&id).await.unwrap_or_default();
    let verdicts = s
        .store
        .load_verifier_verdicts(&id)
        .await
        .unwrap_or_default();
    let aggs = s.store.run_turn_aggregates(&id).await.unwrap_or_default();

    let timeline: Vec<Value> = attempts
        .iter()
        .map(|a| attempt_json(a, &verdicts, &aggs))
        .collect();

    (
        StatusCode::OK,
        Json(json!({
            "task_id": id,
            "goal": goal,
            "status": status,
            "attempts": timeline,
        })),
    )
        .into_response()
}

/// Project one attempt, grafting on its verifier verdict and token/cost totals.
fn attempt_json(
    a: &RunAttemptRow,
    verdicts: &[lopi_memory::VerifierVerdictRow],
    aggs: &[RunTurnAgg],
) -> Value {
    let verifier = verdicts
        .iter()
        .find(|v| v.attempt == a.attempt_num)
        .map(|v| {
            json!({
                "passed": v.passed != 0,
                "confidence": v.confidence,
                "gaps": parse_str_array(&v.gaps_json),
                "fix_hints": parse_str_array(&v.fix_hints_json),
            })
        });
    let agg = aggs.iter().find(|t| t.attempt_number == a.attempt_num);
    json!({
        "attempt": a.attempt_num,
        "test_pass_rate": a.test_pass_rate.unwrap_or(0.0),
        "lint_errors": a.lint_errors.unwrap_or(0),
        "diff_lines": a.diff_lines.unwrap_or(0),
        "outcome": a.outcome,
        "errors": a.errors.as_deref().map(parse_str_array).unwrap_or_default(),
        "verifier": verifier,
        "tokens": agg.map_or(0, |t| t.tokens),
        "cost_usd": agg.map_or(0.0, |t| t.cost_usd),
        "created_at": a.created_at,
    })
}

/// Parse a JSON string array, returning an empty vec on any malformed input.
fn parse_str_array(s: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn row(n: i64, outcome: &str, errors: Option<&str>) -> RunAttemptRow {
        RunAttemptRow {
            attempt_num: n,
            test_pass_rate: Some(0.7),
            lint_errors: Some(2),
            diff_lines: Some(30),
            outcome: outcome.into(),
            errors: errors.map(Into::into),
            created_at: "2026-06-21T00:00:00Z".into(),
        }
    }

    fn verdict(attempt: i64, passed: i64) -> lopi_memory::VerifierVerdictRow {
        lopi_memory::VerifierVerdictRow {
            id: "v".into(),
            task_id: "t".into(),
            attempt,
            passed,
            gaps_json: r#"["missing test"]"#.into(),
            fix_hints_json: r#"["add a test"]"#.into(),
            confidence: 0.8,
            model_used: "opus".into(),
            ts: "2026-06-21T00:00:00Z".into(),
        }
    }

    #[test]
    fn parse_str_array_tolerates_garbage() {
        assert_eq!(parse_str_array(r#"["a","b"]"#), vec!["a", "b"]);
        assert!(parse_str_array("not json").is_empty());
        assert!(parse_str_array("").is_empty());
    }

    #[test]
    fn attempt_json_grafts_verifier_and_aggregates() {
        let aggs = vec![RunTurnAgg {
            attempt_number: 1,
            tokens: 1500,
            cost_usd: 0.05,
        }];
        let v = attempt_json(
            &row(1, "success", Some(r#"["boom"]"#)),
            &[verdict(1, 1)],
            &aggs,
        );
        assert_eq!(v["attempt"], 1);
        assert_eq!(v["tokens"], 1500);
        assert_eq!(v["verifier"]["passed"], true);
        assert_eq!(v["verifier"]["gaps"][0], "missing test");
        assert_eq!(v["errors"][0], "boom");
    }

    #[test]
    fn attempt_json_handles_missing_verifier_and_aggregates() {
        let v = attempt_json(&row(2, "retry", None), &[], &[]);
        assert!(v["verifier"].is_null());
        assert_eq!(v["tokens"], 0);
        assert_eq!(v["cost_usd"], 0.0);
        assert!(v["errors"].as_array().unwrap().is_empty());
    }
}
