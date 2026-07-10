use anyhow::Result;
use axum::{middleware, routing::get, Router};
use dashmap::DashMap;
use lopi_core::{AgentEvent, EventBus, LopiConfig};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use lopi_ratelimit::TokenBucket;

use serde_json::Value;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};
use tower_http::cors::CorsLayer;

/// Simple TTL cache — returns the stored value if it was set within `ttl`.
struct TtlCache {
    data: Option<(Instant, Value)>,
    ttl: Duration,
}

impl TtlCache {
    fn new(ttl: Duration) -> Self {
        Self { data: None, ttl }
    }

    fn get(&self) -> Option<&Value> {
        self.data
            .as_ref()
            .filter(|(t, _)| t.elapsed() < self.ttl)
            .map(|(_, v)| v)
    }

    fn set(&mut self, data: Value) {
        self.data = Some((Instant::now(), data));
    }
}

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
    /// P2 — Durable tool registry. `clone()` is `Arc<RwLock>` under the hood.
    pub tools: lopi_tools::ToolRegistry,
    /// P2 — Agent health registry (heartbeats + classification). The
    /// background sweeper is `spawn_sweeper`'d from `serve`/`serve_with_repo`.
    pub health: lopi_orchestrator::HealthRegistry,
    /// macOS-UI Phase 0 — runtime-mutable cron scheduler. Started (and seeded
    /// from the `schedules` table) inside `serve_with_repo`.
    pub schedules: lopi_orchestrator::ScheduleManager,
    /// Pre-serialized broadcast: each `AgentEvent` serialized once, shared across all WS/SSE subscribers.
    serialized_tx: Arc<broadcast::Sender<Arc<str>>>,
    patterns_cache: Arc<Mutex<TtlCache>>,
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

        // Tool registry — empty in-memory store wired to the default path.
        // Callers that want the on-disk registry pre-loaded should call
        // `state.hydrate_tools()` after construction (e.g. inside `serve`).
        let tools = lopi_tools::ToolRegistry::new(lopi_tools::default_registry_path());
        // Health registry — same lifecycle: heartbeats are ephemeral and
        // re-derived from incoming agent traffic.
        let health =
            lopi_orchestrator::HealthRegistry::new(lopi_orchestrator::HealthConfig::default());
        // Runtime cron scheduler — constructed un-started here; `serve_with_repo`
        // calls `start()` to create the JobScheduler and register stored rows.
        let schedules = lopi_orchestrator::ScheduleManager::new((*pool).clone(), store.clone());

        Self {
            store,
            bus,
            queue,
            pool,
            repo_path,
            tools,
            health,
            schedules,
            serialized_tx,
            patterns_cache: Arc::new(Mutex::new(TtlCache::new(Duration::from_secs(30)))),
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

    /// Hydrate the tool registry from its on-disk path. Call this from an
    /// async context (e.g. inside `serve`) before binding the listener so
    /// the first `/tools` request sees previously registered tools.
    ///
    /// # Errors
    /// Returns `Err` only if the registry file exists but is unreadable or
    /// malformed JSON. A missing file is treated as "empty registry".
    pub async fn hydrate_tools(&mut self) -> Result<()> {
        let path = self.tools.path().to_path_buf();
        self.tools = lopi_tools::ToolRegistry::load(&path).await?;
        Ok(())
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
            "/api/agents/:id/checkpoint",
            axum::routing::post(checkpoint_agent),
        )
        .route("/api/stats", get(get_stats))
        .route("/api/patterns", get(list_patterns))
        .route("/api/plans", get(get_plans))
        .route("/api/spec", get(get_spec))
        .route("/api/quality/trend", get(get_quality_trend))
        .route("/api/routing/q-values", get(get_q_values))
        .route("/api/agents/:id/dag", get(get_agent_dag))
        .route(
            "/api/tools",
            get(list_tools_handler).post(register_tool_handler),
        )
        .route(
            "/api/tools/:name",
            get(get_tool_handler).delete(delete_tool_handler),
        )
        .route("/api/cache/stats", get(cache_stats_handler))
        .route("/api/cache", axum::routing::delete(clear_cache_handler))
        .route(
            "/api/cache/agent/:agent",
            axum::routing::delete(invalidate_agent_cache_handler),
        )
        .route("/api/tasks/dead-letter", get(dlq_handlers::list_dlq))
        .route(
            "/api/tasks/dead-letter/:id",
            get(dlq_handlers::get_dlq).delete(dlq_handlers::delete_dlq),
        )
        .route(
            "/api/tasks/dead-letter/:id/retry",
            axum::routing::post(dlq_handlers::retry_dlq),
        )
        .route("/api/audit", get(audit_handlers::query_audit))
        .route(
            "/api/agents/:id/heartbeat",
            axum::routing::post(health_handlers::heartbeat),
        )
        .route("/api/agents/:id/health", get(health_handlers::get_health))
        .route(
            "/api/agents/health/summary",
            get(health_handlers::health_summary),
        )
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
        // Static handler catches /, /constellation, /favicon.svg,
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
        None,
    )
    .await
}

/// Best-effort startup steps that should not abort the server on failure:
/// hydrate the on-disk tool registry and start the cron scheduler. Each failure
/// is logged and swallowed so the HTTP server still comes up.
async fn warm_up_state(state: &mut AppState) {
    if let Err(e) = state.hydrate_tools().await {
        // Keep the empty in-memory registry — runtime /tools registrations
        // still persist; we just lose previously saved entries until re-added.
        tracing::warn!(error = %e, "tool registry hydrate failed; starting empty");
    }
    if let Err(e) = state.schedules.start().await {
        // Without a live scheduler, cron rows persist but never fire.
        tracing::warn!(error = %e, "cron scheduler start failed; schedules will not fire");
    }
}

/// Variant that also wires the repo path for `/api/spec` serving.
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
    config: Option<LopiConfig>,
) -> Result<()> {
    let mut state =
        AppState::new_with_repo(store, bus, queue, pool, auth_token, repo_path).with_config(config);
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
mod audit_handlers;
mod cache_handlers;
mod config_handlers;
mod dlq_handlers;
mod handlers;
mod health_handlers;
mod loop_handlers;
mod loop_health_handlers;
mod loop_runs_handlers;
mod metrics_handlers;
mod repos_handlers;
mod schedule_handlers;
mod static_assets;
mod task_stream_handlers;
mod tools_handlers;
use api_middleware::{auth_middleware, rate_limit_middleware};
use cache_handlers::{cache_stats_handler, clear_cache_handler, invalidate_agent_cache_handler};
use handlers::{
    approve_plan, cancel_task, checkpoint_agent, create_task, get_spec, get_stats, get_task,
    health, list_patterns, list_tasks, reject_plan,
};
use metrics_handlers::{get_agent_dag, get_plans, get_q_values, get_quality_trend, metrics};
use static_assets::static_handler;
use tools_handlers::{
    delete_tool_handler, get_tool_handler, list_tools_handler, register_tool_handler,
};
mod streaming;
pub(crate) mod types;
use streaming::{sse_handler, ws_handler};

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
