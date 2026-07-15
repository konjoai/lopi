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
}
