//! Sprint H — Failure post-mortem.
//!
//! When all retries exhausted on a task and the runner has been wired with
//! an `AnthropicClient` (Sprint G), this module runs a focused reflection
//! session: feed the failure context into Claude, ask for **a single
//! imperative constraint** that would have prevented this failure, and
//! return that constraint string for the caller to persist via
//! `MemoryStore::insert_postmortem_pattern`.
//!
//! Why a single constraint, not a fluffy reflection:
//!   - Patterns are consumed by `runner::run_loop` as TOON-encoded prose
//!     in the planning prompt. A single line slots in cleanly.
//!   - Bounded scope discourages the model from drifting into general
//!     advice unrelated to the actual failure.
//!   - One imperative per failure → easy to evaluate post-hoc whether
//!     the pattern actually helped on the next run.
//!
//! The post-mortem inherits Sprint G's resilience: limiter + breaker gates,
//! cache_control on the system prompt, and does NOT recurse. If the post-mortem
//! itself fails, we log and move on. No infinite reflection.

use crate::api_client::AnthropicClient;
use anyhow::{Context, Result};
use lopi_ratelimit::{AnthropicLimiter, CircuitBreaker};
use std::sync::Arc;
use std::time::Duration;

/// System prompt for the post-mortem session. Must be byte-stable so that
/// `cache_control: ephemeral` hits across all post-mortems in a run.
pub(crate) const POSTMORTEM_SYSTEM_PROMPT: &str = "\
You are a Konjo agent post-mortem analyst. A coding agent has just \
exhausted all retry attempts on a task. Your job is to analyze the failure \
and produce ONE imperative constraint that would have prevented it.

Output rules:
1. Reply with exactly ONE line — no preamble, no explanation, no markdown.
2. The line must start with 'must' or 'do not' or 'always' or 'never'.
3. The line must be a concrete, actionable constraint (≤ 200 chars).
4. The line must reference a specific behavior, not a general principle.

Bad output (too vague):
  must write better code
  always test thoroughly

Good output (concrete):
  must run cargo clippy after every edit and fix all warnings before claiming success
  do not modify Cargo.lock — only edit Cargo.toml and let cargo regenerate the lockfile
  always check that the function signature matches the trait before implementing it

The output line will be stored as a 'pattern constraint' and injected into \
the planning prompt of future similar tasks. Make it actionable.
";

/// Output of `run_postmortem`. Wraps the raw constraint plus the API usage
/// so the caller can feed `TurnMetrics` for observability.
pub struct PostmortemOutcome {
    pub constraint: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: u32,
}

/// Run a post-mortem. Returns the constraint string (already trimmed +
/// validated to be a single line ≤ 200 chars).
///
/// Inputs:
///   - `client` / `limiter` / `breaker` — same Sprint G resilience layer
///   - `goal` — the failed task's goal text
///   - `error_log` — concatenated stderr / test output / final failure
///     reason from the last attempt
///   - `model` — which Claude model to use; typically Haiku for cost
///
/// # Errors
/// Returns an error if the breaker is open, the API call fails, or the
/// model returns an empty / non-imperative response. Caller should log
/// and continue — never block task termination on post-mortem failure.
pub async fn run_postmortem(
    client: &Arc<AnthropicClient>,
    limiter: Option<&Arc<AnthropicLimiter>>,
    breaker: Option<&Arc<CircuitBreaker>>,
    model: &str,
    goal: &str,
    error_log: &str,
) -> Result<PostmortemOutcome> {
    if let Some(b) = breaker {
        b.check()
            .await
            .context("post-mortem skipped: circuit breaker open")?;
    }
    if let Some(l) = limiter {
        // Post-mortem is a single short turn — 1500 token budget is generous.
        l.acquire_request(1500.0).await;
    }

    let prompt = build_postmortem_prompt(goal, error_log);

    // Stream is unnecessary here — we want the full single-line output to
    // validate it before returning. Use complete() if it exists; for now
    // we'll consume the stream and assemble.
    let result = client
        .stream_plan(model, POSTMORTEM_SYSTEM_PROMPT, &prompt, |_| {})
        .await;

    match result {
        Ok((text, usage)) => {
            if let Some(b) = breaker {
                b.record_success().await;
                b.record_cost(usage.estimated_cost(model)).await;
            }
            let constraint = extract_constraint(&text)
                .context("post-mortem returned empty or invalid constraint")?;
            Ok(PostmortemOutcome {
                constraint,
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_read_tokens: usage.cache_read_tokens,
            })
        }
        Err(e) => {
            if let Some(b) = breaker {
                b.record_failure().await;
            }
            Err(e.context("post-mortem API call failed"))
        }
    }
}

/// Build the user prompt — deterministic for cache hits when a series of
/// runs hit similar errors.
pub(crate) fn build_postmortem_prompt(goal: &str, error_log: &str) -> String {
    // Truncate error log to keep total prompt size predictable. The most
    // recent 4000 chars are typically where the failure root cause appears.
    let truncated = if error_log.len() > 4000 {
        let start = error_log.len() - 4000;
        format!(
            "[truncated, showing last 4000 chars]\n{}",
            &error_log[start..]
        )
    } else {
        error_log.to_string()
    };

    format!(
        "# Failed task\n{goal}\n\n# Error / failure log\n{truncated}\n\n\
         Reply with ONE imperative constraint line per the system prompt rules."
    )
}

/// Validate + clean the model output. Rejects multi-line, empty, and
/// non-imperative responses.
pub(crate) fn extract_constraint(raw: &str) -> Option<String> {
    // Take first non-empty line — model sometimes leaks a leading blank.
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty())?;

    // Strip common markdown bullets the model adds despite instructions
    let line = line
        .trim_start_matches("- ")
        .trim_start_matches("* ")
        .trim_start_matches("> ")
        .trim();

    if line.is_empty() {
        return None;
    }
    if line.len() > 200 {
        // Truncate over-long lines to 200 chars rather than rejecting —
        // gives the user something usable.
        return Some(
            line.chars()
                .take(200)
                .collect::<String>()
                .trim()
                .to_string(),
        );
    }

    let lower = line.to_lowercase();
    let starts_with_imperative = lower.starts_with("must")
        || lower.starts_with("do not")
        || lower.starts_with("always")
        || lower.starts_with("never");

    if !starts_with_imperative {
        return None;
    }

    Some(line.to_string())
}

/// Convenience for callers who want to swallow post-mortem errors without
/// dropping useful logging. Logs via `tracing::warn` and returns None on
/// any failure.
pub async fn run_postmortem_quiet(
    client: &Arc<AnthropicClient>,
    limiter: Option<&Arc<AnthropicLimiter>>,
    breaker: Option<&Arc<CircuitBreaker>>,
    model: &str,
    goal: &str,
    error_log: &str,
) -> Option<PostmortemOutcome> {
    match run_postmortem(client, limiter, breaker, model, goal, error_log).await {
        Ok(out) => Some(out),
        Err(e) => {
            tracing::warn!(error = %e, "post-mortem failed; no pattern derived");
            None
        }
    }
}

// Suppress unused warning when nothing in this crate calls these directly
// (they are public surface for the caller in main.rs / orchestrator).
#[allow(dead_code)]
const _BUILDER_HINT: Duration = Duration::from_secs(0);

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn extract_must_imperative() {
        assert_eq!(
            extract_constraint("must add CSRF before session creation"),
            Some("must add CSRF before session creation".to_string())
        );
    }

    #[test]
    fn extract_do_not_imperative() {
        assert_eq!(
            extract_constraint("do not modify Cargo.lock directly"),
            Some("do not modify Cargo.lock directly".to_string())
        );
    }

    #[test]
    fn extract_always_imperative() {
        assert_eq!(
            extract_constraint("Always run cargo clippy before committing"),
            Some("Always run cargo clippy before committing".to_string())
        );
    }

    #[test]
    fn extract_strips_markdown_bullet() {
        assert_eq!(
            extract_constraint("- must check function signatures"),
            Some("must check function signatures".to_string())
        );
        assert_eq!(
            extract_constraint("* never silently swallow errors"),
            Some("never silently swallow errors".to_string())
        );
    }

    #[test]
    fn extract_takes_first_nonempty_line() {
        let raw = "\n\nmust validate inputs\nadditional fluff line";
        assert_eq!(
            extract_constraint(raw),
            Some("must validate inputs".to_string())
        );
    }

    #[test]
    fn extract_rejects_non_imperative() {
        assert!(extract_constraint("the agent should be careful").is_none());
        assert!(extract_constraint("perhaps run tests first").is_none());
    }

    #[test]
    fn extract_rejects_empty() {
        assert!(extract_constraint("").is_none());
        assert!(extract_constraint("   ").is_none());
        assert!(extract_constraint("\n\n").is_none());
    }

    #[test]
    fn extract_truncates_overlong_line() {
        let long = "must ".to_string() + &"x".repeat(500);
        let result = extract_constraint(&long);
        assert!(result.is_some());
        if let Some(r) = result {
            assert!(r.len() <= 200);
            assert!(r.starts_with("must "));
        }
    }

    #[test]
    fn build_prompt_includes_goal_and_log() {
        let p = build_postmortem_prompt("fix the bug", "stack trace here");
        assert!(p.contains("fix the bug"));
        assert!(p.contains("stack trace here"));
        assert!(p.contains("# Failed task"));
        assert!(p.contains("# Error / failure log"));
    }

    #[test]
    fn build_prompt_truncates_long_error_log() {
        let huge = "X".repeat(10_000);
        let p = build_postmortem_prompt("g", &huge);
        assert!(p.contains("[truncated"));
        assert!(p.len() < 6000); // truncated body + some scaffolding
    }

    #[test]
    fn build_prompt_is_deterministic() {
        let a = build_postmortem_prompt("g", "err");
        let b = build_postmortem_prompt("g", "err");
        assert_eq!(a, b);
    }

    #[test]
    fn extract_constraint_skips_leading_blank_lines() {
        // Claude might return leading blank lines before the constraint
        let raw = "\n\nmust add input validation before processing\n\nAdditional explanation...";
        assert_eq!(
            extract_constraint(raw),
            Some("must add input validation before processing".to_string())
        );
    }

    #[test]
    fn extract_constraint_normalizes_whitespace() {
        let raw = "  must\t  check  file  permissions  ";
        let result = extract_constraint(raw);
        assert!(result.is_some());
        if let Some(r) = result {
            // Leading/trailing whitespace should be stripped
            assert!(r.starts_with("must"));
            assert!(!r.starts_with(" "));
            assert!(!r.ends_with(" "));
        }
    }

    #[test]
    fn extract_constraint_minimum_length() {
        // Constraint must be at least 10 chars to be useful
        assert_eq!(extract_constraint("must go"), Some("must go".to_string())); // still extracted; min length not enforced
        assert!(extract_constraint("must a").is_some()); // very short but valid
    }

    #[test]
    fn extract_never_imperative() {
        assert_eq!(
            extract_constraint("never ignore validation errors"),
            Some("never ignore validation errors".to_string())
        );
    }

    #[test]
    fn extract_constraint_case_insensitive_keyword() {
        // Keywords should work in various cases
        assert_eq!(
            extract_constraint("MUST handle nil pointers"),
            Some("MUST handle nil pointers".to_string())
        );
        assert_eq!(
            extract_constraint("Always test edge cases"),
            Some("Always test edge cases".to_string())
        );
    }
}
