//! Repo + branch discovery for the launch-control dropdowns.
//!
//! The dashboards are sandboxed (or remote), so they can't scan the operator's
//! filesystem — the server enumerates git repos and branches and exposes them
//! here. Both endpoints do their filesystem / subprocess work on a blocking
//! pool so the async runtime is never stalled.

use super::repo_identity::describe_repos;
use super::AppState;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// The server's primary repo plus immediate sibling git repos, each decorated
/// with its GitHub `owner`/`name` — the launch-control repo dropdown's data,
/// shared by `GET /api/repos` and the `lopi_list_repos` MCP tool so both
/// surfaces enumerate the exact same set.
///
/// One blocking hop covers the scan and the per-repo config reads. Decorating
/// *after* the scan's sort/dedup means each surviving repo is read once,
/// rather than every candidate the walk considered.
///
/// A synthetic (`lopi demo`) store never reaches the scan at all — see
/// [`demo_repos_json`] — because demo mode must never touch the real
/// filesystem (`docs/adr/0001-demo-mode-and-measurement.md`, point 4).
pub async fn repos_json(state: &AppState) -> Value {
    if state.store.is_synthetic().await.unwrap_or(false) {
        return demo_repos_json(state).await;
    }
    let base = state.repo_path.clone();
    let extras = state.extra_repos.clone();
    let repos = tokio::task::spawn_blocking(move || describe_repos(scan_repos(&base, &extras)))
        .await
        .unwrap_or_default();
    json!({ "repos": repos })
}

/// The repo list for a synthetic store: the `demo_repos` table's rows,
/// shaped to match what `describe_repos` would emit for a real repo. Never
/// calls `describe_repos` itself — that reads real git config off disk,
/// which demo mode must not touch — so each row is built directly with a
/// fixed synthetic-appropriate `owner`.
async fn demo_repos_json(state: &AppState) -> Value {
    let repos: Vec<Value> = state
        .store
        .load_demo_repos()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|r| json!({ "owner": "demo", "name": r.name, "path": r.path }))
        .collect();
    json!({ "repos": repos })
}

/// `GET /api/repos` — thin axum wrapper over [`repos_json`].
pub(super) async fn list_repos(State(s): State<AppState>) -> impl IntoResponse {
    (StatusCode::OK, Json(repos_json(&s).await)).into_response()
}

/// Query for [`list_branches`].
#[derive(Deserialize)]
pub(super) struct BranchQuery {
    /// Repo path; empty falls back to the server's primary repo.
    #[serde(default)]
    repo: String,
}

/// Local branch names of `repo` (empty falls back to the server's primary
/// repo), plus its default (current HEAD) branch — shared by
/// `GET /api/branches` and the `lopi_list_branches` MCP tool.
///
/// A synthetic (`lopi demo`) store returns a fixed, obviously-synthetic
/// result without shelling out to `git` — demo mode must never touch the
/// real filesystem or spawn a real process.
pub async fn branches_json(state: &AppState, repo: &str) -> Value {
    if state.store.is_synthetic().await.unwrap_or(false) {
        return json!({ "branches": ["main"], "default": "main" });
    }
    let repo = if repo.trim().is_empty() {
        state.repo_path.display().to_string()
    } else {
        repo.to_string()
    };
    let (branches, default) = tokio::task::spawn_blocking(move || git_branches(&repo))
        .await
        .unwrap_or_default();
    json!({ "branches": branches, "default": default })
}

/// `GET /api/branches?repo=<path>` — thin axum wrapper over [`branches_json`].
pub(super) async fn list_branches(
    State(s): State<AppState>,
    Query(q): Query<BranchQuery>,
) -> impl IntoResponse {
    (StatusCode::OK, Json(branches_json(&s, &q.repo).await)).into_response()
}

/// Query for [`list_claude_commands`].
#[derive(Deserialize)]
pub(super) struct ClaudeCommandsQuery {
    /// Repo path; empty falls back to the server's primary repo.
    #[serde(default)]
    repo: String,
}

/// `GET /api/claude-commands?repo=<path>` — every real Claude Code `/name`
/// command available for `repo`: Claude Code's own built-ins, installed
/// plugins (user + project scope), user-level commands/skills
/// (`~/.claude/...`), and `repo`'s own — see
/// [`lopi_skill::discover_claude_commands`] for the full precedence order.
/// Feeds the composer's `/`-triggered autocomplete (Composer-Grammar-2).
/// Mirrors [`list_branches`]'s repo-scoped query shape exactly.
///
/// A synthetic (`lopi demo`) store returns an empty command list without
/// calling [`lopi_skill::discover_claude_commands`], which touches the real
/// filesystem and `$HOME`.
pub(super) async fn list_claude_commands(
    State(s): State<AppState>,
    Query(q): Query<ClaudeCommandsQuery>,
) -> impl IntoResponse {
    if s.store.is_synthetic().await.unwrap_or(false) {
        return (StatusCode::OK, Json(json!({ "commands": [] }))).into_response();
    }
    let repo = if q.repo.trim().is_empty() {
        s.repo_path.display().to_string()
    } else {
        q.repo
    };
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    let commands = tokio::task::spawn_blocking(move || {
        lopi_skill::discover_claude_commands(Path::new(&repo), home.as_deref())
    })
    .await
    .unwrap_or_default();
    (StatusCode::OK, Json(json!({ "commands": commands }))).into_response()
}

/// Upper bound on the repos returned to the dropdown. A backstop against a
/// pathological scan directory, not a curation policy — a developer keeping
/// every checkout in one folder is ordinary (this repo's own author has 164 in
/// `$HOME`), so the limit sits far above any plausible real count and a
/// truncation is logged rather than silently swallowed.
const MAX_REPOS: usize = 500;

/// Collect git repos: the primary repo, sibling directories that contain a
/// `.git`, and any operator-supplied extras (`sail --repos`). Sorted and
/// deduped for a stable dropdown.
///
/// Every path is absolutized first. `--repo` defaults to a *relative* `"."`,
/// whose `parent()` is the empty path — `read_dir("")` then fails, so sibling
/// discovery silently found nothing and the dropdown only ever offered the
/// primary repo itself.
fn scan_repos(primary: &Path, extras: &[PathBuf]) -> Vec<String> {
    let primary = absolutize(primary);
    let mut out: Vec<String> = Vec::new();
    if primary.join(".git").exists() {
        out.push(primary.display().to_string());
    }
    if let Some(parent) = primary.parent() {
        if let Ok(entries) = std::fs::read_dir(parent) {
            // Bounded by repos *found*, not directory entries walked: a scan
            // directory holding thousands of non-repo files must not exhaust
            // the budget before reaching the repos behind them.
            for entry in entries.flatten() {
                let p: PathBuf = entry.path();
                if p.is_dir() && p.join(".git").exists() {
                    out.push(p.display().to_string());
                    if out.len() > MAX_REPOS {
                        break;
                    }
                }
            }
        }
    }
    // Extras are dispatch targets the pool already serves, so they belong in the
    // dropdown even when they live nowhere near the primary repo.
    out.extend(
        extras
            .iter()
            .map(|e| absolutize(e))
            .filter(|e| e.join(".git").exists())
            .map(|e| e.display().to_string()),
    );
    out.sort();
    out.dedup();
    if out.len() > MAX_REPOS {
        tracing::warn!(
            found = out.len(),
            limit = MAX_REPOS,
            "more git repos than the dropdown lists; the remainder are hidden"
        );
        out.truncate(MAX_REPOS);
    }
    out
}

/// Resolve a path to its absolute, symlink-free form. Falls back to the input
/// on failure — a repo path that can't be resolved (deleted, permission-denied)
/// must not empty the entire list.
fn absolutize(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|e| {
        tracing::warn!(
            path = %p.display(),
            error = %e,
            "repo path could not be canonicalized; scanning it as-is"
        );
        p.to_path_buf()
    })
}

/// Branches lopi's own machinery created, rather than a human: the per-task
/// worktree branches (`lopi/<task-id>-attempt-N`) and agent-authored `claude/*`
/// branches.
///
/// They are excluded from the dropdown because they are run artifacts, never a
/// target an operator deliberately picks — and on any repo lopi has worked they
/// swamp the real ones (in lopi's own tree: 32 generated vs 14 human), burying
/// the branches you actually want behind 51-character UUIDs.
fn is_generated_branch(name: &str) -> bool {
    name.starts_with("lopi/") || name.starts_with("claude/")
}

/// Upper bound on the branches returned to the dropdown — see [`MAX_REPOS`]'s
/// doc for the same backstop-not-curation rationale. Unlike the repos cap,
/// this one is a real risk: a long-lived repo with many contributors (or one
/// that doesn't prune merged branches) can easily exceed it, so a truncation
/// is logged rather than silently swallowed.
const MAX_BRANCHES: usize = 100;

/// List human local branch short-names via the git CLI (already a hard
/// dependency of the agent runtime), plus the default (current HEAD) branch —
/// falling back to main/master, then the first branch. Empty on any error.
fn git_branches(repo: &str) -> (Vec<String>, String) {
    let mut branches: Vec<String> = match std::process::Command::new("git")
        .args(["-C", repo, "branch", "--format=%(refname:short)"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !is_generated_branch(l))
            .collect(),
        _ => Vec::new(),
    };
    if branches.len() > MAX_BRANCHES {
        tracing::warn!(
            repo,
            found = branches.len(),
            limit = MAX_BRANCHES,
            "more branches than the dropdown lists; the remainder are hidden"
        );
        branches.truncate(MAX_BRANCHES);
    }

    let default = current_branch(repo)
        // HEAD itself can be a generated branch (a run left the repo on one).
        // Reporting it would name a default that isn't in the list.
        .filter(|h| branches.contains(h))
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

/// The repo's checked-out branch, or `None` when detached / on error.
fn current_branch(repo: &str) -> Option<String> {
    std::process::Command::new("git")
        .args(["-C", repo, "branch", "--show-current"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
#[path = "repos_handlers_tests.rs"]
mod tests;
