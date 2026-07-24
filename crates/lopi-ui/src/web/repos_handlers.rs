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
pub async fn repos_json(state: &AppState) -> Value {
    let base = state.repo_path.clone();
    let extras = state.extra_repos.clone();
    let repos = tokio::task::spawn_blocking(move || describe_repos(scan_repos(&base, &extras)))
        .await
        .unwrap_or_default();
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
pub async fn branches_json(state: &AppState, repo: &str) -> Value {
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
pub(super) async fn list_claude_commands(
    State(s): State<AppState>,
    Query(q): Query<ClaudeCommandsQuery>,
) -> impl IntoResponse {
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
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// `set_current_dir` is process-global, and `cargo test` runs tests as
    /// threads within one process — the cwd-dependent cases must not overlap.
    static CWD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Create `root/name` with a `.git` inside, and return its resolved path.
    fn git_repo(root: &Path, name: &str) -> String {
        let p = root.join(name);
        std::fs::create_dir_all(p.join(".git")).unwrap();
        p.canonicalize().unwrap().display().to_string()
    }

    /// The regression: `sail --repo` defaults to a relative `"."`, whose
    /// `parent()` is the empty path. Sibling discovery used to `read_dir("")`,
    /// fail silently, and offer only the primary repo.
    #[test]
    fn relative_primary_discovers_siblings() {
        let guard = CWD.lock().unwrap_or_else(|e| e.into_inner());
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let a = git_repo(root, "repo-a");
        let b = git_repo(root, "repo-b");
        std::fs::create_dir_all(root.join("not-a-repo")).unwrap();

        let restore = std::env::current_dir().unwrap();
        std::env::set_current_dir(root.join("repo-a")).unwrap();
        let got = scan_repos(Path::new("."), &[]);
        std::env::set_current_dir(restore).unwrap();
        drop(guard);

        assert_eq!(got, vec![a, b], "siblings discovered, non-repo excluded");
    }

    #[test]
    fn extras_are_included_and_deduped_against_siblings() {
        let tmp = tempfile::tempdir().unwrap();
        let a = git_repo(tmp.path(), "repo-a");
        let b = git_repo(tmp.path(), "repo-b");
        // A dispatch target living nowhere near the primary.
        let far = tempfile::tempdir().unwrap();
        let f = git_repo(far.path(), "far-repo");

        let extras = vec![
            PathBuf::from(&f),
            PathBuf::from(&b),         // already found as a sibling
            far.path().join("no-git"), // not a repo — dropped
        ];
        let got = scan_repos(&PathBuf::from(&a), &extras);

        assert!(
            got.contains(&f),
            "extra outside the primary's tree is listed"
        );
        assert_eq!(
            got.iter().filter(|r| **r == b).count(),
            1,
            "an extra that is also a sibling appears once"
        );
        assert!(
            !got.iter().any(|r| r.ends_with("no-git")),
            "a non-repo extra is dropped"
        );
    }

    /// A primary that cannot be resolved must degrade to an empty list, not
    /// panic — `absolutize` falls back to the path as-is.
    #[test]
    fn unresolvable_primary_yields_no_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");
        assert!(scan_repos(&missing, &[]).is_empty());
    }

    fn git(repo: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    /// A repo on `head`, carrying every branch in `branches`.
    fn repo_with_branches(root: &Path, head: &str, branches: &[&str]) -> String {
        std::fs::create_dir_all(root).unwrap();
        git(root, &["init", "-q", "-b", "base"]);
        git(root, &["config", "user.email", "t@t.t"]);
        git(root, &["config", "user.name", "t"]);
        std::fs::write(root.join("f"), "x").unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-qm", "init"]);
        for b in branches {
            git(root, &["branch", b]);
        }
        git(root, &["checkout", "-q", head]);
        root.display().to_string()
    }

    #[test]
    fn generated_branches_are_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = repo_with_branches(
            &tmp.path().join("r"),
            "main",
            &[
                "main",
                "feat/x",
                "lopi/fe125cc0-63b6-43e4-a273-52f1dc84d1e4-attempt-1",
                "claude/forge-polish-m3",
            ],
        );
        let (branches, default) = git_branches(&repo);

        assert_eq!(
            branches,
            vec!["base", "feat/x", "main"],
            "lopi/* and claude/* are dropped"
        );
        assert_eq!(
            default, "main",
            "HEAD is reported when it survives the filter"
        );
    }

    /// Regression: `.take(100)` used to cap the branch list with no signal
    /// that anything was hidden. Assert the cap still applies (unchanged
    /// behavior) now that it's a logged `truncate` — a real repo can easily
    /// carry more than 100 local branches, unlike the 500-repo cap.
    #[test]
    fn truncates_past_max_branches() {
        let tmp = tempfile::tempdir().unwrap();
        let extra: Vec<String> = (0..(MAX_BRANCHES + 5)).map(|i| format!("b{i}")).collect();
        let extra_refs: Vec<&str> = extra.iter().map(String::as_str).collect();
        let repo = repo_with_branches(&tmp.path().join("r"), "base", &extra_refs);
        let (branches, _default) = git_branches(&repo);
        assert_eq!(
            branches.len(),
            MAX_BRANCHES,
            "must cap at MAX_BRANCHES, not return every branch"
        );
    }

    /// A run can leave the repo checked out on a generated branch. The reported
    /// default must still be a branch the dropdown actually offers.
    #[test]
    fn default_falls_back_when_head_is_generated() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = repo_with_branches(
            &tmp.path().join("r"),
            "lopi/abc-attempt-2",
            &["main", "lopi/abc-attempt-2"],
        );
        let (branches, default) = git_branches(&repo);

        assert!(!branches.iter().any(|b| b.starts_with("lopi/")));
        assert_eq!(
            default, "main",
            "a filtered HEAD falls back to main, not itself"
        );
        assert!(
            branches.contains(&default),
            "the default is always selectable"
        );
    }

    #[test]
    fn branch_names_merely_containing_the_prefix_are_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = repo_with_branches(
            &tmp.path().join("r"),
            "main",
            &["main", "lopi-ui-refactor", "feat/claude-integration"],
        );
        let (branches, _) = git_branches(&repo);

        assert!(
            branches.contains(&"lopi-ui-refactor".to_string()),
            "only the `lopi/` path prefix is generated, not `lopi-*`"
        );
        assert!(
            branches.contains(&"feat/claude-integration".to_string()),
            "`claude` mid-name is not a `claude/` prefix"
        );
    }
}
