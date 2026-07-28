//! Konjo Verifier — rubric-guided Opus second-score pass (Sprint S).
//!
//! After the heuristic scorer passes (`Score::passed()`), the verifier asks
//! Opus to grade the diff against a developer-supplied rubric. The structured
//! verdict drives constraint injection into the next retry's planning prompt.
use crate::api_client::AnthropicClient;
use crate::claude::{model_opus, model_sonnet};
use anyhow::{Context, Result};
use lopi_core::{safe_truncate, Rubric, VerifierVerdict};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;

const VERIFIER_SYSTEM: &str = "\
You are a strict code reviewer grading an agent's output against a rubric. \
Respond ONLY with a JSON object. No prose, no markdown fences. Schema: \
{\"passed\":bool,\"gaps\":[string],\"fix_hints\":[string],\"confidence\":float}. \
`gaps` lists unmet criteria. `fix_hints` are imperative instructions for the \
next implementation attempt. `confidence` is 0.0–1.0.";

/// System prompt for [`VerifierAgent::derive_checklist`] — Finding #1's fix
/// for "a reviewer shown the diff first rationalises it". This call never
/// receives a diff at all, so there is nothing to rationalize yet.
const CHECKLIST_SYSTEM: &str = "\
You are a strict code reviewer about to grade an agent's diff against a goal \
and a rubric. You have NOT been shown any code or diff yet — only the goal \
and the rubric. Before you see any implementation, write your own checklist \
of concrete, checkable criteria a correct and complete change must satisfy. \
Do not invent implementation details you cannot know yet; write criteria a \
diff could later be checked against. Respond ONLY with a JSON object. No \
prose, no markdown fences. Schema: {\"checklist\":[string]}.";

/// Resolve the effective verifier model + reasoning-effort hint for a grading
/// pass (Verifier as Explicit Gate).
///
/// "Never grade your own homework": when `verifier_model` is unset, the
/// resolved model is chosen to differ from `worker_model` — [`model_opus`]
/// by default, falling back to [`model_sonnet`] on the one case where the
/// worker itself is already Opus (an escalated retry), so the checker is
/// never the same model as the maker. An explicitly configured
/// `verifier_model` is always honored as-is, even if it happens to match
/// the worker — that is a deliberate operator override, not a default.
///
/// `verifier_effort` passes through unchanged; it carries no "must differ"
/// requirement.
#[must_use]
pub fn resolve_verifier(
    worker_model: &str,
    verifier_model: Option<&str>,
    verifier_effort: Option<&str>,
) -> (String, Option<String>) {
    let model = verifier_model.map(str::to_string).unwrap_or_else(|| {
        if worker_model == model_opus() {
            model_sonnet().to_string()
        } else {
            model_opus().to_string()
        }
    });
    (model, verifier_effort.map(str::to_string))
}

/// Build the verifier's system prompt, folding in an optional reasoning-effort
/// hint the same way worker-side launch controls fold "effort" into planning
/// constraints — a textual instruction, not a wire-level API parameter.
fn build_system_prompt(effort: Option<&str>) -> String {
    match effort {
        Some(e) => format!("{VERIFIER_SYSTEM}\n\nReasoning effort: {e}"),
        None => VERIFIER_SYSTEM.to_string(),
    }
}

/// Same effort-folding as [`build_system_prompt`], for
/// [`VerifierAgent::derive_checklist`]'s system prompt.
fn build_checklist_system_prompt(effort: Option<&str>) -> String {
    match effort {
        Some(e) => format!("{CHECKLIST_SYSTEM}\n\nReasoning effort: {e}"),
        None => CHECKLIST_SYSTEM.to_string(),
    }
}

/// Build the checklist-derivation prompt from goal + rubric alone.
///
/// Deliberately has no `diff`/`plan` parameter at all — not merely "chooses
/// not to use one" — so it is structurally impossible for this prompt to
/// leak any implementation detail into the checker's own checklist.
fn build_checklist_prompt(goal: &str, rubric: &Rubric) -> String {
    let criteria = rubric.criteria.join("\n- ");
    format!(
        "GOAL:\n{goal}\n\n\
         RUBRIC ({}):\n- {criteria}\n\n\
         Write your own checklist of concrete, checkable criteria a correct, \
         complete change would need to satisfy. You have not been shown any \
         code or diff — do not reference implementation details you cannot \
         know yet.",
        rubric.name,
    )
}

/// The `{"checklist": [...]}` payload both backends parse
/// [`VerifierAgent::derive_checklist`]'s response into.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct ChecklistPayload {
    pub(crate) checklist: Vec<String>,
}

/// Parse a checklist from free-text model output (fences stripped first) —
/// the CLI backend's fallback when `structured_output` is absent, mirroring
/// [`parse_verdict`]'s role for the grading call.
pub(crate) fn parse_checklist(text: &str) -> Result<Vec<String>> {
    let clean = strip_fences(text);
    let payload: ChecklistPayload = serde_json::from_str(clean)
        .with_context(|| format!("checklist JSON parse error — raw: {clean}"))?;
    Ok(payload.checklist)
}

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

/// Which transport a [`VerifierAgent`] grades over. Sprint F1 Phase 1 —
/// before this sprint only [`Api`](Backend::Api) existed, and nothing in the
/// built binary ever constructed one (`with_api` production-unwired), so the
/// verifier returned `true` unconditionally on every run. [`Cli`](Backend::Cli)
/// is the default (see `verifier_runner.rs`'s backend-selection point) because
/// it is the path that actually runs, on subscription auth, with no API key.
enum Backend {
    /// Direct-API grading — the pre-F1 path. Kept as the escalation tier
    /// (Phase 5) for when an operator has wired `with_api()`.
    Api(Arc<AnthropicClient>),
    /// `claude -p` subprocess grading (Sprint F1 Phase 1) —
    /// `crate::verifier_cli::grade_via_cli`. `repo_path` is the worktree cwd
    /// the checker reads the diff from, matching `get_repo_diff`.
    Cli { repo_path: PathBuf },
}

/// Grades an agent's diff against a rubric — the Konjo Verifier.
pub struct VerifierAgent {
    backend: Backend,
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
            backend: Backend::Api(client),
            isolated: true,
        }
    }

    /// Sprint F1 Phase 1 — grade via the `claude` CLI instead of a direct-API
    /// client. `repo_path` is the worktree the checker treats as its cwd, so
    /// it reads the same diff the worker just produced. Selected automatically
    /// (not a config flag) whenever no `AnthropicClient` is configured — see
    /// `verifier_runner.rs::run_verifier_pass`. Defaults to **isolated**
    /// grading, same as [`new`](Self::new).
    #[must_use]
    pub fn new_cli(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            backend: Backend::Cli {
                repo_path: repo_path.into(),
            },
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

    /// Derive the checker's own checklist from `goal` + `rubric` alone —
    /// Finding #1's fix for "a reviewer shown the diff first rationalises
    /// it". No diff, no plan, nothing the maker produced is in this call's
    /// context at all: an LLM sees its whole context before producing any
    /// output, so within-prompt ordering cannot prevent anchoring — only a
    /// genuinely separate call, with the diff structurally absent, can. Must
    /// be called (and must return) before [`verify`](Self::verify) grades.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the API/CLI call fails or the response cannot be
    /// parsed. [`verify`](Self::verify) treats a checklist-derivation
    /// failure as non-fatal (falls back to grading without a self-derived
    /// checklist, warns) rather than blocking the whole gate on it — the
    /// rubric alone still gates; this is a strict improvement on ordering,
    /// not a new single point of failure.
    pub async fn derive_checklist(
        &self,
        goal: &str,
        rubric: &Rubric,
        model: &str,
        effort: Option<&str>,
    ) -> Result<Vec<String>> {
        let prompt = build_checklist_prompt(goal, rubric);
        let system = build_checklist_system_prompt(effort);
        match &self.backend {
            Backend::Api(client) => {
                let (text, _) = client
                    .complete(model, &system, &prompt, 512)
                    .await
                    .context("checklist API call")?;
                parse_checklist(&text)
            }
            Backend::Cli { repo_path } => {
                crate::verifier_cli::derive_checklist_via_cli(
                    repo_path, &system, &prompt, model, effort,
                )
                .await
            }
        }
    }

    /// Grade `diff` against `rubric`.
    ///
    /// `plan` provides intent context; `test_output` gives the heuristic scorer
    /// evidence. Both are truncated to keep the prompt within a reasonable bound.
    /// `model` is the resolved verifier model (see [`resolve_verifier`]) —
    /// callers must not grade with the same model that produced the diff.
    /// `effort` is an optional reasoning-effort hint folded into the system
    /// prompt.
    ///
    /// Finding #1 — before grading, this always calls
    /// [`derive_checklist`](Self::derive_checklist) first (goal + rubric
    /// only, no diff) and folds the checker's own resulting checklist into
    /// the grading prompt. This doubles the call count (and roughly the
    /// cost) of a verifier pass; that is the deliberate trade this fixes —
    /// there is no flag to turn it off.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the API call fails or the response cannot be parsed.
    #[allow(clippy::too_many_arguments)]
    pub async fn verify(
        &self,
        goal: &str,
        plan: &str,
        diff: &str,
        test_output: &str,
        rubric: &Rubric,
        model: &str,
        effort: Option<&str>,
    ) -> Result<VerifierVerdict> {
        let checklist = match self.derive_checklist(goal, rubric, model, effort).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    "verifier checklist derivation failed ({e}); grading without a \
                     self-derived checklist"
                );
                Vec::new()
            }
        };
        let prompt = build_prompt(
            goal,
            plan,
            diff,
            test_output,
            rubric,
            !self.isolated,
            &checklist,
        );
        let system = build_system_prompt(effort);
        match &self.backend {
            Backend::Api(client) => {
                let (text, _) = client
                    .complete(model, &system, &prompt, 1_024)
                    .await
                    .context("verifier API call")?;
                parse_verdict(&text)
            }
            Backend::Cli { repo_path } => {
                crate::verifier_cli::grade_via_cli(repo_path, &system, &prompt, model, effort).await
            }
        }
    }
}

/// Build the verifier prompt from the gradeable evidence.
///
/// When `include_plan` is `false` (maker/checker isolation) the maker's plan is
/// omitted entirely — the checker grades the diff against the goal and rubric
/// without ever seeing the maker's reasoning. Excerpt bounds match the original
/// inline construction. Pure, so the isolation guarantee is unit-testable.
///
/// `checklist` is the checker's own, pre-diff-derived checklist (Finding #1,
/// [`VerifierAgent::derive_checklist`]) — folded in as its own labelled
/// section, right after `GOAL`/`PLAN` and before the diff, so the model
/// grades against criteria it already committed to instead of retrofitting
/// a rationale to whatever the diff happens to contain. An empty slice
/// (checklist derivation failed or was skipped) omits the section entirely.
#[allow(clippy::too_many_arguments)]
fn build_prompt(
    goal: &str,
    plan: &str,
    diff: &str,
    test_output: &str,
    rubric: &Rubric,
    include_plan: bool,
    checklist: &[String],
) -> String {
    let criteria = rubric.criteria.join("\n- ");
    let diff_excerpt = safe_truncate(diff, 6_000);
    let test_excerpt = safe_truncate(test_output, 1_000);
    let plan_section = if include_plan {
        format!("PLAN (excerpt):\n{}\n\n", safe_truncate(plan, 1_500))
    } else {
        String::new()
    };
    let checklist_section = if checklist.is_empty() {
        String::new()
    } else {
        format!(
            "YOUR OWN CHECKLIST (written before you saw any code):\n- {}\n\n",
            checklist.join("\n- ")
        )
    };
    format!(
        "GOAL:\n{goal}\n\n{plan_section}{checklist_section}\
         DIFF (excerpt):\n{diff_excerpt}\n\n\
         TEST OUTPUT:\n{test_excerpt}\n\n\
         RUBRIC ({}):\n- {criteria}",
        rubric.name,
    )
}

/// Parse a verdict from free-text model output (fences stripped first).
/// Shared by both backends: the API path always uses it; the CLI path
/// (`verifier_cli.rs`) falls back to it only when `structured_output` is
/// absent — see `.konjo/killtests/F1/KT-1.1.md` (0/30 malformed in that
/// fallback role, measured against a real subscription).
pub(crate) fn parse_verdict(text: &str) -> Result<VerifierVerdict> {
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
#[path = "verifier_tests.rs"]
mod tests;
