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
    let state = WebhookState {
        queue,
        secret,
        triage,
    };
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
        if let (Some(triage), Some(ip)) = (
            s.triage.clone(),
            crate::issue::extract_from_json(&payload, &repo),
        ) {
            spawn_triage(
                ip,
                triage.model,
                triage.api_client,
                triage.limiter,
                triage.breaker,
                triage.github,
                s.queue.clone(),
            );
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
mod tests;
