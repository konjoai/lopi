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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/github")
                .header("X-GitHub-Event", "workflow_run")
                .header("Content-Type", "application/json")
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/github")
                .header("X-GitHub-Event", "workflow_run")
                .header("Content-Type", "application/json")
                .header("X-Hub-Signature-256", sig)
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/github")
                .header("X-GitHub-Event", "workflow_run")
                .header("Content-Type", "application/json")
                .header("X-Hub-Signature-256", bad_sig)
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/github")
                .header("X-GitHub-Event", "pull_request_review")
                .header("Content-Type", "application/json")
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/github")
                .header("X-GitHub-Event", "pull_request_review")
                .header("Content-Type", "application/json")
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/github")
                .header("X-GitHub-Event", "workflow_run")
                .header("Content-Type", "application/json")
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn invalid_json_returns_400() {
    let app = make_test_router(None);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/github")
                .header("X-GitHub-Event", "workflow_run")
                .header("Content-Type", "application/json")
                .body(Body::from("not valid json!!!"))
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/github")
                .header("X-GitHub-Event", "workflow_run")
                .header("Content-Type", "application/json")
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/github")
                .header("X-GitHub-Event", "check_run")
                .header("Content-Type", "application/json")
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
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
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/webhook/github")
                .header("X-GitHub-Event", "pull_request_review")
                .header("Content-Type", "application/json")
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
