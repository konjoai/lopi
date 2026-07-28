//! Budget cost-breakdown REST surface — `GET /api/budget/breakdown` and,
//! Sprint E, `GET /api/economics`.
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

/// Sprint E, Part 5 — `GET /api/economics`: the five unit-economics
/// numbers, current degradation tier, and pool headroom/runway. `null`
/// fields (no `[economics]` pool configured) tell the web cost page there
/// is nothing to render rather than a zeroed-out dashboard.
pub(super) async fn get_economics(State(s): State<AppState>) -> impl IntoResponse {
    let Some(econ) = s.pool.economics() else {
        return Json(json!({ "active": false }));
    };
    let report = match lopi_orchestrator::budget::report::compute(&s.store, &econ.pool, 7, 7).await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("economics report query failed: {e}");
            return Json(json!({ "active": true, "error": e.to_string() }));
        }
    };
    let headroom = econ.pool.headroom().await;
    Json(json!({
        "active": true,
        "tier": econ.ladder.current().as_str(),
        "headroom_usd": headroom.to_usd(),
        "pool_kind": econ.pool.pool().kind(),
        "pool_ceiling_usd": econ.pool.ceiling().to_usd(),
        "cost_per_merged_pr_usd": report.cost_per_merged_pr.map(lopi_core::Money::to_usd),
        "cost_per_gate_pass_usd": report.cost_per_gate_pass.map(lopi_core::Money::to_usd),
        "cost_per_retry_usd": report.cost_per_retry.to_usd(),
        "cache_attributed_saving_usd": report.cache_attributed_saving.to_usd(),
        "pool_runway_days": if report.pool_runway_days.is_finite() {
            json!(report.pool_runway_days)
        } else {
            json!(null)
        },
    }))
}
