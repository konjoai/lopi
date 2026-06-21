//! Repo + branch discovery for the launch-control dropdowns.
//!
//! The dashboards are sandboxed (or remote), so they can't scan the operator's
//! filesystem — the server enumerates git repos and branches and exposes them
//! here. Both endpoints do their filesystem / subprocess work on a blocking
//! pool so the async runtime is never stalled.

use super::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};

/// `GET /api/repos` — the server's primary repo plus immediate sibling git
/// repos, for the launch-control repo dropdown.
pub(super) async fn list_repos(State(s): State<AppState>) -> impl IntoResponse {
    let base = s.repo_path.clone();
    let repos = tokio::task::spawn_blocking(move || scan_repos(&base))
        .await
        .unwrap_or_default();
    (StatusCode::OK, Json(json!({ "repos": repos }))).into_response()
}

/// Query for [`list_branches`].
#[derive(Deserialize)]
pub(super) struct BranchQuery {
    /// Repo path; empty falls back to the server's primary repo.
    #[serde(default)]
    repo: String,
}

/// `GET /api/branches?repo=<path>` — local branch names of `repo`, plus the
/// repo's default (current HEAD) branch so the UI can preselect it.
pub(super) async fn list_branches(
    State(s): State<AppState>,
    Query(q): Query<BranchQuery>,
) -> impl IntoResponse {
    let repo = if q.repo.trim().is_empty() {
        s.repo_path.display().to_string()
    } else {
        q.repo
    };
    let (branches, default) = tokio::task::spawn_blocking(move || git_branches(&repo))
        .await
        .unwrap_or_default();
    (
        StatusCode::OK,
        Json(json!({ "branches": branches, "default": default })),
    )
        .into_response()
}

/// Collect git repos: the primary repo, then sibling directories that contain a
/// `.git`. Bounded and sorted for a stable dropdown.
fn scan_repos(primary: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if primary.join(".git").exists() {
        out.push(primary.display().to_string());
    }
    if let Some(parent) = primary.parent() {
        if let Ok(entries) = std::fs::read_dir(parent) {
            for entry in entries.flatten().take(500) {
                let p: PathBuf = entry.path();
                if p.is_dir() && p.join(".git").exists() {
                    let s = p.display().to_string();
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
        }
    }
    out.sort();
    out.truncate(50);
    out
}

/// List local branch short-names via the git CLI (already a hard dependency of
/// the agent runtime), plus the default (current HEAD) branch — falling back to
/// main/master, then the first branch. Returns empty on any error.
fn git_branches(repo: &str) -> (Vec<String>, String) {
    let branches: Vec<String> = match std::process::Command::new("git")
        .args(["-C", repo, "branch", "--format=%(refname:short)"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .take(100)
            .collect(),
        _ => Vec::new(),
    };

    let default = std::process::Command::new("git")
        .args(["-C", repo, "branch", "--show-current"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            branches
                .iter()
                .find(|b| *b == "main" || *b == "master")
                .cloned()
        })
        .or_else(|| branches.first().cloned())
        .unwrap_or_default();

    (branches, default)
}
