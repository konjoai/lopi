//! Read-only config + version endpoints.
//!
//! The macOS dashboard (and the web UI's admin panel) needs to show the
//! effective server configuration and identify which server it is talking to.
//! These two endpoints are deliberately read-only for now — editing config at
//! runtime is a later phase that requires schema validation and a concurrent-
//! edit guard.
//!
//! Routes:
//! - `GET /api/config`  — `lopi.toml` as loaded, secrets redacted to `"***"`.
//! - `GET /api/version` — crate version + server uptime.

use super::AppState;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde_json::{json, Value};

/// JSON pointers to secret fields blanked before the config leaves the server.
const SECRET_POINTERS: &[&str] = &[
    "/web/auth_token",
    "/remote/telegram/token",
    "/remote/whatsapp/account_sid",
    "/remote/whatsapp/auth_token",
    "/remote/whatsapp/signing_secret",
];

/// `GET /api/config` — the effective `lopi.toml` with secrets redacted.
/// Returns `{config: null, source: "none"}` when the server was started without
/// a config file.
///
/// Reflects the config the server actually loaded at startup (from `--config`
/// or the standard search), not an independent re-discovery — the latter
/// returned `null` whenever `--config` pointed outside the standard search path,
/// disagreeing with the running server (Ops-2 bug #6).
pub(super) async fn get_config(State(s): State<AppState>) -> impl IntoResponse {
    let Some(cfg) = s.config.as_ref() else {
        return (
            StatusCode::OK,
            Json(json!({ "config": null, "source": "none" })),
        )
            .into_response();
    };

    match serde_json::to_value(cfg.as_ref()) {
        Ok(mut value) => {
            redact(&mut value);
            (
                StatusCode::OK,
                Json(json!({ "config": value, "source": "file" })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!("config serialize failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "failed to serialize config" })),
            )
                .into_response()
        }
    }
}

/// `GET /api/version` — server identity for the client to display and to detect
/// an incompatible server.
pub(super) async fn get_version(State(s): State<AppState>) -> impl IntoResponse {
    Json(json!({
        "service": "lopi",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": s.pool.stats().uptime_secs,
    }))
}

/// Replace every non-null secret field with `"***"` so tokens never travel to
/// the client.
fn redact(value: &mut Value) {
    for ptr in SECRET_POINTERS {
        if let Some(field) = value.pointer_mut(ptr) {
            if !field.is_null() {
                *field = Value::String("***".to_string());
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn redact_blanks_present_secrets_only() {
        let mut v = json!({
            "web": { "auth_token": "supersecret", "port": 3000 },
            "remote": {
                "telegram": { "token": "tg-token", "chat_id": 42 },
                "whatsapp": { "auth_token": null, "from": "whatsapp:+1" }
            }
        });
        redact(&mut v);
        assert_eq!(v["web"]["auth_token"], "***");
        assert_eq!(v["web"]["port"], 3000);
        assert_eq!(v["remote"]["telegram"]["token"], "***");
        assert_eq!(v["remote"]["telegram"]["chat_id"], 42);
        // Null secrets stay null (nothing to hide).
        assert!(v["remote"]["whatsapp"]["auth_token"].is_null());
        assert_eq!(v["remote"]["whatsapp"]["from"], "whatsapp:+1");
    }

    #[test]
    fn redact_is_noop_when_fields_absent() {
        let mut v = json!({ "web": { "port": 8080 } });
        redact(&mut v);
        assert_eq!(v["web"]["port"], 8080);
    }
}
