//! Sprint P3a (review-pipeline plan, Phase 3 prerequisite) — Path A: the
//! readonly Planner call *is* this attempt's plan phase, not a second
//! mechanism run alongside the classic one.
//!
//! KT-3A (`LEDGER.md`'s Review-Pipeline-Phase-3a entry) confirmed live that
//! a single CLI session can carry a readonly plan phase followed by a
//! mutating implement phase: `--permission-mode` alone is not re-applied on
//! `--resume`, but an explicitly-passed `--allowedTools`/`--disallowedTools`
//! pair is honored fresh on every call, including a resume, and can widen
//! what a resumed session may do. `spawn_planner`'s own command construction
//! forces `ToolProfile::Readonly`'s caps for exactly one call; `run_loop.rs`'s
//! `claude: ClaudeCode` builder carries the task's own normal (mutating)
//! caps throughout and is never touched by that forcing, so resuming
//! `claude` for implement naturally uses the task's real permissions, not
//! the Planner's.

use super::AgentRunner;
use crate::claude_support::SessionMode;
use anyhow::Result;
use lopi_core::PlanArtifact;
use std::path::Path;

impl AgentRunner {
    /// Run the readonly Planner under `attempt_session_id` (`--session-id`,
    /// so implement can `--resume` it), persist the resulting
    /// `PlanArtifact` before returning, and render it as the plain-text
    /// `plan` string the rest of the loop already expects (dry-run
    /// printing, `plan_gate`, the verifier's intent context,
    /// `stream_implement`'s prompt) — so none of that machinery needs to
    /// know a structured artifact exists at all.
    ///
    /// # Errors
    /// Returns `Err` unchanged from `planner_executor::spawn_planner` on any
    /// failure. The caller treats this exactly like a classic plan
    /// failure — `abort_attempt`, then retry or terminal-fail. Nothing is
    /// persisted and `self.last_plan_artifact` is left untouched (still
    /// `None`, or still the prior attempt's value) when this returns
    /// `Err` — section 2's "absent, never synthesized" rule: there is no
    /// code path here that constructs a placeholder `PlanArtifact`.
    pub(super) async fn plan_via_readonly_planner(
        &mut self,
        model: &str,
        attempt_session_id: &str,
    ) -> Result<String> {
        let planner_commit = head_oid_or_empty(&self.repo_path);
        let plan = crate::planner_executor::spawn_planner(
            &self.repo_path,
            &self.task.goal,
            model,
            &planner_commit,
            SessionMode::New(attempt_session_id),
        )
        .await?;
        // PF-3: persisted before the caller proceeds to implement, so a
        // crashed Executor still leaves the plan on record.
        self.persist_plan_artifact(&plan);
        let text = render_plan_artifact(&plan);
        self.last_plan_artifact = Some(plan);
        Ok(text)
    }
}

/// The repo's current commit, for `PlanArtifact.planner_commit` — best
/// effort: an unreadable HEAD (a repo mid-rebase, a fresh `git init` with no
/// commits yet) degrades to an empty string rather than failing the whole
/// Planner call over a provenance field nothing downstream gates on.
fn head_oid_or_empty(repo_path: &Path) -> String {
    lopi_git::GitManager::new(repo_path)
        .and_then(|g| g.head_oid())
        .unwrap_or_default()
}

/// Render a `PlanArtifact` as the plain-text `plan` string the rest of the
/// loop already expects. Keeps every existing downstream consumer of
/// `self.last_plan: Option<String>` unchanged — they see a readable plan,
/// not a JSON blob, and don't need to know it came from a schema-valid
/// artifact instead of free-form prose.
fn render_plan_artifact(plan: &PlanArtifact) -> String {
    let mut out = format!("Goal: {}\n\nScope:\n", plan.goal());
    for s in plan.scope() {
        out.push_str(&format!("- {s}\n"));
    }
    out.push_str("\nInvariants:\n");
    for i in plan.invariants() {
        out.push_str(&format!("- {i}\n"));
    }
    out.push_str(&format!("\nTest strategy: {}\n", plan.test_strategy()));
    if !plan.non_goals().is_empty() {
        out.push_str("\nNon-goals:\n");
        for n in plan.non_goals() {
            out.push_str(&format!("- {n}\n"));
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::print_stdout)]
mod tests {
    use super::*;

    fn sample_plan() -> PlanArtifact {
        PlanArtifact::new(
            "Add a subtract function",
            vec!["math.py".to_string()],
            vec!["Existing add() must keep working".to_string()],
            "Run pytest",
            vec!["No new dependencies".to_string()],
            Some("low".to_string()),
            "claude-sonnet-5",
            "abc1234",
        )
        .unwrap()
    }

    #[test]
    fn render_plan_artifact_includes_every_field_a_reader_needs() {
        let plan = sample_plan();
        let text = render_plan_artifact(&plan);
        assert!(text.contains("Add a subtract function"));
        assert!(text.contains("math.py"));
        assert!(text.contains("Existing add() must keep working"));
        assert!(text.contains("Run pytest"));
        assert!(text.contains("No new dependencies"));
    }

    #[test]
    fn render_plan_artifact_omits_non_goals_section_when_empty() {
        let plan = PlanArtifact::new(
            "goal",
            vec!["f.rs".to_string()],
            vec!["inv".to_string()],
            "test",
            vec![],
            None,
            "claude-sonnet-5",
            "abc1234",
        )
        .unwrap();
        let text = render_plan_artifact(&plan);
        assert!(!text.contains("Non-goals"));
    }

    #[test]
    fn head_oid_or_empty_degrades_on_a_non_repo_path() {
        let dir = tempfile::TempDir::new().unwrap();
        assert_eq!(head_oid_or_empty(dir.path()), "");
    }

    /// Section 2 — a Planner call that can't even start (a `repo_path` the
    /// CLI can't `chdir` into, so `spawn_planner`'s process spawn itself
    /// fails before any output exists to parse) must degrade to "no
    /// artifact this attempt," not a placeholder. This is the same failure
    /// shape a genuine CLI crash/timeout/schema-violation takes — all of
    /// them return `Err` from `spawn_planner` before `plan_via_readonly_planner`
    /// ever reaches its `persist_plan_artifact`/`last_plan_artifact = Some(..)`
    /// lines, so there is no code path here that could synthesize or
    /// backfill one.
    #[tokio::test]
    async fn plan_via_readonly_planner_leaves_last_plan_artifact_none_on_spawn_failure() {
        use lopi_core::Task;
        use std::path::PathBuf;

        let task = Task::new("goal that will never get a plan");
        let (mut runner, _bus) =
            super::super::AgentRunner::standalone(task, PathBuf::from("/nonexistent/not-a-repo"));
        assert!(runner.last_plan_artifact.is_none());

        let result = runner
            .plan_via_readonly_planner("claude-sonnet-5", "session-does-not-matter")
            .await;

        assert!(
            result.is_err(),
            "an unspawnable Planner call must surface as Err, not a fabricated plan"
        );
        assert!(
            runner.last_plan_artifact.is_none(),
            "a failed Planner call must never leave a synthesized/backfilled PlanArtifact behind"
        );
    }

    /// Sprint P3a §3 — three real, distinct Planner runs against this repo,
    /// each producing a genuine schema-valid `PlanArtifact` (not a
    /// hand-built record, unlike P1's single sample). `#[ignore]`d because
    /// it spawns the real `claude` CLI three times (cost + wall-clock,
    /// network-dependent); run explicitly with
    /// `cargo test -p lopi-agent --lib -- --ignored --nocapture
    /// three_real_planner_runs_each_produce_a_telemetry_ready_artifact`.
    /// Prints each artifact as one JSON line prefixed `P3A-TELEMETRY-RUN:`
    /// so a caller can grep stdout and feed the three records into kiban's
    /// `PrTelemetryRecord.apply_plan_artifact` for §3's live-run telemetry
    /// evidence.
    #[tokio::test]
    #[ignore = "spawns the real claude CLI three times; run explicitly for §3 evidence"]
    async fn three_real_planner_runs_each_produce_a_telemetry_ready_artifact() {
        use lopi_core::Task;
        use std::path::PathBuf;

        let repo_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();

        let goals = [
            "Identify which file defines the PlanArtifact struct and list its public \
             accessor methods. Do not modify anything.",
            "Describe how spawn_planner enforces ToolProfile::Readonly, citing the exact \
             file and function. Do not modify anything.",
            "Explain what AgentRunner::run's standard (non-speculative) plan phase does \
             and which file it lives in. Do not modify anything.",
        ];

        for (i, goal) in goals.iter().enumerate() {
            let task = Task::new(*goal);
            let (mut runner, _bus) = super::super::AgentRunner::standalone(task, repo_path.clone());
            let session_id = uuid::Uuid::new_v4().to_string();
            let plan = runner
                .plan_via_readonly_planner("claude-sonnet-5", &session_id)
                .await
                .unwrap_or_else(|e| panic!("real planner run {i} failed: {e:#}"));
            assert!(!plan.is_empty(), "rendered plan text must not be empty");
            let artifact = runner
                .last_plan_artifact
                .as_ref()
                .unwrap_or_else(|| panic!("run {i} succeeded but left last_plan_artifact None"));
            let json = serde_json::to_string(artifact).unwrap();
            println!("P3A-TELEMETRY-RUN: {json}");
        }
    }
}
