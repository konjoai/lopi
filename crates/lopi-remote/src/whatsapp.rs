#![allow(clippy::missing_errors_doc)]
use anyhow::Result;
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use lopi_core::{Task, TaskSource};
use lopi_orchestrator::TaskQueue;
use serde::Deserialize;
use std::net::SocketAddr;

/// Shared state for the `WhatsApp` webhook handler.
#[derive(Clone)]
pub struct WhatsappState {
    /// Task queue to submit parsed goals into.
    pub queue: TaskQueue,
    /// Twilio signing secret for `HMAC-SHA1` webhook verification.
    /// None = verification disabled (dev mode).
    pub signing_secret: Option<String>,
}

/// Inbound message payload from Twilio's `WhatsApp` webhook.
#[derive(Debug, Deserialize)]
pub struct TwilioInbound {
    /// The message body text.
    #[serde(rename = "Body")]
    pub body: String,
    /// The sender's phone number (E.164 format).
    #[serde(rename = "From")]
    pub from: Option<String>,
}

/// Start the `WhatsApp` webhook server on `addr`.
///
/// # Errors
/// Returns an error if the TCP listener cannot be bound or if the server exits unexpectedly.
pub async fn serve(
    queue: TaskQueue,
    signing_secret: Option<String>,
    addr: SocketAddr,
) -> Result<()> {
    let state = WhatsappState {
        queue,
        signing_secret,
    };
    let app = Router::new()
        .route("/webhook/whatsapp", post(handle))
        .with_state(state);
    tracing::info!("📱 lopi whatsapp webhook on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn handle(
    State(s): State<WhatsappState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    // Verify Twilio HMAC-SHA1 signature when a secret is configured.
    if let Some(secret) = &s.signing_secret {
        let sig = headers
            .get("x-twilio-signature")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !verify_twilio_signature(secret.as_bytes(), &body, sig) {
            tracing::warn!("whatsapp: rejected request with invalid Twilio signature");
            return (
                StatusCode::FORBIDDEN,
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response/>",
            )
                .into_response();
        }
    }

    let payload: TwilioInbound = match serde_urlencoded::from_bytes(&body) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("whatsapp: failed to parse form body: {e}");
            return (
                StatusCode::BAD_REQUEST,
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response/>",
            )
                .into_response();
        }
    };

    let text = payload.body.trim();
    if let Some(goal) = text.strip_prefix("/task ") {
        let mut t = Task::new(goal);
        t.source = TaskSource::Webhook {
            repo: "whatsapp".into(),
            event: "message".into(),
        };
        s.queue.push(t).await;
    }
    // Twilio expects 200 with TwiML; an empty 200 is fine.
    (
        StatusCode::OK,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response/>",
    )
        .into_response()
}

/// Verify a Twilio webhook signature.
///
/// Twilio signs the request body with HMAC-SHA1 using the auth token as the key
/// and base64-encodes the result. We use constant-time comparison to prevent
/// timing-based attacks.
fn verify_twilio_signature(secret: &[u8], body: &[u8], sig_header: &str) -> bool {
    use base64::Engine as _;
    use hmac::{Hmac, Mac};
    use sha1::Sha1;

    let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(secret) else {
        return false;
    };
    mac.update(body);
    let result = mac.finalize().into_bytes();
    let expected = base64::engine::general_purpose::STANDARD.encode(result.as_slice());

    lopi_core::constant_time_eq(sig_header, &expected)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::post,
    };
    use tower::ServiceExt;

    fn make_signature(secret: &[u8], body: &[u8]) -> String {
        use base64::Engine as _;
        use hmac::{Hmac, Mac};
        use sha1::Sha1;
        let mut mac = Hmac::<Sha1>::new_from_slice(secret).unwrap();
        mac.update(body);
        base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes().as_slice())
    }

    fn make_test_router(signing_secret: Option<&str>) -> Router {
        let queue = TaskQueue::new();
        let state = WhatsappState {
            queue,
            signing_secret: signing_secret.map(ToString::to_string),
        };
        Router::new()
            .route("/webhook/whatsapp", post(handle))
            .with_state(state)
    }

    /// POST `body` to `/webhook/whatsapp` (with an optional precomputed
    /// `x-twilio-signature` header) and return the response. Shared by every
    /// handler test so the request-construction boilerplate is written once.
    async fn post_webhook(
        app: Router,
        body: impl Into<Vec<u8>>,
        sig: Option<&str>,
    ) -> axum::response::Response {
        let mut req = Request::builder()
            .method("POST")
            .uri("/webhook/whatsapp")
            .header("Content-Type", "application/x-www-form-urlencoded");
        if let Some(s) = sig {
            req = req.header("x-twilio-signature", s);
        }
        app.oneshot(req.body(Body::from(body.into())).unwrap())
            .await
            .unwrap()
    }

    #[test]
    fn valid_signature_passes() {
        let secret = b"my_twilio_auth_token";
        let body = b"Body=hello+world&From=whatsapp%3A%2B15551234567";
        let sig = make_signature(secret, body);
        assert!(verify_twilio_signature(secret, body, &sig));
    }

    #[test]
    fn wrong_secret_fails() {
        let body = b"Body=hello";
        let sig = make_signature(b"correct_secret", body);
        assert!(!verify_twilio_signature(b"wrong_secret", body, &sig));
    }

    #[test]
    fn tampered_body_fails() {
        let secret = b"my_secret";
        let sig = make_signature(secret, b"original body");
        assert!(!verify_twilio_signature(secret, b"tampered body", &sig));
    }

    #[test]
    fn empty_signature_fails() {
        let secret = b"my_secret";
        let body = b"Body=hello";
        assert!(!verify_twilio_signature(secret, body, ""));
    }

    #[tokio::test]
    async fn no_secret_task_message_queues_task() {
        let app = make_test_router(None);
        let body = "Body=%2Ftask+fix+the+bug&From=whatsapp%3A%2B15551234567";
        let resp = post_webhook(app, body, None).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn no_secret_non_task_message_returns_ok() {
        let app = make_test_router(None);
        let body = "Body=hello+world&From=whatsapp%3A%2B15551234567";
        let resp = post_webhook(app, body, None).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_signature_returns_403() {
        let app = make_test_router(Some("correct_secret"));
        let body = "Body=hello&From=whatsapp%3A%2B15551234567";
        let bad_sig = make_signature(b"wrong_secret", body.as_bytes());
        let resp = post_webhook(app, body, Some(&bad_sig)).await;
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn valid_signature_accepted() {
        let secret = "my_signing_secret";
        let app = make_test_router(Some(secret));
        let body = "Body=hello&From=whatsapp%3A%2B15551234567";
        let sig = make_signature(secret.as_bytes(), body.as_bytes());
        let resp = post_webhook(app, body, Some(&sig)).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn invalid_form_body_returns_400() {
        let app = make_test_router(None);
        // Send something that's not valid URL-encoded form data (binary garbage)
        let body: &[u8] = b"\xff\xfe invalid bytes that cannot be parsed as form data \x00\x01\x02";
        let resp = post_webhook(app, body, None).await;
        // Missing required "Body" field should cause a parse error
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn task_message_with_valid_secret() {
        let secret = "twilio_secret";
        let app = make_test_router(Some(secret));
        let body = "Body=%2Ftask+update+all+dependencies&From=whatsapp%3A%2B15551234567";
        let sig = make_signature(secret.as_bytes(), body.as_bytes());
        let resp = post_webhook(app, body, Some(&sig)).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
