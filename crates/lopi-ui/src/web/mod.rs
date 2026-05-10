//! Web/HTTP layer: shared `AppState`, axum router wiring, and TCP serve loop.
//!
//! Submodules carry the actual logic:
//! - [`handlers`] — /api/* and /metrics request handlers.
//! - [`middleware`] — Bearer-token auth + per-IP rate limiting for /api/*.
//! - [`static_assets`] — embedded SvelteKit Forge build + SPA fallback.
//! - [`streaming`] — WebSocket and SSE event streams.
//! - [`types`] — request/response DTOs shared by handlers.

use anyhow::Result;
use axum::{routing::get, Router};
use dashmap::DashMap;
use lopi_core::{AgentEvent, EventBus};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use lopi_ratelimit::TokenBucket;
use serde_json::Value;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, Mutex};
use tower_http::cors::CorsLayer;

mod handlers;
mod middleware;
mod static_assets;
mod streaming;
mod types;

use handlers::{
    cancel_task, create_task, get_stats, get_task, health, list_agents, list_patterns, list_tasks,
    metrics,
};
use middleware::{auth_middleware, rate_limit_middleware};
use static_assets::static_handler;
use streaming::{sse_handler, ws_handler};
#[cfg(test)]
use types::MAX_GOAL_LENGTH;

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
    /// Pre-serialized broadcast: each `AgentEvent` serialized once, shared across all WS/SSE subscribers.
    serialized_tx: Arc<broadcast::Sender<Arc<str>>>,
    patterns_cache: Arc<Mutex<TtlCache>>,
    /// Bearer token required on /api/* routes. None = auth disabled (dev mode).
    auth_token: Option<Arc<str>>,
    /// Per-IP token-bucket rate limiter for API endpoints.
    rate_limiter: Arc<DashMap<IpAddr, TokenBucket>>,
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
        let (serialized_tx, _) = broadcast::channel::<Arc<str>>(512);
        let serialized_tx = Arc::new(serialized_tx);

        // Bridge: subscribe to raw AgentEvent bus, serialize once, re-broadcast as Arc<str>.
        {
            let mut rx = bus.subscribe();
            let tx = serialized_tx.clone();
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(ev) => {
                            if let Ok(json) = serde_json::to_string(&ev) {
                                let _ = tx.send(Arc::from(json.as_str()));
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

        Self {
            store,
            bus,
            queue,
            pool,
            serialized_tx,
            patterns_cache: Arc::new(Mutex::new(TtlCache::new(Duration::from_secs(30)))),
            auth_token: auth_token.map(|t| Arc::from(t.as_str())),
            rate_limiter: Arc::new(DashMap::new()),
        }
    }
}

/// Build the axum router with all routes wired to `state`.
pub fn build_app(state: AppState) -> Router {
    // /api/* routes — protected by Bearer auth and per-IP rate limiting.
    let api = Router::new()
        .route("/api/health", get(health))
        .route("/api/tasks", get(list_tasks).post(create_task))
        .route("/api/tasks/:id", get(get_task).delete(cancel_task))
        .route("/api/agents", get(list_agents))
        .route("/api/stats", get(get_stats))
        .route("/api/patterns", get(list_patterns))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .route_layer(axum::middleware::from_fn_with_state(
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
    let state = AppState::new(store, bus, queue, pool, auth_token);
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

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
