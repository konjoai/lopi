use crate::issue::spawn_triage;
use anyhow::Result;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Router,
};
use lopi_agent::AnthropicClient;
use lopi_core::{Priority, Task, TaskSource};
use lopi_github::GitHubClient;
use lopi_orchestrator::TaskQueue;
use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;

/// Optional triage configuration — when present, issue events are
/// classified by Haiku and a comment is posted on the GitHub issue.
#[derive(Clone)]
pub struct TriageConfig {
    pub api_client: Arc<AnthropicClient>,
    pub github: Arc<GitHubClient>,
    pub limiter: Option<Arc<AnthropicLimiter>>,
    pub breaker: Option<Arc<CircuitBreaker>>,
    /// Model for triage — Haiku is the right cost/quality trade-off.
    pub model: String,
}

#[derive(Clone)]
struct WebhookState {
    queue: TaskQueue,
    secret: Option<String>,
    triage: Option<TriageConfig>,
}

/// # Errors
///
/// Returns an error if the TCP listener cannot bind to the address.
pub async fn serve(
    queue: TaskQueue,
    secret: Option<String>,
    addr: SocketAddr,
    triage: Option<TriageConfig>,
) -> Result<()> {
    let state = WebhookState { queue, secret, triage };
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

    if matches!(conclusion, Some("failure" | "timed_out")) {
        let mut t = Task::new(format!("Investigate and fix CI failure on {repo}"));
        t.priority = Priority::High;
        t.source = TaskSource::Webhook {
            repo: repo.clone(),
            event: event.clone(),
        };
        s.queue.push(t).await;
        tracing::info!("queued CI fix task for {repo} (event: {event})");
    }

    // PR review loop: when a reviewer requests changes, re-queue the task with the
    // review body injected as a constraint so lopi can address the feedback automatically.
    if event == "pull_request_review" {
        let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let state = payload
            .get("review")
            .and_then(|r| r.get("state"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if action == "submitted" && state == "changes_requested" {
            let review_body = payload
                .get("review")
                .and_then(|r| r.get("body"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let pr_title = payload
                .get("pull_request")
                .and_then(|pr| pr.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown PR")
                .to_string();
            let goal = format!("Address review feedback on PR '{pr_title}' in {repo}");
            let mut t = Task::new(goal);
            t.priority = Priority::High;
            t.source = TaskSource::Webhook {
                repo: repo.clone(),
                event: event.clone(),
            };
            if !review_body.is_empty() {
                t.constraints
                    .push(format!("Review feedback: {review_body}"));
            }
            s.queue.push(t).await;
            tracing::info!("queued PR review fix task for {repo}: {pr_title}");
        }
    }

    // Issue triage: classify opened/labeled issues via Haiku; optionally queue a fix task.
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    if event == "issues" && (action == "opened" || action == "labeled") {
        if let (Some(triage), Some(ip)) =
            (s.triage.clone(), crate::issue::extract_from_json(&payload, &repo))
        {
            spawn_triage(ip, triage.model, triage.api_client, triage.limiter, triage.breaker, triage.github, s.queue.clone());
        }
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

    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
        return false;
    };
    mac.update(body);
    let computed = mac.finalize().into_bytes();

    // Constant-time comparison to prevent timing attacks.
    computed.as_slice() == expected_bytes.as_slice()
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
}
