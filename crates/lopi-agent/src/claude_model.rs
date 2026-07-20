//! Model identifiers, model-routing heuristic, and the one-shot CLI output
//! envelope — split out of `claude.rs` purely to keep that file under the
//! 500-line CI file-size gate; every public item here is re-exported from
//! `claude` by an explicit named `pub use` list (not a glob), so every
//! existing `crate::claude::MODEL_*`/`select_model`/`ClaudeOutput` path
//! stays valid. Adding a new `pub` item to this module does NOT
//! automatically make it visible at `crate::claude::*` — extend that list
//! in `claude.rs` too.

use lopi_core::Task;
use serde::Deserialize;

// ── Model identifiers ─────────────────────────────────────────────────────────

/// Claude Haiku model identifier — lowest cost, fast latency.
pub const MODEL_HAIKU: &str = "claude-haiku-4-5-20251001";
/// Claude Sonnet model identifier — default balanced model.
pub const MODEL_SONNET: &str = "claude-sonnet-4-6";
/// Claude Opus model identifier — highest capability, used for complex or retried tasks.
pub const MODEL_OPUS: &str = "claude-opus-4-7";

/// Sentinel substring used by the run loop to detect a non-retryable billing
/// failure from the Anthropic API. Matched against the error chain so we don't
/// burn the retry budget looping on a credit-exhausted account.
pub const ERR_CREDIT_EXHAUSTED: &str = "anthropic credits exhausted";

/// Sentinel substring for a streamed session lopi itself killed for crossing
/// its resolved `--max-budget-usd` cap (see `runner::stream`'s hard-stop
/// check). Matched against the error chain the same way as
/// [`ERR_CREDIT_EXHAUSTED`] — retrying would spend a fresh session against
/// the exact same cap and, absent a wider budget, blow it again, so the run
/// loop treats this as terminal rather than burning another attempt.
pub const ERR_BUDGET_HARD_STOP: &str = "lopi budget hard-stop";

/// Route a task to the cheapest model capable of handling its complexity.
///
/// `task.model`, when set, is always honored verbatim — an explicit override
/// wins over both the complexity heuristic and the retry escalation, the
/// same "explicit wins over default" precedent already established by
/// `verifier_model`. Otherwise:
///
/// Heuristic: task size = constraints + `allowed_dirs` count.
/// - ≤ 2: Haiku (read-only discovery, simple rewrites) — ~20× cheaper than Opus
/// - 3–6: Sonnet (default — implementation, test writing)
/// - > 6 or retry ≥ 2: Opus (complex multi-file changes, repeated failures)
#[must_use]
pub fn select_model(task: &Task, attempt: u8) -> String {
    if let Some(m) = &task.model {
        return m.clone();
    }
    if attempt >= 2 {
        return MODEL_OPUS.to_string(); // escalate on repeated failure
    }
    let size = task.constraints.len() + task.allowed_dirs.len();
    match size {
        0..=2 => MODEL_HAIKU,
        3..=6 => MODEL_SONNET,
        _ => MODEL_OPUS,
    }
    .to_string()
}

/// Structured output from `claude --output-format json`.
#[derive(Debug, Deserialize)]
pub struct ClaudeOutput {
    /// JSON `type` field from the CLI response envelope.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// The assistant's text response, if present.
    pub result: Option<String>,
    /// `true` when the CLI reports an error outcome.
    pub is_error: Option<bool>,
    /// Estimated cost in USD as reported by the CLI.
    pub cost_usd: Option<f64>,
    /// Wall-clock duration of the CLI invocation in milliseconds.
    pub duration_ms: Option<u64>,
    /// Cumulative token usage, parsed separately from the envelope's
    /// `modelUsage`/`usage` object (same shape and precedence as the
    /// streaming `result` message's usage — see
    /// [`claude_events::parse_result_usage`](crate::claude_events::parse_result_usage)).
    /// `None` when the envelope carried neither field (e.g. an error result).
    #[serde(skip)]
    pub usage: Option<crate::claude_events::ResultUsage>,
    /// Raw stdout from the CLI process — fallback when JSON parsing fails.
    #[serde(skip)]
    pub raw: String,
}

impl ClaudeOutput {
    /// Return the response text, falling back to raw stdout when `result` is absent.
    #[must_use]
    pub fn text(&self) -> &str {
        self.result.as_deref().unwrap_or(&self.raw)
    }
    /// Return `true` when the CLI did not report an error.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        !self.is_error.unwrap_or(false)
    }
}

/// Parse a successful `claude` CLI invocation's stdout into a [`ClaudeOutput`].
/// When `json_output` is `false`, or the stdout isn't valid JSON (the CLI can
/// still emit plain text on some paths), falls back to a bare envelope
/// carrying the raw text as both `result` and `raw` rather than erroring —
/// the caller only cares about `.text()`/`.succeeded()`, both of which
/// degrade sensibly on the fallback.
#[must_use]
pub(crate) fn parse_claude_output(stdout: String, json_output: bool) -> ClaudeOutput {
    if json_output {
        match serde_json::from_str::<serde_json::Value>(&stdout) {
            Ok(v) => {
                let usage = crate::claude_events::parse_result_usage(&v);
                match serde_json::from_value::<ClaudeOutput>(v) {
                    Ok(mut o) => {
                        o.raw = stdout;
                        o.usage = usage;
                        o
                    }
                    Err(_) => ClaudeOutput {
                        kind: None,
                        result: Some(stdout.clone()),
                        is_error: None,
                        cost_usd: None,
                        duration_ms: None,
                        usage: None,
                        raw: stdout,
                    },
                }
            }
            Err(_) => ClaudeOutput {
                kind: None,
                result: Some(stdout.clone()),
                is_error: None,
                cost_usd: None,
                duration_ms: None,
                usage: None,
                raw: stdout,
            },
        }
    } else {
        ClaudeOutput {
            kind: None,
            result: Some(stdout.clone()),
            is_error: None,
            cost_usd: None,
            duration_ms: None,
            usage: None,
            raw: stdout,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn select_model_haiku_for_minimal_task() {
        // 0 constraints + 2 default allowed_dirs = size 2 → Haiku
        let t = Task::new("fix a typo");
        assert_eq!(select_model(&t, 0), MODEL_HAIKU);
    }

    #[test]
    fn select_model_sonnet_for_medium_task() {
        let mut t = Task::new("implement feature");
        t.constraints = vec!["no new deps".into(), "keep API stable".into()];
        // 2 constraints + 2 default dirs = size 4 → Sonnet
        assert_eq!(select_model(&t, 0), MODEL_SONNET);
    }

    #[test]
    fn select_model_opus_for_large_task() {
        let mut t = Task::new("big refactor");
        t.constraints = vec![
            "c1".into(),
            "c2".into(),
            "c3".into(),
            "c4".into(),
            "c5".into(),
        ];
        // 5 constraints + 2 dirs = size 7 → Opus
        assert_eq!(select_model(&t, 0), MODEL_OPUS);
    }

    #[test]
    fn select_model_escalates_to_opus_at_attempt_2() {
        let t = Task::new("simple task");
        assert_eq!(select_model(&t, 2), MODEL_OPUS);
    }

    #[test]
    fn select_model_escalates_to_opus_at_attempt_3() {
        let t = Task::new("simple task");
        assert_eq!(select_model(&t, 3), MODEL_OPUS);
    }

    #[test]
    fn select_model_honors_explicit_override_over_heuristic_and_escalation() {
        let mut t = Task::new("big refactor");
        t.constraints = vec![
            "c1".into(),
            "c2".into(),
            "c3".into(),
            "c4".into(),
            "c5".into(),
        ];
        t.model = Some(MODEL_HAIKU.to_string());
        // Would heuristically resolve to Opus (size 7) and escalate at attempt
        // 2 — the explicit override wins over both, mirroring verifier_model.
        assert_eq!(select_model(&t, 2), MODEL_HAIKU);
    }

    /// Regression test: the one-shot JSON envelope (`self.run()`, backing
    /// `fix()` and speculative mode's `implement_step()`) used to discard
    /// token usage entirely — `ClaudeOutput` had no `usage` field — so those
    /// paths' real spend never reached `tokens_used`/`turn_metrics`. It must
    /// now parse the same `modelUsage` breakdown the streaming `result`
    /// envelope uses.
    #[test]
    fn parse_claude_output_captures_usage_from_model_usage_map() {
        let stdout = serde_json::json!({
            "type": "result",
            "result": "done",
            "is_error": false,
            "cost_usd": 0.0123,
            "duration_ms": 4200,
            "modelUsage": {
                "claude-sonnet-4-6": {
                    "inputTokens": 1000,
                    "outputTokens": 250,
                    "cacheReadInputTokens": 10,
                    "cacheCreationInputTokens": 5,
                }
            }
        })
        .to_string();
        let out = parse_claude_output(stdout, true);
        let usage = out.usage.expect("usage must be parsed");
        assert_eq!(usage.input_tokens, 1000);
        assert_eq!(usage.output_tokens, 250);
        assert_eq!(usage.cache_read_tokens, 10);
        assert_eq!(usage.cache_write_tokens, 5);
        assert_eq!(out.cost_usd, Some(0.0123));
    }

    /// Same as above, but for the flat top-level `usage` object the envelope
    /// falls back to when `modelUsage` is absent.
    #[test]
    fn parse_claude_output_captures_usage_from_flat_usage_object() {
        let stdout = serde_json::json!({
            "type": "result",
            "result": "done",
            "is_error": false,
            "usage": {
                "input_tokens": 42,
                "output_tokens": 7,
            }
        })
        .to_string();
        let out = parse_claude_output(stdout, true);
        let usage = out.usage.expect("usage must be parsed");
        assert_eq!(usage.input_tokens, 42);
        assert_eq!(usage.output_tokens, 7);
    }

    #[test]
    fn parse_claude_output_usage_is_none_without_usage_fields() {
        let stdout = serde_json::json!({
            "type": "result",
            "result": "done",
            "is_error": true,
        })
        .to_string();
        let out = parse_claude_output(stdout, true);
        assert!(out.usage.is_none());
    }
}
