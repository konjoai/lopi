use anyhow::Result;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use lopi_core::{Priority, Task, TaskSource};
use lopi_orchestrator::TaskQueue;
use serde_json::Value;
use std::net::SocketAddr;

#[derive(Clone)]
struct WebhookState {
    queue: TaskQueue,
    secret: Option<String>,
}

pub async fn serve(queue: TaskQueue, secret: Option<String>, addr: SocketAddr) -> Result<()> {
    let state = WebhookState { queue, secret };
    let app = Router::new()
        .route("/webhook/github", post(handle))
        .with_state(state);
    tracing::info!("🪝 lopi github webhook on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle(
    State(s): State<WebhookState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // Verify HMAC-SHA256 signature when a secret is configured.
    if let Some(ref secret) = s.secret {
        let sig_header = headers
            .get("X-Hub-Signature-256")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");
        if !verify_signature(secret.as_bytes(), &body, sig_header) {
            tracing::warn!("GitHub webhook HMAC verification failed");
            return (StatusCode::UNAUTHORIZED, "invalid signature").into_response();
        }
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("failed to parse GitHub webhook payload: {e}");
            return (StatusCode::BAD_REQUEST, "invalid JSON").into_response();
        }
    };

    let event = headers
        .get("X-GitHub-Event")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let repo = payload
        .get("repository")
        .and_then(|r| r.get("full_name"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let conclusion = payload
        .get("workflow_run")
        .or_else(|| payload.get("check_run"))
        .and_then(|w| w.get("conclusion"))
        .and_then(|c| c.as_str());

    if matches!(conclusion, Some("failure") | Some("timed_out")) {
        let mut t = Task::new(format!("Investigate and fix CI failure on {repo}"));
        t.priority = Priority::High;
        t.source = TaskSource::Webhook { repo: repo.clone(), event: event.clone() };
        s.queue.push(t).await;
        tracing::info!("queued CI fix task for {repo} (event: {event})");
    }

    (StatusCode::OK, "ok").into_response()
}

/// Verify GitHub's `X-Hub-Signature-256: sha256=<hex>` header against `body`.
fn verify_signature(secret: &[u8], body: &[u8], sig_header: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let expected_hex = sig_header.strip_prefix("sha256=").unwrap_or("");
    let Ok(expected_bytes) = hex::decode(expected_hex) else {
        return false;
    };

    let mut mac = match Hmac::<Sha256>::new_from_slice(secret) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    let computed = mac.finalize().into_bytes();

    // Constant-time comparison to prevent timing attacks.
    computed.as_slice() == expected_bytes.as_slice()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_signature(secret: &[u8], body: &[u8]) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
        mac.update(body);
        let result = mac.finalize().into_bytes();
        format!("sha256={}", hex::encode(result))
    }

    #[test]
    fn valid_signature_passes() {
        let secret = b"mysecret";
        let body = b"hello github";
        let sig = make_signature(secret, body);
        assert!(verify_signature(secret, body, &sig));
    }

    #[test]
    fn wrong_secret_fails() {
        let body = b"hello github";
        let sig = make_signature(b"correct_secret", body);
        assert!(!verify_signature(b"wrong_secret", body, &sig));
    }

    #[test]
    fn tampered_body_fails() {
        let secret = b"mysecret";
        let sig = make_signature(secret, b"original body");
        assert!(!verify_signature(secret, b"tampered body", &sig));
    }

    #[test]
    fn missing_prefix_fails() {
        let secret = b"mysecret";
        let body = b"test";
        let raw_hex = hex::encode({
            use hmac::{Hmac, Mac};
            use sha2::Sha256;
            let mut m = Hmac::<Sha256>::new_from_slice(secret).unwrap();
            m.update(body);
            m.finalize().into_bytes()
        });
        // Without "sha256=" prefix, should fail.
        assert!(!verify_signature(secret, body, &raw_hex));
    }

    #[test]
    fn empty_signature_fails() {
        assert!(!verify_signature(b"secret", b"body", ""));
    }
}
