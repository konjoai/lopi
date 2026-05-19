//! Stripe webhook handler for subscription lifecycle events.
//!
//! Verifies the `Stripe-Signature` header (HMAC-SHA256 with timestamp replay
//! protection) and dispatches on relevant event types:
//! - `customer.subscription.created`  → tier activated
//! - `customer.subscription.updated`  → tier changed
//! - `customer.subscription.deleted`  → tier cancelled (downgrade to Free)
//!
//! The handler reads `metadata.lopi_installation_id` from the Stripe
//! subscription object to identify which GitHub App installation to update.
//! Set this metadata when creating the Stripe checkout session:
//!
//! ```json
//! { "metadata": { "lopi_installation_id": "12345678" } }
//! ```

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use lopi_core::CustomerTier;
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

    dispatch_stripe_event(&payload, &s).await;
    (StatusCode::OK, "ok").into_response()
}

/// Dispatch a parsed Stripe event, updating the installation tier in the DB.
///
/// Reads `metadata.lopi_installation_id` from the subscription object to
/// map the Stripe event to a lopi `github_installations` row.
async fn dispatch_stripe_event(payload: &serde_json::Value, s: &AppState) {
    let event_type = payload["type"].as_str().unwrap_or("unknown");
    let obj = &payload["data"]["object"];
    let stripe_customer = obj["customer"].as_str().unwrap_or("unknown");

    match event_type {
        "customer.subscription.created" | "customer.subscription.updated" => {
            let tier = extract_tier_from_subscription(obj);
            let installation_id = extract_installation_id(obj);
            if let Some(id) = installation_id {
                match s.store.set_installation_tier(id, tier).await {
                    Ok(()) => tracing::info!(
                        event_type,
                        stripe_customer,
                        installation_id = id,
                        tier = %tier,
                        "subscription tier updated"
                    ),
                    Err(e) => tracing::warn!(
                        installation_id = id,
                        "failed to update tier: {e}"
                    ),
                }
            } else {
                tracing::warn!(
                    event_type,
                    stripe_customer,
                    "Stripe event missing lopi_installation_id in metadata — tier not updated"
                );
            }
        }
        "customer.subscription.deleted" => {
            let installation_id = extract_installation_id(obj);
            if let Some(id) = installation_id {
                if let Err(e) = s.store.set_installation_tier(id, CustomerTier::Free).await {
                    tracing::warn!(installation_id = id, "failed to downgrade tier: {e}");
                } else {
                    tracing::info!(
                        stripe_customer,
                        installation_id = id,
                        "subscription cancelled — tier downgraded to Free"
                    );
                }
            }
        }
        _ => tracing::debug!(event_type, "unhandled Stripe event"),
    }
}

/// Extract `CustomerTier` from a Stripe subscription object.
///
/// Checks `items.data[0].price.nickname` first, then `metadata.lopi_plan`.
fn extract_tier_from_subscription(obj: &serde_json::Value) -> CustomerTier {
    // Try price nickname on the first line item.
    if let Some(nickname) = obj["items"]["data"][0]["price"]["nickname"].as_str() {
        if !nickname.is_empty() {
            return CustomerTier::from_stripe_name(nickname);
        }
    }
    // Fallback: explicit metadata field.
    if let Some(plan) = obj["metadata"]["lopi_plan"].as_str() {
        return CustomerTier::from_stripe_name(plan);
    }
    CustomerTier::Free
}

/// Extract `lopi_installation_id` from a Stripe subscription's metadata.
fn extract_installation_id(obj: &serde_json::Value) -> Option<i64> {
    obj["metadata"]["lopi_installation_id"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
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
