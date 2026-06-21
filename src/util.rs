//! Shared helpers used across the lopi binary — extracted from main.rs.
use lopi_core::{LopiConfig, TaskStatus};
use std::path::{Path, PathBuf};

/// Canonical path to the lopi SQLite database.
pub(crate) fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".lopi").join("lopi.db")
}

/// Map a raw status string to its emoji-prefixed display form.
pub(crate) fn fmt_status(s: &str) -> &str {
    match s {
        "queued" => "⏳ queued",
        "planning" => "📋 planning",
        "implementing" => "🔨 implementing",
        "testing" => "🧪 testing",
        "scoring" => "📊 scoring",
        "success" => "✅ success",
        "failed" => "❌ failed",
        "rolled_back" => "⏪ rolled back",
        _ => s,
    }
}

/// Convert a `TaskStatus` to a human-readable log string.
pub(crate) fn status_label(s: &TaskStatus) -> String {
    match s {
        TaskStatus::Queued => "queued".into(),
        TaskStatus::Planning => "planning".into(),
        TaskStatus::AwaitingPlanApproval { attempt } => {
            format!("awaiting plan approval (attempt {attempt})")
        }
        TaskStatus::Implementing => "implementing".into(),
        TaskStatus::Testing => "testing".into(),
        TaskStatus::Scoring => "scoring".into(),
        TaskStatus::Retrying { attempt } => format!("retrying (attempt {attempt})"),
        TaskStatus::Success { branch, pr_url } => format!(
            "success ✅ branch={branch}{}",
            pr_url
                .as_deref()
                .map(|u| format!(", pr={u}"))
                .unwrap_or_default()
        ),
        TaskStatus::Failed { reason } => format!("failed ❌ {reason}"),
        TaskStatus::RolledBack => "rolled back".into(),
    }
}

/// Load lopi config from `path` if given, otherwise search standard locations.
pub(crate) fn load_config(path: Option<&PathBuf>) -> Option<LopiConfig> {
    if let Some(p) = path {
        LopiConfig::load(p).ok()
    } else {
        LopiConfig::find_and_load()
    }
}

/// Return `true` if `repo` appears to be the lopi binary's own workspace.
pub(crate) fn is_self_modify_attempt(repo: &Path) -> bool {
    if let Ok(exe) = std::env::current_exe() {
        if let (Some(parent), Ok(repo_canonical)) =
            (exe.parent().and_then(|p| p.parent()), repo.canonicalize())
        {
            if let Ok(exe_canonical) = parent.canonicalize() {
                return repo_canonical.starts_with(&exe_canonical);
            }
        }
    }
    false
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn status_label_maps_simple_states() {
        assert_eq!(status_label(&TaskStatus::Queued), "queued");
        assert_eq!(status_label(&TaskStatus::Planning), "planning");
        assert_eq!(status_label(&TaskStatus::Implementing), "implementing");
        assert_eq!(status_label(&TaskStatus::Testing), "testing");
        assert_eq!(status_label(&TaskStatus::Scoring), "scoring");
        assert_eq!(status_label(&TaskStatus::RolledBack), "rolled back");
    }

    #[test]
    fn status_label_includes_attempt_and_reason_detail() {
        assert_eq!(
            status_label(&TaskStatus::AwaitingPlanApproval { attempt: 2 }),
            "awaiting plan approval (attempt 2)"
        );
        assert_eq!(
            status_label(&TaskStatus::Retrying { attempt: 3 }),
            "retrying (attempt 3)"
        );
        assert_eq!(
            status_label(&TaskStatus::Failed {
                reason: "boom".into()
            }),
            "failed ❌ boom"
        );
    }

    #[test]
    fn status_label_success_carries_branch_and_pr() {
        let with_pr = status_label(&TaskStatus::Success {
            branch: "lopi/x".into(),
            pr_url: Some("https://example/pr/1".into()),
        });
        assert_eq!(with_pr, "success ✅ branch=lopi/x, pr=https://example/pr/1");
        let no_pr = status_label(&TaskStatus::Success {
            branch: "lopi/y".into(),
            pr_url: None,
        });
        assert_eq!(no_pr, "success ✅ branch=lopi/y");
    }

    #[test]
    fn fmt_status_decorates_known_states_and_passes_through() {
        assert_eq!(fmt_status("failed"), "❌ failed");
        assert_eq!(fmt_status("rolled_back"), "⏪ rolled back");
        assert_eq!(fmt_status("anything-else"), "anything-else");
    }
}
