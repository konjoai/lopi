//! Budget cost-breakdown REST surface — `GET /api/budget/breakdown`.
//!
//! Backs the Budget page's "by model" panel and 7-day spend trend, both
//! projected from the same `turn_metrics` ledger `/api/stats`'s
//! `total_cost_usd_today` already draws from. No new persistence.

use super::AppState;
use axum::{extract::State, response::IntoResponse, response::Json};
use lopi_core::Provenance;
use serde_json::json;

pub(super) async fn get_budget_breakdown(State(s): State<AppState>) -> impl IntoResponse {
    let by_model = s.store.cost_by_model_today().await.unwrap_or_else(|e| {
        tracing::warn!("cost_by_model_today query failed: {e}");
        Vec::new()
    });
    let trend = s.store.daily_cost_trend(7).await.unwrap_or_else(|e| {
        tracing::warn!("daily_cost_trend query failed: {e}");
        Vec::new()
    });
    // See `docs/MEASUREMENT.md` — named `measurement_provenance`, never bare
    // `"provenance"`, to avoid colliding with `TaskRow::provenance()`'s
    // unrelated trust field elsewhere on the wire.
    let measurement_provenance = serde_json::to_value(Provenance::measured(
        "turn_metrics table, by model",
    ))
    .unwrap_or_else(|e| {
        tracing::warn!(error = %e, "measurement_provenance serialization failed");
        serde_json::Value::Null
    });
    Json(json!({
        "by_model": by_model.into_iter().map(|(model, cost)| json!({
            "model": model, "cost_usd": cost,
        })).collect::<Vec<_>>(),
        "trend": trend.into_iter().map(|(date, cost)| json!({
            "date": date, "cost_usd": cost,
        })).collect::<Vec<_>>(),
        "measurement_provenance": measurement_provenance,
    }))
}
