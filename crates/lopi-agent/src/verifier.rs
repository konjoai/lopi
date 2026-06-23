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

/// Directory, relative to the repo root, where canonical rubric files live.
const RUBRIC_DIR: &str = ".konjo/rubrics";
/// Rubric loaded from disk when a task carries no inline rubric.
const DEFAULT_RUBRIC_FILE: &str = "feature_completeness";

/// Resolve the rubric for a verifier pass.
///
/// Resolution chain (first match wins):
/// 1. `task_rubric` — an inline rubric attached to the task.
/// 2. `.konjo/rubrics/feature_completeness.toml` under the repo root.
/// 3. [`default_rubric`] — the hardcoded workspace fallback.
pub async fn resolve_rubric(task_rubric: Option<Rubric>, repo_path: &std::path::Path) -> Rubric {
    if let Some(rubric) = task_rubric {
        return rubric;
    }
    load_rubric_file(repo_path, DEFAULT_RUBRIC_FILE)
        .await
        .unwrap_or_else(default_rubric)
}

/// Load a named rubric from `.konjo/rubrics/<name>.toml` under `repo_path`.
///
/// Returns `None` when the file is absent or fails to parse — a missing or
/// malformed rubric file is non-fatal and falls back to the default.
pub async fn load_rubric_file(repo_path: &std::path::Path, name: &str) -> Option<Rubric> {
    let path = repo_path.join(RUBRIC_DIR).join(format!("{name}.toml"));
    let text = tokio::fs::read_to_string(&path).await.ok()?;
    match Rubric::from_toml_str(&text) {
        Ok(rubric) => Some(rubric),
        Err(e) => {
            tracing::warn!("rubric parse failed for {}: {e}", path.display());
            None
        }
    }
}

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
    /// Maker/checker isolation: when `true` (the default), the verifier never
    /// sees the maker's plan/chain-of-thought — it grades the artifact (diff)
    /// against the goal and rubric on its own merits, so the checker is not
    /// anchored to the maker's reasoning.
    isolated: bool,
}

impl VerifierAgent {
    /// Wrap a shared `AnthropicClient`. Defaults to **isolated** grading — the
    /// maker's plan is excluded from the verifier's context (true maker/checker).
    pub fn new(client: Arc<AnthropicClient>) -> Self {
        Self {
            client,
            isolated: true,
        }
    }

    /// Opt out of isolation: include the maker's plan as intent context (the
    /// legacy behavior). Weakens the maker/checker split — use only when the
    /// checker genuinely needs the maker's reasoning.
    #[must_use]
    pub fn with_plan_context(mut self) -> Self {
        self.isolated = false;
        self
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
        let prompt = build_prompt(goal, plan, diff, test_output, rubric, !self.isolated);
        let (text, _) = self
            .client
            .complete(MODEL_OPUS, VERIFIER_SYSTEM, &prompt, 1_024)
            .await
            .context("verifier API call")?;
        parse_verdict(&text)
    }
}

/// Build the verifier prompt from the gradeable evidence.
///
/// When `include_plan` is `false` (maker/checker isolation) the maker's plan is
/// omitted entirely — the checker grades the diff against the goal and rubric
/// without ever seeing the maker's reasoning. Excerpt bounds match the original
/// inline construction. Pure, so the isolation guarantee is unit-testable.
fn build_prompt(
    goal: &str,
    plan: &str,
    diff: &str,
    test_output: &str,
    rubric: &Rubric,
    include_plan: bool,
) -> String {
    let criteria = rubric.criteria.join("\n- ");
    let diff_excerpt = &diff[..diff.len().min(6_000)];
    let test_excerpt = &test_output[..test_output.len().min(1_000)];
    let plan_section = if include_plan {
        format!("PLAN (excerpt):\n{}\n\n", &plan[..plan.len().min(1_500)])
    } else {
        String::new()
    };
    format!(
        "GOAL:\n{goal}\n\n{plan_section}\
         DIFF (excerpt):\n{diff_excerpt}\n\n\
         TEST OUTPUT:\n{test_excerpt}\n\n\
         RUBRIC ({}):\n- {criteria}",
        rubric.name,
    )
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

    fn sample_rubric() -> Rubric {
        Rubric {
            name: "safety".into(),
            criteria: vec!["tests pass".into(), "no scope creep".into()],
        }
    }

    /// Maker/checker isolation (Pentad M4.1): the isolated prompt must NOT
    /// contain the maker's plan, so the checker cannot be anchored to it.
    #[test]
    fn isolated_prompt_excludes_the_maker_plan() {
        let plan = "MAKER SECRET REASONING: I hacked the test to pass";
        let prompt = build_prompt(
            "fix the bug",
            plan,
            "diff --git a b",
            "ok",
            &sample_rubric(),
            false, // isolated
        );
        assert!(!prompt.contains("MAKER SECRET REASONING"), "plan excluded");
        assert!(!prompt.contains("PLAN (excerpt)"), "no plan section header");
        // The artifact + intent + rubric are still present.
        assert!(prompt.contains("GOAL:\nfix the bug"));
        assert!(prompt.contains("DIFF (excerpt):\ndiff --git a b"));
        assert!(prompt.contains("RUBRIC (safety):"));
        assert!(prompt.contains("- no scope creep"));
    }

    #[test]
    fn plan_context_mode_includes_the_plan() {
        let prompt = build_prompt(
            "fix the bug",
            "MAKER REASONING here",
            "diff",
            "ok",
            &sample_rubric(),
            true, // include plan (legacy)
        );
        assert!(prompt.contains("PLAN (excerpt):\nMAKER REASONING here"));
    }

    #[test]
    fn new_verifier_is_isolated_by_default_and_builder_opts_out() {
        let client = std::sync::Arc::new(crate::api_client::AnthropicClient::new("test-key"));
        assert!(
            VerifierAgent::new(client.clone()).isolated,
            "isolated by default"
        );
        assert!(
            !VerifierAgent::new(client).with_plan_context().isolated,
            "builder opts back into plan context"
        );
    }

    #[test]
    fn default_rubric_has_criteria() {
        let r = default_rubric();
        assert!(!r.criteria.is_empty());
        assert_eq!(r.name, "default");
    }

    #[tokio::test]
    async fn resolve_rubric_prefers_inline_task_rubric() {
        let inline = Rubric {
            name: "inline".into(),
            criteria: vec!["only this".into()],
        };
        let resolved = resolve_rubric(Some(inline), std::path::Path::new("/nonexistent")).await;
        assert_eq!(resolved.name, "inline");
    }

    #[tokio::test]
    async fn resolve_rubric_loads_file_when_no_inline() {
        let dir = std::env::temp_dir().join(format!("lopi-rubric-{}", std::process::id()));
        let rubric_dir = dir.join(RUBRIC_DIR);
        tokio::fs::create_dir_all(&rubric_dir).await.unwrap();
        tokio::fs::write(
            rubric_dir.join("feature_completeness.toml"),
            "name = \"from_disk\"\ncriteria = [\"loaded from file\"]\n",
        )
        .await
        .unwrap();
        let resolved = resolve_rubric(None, &dir).await;
        assert_eq!(resolved.name, "from_disk");
        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn resolve_rubric_falls_back_to_default_when_file_absent() {
        let resolved = resolve_rubric(None, std::path::Path::new("/nonexistent")).await;
        assert_eq!(resolved.name, "default");
    }

    #[tokio::test]
    async fn load_rubric_file_returns_none_for_missing() {
        assert!(load_rubric_file(std::path::Path::new("/nonexistent"), "x")
            .await
            .is_none());
    }
}
