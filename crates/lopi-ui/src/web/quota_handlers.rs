//! Quota headroom REST surface (MAXX Phase 0).
//!
//! - `GET /api/quota` — both rate-limit window snapshots (`five_hour` and
//!   `seven_day`), read from the live `QuotaTracker`. A window with no
//!   observation yet is reported as `null`, not omitted, so the UI can tell
//!   "never observed" apart from "0% used".

use super::AppState;
use axum::{extract::State, response::IntoResponse, response::Json};
use lopi_orchestrator::QuotaObservation;
use serde_json::{json, Value};

pub(super) async fn get_quota(State(s): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "five_hour": observation_to_json(s.quota.snapshot("five_hour")),
        "seven_day": observation_to_json(s.quota.snapshot("seven_day")),
    }))
}

fn observation_to_json(obs: Option<QuotaObservation>) -> Value {
    match obs {
        Some(o) => json!({
            "status": o.status,
            "utilization": o.utilization,
            "resets_at": o.resets_at,
            "observed_at": o.observed_at,
        }),
        None => Value::Null,
    }
}
