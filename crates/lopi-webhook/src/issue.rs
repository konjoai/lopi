//! Handler for GitHub `issues` webhook events.
//!
//! Dispatched from `github.rs` when `action == "opened"` or the issue
//! carries the `lopi:fix` label. Runs triage in a background task so
//! the webhook endpoint can return 200 immediately (GitHub retries on
//! non-2xx within 10 s — we must never block).
//!
//! Flow per `issues.opened` event:
//!   1. Spawn a Tokio task (best-effort; failure logged, not bubbled)
//!   2. Classify the issue via Haiku (issue_triage::classify_issue)
//!   3. Post the triage comment on the GitHub issue
//!   4. Add the category label
//!   5. If category == Bug ∧ confidence ≥ 0.7 → auto-queue a fix task
//!      If `lopi:fix` label present → auto-queue regardless of category

use crate::issue_triage::{self, IssueCategory};
use lopi_agent::AnthropicClient;
use lopi_core::{Priority, Task, TaskSource};
use lopi_github::GitHubClient;
use lopi_orchestrator::TaskQueue;
use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
use std::sync::Arc;

/// Extracted issue payload — constructed from the raw webhook JSON.
#[derive(Debug, Clone)]
pub struct IssuePayload {
    pub owner: String,
    pub repo: String,
    pub full_name: String,
    pub number: u64,
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
}

/// Extract an `IssuePayload` from a raw GitHub `issues` webhook JSON value.
/// Returns `None` if the payload is missing required fields.
pub fn extract_from_json(payload: &serde_json::Value, full_name: &str) -> Option<IssuePayload> {
    let issue = payload.get("issue")?;
    let number = issue.get("number")?.as_u64()?;
    let title = issue.get("title")?.as_str()?.to_string();
    if title.is_empty() {
        return None;
    }
    let body = issue
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let labels: Vec<String> = issue
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l.get("name").and_then(|n| n.as_str()))
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let (owner, repo) = full_name
        .split_once('/')
        .map(|(o, r)| (o.to_string(), r.to_string()))
        .unwrap_or_else(|| ("unknown".into(), full_name.to_string()));
    Some(IssuePayload {
        owner,
        repo,
        full_name: full_name.to_string(),
        number,
        title,
        body,
        labels,
    })
}

impl IssuePayload {
    /// True when the issue has been labelled `lopi:fix` by a human,
    /// which forces auto-queue regardless of triage classification.
    pub fn has_lopi_fix_label(&self) -> bool {
        self.labels
            .iter()
            .any(|l| l.eq_ignore_ascii_case("lopi:fix"))
    }
}

/// Spawn a background triage task for an opened issue.
/// Returns immediately — all work happens inside the spawned task.
pub fn spawn_triage(
    payload: IssuePayload,
    model: String,
    api_client: Arc<AnthropicClient>,
    limiter: Option<Arc<AnthropicLimiter>>,
    breaker: Option<Arc<CircuitBreaker>>,
    github: Arc<GitHubClient>,
    queue: TaskQueue,
) {
    tokio::spawn(async move {
        let result = run_triage(
            &payload,
            &model,
            &api_client,
            limiter.as_ref(),
            breaker.as_ref(),
            &github,
            &queue,
        )
        .await;
        if let Err(e) = result {
            tracing::warn!(
                repo = %payload.full_name,
                issue = payload.number,
                "issue triage failed: {e}"
            );
        }
    });
}

async fn run_triage(
    payload: &IssuePayload,
    model: &str,
    api_client: &Arc<AnthropicClient>,
    limiter: Option<&Arc<AnthropicLimiter>>,
    breaker: Option<&Arc<CircuitBreaker>>,
    github: &Arc<GitHubClient>,
    queue: &TaskQueue,
) -> anyhow::Result<()> {
    let triage = issue_triage::classify_issue(
        api_client,
        limiter,
        breaker,
        model,
        &payload.title,
        &payload.body,
    )
    .await?;

    tracing::info!(
        repo = %payload.full_name,
        issue = payload.number,
        category = triage.category.as_str(),
        confidence = triage.confidence,
        "issue triaged"
    );

    // Post the triage comment.
    let comment = issue_triage::format_triage_comment(&triage, &payload.full_name);
    github
        .post_comment(&payload.owner, &payload.repo, payload.number, &comment)
        .await?;

    // Add the category label.
    github
        .add_labels(
            &payload.owner,
            &payload.repo,
            payload.number,
            &[triage.category.label()],
        )
        .await?;

    // Auto-queue logic.
    let should_queue = payload.has_lopi_fix_label()
        || matches!(triage.category, IssueCategory::Bug if triage.confidence >= 0.7);

    if should_queue {
        let goal = format!(
            "Fix GitHub issue #{} on {}: {}",
            payload.number, payload.full_name, payload.title
        );
        let mut task = Task::new(goal);
        task.priority = Priority::High;
        task.source = TaskSource::Webhook {
            repo: payload.full_name.clone(),
            event: "issues".into(),
        };
        // Inject the issue body as a constraint so the planning prompt has context.
        if !payload.body.is_empty() {
            let preview = payload.body.chars().take(500).collect::<String>();
            task.constraints.push(format!("Issue body: {preview}"));
        }
        queue.push(task).await;
        tracing::info!(
            repo = %payload.full_name,
            issue = payload.number,
            "auto-queued fix task"
        );
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn make_payload(labels: Vec<&str>) -> IssuePayload {
        IssuePayload {
            owner: "org".into(),
            repo: "repo".into(),
            full_name: "org/repo".into(),
            number: 42,
            title: "Something is broken".into(),
            body: "Steps to reproduce…".into(),
            labels: labels.into_iter().map(String::from).collect(),
        }
    }

    #[test]
    fn extract_from_json_valid_payload() {
        let payload = serde_json::json!({
            "issue": {
                "number": 7,
                "title": "Bug: crash on empty input",
                "body": "Steps to reproduce…",
                "labels": [{"name": "bug"}, {"name": "lopi:fix"}]
            }
        });
        let result = extract_from_json(&payload, "myorg/myrepo").unwrap();
        assert_eq!(result.number, 7);
        assert_eq!(result.title, "Bug: crash on empty input");
        assert_eq!(result.owner, "myorg");
        assert_eq!(result.repo, "myrepo");
        assert_eq!(result.labels, vec!["bug", "lopi:fix"]);
    }

    #[test]
    fn extract_from_json_missing_issue_returns_none() {
        let payload = serde_json::json!({ "action": "opened" });
        assert!(extract_from_json(&payload, "org/repo").is_none());
    }

    #[test]
    fn extract_from_json_empty_title_returns_none() {
        let payload = serde_json::json!({
            "issue": { "number": 1, "title": "", "body": "x", "labels": [] }
        });
        assert!(extract_from_json(&payload, "org/repo").is_none());
    }

    #[test]
    fn extract_from_json_no_slash_in_full_name_uses_unknown_owner() {
        let payload = serde_json::json!({
            "issue": { "number": 1, "title": "T", "body": null, "labels": [] }
        });
        let result = extract_from_json(&payload, "noslash").unwrap();
        assert_eq!(result.owner, "unknown");
        assert_eq!(result.repo, "noslash");
    }

    #[test]
    fn extract_from_json_null_body_becomes_empty_string() {
        let payload = serde_json::json!({
            "issue": { "number": 3, "title": "T", "body": null, "labels": [] }
        });
        let result = extract_from_json(&payload, "a/b").unwrap();
        assert_eq!(result.body, "");
    }

    #[test]
    fn has_lopi_fix_label_case_insensitive() {
        assert!(make_payload(vec!["lopi:fix"]).has_lopi_fix_label());
        assert!(make_payload(vec!["LOPI:FIX"]).has_lopi_fix_label());
        assert!(make_payload(vec!["Lopi:Fix"]).has_lopi_fix_label());
    }

    #[test]
    fn no_lopi_fix_label() {
        assert!(!make_payload(vec!["bug", "enhancement"]).has_lopi_fix_label());
        assert!(!make_payload(vec![]).has_lopi_fix_label());
    }
}
