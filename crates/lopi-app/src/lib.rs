//! lopi-app — GitHub App OAuth + Stripe webhook server.
//!
//! Runs as a standalone axum server on a separate port from `lopi sail`.
//! All credentials are read from environment variables at startup so the
//! binary can be deployed without recompilation.
//!
//! ## Environment variables
//!
//! | Variable                 | Description                                    |
//! |--------------------------|------------------------------------------------|
//! | `GITHUB_APP_ID`          | Numeric App ID from the GitHub App settings    |
//! | `GITHUB_CLIENT_ID`       | OAuth client ID                                |
//! | `GITHUB_CLIENT_SECRET`   | OAuth client secret                            |
//! | `GITHUB_WEBHOOK_SECRET`  | HMAC secret for incoming GitHub App webhooks   |
//! | `GITHUB_REDIRECT_URI`    | Callback URL (e.g. https://lopi.example.com/app/callback) |
//! | `STRIPE_WEBHOOK_SECRET`  | Stripe webhook signing secret (`whsec_…`)      |
//!
//! When variables are absent the server starts but the relevant routes
//! return 503 with a clear message.

pub mod github;
pub mod stripe;

use anyhow::Result;
use axum::{routing::get, Router};
use lopi_memory::MemoryStore;
use std::net::SocketAddr;
use std::sync::Arc;

/// Configuration loaded from environment variables.
#[derive(Clone)]
pub struct AppConfig {
    pub github_app_id: Option<String>,
    pub github_client_id: Option<String>,
    pub github_client_secret: Option<String>,
    pub github_webhook_secret: Option<String>,
    pub github_redirect_uri: Option<String>,
    pub stripe_webhook_secret: Option<String>,
    /// Base directory for per-customer isolated stores.
    pub customer_store_base: std::path::PathBuf,
}

impl AppConfig {
    /// Load from environment. Missing variables are stored as `None`.
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            github_app_id: std::env::var("GITHUB_APP_ID").ok(),
            github_client_id: std::env::var("GITHUB_CLIENT_ID").ok(),
            github_client_secret: std::env::var("GITHUB_CLIENT_SECRET").ok(),
            github_webhook_secret: std::env::var("GITHUB_WEBHOOK_SECRET").ok(),
            github_redirect_uri: std::env::var("GITHUB_REDIRECT_URI").ok(),
            stripe_webhook_secret: std::env::var("STRIPE_WEBHOOK_SECRET").ok(),
            customer_store_base: std::env::var("LOPI_CUSTOMER_STORES")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    dirs_home().join(".lopi").join("customers")
                }),
        }
    }

    /// True when GitHub OAuth credentials are fully configured.
    #[must_use]
    pub fn github_configured(&self) -> bool {
        self.github_client_id.is_some()
            && self.github_client_secret.is_some()
            && self.github_redirect_uri.is_some()
    }

    /// True when Stripe webhook secret is configured.
    #[must_use]
    pub fn stripe_configured(&self) -> bool {
        self.stripe_webhook_secret.is_some()
    }
}

fn dirs_home() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Shared application state for the app server.
#[derive(Clone)]
pub struct AppState {
    pub cfg: AppConfig,
    pub store: MemoryStore,
}

/// Build the axum router for the GitHub App + Stripe server.
pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .route("/app/install", get(github::install_redirect))
        .route("/app/callback", get(github::oauth_callback))
        .route("/app/webhook", axum::routing::post(github::webhook))
        .route("/stripe/webhook", axum::routing::post(stripe::webhook))
        .with_state(Arc::new(state))
}

/// Start the lopi-app server on `addr`.
///
/// # Errors
///
/// Returns an error if the TCP listener cannot bind.
pub async fn serve(state: AppState, addr: SocketAddr) -> Result<()> {
    let app = build_app(state);
    tracing::info!("🔐 lopi-app server on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn root_handler() -> axum::response::Json<serde_json::Value> {
    axum::response::Json(serde_json::json!({
        "service": "lopi-app",
        "endpoints": ["/app/install", "/app/callback", "/app/webhook", "/stripe/webhook"],
    }))
}
