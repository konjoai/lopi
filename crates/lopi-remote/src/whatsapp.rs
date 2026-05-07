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

#[derive(Clone)]
pub struct WhatsappState {
    pub queue: TaskQueue,
    /// Twilio signing secret for HMAC-SHA1 webhook verification.
    /// None = verification disabled (dev mode).
    pub signing_secret: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TwilioInbound {
    #[serde(rename = "Body")]
    pub body: String,
    #[serde(rename = "From")]
    pub from: Option<String>,
}

pub async fn serve(
    queue: TaskQueue,
    signing_secret: Option<String>,
    addr: SocketAddr,
) -> Result<()> {
    let state = WhatsappState { queue, signing_secret };
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
            return (StatusCode::FORBIDDEN, "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response/>")
                .into_response();
        }
    }

    let payload: TwilioInbound = match serde_urlencoded::from_bytes(&body) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("whatsapp: failed to parse form body: {e}");
            return (StatusCode::BAD_REQUEST, "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response/>")
                .into_response();
        }
    };

    let text = payload.body.trim();
    if let Some(goal) = text.strip_prefix("/task ") {
        let mut t = Task::new(goal);
        t.source = TaskSource::Webhook { repo: "whatsapp".into(), event: "message".into() };
        s.queue.push(t).await;
    }
    // Twilio expects 200 with TwiML; an empty 200 is fine.
    (StatusCode::OK, "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Response/>").into_response()
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

    constant_time_eq(sig_header, &expected)
}

/// Constant-time string comparison to prevent timing-based side-channel attacks.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_signature(secret: &[u8], body: &[u8]) -> String {
        use base64::Engine as _;
        use hmac::{Hmac, Mac};
        use sha1::Sha1;
        let mut mac = Hmac::<Sha1>::new_from_slice(secret).unwrap();
        mac.update(body);
        base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes().as_slice())
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
}
