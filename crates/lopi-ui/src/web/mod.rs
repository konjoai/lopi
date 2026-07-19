use anyhow::Result;
use axum::{middleware, routing::get, Router};
use dashmap::DashMap;
use lopi_core::{AgentEvent, EventBus, LopiConfig};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use lopi_ratelimit::TokenBucket;

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;

/// Shared application state injected into every axum handler.
#[derive(Clone)]
pub struct AppState {
    /// Persistent `SQLite` memory store for tasks and patterns.
    pub store: MemoryStore,
    /// Event bus for broadcasting `AgentEvent`s to connected clients.
    pub bus: EventBus<AgentEvent>,
    /// Priority task queue shared with the orchestrator pool.
    pub queue: TaskQueue,
    /// Handle to the running agent pool for status queries and cancellation.
    pub pool: Arc<AgentPool>,
    /// Repo root path — used to extract the spec surface on demand.
    pub repo_path: std::path::PathBuf,
    /// Additional repos the pool dispatches to (`sail --repos`). Listed by
    /// `GET /api/repos` alongside `repo_path` and its siblings, so the launch
    /// dropdowns offer every repo the server actually serves.
    pub extra_repos: Vec<std::path::PathBuf>,
    /// macOS-UI Phase 0 — runtime-mutable cron scheduler. Started (and seeded
    /// from the `schedules` table) inside `serve_with_repo`.
    pub schedules: lopi_orchestrator::ScheduleManager,
    /// Stack-Chain-1 — runtime-mutable whole-stack cron chain scheduler.
    /// Started (and seeded from the `schedule_chains` table, resuming any
    /// run orphaned by a prior restart) inside `serve_with_repo`.
    pub schedule_chains: lopi_orchestrator::ChainScheduleManager,
    /// MAXX Phase 0 — quota headroom tracker. Started (loads persisted
    /// observations, subscribes to the bus) inside `serve_with_repo`.
    pub quota: lopi_orchestrator::QuotaTracker,
    /// In-process TTL cache for `GET /api/models`'s live Anthropic catalog
    /// fetch. `Clone`-cheap (an `Arc` inside), shared across every request.
    pub models_cache: model_handlers::ModelsCache,
    /// Pre-serialized broadcast: each `AgentEvent` serialized once, shared across all WS/SSE subscribers.
    serialized_tx: Arc<broadcast::Sender<Arc<str>>>,
    /// Bearer token required on /api/* routes. None = auth disabled (dev mode).
    auth_token: Option<Arc<str>>,
    /// Per-IP token-bucket rate limiter for API endpoints.
    rate_limiter: Arc<DashMap<IpAddr, TokenBucket>>,
    /// The effective config the server was started with — from `--config` or
    /// the standard search — or `None` when no `lopi.toml` was loaded. Surfaced
    /// (secrets redacted) by `GET /api/config` so the endpoint reflects what is
    /// actually in effect rather than independently re-discovering a file and
    /// disagreeing with the running server (Ops-2 bug #6).
    config: Option<Arc<LopiConfig>>,
}

impl AppState {
    /// Construct a new `AppState`, wiring together the store, event bus, queue, pool, and optional auth token.
    #[must_use]
    pub fn new(
        store: MemoryStore,
        bus: EventBus<AgentEvent>,
        queue: TaskQueue,
        pool: Arc<AgentPool>,
        auth_token: Option<String>,
    ) -> Self {
        Self::new_with_repo(
            store,
            bus,
            queue,
            pool,
            auth_token,
            std::path::PathBuf::from("."),
        )
    }

    /// Variant that also records the repo path for spec surface serving.
    #[must_use]
    pub fn new_with_repo(
        store: MemoryStore,
        bus: EventBus<AgentEvent>,
        queue: TaskQueue,
        pool: Arc<AgentPool>,
        auth_token: Option<String>,
        repo_path: std::path::PathBuf,
    ) -> Self {
        let (serialized_tx, _) = broadcast::channel::<Arc<str>>(512);
        let serialized_tx = Arc::new(serialized_tx);

        // Bridge: subscribe to raw AgentEvent bus, serialize once,
        // re-broadcast as Arc<str>. Side-effect: mirror every
        // `AgentEvent::LogLine` to the `task_logs` SQLite table so the
        // per-task SSE endpoint has a historical tail and the web UI
        // can render progress retroactively.
        {
            let mut rx = bus.subscribe();
            let tx = serialized_tx.clone();
            let log_store = store.clone();
            tokio::spawn(async move {
                let mut log_counter: u64 = 0;
                loop {
                    match rx.recv().await {
                        Ok(ev) => {
                            if let Ok(json) = serde_json::to_string(&ev) {
                                let _ = tx.send(Arc::from(json.as_str()));
                            }
                            if let lopi_core::AgentEvent::LogLine {
                                task_id,
                                line,
                                level,
                                ts,
                            } = &ev
                            {
                                let tid = task_id.0.to_string();
                                let lvl = match level {
                                    lopi_core::LogLevel::Info => "info",
                                    lopi_core::LogLevel::Warn => "warn",
                                    lopi_core::LogLevel::Error => "error",
                                    lopi_core::LogLevel::Debug => "debug",
                                };
                                if let Err(e) =
                                    log_store.record_task_log(&tid, *ts, lvl, line).await
                                {
                                    tracing::warn!("task_log persist failed: {e}");
                                }
                                // Amortise pruning: run every 64 inserts.
                                log_counter = log_counter.wrapping_add(1);
                                if log_counter.is_multiple_of(64) {
                                    if let Err(e) = log_store.prune_task_logs(&tid).await {
                                        tracing::warn!("task_log prune failed: {e}");
                                    }
                                }
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!("serializer bridge lagged {n} events");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            });
        }

        // Runtime cron scheduler — constructed un-started here; `serve_with_repo`
        // calls `start()` to create the JobScheduler and register stored rows.
        let schedules = lopi_orchestrator::ScheduleManager::new((*pool).clone(), store.clone());
        // Chain cron scheduler — constructed un-started here; `serve_with_repo`
        // calls `start()` to create the JobScheduler, register stored chains,
        // and resume any run orphaned by a prior restart.
        let schedule_chains =
            lopi_orchestrator::ChainScheduleManager::new((*pool).clone(), store.clone());
        // Quota tracker — constructed un-started here; `serve_with_repo` calls
        // `start()` to load persisted observations and subscribe to the bus.
        let quota = lopi_orchestrator::QuotaTracker::new(store.clone());

        Self {
            store,
            bus,
            queue,
            pool,
            repo_path,
            extra_repos: Vec::new(),
            schedules,
            schedule_chains,
            quota,
            models_cache: model_handlers::ModelsCache::default(),
            serialized_tx,
            auth_token: auth_token.map(|t| Arc::from(t.as_str())),
            rate_limiter: Arc::new(DashMap::new()),
            config: None,
        }
    }

    /// Record the effective config the server was started with, so
    /// `GET /api/config` reflects what's in effect. `None` leaves the endpoint
    /// reporting `source: "none"`.
    #[must_use]
    pub fn with_config(mut self, config: Option<LopiConfig>) -> Self {
        self.config = config.map(Arc::new);
        self
    }

    /// Record the extra repos (`sail --repos`) the pool dispatches to, so
    /// `GET /api/repos` lists every repo the server serves rather than only the
    /// primary and its siblings.
    #[must_use]
    pub fn with_extra_repos(mut self, extra_repos: Vec<std::path::PathBuf>) -> Self {
        self.extra_repos = extra_repos;
        self
    }
}

/// Build the axum router with all routes wired to `state`.
pub fn build_app(state: AppState) -> Router {
    // /api/* routes — protected by Bearer auth and per-IP rate limiting.
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/:id", get(get_task).delete(cancel_task))
        .route(
            "/api/tasks/:id/plan/approve",
            axum::routing::post(approve_plan),
        )
        .route(
            "/api/tasks/:id/plan/reject",
            axum::routing::post(reject_plan),
        )
        .route("/api/repos", get(repos_handlers::list_repos))
        .route("/api/branches", get(repos_handlers::list_branches))
        .route(
            "/api/claude-commands",
            get(repos_handlers::list_claude_commands),
        )
        .route(
            "/api/agents/:id/checkpoint",
            axum::routing::post(checkpoint_agent),
        )
        .route("/api/stats", get(get_stats))
        .route("/api/plans", get(get_plans))
        .route("/api/spec", get(get_spec))
        .route("/api/quality/trend", get(get_quality_trend))
        .route("/api/routing/q-values", get(get_q_values))
        .route("/api/agents/:id/dag", get(get_agent_dag))
        .route(
            "/api/tasks/:id/stream",
            get(task_stream_handlers::stream_task),
        )
        .route("/api/tasks/:id/logs", get(task_stream_handlers::get_logs))
        .route("/api/logs", get(task_stream_handlers::get_recent_logs))
        .route(
            "/api/agents/:id/rate-limit",
            get(agent_rate_handlers::get_rate_limit)
                .post(agent_rate_handlers::register_rate_limit)
                .delete(agent_rate_handlers::delete_rate_limit),
        )
        .route(
            "/api/schedules",
            get(schedule_handlers::list_schedules).post(schedule_handlers::create_schedule),
        )
        .route(
            "/api/schedules/:id",
            get(schedule_handlers::get_schedule)
                .put(schedule_handlers::update_schedule)
                .delete(schedule_handlers::delete_schedule),
        )
        .route(
            "/api/schedules/:id/enable",
            axum::routing::post(schedule_handlers::enable_schedule),
        )
        .route(
            "/api/schedules/:id/disable",
            axum::routing::post(schedule_handlers::disable_schedule),
        )
        .route(
            "/api/schedules/:id/run-now",
            axum::routing::post(schedule_handlers::run_now),
        )
        .route(
            "/api/schedules/:id/autonomy",
            axum::routing::post(schedule_handlers::set_autonomy),
        )
        .route(
            "/api/schedule-chains",
            get(schedule_chain_handlers::list_chains).post(schedule_chain_handlers::create_chain),
        )
        .route(
            "/api/schedule-chains/:id",
            get(schedule_chain_handlers::get_chain)
                .put(schedule_chain_handlers::update_chain)
                .delete(schedule_chain_handlers::delete_chain),
        )
        .route(
            "/api/schedule-chains/:id/enable",
            axum::routing::post(schedule_chain_handlers::enable_chain),
        )
        .route(
            "/api/schedule-chains/:id/disable",
            axum::routing::post(schedule_chain_handlers::disable_chain),
        )
        .route(
            "/api/schedule-chains/:id/run-now",
            axum::routing::post(schedule_chain_handlers::run_now),
        )
        .route("/api/quota", get(quota_handlers::get_quota))
        .route(
            "/api/maxx",
            get(maxx_handlers::list_maxx).post(maxx_handlers::create_maxx),
        )
        .route(
            "/api/maxx/:id",
            get(maxx_handlers::get_maxx)
                .put(maxx_handlers::update_maxx)
                .delete(maxx_handlers::delete_maxx),
        )
        .route(
            "/api/maxx/:id/enable",
            axum::routing::post(maxx_handlers::enable_maxx),
        )
        .route(
            "/api/maxx/:id/disable",
            axum::routing::post(maxx_handlers::disable_maxx),
        )
        .route("/api/loop-engineering", get(loop_handlers::get_loop))
        .route(
            "/api/loop-engineering/health",
            get(loop_health_handlers::get_loop_health),
        )
        .route(
            "/api/loop-engineering/runs",
            get(loop_runs_handlers::list_runs),
        )
        .route(
            "/api/loop-engineering/runs/:id",
            get(loop_runs_handlers::get_run_trace),
        )
        .route(
            "/api/loop-engineering/strategy",
            axum::routing::post(loop_handlers::set_strategy),
        )
        .route(
            "/api/loop-engineering/escalation",
            axum::routing::post(loop_handlers::set_escalation),
        )
        .route("/api/config", get(config_handlers::get_config))
        .route("/api/version", get(config_handlers::get_version))
        .route("/api/models", get(model_handlers::get_models))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    Router::new()
        .merge(api)
        .route("/metrics", get(metrics))
        .route("/sse", get(sse_handler))
        .route("/ws", get(ws_handler))
        // Legacy endpoint — kept for compat.
        .route("/ws/tasks", get(ws_handler))
        // Static handler catches /, /overview, /favicon.svg,
        // /_app/**/*, and any SPA route fallthrough. Explicit routes above
        // take precedence.
        .fallback(get(static_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

/// # Errors
///
/// Returns an error if the TCP listener cannot bind to the address.
pub async fn serve(
    store: MemoryStore,
    bus: EventBus<AgentEvent>,
    queue: TaskQueue,
    pool: Arc<AgentPool>,
    host: &str,
    port: u16,
    auth_token: Option<String>,
) -> Result<()> {
    serve_with_repo(
        store,
        bus,
        queue,
        pool,
        host,
        port,
        auth_token,
        std::path::PathBuf::from("."),
        Vec::new(),
        None,
    )
    .await
}

/// Best-effort startup steps that should not abort the server on failure:
/// start the cron scheduler. Failure is logged and swallowed so the HTTP
/// server still comes up.
async fn warm_up_state(state: &mut AppState) {
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

/// Variant that also wires the repo path for `/api/spec` serving, plus any
/// extra dispatch repos for `/api/repos`.
#[allow(clippy::too_many_arguments)]
pub async fn serve_with_repo(
    store: MemoryStore,
    bus: EventBus<AgentEvent>,
    queue: TaskQueue,
    pool: Arc<AgentPool>,
    host: &str,
    port: u16,
    auth_token: Option<String>,
    repo_path: std::path::PathBuf,
    extra_repos: Vec<std::path::PathBuf>,
    config: Option<LopiConfig>,
) -> Result<()> {
    let mut state = AppState::new_with_repo(store, bus, queue, pool, auth_token, repo_path)
        .with_extra_repos(extra_repos)
        .with_config(config);
    warm_up_state(&mut state).await;
    let app = build_app(state);

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    tracing::info!(
        "🌐 lopi sail: http://{addr}  ws://{addr}/ws  sse://{addr}/sse  metrics://{addr}/metrics"
    );
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // Use connect_info so rate limiter middleware can extract client IPs.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

mod agent_rate_handlers;
mod api_middleware;
mod config_handlers;
mod handlers;
mod loop_handlers;
mod loop_health_handlers;
mod loop_runs_handlers;
mod maxx_handlers;
mod metrics_handlers;
mod model_handlers;
mod quota_handlers;
mod repo_identity;
mod repos_handlers;
mod schedule_chain_handlers;
mod schedule_handlers;
mod static_assets;
mod task_stream_handlers;
use api_middleware::{auth_middleware, rate_limit_middleware};
use handlers::{
    approve_plan, cancel_task, checkpoint_agent, create_task, get_spec, get_stats, get_task,
    health, list_tasks, reject_plan,
};
use metrics_handlers::{get_agent_dag, get_plans, get_q_values, get_quality_trend, metrics};
use static_assets::static_handler;
mod streaming;
pub(crate) mod types;
use streaming::{sse_handler, ws_handler};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
