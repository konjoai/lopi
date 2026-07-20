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
    /// Anthropic API client used to call the triage model.
    pub api_client: Arc<AnthropicClient>,
    /// GitHub client used to post triage comment results on issues.
    pub github: Arc<GitHubClient>,
    /// Optional token-bucket rate limiter applied to triage API calls.
    pub limiter: Option<Arc<AnthropicLimiter>>,
    /// Optional circuit breaker to stop triage calls when the API is unhealthy.
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
    if let Some(reject) = hmac_guard(&s.secret, &headers, &body) {
        return reject;
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

    dispatch_event(&s, &payload, &event, &repo).await;
    (StatusCode::OK, "ok").into_response()
}

/// Return a rejection response if the HMAC signature is invalid; otherwise `None`.
fn hmac_guard(
    secret: &Option<String>,
    headers: &HeaderMap,
    body: &[u8],
) -> Option<axum::response::Response> {
    let secret = secret.as_deref()?;
    let sig = headers
        .get("X-Hub-Signature-256")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if verify_signature(secret.as_bytes(), body, sig) {
        None
    } else {
        tracing::warn!("GitHub webhook HMAC verification failed");
        Some((StatusCode::UNAUTHORIZED, "invalid signature").into_response())
    }
}

/// Route a verified webhook event to the appropriate handler.
async fn dispatch_event(s: &WebhookState, payload: &Value, event: &str, repo: &str) {
    let conclusion = payload
        .get("workflow_run")
        .or_else(|| payload.get("check_run"))
        .and_then(|w| w.get("conclusion"))
        .and_then(|c| c.as_str());
    if matches!(conclusion, Some("failure" | "timed_out")) {
        queue_ci_fix(repo, event, &s.queue).await;
    }
    if event == "pull_request_review" {
        handle_pr_review(payload, repo, event, &s.queue).await;
    }
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    if event == "issues" && should_triage_issue_event(action, payload) {
        handle_issue_triage(payload, repo, s);
    }
}

/// Decide whether an `issues` webhook event should trigger (re-)triage.
///
/// Every newly `opened` issue is triaged once. For `labeled` events, only
/// the `lopi:fix` label should re-trigger triage — otherwise adding any
/// unrelated label (e.g. `good first issue`) would re-classify the issue
/// and re-post the triage comment on every label change.
fn should_triage_issue_event(action: &str, payload: &Value) -> bool {
    match action {
        "opened" => true,
        "labeled" => payload
            .get("label")
            .and_then(|l| l.get("name"))
            .and_then(|v| v.as_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("lopi:fix")),
        _ => false,
    }
}

/// Queue a high-priority CI fix task.
async fn queue_ci_fix(repo: &str, event: &str, queue: &TaskQueue) {
    let mut t = Task::new(format!("Investigate and fix CI failure on {repo}"));
    t.priority = Priority::High;
    t.source = TaskSource::Webhook {
        repo: repo.to_string(),
        event: event.to_string(),
    };
    queue.push(t).await;
    tracing::info!("queued CI fix task for {repo} (event: {event})");
}

/// Re-queue a fix task when a reviewer requests changes on a PR.
async fn handle_pr_review(payload: &Value, repo: &str, event: &str, queue: &TaskQueue) {
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let review_state = payload
        .get("review")
        .and_then(|r| r.get("state"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if action != "submitted" || review_state != "changes_requested" {
        return;
    }
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
        repo: repo.to_string(),
        event: event.to_string(),
    };
    if !review_body.is_empty() {
        t.constraints
            .push(format!("Review feedback: {review_body}"));
    }
    queue.push(t).await;
    tracing::info!("queued PR review fix task for {repo}: {pr_title}");
}

/// Classify an opened/labeled issue via Haiku and optionally queue a fix task.
fn handle_issue_triage(payload: &Value, repo: &str, s: &WebhookState) {
    if let (Some(triage), Some(ip)) = (
        s.triage.clone(),
        crate::issue::extract_from_json(payload, repo),
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

/// Verify GitHub's `X-Hub-Signature-256: sha256=<hex>` header against `body`.
fn verify_signature(secret: &[u8], body: &[u8], sig_header: &str) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    // Lowercased so casing differences in the header don't cause a spurious
    // mismatch — `hex::decode` (the previous approach) was case-insensitive
    // too, this preserves that.
    let expected_hex = sig_header
        .strip_prefix("sha256=")
        .unwrap_or("")
        .to_lowercase();

    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
        return false;
    };
    mac.update(body);
    let computed_hex = hex::encode(mac.finalize().into_bytes());

    // Constant-time comparison to prevent timing attacks.
    lopi_core::constant_time_eq(&computed_hex, &expected_hex)
}

#[cfg(test)]
#[path = "github_tests.rs"]
mod tests;
