//! Budget cost-breakdown REST surface — `GET /api/budget/breakdown`.
//!
//! Backs the Budget page's "by model" panel and 7-day spend trend, both
//! projected from the same `turn_metrics` ledger `/api/stats`'s
//! `total_cost_usd_today` already draws from. No new persistence.

use super::AppState;
use axum::{extract::State, response::IntoResponse, response::Json};
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
    Json(json!({
        "by_model": by_model.into_iter().map(|(model, cost)| json!({
            "model": model, "cost_usd": cost,
        })).collect::<Vec<_>>(),
        "trend": trend.into_iter().map(|(date, cost)| json!({
            "date": date, "cost_usd": cost,
        })).collect::<Vec<_>>(),
    }))
}
