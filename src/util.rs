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
