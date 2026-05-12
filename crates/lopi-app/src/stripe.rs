//! Stripe webhook handler for subscription lifecycle events.
//!
//! Verifies the `Stripe-Signature` header (HMAC-SHA256 with timestamp replay
//! protection) and dispatches on relevant event types:
//! - `customer.subscription.created`  → tier activated
//! - `customer.subscription.updated`  → tier changed
//! - `customer.subscription.deleted`  → tier cancelled
//!
//! Event handling is currently a stub that logs and returns 200. Wire in
//! tier-gate logic once the Stripe account and product IDs are configured.

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use std::sync::Arc;

use crate::AppState;

/// Receive and verify Stripe webhook events.
pub async fn webhook(
    headers: HeaderMap,
    State(s): State<Arc<AppState>>,
    body: Bytes,
) -> impl IntoResponse {
    if !s.cfg.stripe_configured() {
        return (StatusCode::SERVICE_UNAVAILABLE, "Stripe not configured").into_response();
    }

    let stripe_sig = headers
        .get("Stripe-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let secret = s.cfg.stripe_webhook_secret.as_deref().unwrap_or("");
    if !verify_stripe_signature(secret, &body, stripe_sig) {
        tracing::warn!("Stripe HMAC verification failed");
        return (StatusCode::UNAUTHORIZED, "invalid Stripe signature").into_response();
    }

    let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return (StatusCode::BAD_REQUEST, "invalid JSON").into_response();
    };

    let event_type = payload["type"].as_str().unwrap_or("unknown");
    let customer_id = payload["data"]["object"]["customer"]
        .as_str()
        .unwrap_or("unknown");

    match event_type {
        "customer.subscription.created" | "customer.subscription.updated" => {
            tracing::info!(event_type, customer_id, "Stripe subscription event — tier activation stub");
        }
        "customer.subscription.deleted" => {
            tracing::info!(customer_id, "Stripe subscription cancelled");
        }
        _ => {
            tracing::debug!(event_type, "unhandled Stripe event");
        }
    }

    (StatusCode::OK, "ok").into_response()
}

/// Stripe uses `HMAC-SHA256` + a timestamp to prevent replay attacks.
///
/// The signed payload is: `{timestamp}.{body}` where `timestamp` is the
/// Unix second from the `Stripe-Signature` header's `t=` component.
///
/// Replay protection: reject events where the timestamp is more than
/// 300 seconds old (matches Stripe's official recommendation).
fn verify_stripe_signature(secret: &str, body: &[u8], sig_header: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut timestamp = None;
    let mut expected_v1 = None;

    for part in sig_header.split(',') {
        let part = part.trim();
        if let Some(ts) = part.strip_prefix("t=") {
            timestamp = ts.parse::<i64>().ok();
        }
        if let Some(sig) = part.strip_prefix("v1=") {
            expected_v1 = hex::decode(sig).ok();
        }
    }

    let (Some(ts), Some(expected)) = (timestamp, expected_v1) else {
        return false;
    };

    // Replay protection: 300s window.
    let now = chrono::Utc::now().timestamp();
    if (now - ts).abs() > 300 {
        tracing::warn!(ts, now, "Stripe webhook timestamp outside 300s window");
        return false;
    }

    let signed_payload = format!("{ts}.");
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(signed_payload.as_bytes());
    mac.update(body);
    mac.finalize().into_bytes().as_slice() == expected.as_slice()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_stripe_sig(secret: &str, body: &[u8]) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let ts = chrono::Utc::now().timestamp();
        let signed = format!("{ts}.");
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(signed.as_bytes());
        mac.update(body);
        let sig = hex::encode(mac.finalize().into_bytes());
        format!("t={ts},v1={sig}")
    }

    #[test]
    fn valid_stripe_sig() {
        let body = b"{}";
        let sig = make_stripe_sig("test_secret", body);
        assert!(verify_stripe_signature("test_secret", body, &sig));
    }

    #[test]
    fn wrong_secret_fails() {
        let body = b"{}";
        let sig = make_stripe_sig("correct", body);
        assert!(!verify_stripe_signature("wrong", body, &sig));
    }

    #[test]
    fn tampered_body_fails() {
        let sig = make_stripe_sig("secret", b"original");
        assert!(!verify_stripe_signature("secret", b"tampered", &sig));
    }

    #[test]
    fn missing_components_fail() {
        assert!(!verify_stripe_signature("secret", b"body", ""));
        assert!(!verify_stripe_signature("secret", b"body", "t=123"));
        assert!(!verify_stripe_signature("secret", b"body", "v1=abc"));
    }
}
