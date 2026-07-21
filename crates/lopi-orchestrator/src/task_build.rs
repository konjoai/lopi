//! Shared `Task`-from-spec construction, used by both [`crate::schedule_manager`]
//! (cron-fired tasks) and [`crate::maxx_loop`] (favorability-fired tasks).
//! Both specs carry the same subset of fields — goal, repo, priority,
//! directory overrides, autonomy level — so the mapping lives here once
//! rather than being copy-pasted per caller.

use std::path::Path;

use lopi_core::{AutonomyLevel, Priority, RepoProfile, Task, TaskSource};

/// Build a [`Task`] from the fields common to a schedule/MAXX spec, applying
/// priority, directory overrides, and any per-repo `.lopi.toml` profile.
#[must_use]
pub(crate) fn build_task_from_fields(
    goal: &str,
    repo: Option<&Path>,
    priority: &str,
    allowed_dirs: &[String],
    forbidden_dirs: &[String],
    autonomy_level: AutonomyLevel,
) -> Task {
    let mut task = Task::new(goal.to_string());
    task.source = TaskSource::Api;
    task.priority = match priority {
        "low" => Priority::Low,
        "high" => Priority::High,
        "critical" => Priority::Critical,
        _ => Priority::Normal,
    };
    if !allowed_dirs.is_empty() {
        task.allowed_dirs = allowed_dirs.to_vec();
    }
    if !forbidden_dirs.is_empty() {
        task.forbidden_dirs = forbidden_dirs.to_vec();
    }
    if let Some(repo) = repo {
        task.repo_path = Some(repo.to_path_buf());
        RepoProfile::load_from_repo(repo).apply(&mut task);
    }
    task.autonomy_level = autonomy_level;
    task
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn maps_priority_and_dirs() {
        let task = build_task_from_fields(
            "run tests",
            None,
            "high",
            &["src/".to_string()],
            &[],
            AutonomyLevel::default(),
        );
        assert_eq!(task.priority, Priority::High);
        assert_eq!(task.allowed_dirs, vec!["src/".to_string()]);
        assert_eq!(task.goal, "run tests");
    }

    #[test]
    fn unknown_priority_defaults_to_normal() {
        let task = build_task_from_fields("g", None, "weird", &[], &[], AutonomyLevel::default());
        assert_eq!(task.priority, Priority::Normal);
    }

    #[test]
    fn empty_dir_overrides_leave_task_defaults_untouched() {
        let bare = Task::new("g");
        let task = build_task_from_fields("g", None, "normal", &[], &[], AutonomyLevel::default());
        assert_eq!(
            task.allowed_dirs, bare.allowed_dirs,
            "empty override doesn't clobber Task::new's defaults"
        );
        assert_eq!(task.forbidden_dirs, bare.forbidden_dirs);
    }

    /// Sprint Successor-1, KT-A (pre-flight kill test) — containment is
    /// currently absent. A constrained, untrusted-origin "parent" task
    /// (`Webhook` source, `DraftPr` autonomy, non-empty `forbidden_dirs`) can
    /// have a "successor" built from it via `build_task_from_fields` with a
    /// *wider* autonomy level and *empty* `forbidden_dirs` — nothing in the
    /// current codebase links the two tasks or enforces that the child can
    /// only narrow, never widen, what the parent was allowed to do.
    ///
    /// This test MUST PASS today (it demonstrates the gap: no containment
    /// exists yet). Phase 2 introduces `derive_successor_task`, the actual
    /// containment gate for the parent→successor edge; the *equivalent*
    /// escalation attempt run through that new function is asserted to be
    /// blocked by `successor::gates_block_the_kt_a_escalation` in
    /// `crates/lopi-core/src/successor.rs` — that's this test's inversion.
    /// `build_task_from_fields` itself is untouched by Phase 2 (it builds
    /// cron/MAXX-spec tasks with no parent at all, a different call path),
    /// so it continues to permit this — by design, not oversight.
    #[test]
    fn kt_a_containment_is_currently_absent() {
        let parent = Task {
            source: TaskSource::Webhook {
                repo: "org/repo".into(),
                event: "check_run".into(),
            },
            autonomy_level: AutonomyLevel::DraftPr,
            forbidden_dirs: vec!["infra/".to_string(), "secrets/".to_string()],
            ..Task::new("fix the failing check")
        };
        assert!(parent.forbidden_dirs.iter().any(|d| d == "secrets/"));

        // Nothing about `parent` is consulted here — no parent id, no
        // inherited constraint. A second task widens autonomy past the
        // parent's DraftPr to AutoMerge and declares its own forbidden dirs
        // with no relation whatsoever to the parent's — the builder never
        // even sees `parent`, so it has no way to union or narrow anything.
        let successor = build_task_from_fields(
            "escalate and merge everything",
            None,
            "critical",
            &[],
            &["docs/".to_string()], // disjoint from the parent's forbidden set
            AutonomyLevel::AutoMerge, // wider than the parent's DraftPr
        );

        assert_eq!(successor.autonomy_level, AutonomyLevel::AutoMerge);
        assert!(
            !successor.forbidden_dirs.iter().any(|d| d == "secrets/"),
            "gap: the successor has no memory of the parent's `secrets/` restriction"
        );
        assert!(
            successor.autonomy_level.rank() > parent.autonomy_level.rank(),
            "gap: the successor can freely widen past the parent's autonomy level"
        );
    }
}
