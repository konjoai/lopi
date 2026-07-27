//! Auth + rate-limit middleware for every route the server registers.
//!
//! Split out of `web/mod.rs` to keep that module within the 500-line budget.
//!
//! Sprint S11, Phase 0: `auth_middleware` used to be applied only to the
//! `/api/*` route table via `route_layer`; `/sse`, `/ws`, `/ws/tasks`, and
//! `/metrics` were registered on the *outer* router after that layer was
//! already built, so they received no auth (or rate-limit) check at all.
//! The fix is structural, not four new checks bolted onto four routes: every
//! route those four sit alongside now lives in the single `protected` router
//! `build_app` constructs, and this middleware is the only thing standing
//! between any of them and the network — see `mod.rs`'s `build_app` doc
//! comment for the router shape this middleware is wired into.

use super::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use lopi_ratelimit::TokenBucket;
use serde::Deserialize;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": "unauthorized"})),
    )
        .into_response()
}

/// Query-string shape accepted by the ticket-eligible routes — see
/// [`is_ticket_eligible`].
#[derive(Deserialize)]
struct TicketQuery {
    ticket: Option<String>,
}

/// `/ws`, `/ws/tasks`, and `/sse` accept a single-use ticket
/// (`?ticket=<value>`, minted by `POST /api/ws-ticket`) as an alternative to
/// a Bearer header, because a browser `WebSocket`/`EventSource` cannot set
/// custom headers on the upgrade request. No other route gets this
/// alternative — a stolen/leaked ticket is scoped to exactly the streams it
/// was built for.
fn is_ticket_eligible(path: &str) -> bool {
    matches!(path, "/ws" | "/ws/tasks" | "/sse")
}

fn bearer_ok(request: &axum::extract::Request, expected: &str) -> bool {
    request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|p| lopi_core::constant_time_eq(p, expected))
}

fn ticket_ok(s: &AppState, request: &axum::extract::Request) -> bool {
    Query::<TicketQuery>::try_from_uri(request.uri())
        .ok()
        .and_then(|q| q.0.ticket)
        .is_some_and(|t| s.ws_tickets.consume(&t))
}

/// Middleware: validate `Authorization: Bearer <token>` (or, on the three
/// streaming routes, a valid ticket) on every route it is applied to.
/// Skipped entirely when `auth_token` is not configured (dev mode /
/// `--insecure-no-auth`).
pub(super) async fn auth_middleware(
    State(s): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if let Some(expected) = &s.auth_token {
        let path = request.uri().path().to_string();
        let authorized =
            bearer_ok(&request, expected) || (is_ticket_eligible(&path) && ticket_ok(&s, &request));
        if !authorized {
            return unauthorized();
        }
    }
    next.run(request).await
}

/// `POST /api/ws-ticket` — mint a single-use ticket for the caller to attach
/// to a subsequent `/ws`, `/ws/tasks`, or `/sse` connection as `?ticket=`.
/// Behind the same auth this whole router requires, so minting one still
/// needs the real Bearer token; it exists only to hand a *browser* something
/// it's actually able to attach to a `WebSocket`/`EventSource` URL.
pub(super) async fn mint_ws_ticket(State(s): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "ticket": s.ws_tickets.mint(),
        "expires_in_secs": 30,
    }))
}

/// Middleware: per-IP token-bucket rate limiter (60 req/min burst, 1 req/sec refill).
/// Falls back to `127.0.0.1` when `ConnectInfo` is unavailable (e.g., in tests).
pub(super) async fn rate_limit_middleware(
    State(s): State<AppState>,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    use axum::extract::connect_info::ConnectInfo;

    let ip: IpAddr = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .or_else(|| {
            req.extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|c| c.0.ip())
        })
        .unwrap_or_else(|| IpAddr::from([127, 0, 0, 1]));

    // Get or create a per-IP bucket: 60-token burst, 1 token/sec refill.
    let bucket = s.rate_limiter.get(&ip).map_or_else(
        || {
            let new_bucket = TokenBucket::new(60.0, 1.0);
            s.rate_limiter.insert(ip, new_bucket.clone());
            new_bucket
        },
        |b| b.clone(),
    );

    if !bucket.try_acquire(1.0).await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({"error": "rate limit exceeded"})),
        )
            .into_response();
    }

    next.run(req).await
}
