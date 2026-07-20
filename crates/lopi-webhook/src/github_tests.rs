#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::post,
};
use tower::ServiceExt;

fn make_signature(secret: &[u8], body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).unwrap();
    mac.update(body);
    let result = mac.finalize().into_bytes();
    format!("sha256={}", hex::encode(result))
}

/// POST `body` to `/webhook/github` as the given event type (and,
/// optionally, a precomputed `X-Hub-Signature-256` header) and return the
/// response. Shared by every handler test so the request-construction
/// boilerplate is written once.
async fn post_webhook(
    app: Router,
    event: &str,
    body: impl Into<Vec<u8>>,
    sig: Option<&str>,
) -> axum::response::Response {
    let mut req = Request::builder()
        .method("POST")
        .uri("/webhook/github")
        .header("X-GitHub-Event", event)
        .header("Content-Type", "application/json");
    if let Some(s) = sig {
        req = req.header("X-Hub-Signature-256", s);
    }
    app.oneshot(req.body(Body::from(body.into())).unwrap())
        .await
        .unwrap()
}

fn make_test_router(secret: Option<&str>) -> Router {
    let queue = TaskQueue::new();
    let state = WebhookState {
        queue,
        secret: secret.map(ToString::to_string),
        triage: None,
    };
    Router::new()
        .route("/webhook/github", post(handle))
        .with_state(state)
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

/// Regression test for the switch to `lopi_core::constant_time_eq`
/// (hex-string comparison): an uppercase-hex signature header must still
/// verify, matching the case-insensitive `hex::decode` behavior this
/// replaced.
#[test]
fn uppercase_hex_signature_still_passes() {
    let secret = b"mysecret";
    let body = b"hello github";
    let sig = make_signature(secret, body).to_uppercase();
    // `to_uppercase` also uppercases the "sha256=" prefix; rebuild it.
    let sig = format!("sha256={}", sig.trim_start_matches("SHA256="));
    assert!(verify_signature(secret, body, &sig));
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

#[tokio::test]
async fn no_secret_ci_failure_queues_task() {
    let app = make_test_router(None);
    let body = serde_json::json!({
        "repository": { "full_name": "org/repo" },
        "workflow_run": { "conclusion": "failure" }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let resp = post_webhook(app, "workflow_run", body_bytes, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn valid_secret_ci_failure_queues_task() {
    let secret = "mysecret";
    let app = make_test_router(Some(secret));
    let body = serde_json::json!({
        "repository": { "full_name": "org/repo" },
        "workflow_run": { "conclusion": "failure" }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let sig = make_signature(secret.as_bytes(), &body_bytes);
    let resp = post_webhook(app, "workflow_run", body_bytes, Some(&sig)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn invalid_secret_returns_401() {
    let app = make_test_router(Some("correct_secret"));
    let body = serde_json::json!({
        "repository": { "full_name": "org/repo" },
        "workflow_run": { "conclusion": "failure" }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let bad_sig = make_signature(b"wrong_secret", &body_bytes);
    let resp = post_webhook(app, "workflow_run", body_bytes, Some(&bad_sig)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn pr_review_changes_requested_queues_task() {
    let app = make_test_router(None);
    let body = serde_json::json!({
        "repository": { "full_name": "org/repo" },
        "action": "submitted",
        "review": { "state": "changes_requested", "body": "Please fix the linting issues." },
        "pull_request": { "title": "feat: add new feature" }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let resp = post_webhook(app, "pull_request_review", body_bytes, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn pr_review_approved_no_task() {
    let app = make_test_router(None);
    let body = serde_json::json!({
        "repository": { "full_name": "org/repo" },
        "action": "submitted",
        "review": { "state": "approved", "body": "LGTM!" },
        "pull_request": { "title": "feat: nice work" }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let resp = post_webhook(app, "pull_request_review", body_bytes, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn ci_success_no_task() {
    let app = make_test_router(None);
    let body = serde_json::json!({
        "repository": { "full_name": "org/repo" },
        "workflow_run": { "conclusion": "success" }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let resp = post_webhook(app, "workflow_run", body_bytes, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn invalid_json_returns_400() {
    let app = make_test_router(None);
    let resp = post_webhook(app, "workflow_run", "not valid json!!!", None).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn timed_out_conclusion_queues_task() {
    let app = make_test_router(None);
    let body = serde_json::json!({
        "repository": { "full_name": "org/repo" },
        "workflow_run": { "conclusion": "timed_out" }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let resp = post_webhook(app, "workflow_run", body_bytes, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn check_run_failure_queues_task() {
    let app = make_test_router(None);
    let body = serde_json::json!({
        "repository": { "full_name": "org/repo" },
        "check_run": { "conclusion": "failure" }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let resp = post_webhook(app, "check_run", body_bytes, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[test]
fn should_triage_opened_issue() {
    let payload = serde_json::json!({});
    assert!(should_triage_issue_event("opened", &payload));
}

#[test]
fn should_triage_lopi_fix_label() {
    let payload = serde_json::json!({ "label": { "name": "lopi:fix" } });
    assert!(should_triage_issue_event("labeled", &payload));
}

#[test]
fn should_triage_lopi_fix_label_case_insensitive() {
    let payload = serde_json::json!({ "label": { "name": "LOPI:FIX" } });
    assert!(should_triage_issue_event("labeled", &payload));
}

/// Regression test for the original bug: a `labeled` event for an
/// unrelated label (e.g. `good first issue`) must not re-trigger
/// classification/re-commenting.
#[test]
fn should_not_triage_unrelated_label() {
    let payload = serde_json::json!({ "label": { "name": "good first issue" } });
    assert!(!should_triage_issue_event("labeled", &payload));
}

#[test]
fn should_not_triage_labeled_with_missing_label_field() {
    let payload = serde_json::json!({});
    assert!(!should_triage_issue_event("labeled", &payload));
}

#[test]
fn should_not_triage_other_actions() {
    let payload = serde_json::json!({ "label": { "name": "lopi:fix" } });
    assert!(!should_triage_issue_event("closed", &payload));
    assert!(!should_triage_issue_event("unlabeled", &payload));
}

#[tokio::test]
async fn labeled_event_with_unrelated_label_returns_ok_without_triage() {
    let app = make_test_router(None);
    let body = serde_json::json!({
        "repository": { "full_name": "org/repo" },
        "action": "labeled",
        "label": { "name": "good first issue" },
        "issue": { "number": 1, "title": "t", "body": "b", "labels": [] }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let resp = post_webhook(app, "issues", body_bytes, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn pr_review_empty_body_no_constraint() {
    let app = make_test_router(None);
    let body = serde_json::json!({
        "repository": { "full_name": "org/repo" },
        "action": "submitted",
        "review": { "state": "changes_requested", "body": "" },
        "pull_request": { "title": "feat: something" }
    });
    let body_bytes = serde_json::to_vec(&body).unwrap();
    let resp = post_webhook(app, "pull_request_review", body_bytes, None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}
