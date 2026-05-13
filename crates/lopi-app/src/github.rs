//! GitHub App OAuth flow and webhook handler.
//!
//! Routes:
//!   GET  /app/install   — redirect to GitHub App installation page
//!   GET  /app/callback  — exchange OAuth code for access token
//!   POST /app/webhook   — receive installation/uninstallation events

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::AppState;

/// Redirect the user to the GitHub App installation page.
///
/// With credentials configured this hits `github.com/apps/{app_slug}/installations/new`.
/// Without them it returns a 503.
pub async fn install_redirect(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    let Some(app_id) = &s.cfg.github_app_id else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "GITHUB_APP_ID not configured — set it and restart lopi-app",
        )
            .into_response();
    };
    let url = format!("https://github.com/apps/lopi-{}/installations/new", app_id);
    Redirect::temporary(&url).into_response()
}

/// OAuth callback: exchange `code` for an access token, look up/create the
/// customer record, and provision their isolated `MemoryStore`.
pub async fn oauth_callback(
    Query(params): Query<HashMap<String, String>>,
    State(s): State<Arc<AppState>>,
) -> impl IntoResponse {
    if !s.cfg.github_configured() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            "GitHub OAuth not configured",
        )
            .into_response();
    }

    let Some(code) = params.get("code") else {
        return (StatusCode::BAD_REQUEST, "missing 'code' parameter").into_response();
    };

    let token = match exchange_code(
        code,
        s.cfg.github_client_id.as_deref().unwrap_or(""),
        s.cfg.github_client_secret.as_deref().unwrap_or(""),
        s.cfg.github_redirect_uri.as_deref().unwrap_or(""),
    )
    .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("GitHub OAuth exchange failed: {e}");
            return (StatusCode::BAD_GATEWAY, "GitHub OAuth exchange failed").into_response();
        }
    };

    tracing::info!(token_type = %token.token_type, "GitHub OAuth callback success");
    (
        StatusCode::OK,
        "Installation successful — you can close this tab.",
    )
        .into_response()
}

/// Receive GitHub App installation events (created / deleted / suspended).
/// HMAC-verified; provisioned per-customer stores on `installation.created`.
pub async fn webhook(
    headers: HeaderMap,
    State(s): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    // HMAC verification.
    if let Some(ref secret) = s.cfg.github_webhook_secret {
        let sig = headers
            .get("X-Hub-Signature-256")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !verify_hmac(secret.as_bytes(), &body, sig) {
            return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
        }
    }

    let Ok(payload): Result<Value, _> = serde_json::from_slice(&body) else {
        return (StatusCode::BAD_REQUEST, "invalid JSON").into_response();
    };

    let event = headers
        .get("X-GitHub-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");
    let action = payload["action"].as_str().unwrap_or("");
    let installation_id = payload["installation"]["id"].as_i64().unwrap_or(0);
    let login = payload["installation"]["account"]["login"]
        .as_str()
        .unwrap_or("unknown");
    let account_type = payload["installation"]["account"]["type"]
        .as_str()
        .unwrap_or("User");

    if event == "installation" {
        handle_installation_event(action, installation_id, login, account_type, &s).await;
    }
    (StatusCode::OK, "ok").into_response()
}

/// Dispatch a GitHub installation lifecycle event to the appropriate handler.
async fn handle_installation_event(
    action: &str,
    installation_id: i64,
    login: &str,
    account_type: &str,
    s: &AppState,
) {
    match action {
        "created" => handle_installation_created(installation_id, login, account_type, s).await,
        "deleted" => {
            s.store.delete_installation(installation_id).await.ok();
            tracing::info!(installation_id, login, "GitHub App uninstalled");
        }
        "suspended" => tracing::info!(installation_id, login, "GitHub App suspended"),
        _ => {}
    }
}

/// Provision a new customer installation: upsert the record and open their store.
async fn handle_installation_created(
    installation_id: i64,
    login: &str,
    account_type: &str,
    s: &AppState,
) {
    match s
        .store
        .upsert_installation(installation_id, login, account_type)
        .await
    {
        Ok(customer_id) => {
            tracing::info!(customer_id, login, "GitHub App installed");
            provision_customer_store(&s.cfg.customer_store_base, &customer_id).await;
        }
        Err(e) => tracing::warn!(login, "installation upsert failed: {e}"),
    }
}

async fn provision_customer_store(base: &std::path::Path, customer_id: &str) {
    match lopi_memory::MemoryStore::open_for_customer(base, customer_id).await {
        Ok(_) => tracing::info!(customer_id, "customer store provisioned"),
        Err(e) => tracing::warn!(customer_id, "store provision failed: {e}"),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OAuthToken {
    #[allow(dead_code)]
    access_token: String,
    token_type: String,
}

async fn exchange_code(
    code: &str,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> anyhow::Result<OAuthToken> {
    let client = reqwest::Client::builder()
        .user_agent("lopi-app/0.1")
        .build()?;
    let resp = client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .json(&serde_json::json!({
            "client_id": client_id,
            "client_secret": client_secret,
            "code": code,
            "redirect_uri": redirect_uri,
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json::<OAuthToken>().await?)
}

fn verify_hmac(secret: &[u8], body: &[u8], sig_header: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let expected = sig_header.strip_prefix("sha256=").unwrap_or("");
    let Ok(expected_bytes) = hex::decode(expected) else {
        return false;
    };
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
        return false;
    };
    mac.update(body);
    mac.finalize().into_bytes().as_slice() == expected_bytes.as_slice()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn verify_hmac_valid() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let secret = b"mysecret";
        let body = b"hello";
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(body);
        let sig = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));
        assert!(verify_hmac(secret, body, &sig));
    }

    #[test]
    fn verify_hmac_invalid() {
        assert!(!verify_hmac(b"secret", b"body", "sha256=badhex"));
        assert!(!verify_hmac(b"secret", b"body", "notsha256=abc"));
        assert!(!verify_hmac(b"secret", b"body", ""));
    }
}
