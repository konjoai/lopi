//! Loop Health aggregation endpoint — the backend for the Loop Health view.
//!
//! `GET /api/loop-engineering/health` projects the data the agent loop already
//! persists (`attempts`, `turn_metrics`, `verifier_verdicts`) into a single
//! observability snapshot: headline stats, an attempt-by-attempt score series,
//! the outcome distribution, a token/cost burn series, and the verifier pass
//! rate. Both the web and macOS Loop Health screens render this one payload.
//!
//! Route (behind the shared Bearer-auth + rate-limit middleware):
//! - `GET /api/loop-engineering/health` — the loop-health snapshot.

use super::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use lopi_memory::{LoopAttemptRow, LoopTurnRow};
use serde_json::{json, Value};

/// How many recent attempts / turns to project into the timelines.
const ATTEMPT_WINDOW: i64 = 60;
const TURN_WINDOW: i64 = 60;

/// `GET /api/loop-engineering/health` — the loop-health snapshot for the dashboard.
pub(super) async fn get_loop_health(State(s): State<AppState>) -> impl IntoResponse {
    let attempts = s
        .store
        .recent_loop_attempts(ATTEMPT_WINDOW)
        .await
        .unwrap_or_default();
    let outcomes = s.store.loop_outcome_counts().await.unwrap_or_default();
    let turns = s
        .store
        .recent_turn_metrics(TURN_WINDOW)
        .await
        .unwrap_or_default();
    let (verifier_passed, verifier_total) = s.store.verifier_pass_rate().await.unwrap_or((0, 0));

    (
        StatusCode::OK,
        Json(json!({
            "stats": headline_stats(&attempts, &turns, verifier_passed, verifier_total),
            "attempts": attempt_series(&attempts),
            "outcomes": outcome_series(&outcomes),
            "burn": burn_series(&turns),
        })),
    )
        .into_response()
}

/// Headline KPI tiles: run/attempt counts, success & verifier pass rates, spend.
fn headline_stats(
    attempts: &[LoopAttemptRow],
    turns: &[LoopTurnRow],
    verifier_passed: i64,
    verifier_total: i64,
) -> Value {
    let total_attempts = attempts.len();
    let successes = attempts.iter().filter(|a| a.outcome == "success").count();
    let runs: std::collections::HashSet<&str> =
        attempts.iter().map(|a| a.task_id.as_str()).collect();
    let spend: f64 = turns.iter().map(|t| t.estimated_cost_usd).sum();
    let tokens: i64 = turns.iter().map(|t| t.input_tokens + t.output_tokens).sum();
    json!({
        "runs": runs.len(),
        "attempts": total_attempts,
        "success_rate": ratio(successes, total_attempts),
        "verifier_pass_rate": ratio_i(verifier_passed, verifier_total),
        "verifier_total": verifier_total,
        "spend_usd": spend,
        "tokens": tokens,
    })
}

/// Per-attempt series (oldest → newest) for the score & diff timelines.
fn attempt_series(attempts: &[LoopAttemptRow]) -> Vec<Value> {
    // Stored newest-first; reverse so charts read left→right in time order.
    attempts
        .iter()
        .rev()
        .map(|a| {
            json!({
                "task_id": a.task_id,
                "attempt": a.attempt_num,
                "test_pass_rate": a.test_pass_rate.unwrap_or(0.0),
                "lint_errors": a.lint_errors.unwrap_or(0),
                "diff_lines": a.diff_lines.unwrap_or(0),
                "outcome": a.outcome,
                "created_at": a.created_at,
            })
        })
        .collect()
}

/// Outcome distribution as `{label, count}` pairs.
fn outcome_series(outcomes: &[(String, i64)]) -> Vec<Value> {
    outcomes
        .iter()
        .map(|(label, count)| json!({ "label": label, "count": count }))
        .collect()
}

/// Token/cost/pressure burn series (oldest → newest).
fn burn_series(turns: &[LoopTurnRow]) -> Vec<Value> {
    turns
        .iter()
        .rev()
        .map(|t| {
            json!({
                "cost_usd": t.estimated_cost_usd,
                "tokens": t.input_tokens + t.output_tokens,
                "context_pressure": t.context_pressure,
                "timestamp": t.timestamp,
            })
        })
        .collect()
}

/// Ratio of two `usize` counts in `[0,1]`, guarding division by zero.
fn ratio(num: usize, den: usize) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

/// Ratio of two `i64` counts in `[0,1]`, guarding division by zero.
fn ratio_i(num: i64, den: i64) -> f64 {
    if den == 0 {
        0.0
    } else {
        num as f64 / den as f64
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn row(task: &str, n: i64, pass: f64, outcome: &str) -> LoopAttemptRow {
        LoopAttemptRow {
            task_id: task.into(),
            attempt_num: n,
            test_pass_rate: Some(pass),
            lint_errors: Some(0),
            diff_lines: Some(10),
            outcome: outcome.into(),
            created_at: format!("2026-06-21T00:0{n}:00Z"),
        }
    }

    #[test]
    fn ratio_guards_zero_denominator() {
        assert_eq!(ratio(0, 0), 0.0);
        assert_eq!(ratio(1, 2), 0.5);
        assert_eq!(ratio_i(0, 0), 0.0);
        assert_eq!(ratio_i(3, 4), 0.75);
    }

    #[test]
    fn headline_counts_distinct_runs_and_successes() {
        let attempts = vec![
            row("t1", 1, 0.4, "retry"),
            row("t1", 2, 1.0, "success"),
            row("t2", 1, 1.0, "success"),
        ];
        let stats = headline_stats(&attempts, &[], 1, 2);
        assert_eq!(stats["runs"], 2);
        assert_eq!(stats["attempts"], 3);
        assert!((stats["success_rate"].as_f64().unwrap() - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(stats["verifier_pass_rate"], 0.5);
    }

    #[test]
    fn attempt_series_is_time_ordered_oldest_first() {
        // Input is newest-first (as the store returns it).
        let attempts = vec![row("t1", 2, 1.0, "success"), row("t1", 1, 0.4, "retry")];
        let series = attempt_series(&attempts);
        assert_eq!(series[0]["attempt"], 1);
        assert_eq!(series[1]["attempt"], 2);
    }

    #[test]
    fn outcome_series_maps_pairs() {
        let series = outcome_series(&[("retry".into(), 2), ("success".into(), 1)]);
        assert_eq!(series[0]["label"], "retry");
        assert_eq!(series[0]["count"], 2);
    }
}
