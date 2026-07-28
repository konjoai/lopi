//! Best-effort server startup steps, split out of `mod.rs` to keep that
//! file under the 500-line CI file-size gate (Sprint E's `/api/economics`
//! route pushed it over).

use super::AppState;

/// Best-effort startup steps that should not abort the server on failure:
/// start the cron scheduler. Failure is logged and swallowed so the HTTP
/// server still comes up.
pub(super) async fn warm_up_state(state: &mut AppState) {
    start_schedules(state).await;
    start_schedule_chains(state).await;
    start_quota(state).await;
    spawn_maxx_loop(state);
}

/// Without a live scheduler, cron rows persist but never fire.
async fn start_schedules(state: &AppState) {
    if let Err(e) = state.schedules.start().await {
        tracing::warn!(error = %e, "cron scheduler start failed; schedules will not fire");
    }
}

/// Without a live chain scheduler, chains persist but never fire, and any run
/// orphaned by a prior restart stays stuck at its last step.
async fn start_schedule_chains(state: &AppState) {
    if let Err(e) = state.schedule_chains.start().await {
        tracing::warn!(error = %e, "chain scheduler start failed; schedule chains will not fire");
    }
}

/// Without a loaded tracker, /api/quota and maxx_loop just see `None` until
/// the next `ApiRetry` event — degraded, not broken.
async fn start_quota(state: &AppState) {
    if let Err(e) = state.quota.start(&state.bus).await {
        tracing::warn!(error = %e, "quota tracker start failed; quota observations will not persist across restart");
    }
}

/// MAXX Phase 1 — the tick has no explicit shutdown handle (same as the cron
/// scheduler's jobs); it runs for the life of the process.
fn spawn_maxx_loop(state: &AppState) {
    lopi_orchestrator::MaxxLoop::new(
        state.store.clone(),
        state.quota.clone(),
        (*state.pool).clone(),
    )
    .spawn();
}
