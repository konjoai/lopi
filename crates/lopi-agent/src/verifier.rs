//! Konjo Verifier — rubric-guided Opus second-score pass (Sprint S).
//!
//! After the heuristic scorer passes (`Score::passed()`), the verifier asks
//! Opus to grade the diff against a developer-supplied rubric. The structured
//! verdict drives constraint injection into the next retry's planning prompt.
use crate::api_client::AnthropicClient;
use crate::claude::MODEL_OPUS;
use anyhow::{Context, Result};
use lopi_core::{Rubric, VerifierVerdict};
use std::sync::Arc;
use tokio::process::Command;

const VERIFIER_SYSTEM: &str = "\
You are a strict code reviewer grading an agent's output against a rubric. \
Respond ONLY with a JSON object. No prose, no markdown fences. Schema: \
{\"passed\":bool,\"gaps\":[string],\"fix_hints\":[string],\"confidence\":float}. \
`gaps` lists unmet criteria. `fix_hints` are imperative instructions for the \
next implementation attempt. `confidence` is 0.0–1.0.";

/// Hardcoded workspace fallback used when no rubric is attached to the task.
pub fn default_rubric() -> Rubric {
    Rubric {
        name: "default".into(),
        criteria: vec![
            "All existing tests still pass".into(),
            "No new clippy warnings introduced".into(),
            "Changes are limited to files relevant to the stated goal".into(),
            "New or modified code follows the existing patterns in those files".into(),
            "No debugging artefacts (dbg!, println!, unresolved task markers) left in the diff"
                .into(),
        ],
    }
}

/// Calls Opus to grade an agent's diff against a rubric.
pub struct VerifierAgent {
    client: Arc<AnthropicClient>,
}

impl VerifierAgent {
    /// Wrap a shared `AnthropicClient`.
    pub fn new(client: Arc<AnthropicClient>) -> Self {
        Self { client }
    }

    /// Grade `diff` against `rubric`.
    ///
    /// `plan` provides intent context; `test_output` gives the heuristic scorer
    /// evidence. Both are truncated to keep the prompt within a reasonable bound.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the API call fails or the response cannot be parsed.
    pub async fn verify(
        &self,
        goal: &str,
        plan: &str,
        diff: &str,
        test_output: &str,
        rubric: &Rubric,
    ) -> Result<VerifierVerdict> {
        let criteria = rubric.criteria.join("\n- ");
        let plan_excerpt = &plan[..plan.len().min(1_500)];
        let diff_excerpt = &diff[..diff.len().min(6_000)];
        let test_excerpt = &test_output[..test_output.len().min(1_000)];
        let prompt = format!(
            "GOAL:\n{goal}\n\nPLAN (excerpt):\n{plan_excerpt}\n\n\
             DIFF (excerpt):\n{diff_excerpt}\n\n\
             TEST OUTPUT:\n{test_excerpt}\n\n\
             RUBRIC ({}):\n- {criteria}",
            rubric.name,
        );
        let (text, _) = self
            .client
            .complete(MODEL_OPUS, VERIFIER_SYSTEM, &prompt, 1_024)
            .await
            .context("verifier API call")?;
        parse_verdict(&text)
    }
}

fn parse_verdict(text: &str) -> Result<VerifierVerdict> {
    let clean = strip_fences(text);
    serde_json::from_str(clean).with_context(|| format!("verifier JSON parse error — raw: {clean}"))
}

fn strip_fences(s: &str) -> &str {
    let s = s.trim();
    // Strip ```json ... ``` or ``` ... ``` wrappers the model may add.
    let inner = s
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim();
    inner.trim_end_matches("```").trim()
}

/// Obtain the current uncommitted diff from the repository.
///
/// Used to give the verifier a concrete view of what the agent changed.
/// Returns an empty string if git is unavailable or no changes exist.
pub async fn get_repo_diff(repo_path: &std::path::Path) -> String {
    let out = Command::new("git")
        .arg("diff")
        .arg("HEAD")
        .current_dir(repo_path)
        .output()
        .await;
    match out {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn strip_fences_removes_markdown_wrapper() {
        assert_eq!(
            strip_fences("```json\n{\"passed\":true}\n```"),
            "{\"passed\":true}"
        );
    }

    #[test]
    fn strip_fences_passthrough_for_clean_json() {
        assert_eq!(strip_fences("{\"passed\":false}"), "{\"passed\":false}");
    }

    #[test]
    fn parse_verdict_valid_json() {
        let raw = r#"{"passed":true,"gaps":[],"fix_hints":[],"confidence":0.9}"#;
        let v = parse_verdict(raw).unwrap();
        assert!(v.passed);
        assert!(v.gaps.is_empty());
        assert!((v.confidence - 0.9).abs() < 1e-6);
    }

    #[test]
    fn parse_verdict_failed_with_hints() {
        let raw = r#"{"passed":false,"gaps":["tests do not cover new branch"],"fix_hints":["add test for the else branch"],"confidence":0.8}"#;
        let v = parse_verdict(raw).unwrap();
        assert!(!v.passed);
        assert_eq!(v.gaps.len(), 1);
        assert_eq!(v.fix_hints[0], "add test for the else branch");
    }

    #[test]
    fn parse_verdict_invalid_json_returns_err() {
        assert!(parse_verdict("not json").is_err());
    }

    #[test]
    fn default_rubric_has_criteria() {
        let r = default_rubric();
        assert!(!r.criteria.is_empty());
        assert_eq!(r.name, "default");
    }
}
