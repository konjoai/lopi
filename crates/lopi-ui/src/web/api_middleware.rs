//! Auth + rate-limit middleware for `/api/*`.
//!
//! Split out of `web/mod.rs` to keep that module within the 500-line budget.

use super::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use lopi_ratelimit::TokenBucket;
use serde_json::json;
use std::net::{IpAddr, SocketAddr};

/// Middleware: validate `Authorization: Bearer <token>` on all /api/* routes.
/// Skipped entirely when `auth_token` is not configured (dev mode).
pub(super) async fn auth_middleware(
    State(s): State<AppState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if let Some(expected) = &s.auth_token {
        let provided = request
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "));

        if !provided.is_some_and(|p| lopi_core::constant_time_eq(p, expected.as_ref())) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "unauthorized"})),
            )
                .into_response();
        }
    }
    next.run(request).await
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
