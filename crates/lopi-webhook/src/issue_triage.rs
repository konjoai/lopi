//! GitHub issue triage via Claude Haiku.
//!
//! One-shot classification: Bug | Feature | Question | WontFix + confidence
//! score + one-line summary. Cheap by design — Haiku at <1500 tokens/call
//! with a byte-stable system prompt so `cache_control: ephemeral` hits
//! across every issue in the same lopi process lifetime.
//!
//! The triage result is used by `issue.rs` to:
//!   - Post a standardised comment on the GitHub issue
//!   - Auto-queue a fix task when category == Bug and confidence >= 0.7
//!   - Add appropriate labels via the GitHubClient

use anyhow::{Context, Result};
use lopi_agent::AnthropicClient;
use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
use std::sync::Arc;

/// Issue classification output from the triage model.
#[derive(Debug, Clone, PartialEq)]
pub struct IssueTriage {
    pub category: IssueCategory,
    /// Model confidence 0.0–1.0 in the classification.
    pub confidence: f32,
    /// One-line human-readable summary for the GitHub comment.
    pub summary: String,
}

/// Four-way classification matching the categories in the triage prompt.
#[derive(Debug, Clone, PartialEq)]
pub enum IssueCategory {
    Bug,
    Feature,
    Question,
    WontFix,
}

impl IssueCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Feature => "feature",
            Self::Question => "question",
            Self::WontFix => "wontfix",
        }
    }

    /// GitHub label to apply for this category.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Feature => "enhancement",
            Self::Question => "question",
            Self::WontFix => "wontfix",
        }
    }
}

/// Byte-stable system prompt — cached by the Anthropic API across calls.
/// Any edit resets the cache; edit intentionally and with awareness of cost.
const TRIAGE_SYSTEM_PROMPT: &str = "\
You are a Konjo issue triage analyst. Your job is to classify GitHub issues \
quickly and accurately so an autonomous agent can act on them.

Output format — exactly three lines, no preamble:
LINE 1: category (one of: bug, feature, question, wontfix)
LINE 2: confidence (0.0–1.0, two decimal places, e.g. 0.87)
LINE 3: summary (≤ 120 chars, present-tense description of the issue)

Classification rules:
- bug: something that was working and is now broken, or clearly incorrect behaviour
- feature: a request for new functionality that does not currently exist
- question: asking how something works; documentation or usage help
- wontfix: duplicate, out of scope, intentional behaviour, or spam

Respond with exactly three lines. No markdown. No explanation.
";

/// Classify a GitHub issue using Claude Haiku.
///
/// The call is best-effort: the caller should spawn this in a task and handle
/// errors gracefully rather than blocking the webhook response.
///
/// # Errors
///
/// Returns an error if the breaker is open, the API call fails, or the model
/// returns an unparseable response.
pub async fn classify_issue(
    client: &Arc<AnthropicClient>,
    limiter: Option<&Arc<AnthropicLimiter>>,
    breaker: Option<&Arc<CircuitBreaker>>,
    model: &str,
    title: &str,
    body: &str,
) -> Result<IssueTriage> {
    if let Some(b) = breaker {
        b.check()
            .await
            .context("triage skipped: circuit breaker open")?;
    }
    if let Some(l) = limiter {
        l.acquire_request(1500.0).await;
    }

    let prompt = build_triage_prompt(title, body);
    let (text, _usage) = client
        .stream_plan(model, TRIAGE_SYSTEM_PROMPT, &prompt, |_| {})
        .await
        .context("triage API call failed")?;

    if let Some(b) = breaker {
        b.record_success().await;
    }

    parse_triage_response(&text).context("failed to parse triage response")
}

pub(crate) fn build_triage_prompt(title: &str, body: &str) -> String {
    let body_preview = if body.len() > 2000 {
        format!("{}…[truncated]", &body[..2000])
    } else {
        body.to_string()
    };
    format!("# Issue title\n{title}\n\n# Issue body\n{body_preview}")
}

pub(crate) fn parse_triage_response(raw: &str) -> Option<IssueTriage> {
    let mut lines = raw.lines().map(str::trim).filter(|l| !l.is_empty());

    let category_str = lines.next()?;
    let category = match category_str.to_lowercase().as_str() {
        "bug" => IssueCategory::Bug,
        "feature" => IssueCategory::Feature,
        "question" => IssueCategory::Question,
        "wontfix" => IssueCategory::WontFix,
        _ => return None,
    };

    let confidence: f32 = lines.next()?.parse().ok()?;
    let confidence = confidence.clamp(0.0, 1.0);

    let summary = lines.next()?.chars().take(120).collect::<String>();
    if summary.is_empty() {
        return None;
    }

    Some(IssueTriage { category, confidence, summary })
}

/// Format the comment lopi posts on a triaged issue.
pub(crate) fn format_triage_comment(triage: &IssueTriage, repo: &str) -> String {
    let icon = match triage.category {
        IssueCategory::Bug => "🐛",
        IssueCategory::Feature => "✨",
        IssueCategory::Question => "❓",
        IssueCategory::WontFix => "🚫",
    };
    let action = match triage.category {
        IssueCategory::Bug if triage.confidence >= 0.7 => {
            "\n\n**Auto-queuing a fix task** — lopi will attempt a patch and open a PR."
        }
        IssueCategory::Bug => {
            "\n\n_Confidence below threshold — not auto-queuing. Add the `lopi:fix` label to force a fix attempt._"
        }
        IssueCategory::Feature => "\n\n_Queued for backlog review. Add the `lopi:fix` label to request an implementation attempt._",
        IssueCategory::Question => "\n\n_No automated action needed — a team member will respond._",
        IssueCategory::WontFix => "\n\n_This issue will be closed as out-of-scope or resolved._",
    };

    format!(
        "{icon} **lopi triage** · `{repo}`\n\n\
         **Category:** {cat} (confidence: {conf:.0}%)\n\
         **Summary:** {summary}{action}\n\n\
         ---\n\
         *Triaged automatically by [lopi](https://github.com/konjoai/lopi)*",
        cat = triage.category.as_str(),
        conf = triage.confidence * 100.0,
        summary = triage.summary,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_bug_response() {
        let raw = "bug\n0.92\nButton click handler panics on double-tap in Safari";
        let t = parse_triage_response(raw).unwrap();
        assert_eq!(t.category, IssueCategory::Bug);
        assert!((t.confidence - 0.92).abs() < 0.001);
        assert!(t.summary.contains("panic"));
    }

    #[test]
    fn parse_feature_response() {
        let raw = "feature\n0.85\nAdd dark mode support to the settings panel";
        let t = parse_triage_response(raw).unwrap();
        assert_eq!(t.category, IssueCategory::Feature);
        assert!((t.confidence - 0.85).abs() < 0.001);
    }

    #[test]
    fn parse_question_response() {
        let raw = "question\n0.95\nUser asking how to configure the timeout setting";
        let t = parse_triage_response(raw).unwrap();
        assert_eq!(t.category, IssueCategory::Question);
    }

    #[test]
    fn parse_wontfix_response() {
        let raw = "wontfix\n0.80\nDuplicate of issue #42";
        let t = parse_triage_response(raw).unwrap();
        assert_eq!(t.category, IssueCategory::WontFix);
    }

    #[test]
    fn parse_case_insensitive() {
        let raw = "BUG\n0.75\nCrash on startup";
        let t = parse_triage_response(raw).unwrap();
        assert_eq!(t.category, IssueCategory::Bug);
    }

    #[test]
    fn parse_rejects_unknown_category() {
        let raw = "unknown_category\n0.90\nSome summary";
        assert!(parse_triage_response(raw).is_none());
    }

    #[test]
    fn parse_rejects_missing_lines() {
        assert!(parse_triage_response("bug\n0.9").is_none());
        assert!(parse_triage_response("bug").is_none());
        assert!(parse_triage_response("").is_none());
    }

    #[test]
    fn parse_clamps_confidence() {
        let raw = "bug\n1.5\nOverflow confidence";
        let t = parse_triage_response(raw).unwrap();
        assert!(t.confidence <= 1.0);
    }

    #[test]
    fn parse_skips_blank_lines() {
        let raw = "\nbug\n\n0.88\n\nPanic in serializer when input is null\n";
        let t = parse_triage_response(raw).unwrap();
        assert_eq!(t.category, IssueCategory::Bug);
        assert!((t.confidence - 0.88).abs() < 0.001);
    }

    #[test]
    fn summary_truncated_at_120_chars() {
        let long_summary = "x".repeat(200);
        let raw = format!("bug\n0.80\n{long_summary}");
        let t = parse_triage_response(&raw).unwrap();
        assert!(t.summary.len() <= 120);
    }

    #[test]
    fn category_labels() {
        assert_eq!(IssueCategory::Bug.label(), "bug");
        assert_eq!(IssueCategory::Feature.label(), "enhancement");
        assert_eq!(IssueCategory::Question.label(), "question");
        assert_eq!(IssueCategory::WontFix.label(), "wontfix");
    }

    #[test]
    fn format_comment_high_confidence_bug_mentions_auto_queue() {
        let triage = IssueTriage {
            category: IssueCategory::Bug,
            confidence: 0.92,
            summary: "Panic on startup".to_string(),
        };
        let comment = format_triage_comment(&triage, "org/repo");
        assert!(comment.contains("Auto-queuing"));
        assert!(comment.contains("org/repo"));
        assert!(comment.contains("92%"));
    }

    #[test]
    fn format_comment_low_confidence_bug_no_auto_queue() {
        let triage = IssueTriage {
            category: IssueCategory::Bug,
            confidence: 0.55,
            summary: "Might be a bug".to_string(),
        };
        let comment = format_triage_comment(&triage, "org/repo");
        assert!(!comment.contains("Auto-queuing"));
        assert!(comment.contains("lopi:fix"));
    }

    #[test]
    fn build_prompt_truncates_long_body() {
        let long_body = "x".repeat(5000);
        let prompt = build_triage_prompt("Test title", &long_body);
        assert!(prompt.contains("truncated"));
        assert!(prompt.len() < 4000);
    }
}
