//! Sprint S2, Phase 2 — CORS allowlist.
//!
//! `CorsLayer::permissive()` on a server that holds a bearer token is a
//! browser-side credential-leak path once the server isn't localhost-only.
//! Default to an explicit origin allowlist; permissive mode is an opt-out
//! with the same shape as `--insecure-no-auth` (`auth_policy`) — never the
//! default, always a deliberate, loudly-logged choice.

use lopi_core::LopiConfig;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Pull `(cors_allowed_origins, cors_permissive)` out of the effective
/// config, defaulting to `(empty, false)` — i.e. the dev-origin fallback,
/// never permissive — when no `lopi.toml` was loaded.
#[must_use]
pub fn resolve_from_config(config: Option<&LopiConfig>) -> (Vec<String>, bool) {
    config.map_or((Vec::new(), false), |c| {
        (c.web.cors_allowed_origins.clone(), c.web.cors_permissive)
    })
}

/// Local dev origins the web app actually uses — `web/vite.config.js` runs
/// the SvelteKit dev server on `5173` and proxies `/api` + `/ws` to the
/// `sail` server, but a browser hitting the API directly (or a future
/// non-proxied client) still needs these allowed.
const DEFAULT_DEV_ORIGINS: [&str; 2] = ["http://localhost:5173", "http://127.0.0.1:5173"];

/// Build the CORS layer: an explicit origin allowlist by default, falling
/// back to [`DEFAULT_DEV_ORIGINS`] when none is configured.
pub fn resolve_cors_layer(allowed_origins: &[String], permissive: bool) -> CorsLayer {
    if permissive {
        tracing::warn!(
            "⚠️  CORS is fully permissive (cors_permissive = true) — any origin may call \
             /api/*. Do not use this on a public interface."
        );
        return CorsLayer::permissive();
    }

    let origins: Vec<axum::http::HeaderValue> = if allowed_origins.is_empty() {
        DEFAULT_DEV_ORIGINS
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect()
    } else {
        allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect()
    };

    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}
