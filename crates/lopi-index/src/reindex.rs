//! Incremental reindex: `git diff --name-status <indexed_commit> HEAD` when
//! there's a clean commit to diff against, blake3 hash-mismatch reparse for
//! a dirty working tree, full reindex only on first run or a
//! grammar/schema version bump.

use crate::hash::hash_bytes;
use crate::parse::parse_file;
use crate::store::IndexStore;
use crate::types::{IndexDelta, Language};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Bumped whenever a grammar upgrade or schema change means every existing
/// row must be considered stale — forces [`reindex`] to do a full pass
/// instead of trusting `git diff`/file hashes.
const GRAMMAR_VERSION: &str = "1";

/// Reindex `repo_id`'s tree rooted at `repo_root` into `store`, doing the
/// least work that's still correct: a full walk on first run or a
/// grammar-version bump, otherwise a `git diff` against the last indexed
/// commit for tracked changes plus a hash-mismatch sweep for anything the
/// working tree has dirtied since.
///
/// # Errors
/// Returns `Err` only for a store I/O failure — a single file's parse
/// failure is logged and skipped, never fatal (see `types::IndexDelta::parse_failures`).
pub async fn reindex(store: &IndexStore, repo_id: &str, repo_root: &Path) -> Result<IndexDelta> {
    let t0 = std::time::Instant::now();
    let stored_version = store.get_meta("grammar_version").await?;
    let stored_commit = store.get_meta("indexed_commit").await?;
    let current_commit = git_head(repo_root);

    let grammar_current = stored_version.as_deref() == Some(GRAMMAR_VERSION);

    let mut touched = TouchTracker::default();
    let mut delta = match (grammar_current, stored_commit) {
        (true, Some(stored_commit)) => {
            incremental_reindex(
                store,
                repo_id,
                repo_root,
                &stored_commit,
                current_commit.as_deref(),
                &mut touched,
            )
            .await?
        }
        _ => full_reindex(store, repo_id, repo_root, &mut touched).await?,
    };
    tracing::debug!(
        elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0,
        "reindex: file pass done"
    );

    // Scoped resolution (not the full-repo `resolve_refs`): only refs from
    // files this pass touched, or previously-unresolved refs whose target
    // just appeared, can possibly have changed resolvability. See
    // `store::refs::resolve_refs_for`'s doc comment for the full reasoning
    // and `LEDGER.md`'s Finding #4 entry for the measured win — on this repo,
    // the unscoped full scan was the dominant cost of a one-file reindex.
    let t1 = std::time::Instant::now();
    let resolved = store
        .resolve_refs_for(repo_id, &touched.paths, &touched.new_names)
        .await?;
    tracing::debug!(
        resolved,
        touched_paths = touched.paths.len(),
        new_names = touched.new_names.len(),
        elapsed_ms = t1.elapsed().as_secs_f64() * 1000.0,
        "reindex: resolved refs"
    );

    store.set_meta("grammar_version", GRAMMAR_VERSION).await?;
    if let Some(commit) = &current_commit {
        store.set_meta("indexed_commit", commit).await?;
    }
    delta.refs_added = delta.refs_added.max(resolved);
    Ok(delta)
}

/// Accumulates what a reindex pass touched, so the scoped ref-resolution
/// pass (`store::refs::resolve_refs_for`) knows what's worth re-checking
/// without re-scanning the repo's whole unresolved backlog.
#[derive(Debug, Default)]
struct TouchTracker {
    /// Repo-relative paths reparsed this pass.
    paths: Vec<String>,
    /// Names of symbols inserted this pass (across all touched files).
    new_names: Vec<String>,
}

/// Every file under `repo_root` this crate has a grammar for.
async fn full_reindex(
    store: &IndexStore,
    repo_id: &str,
    repo_root: &Path,
    touched: &mut TouchTracker,
) -> Result<IndexDelta> {
    let files = walk_source_files(repo_root);
    let mut delta = IndexDelta::default();
    for rel_path in files {
        reindex_one_file(store, repo_id, repo_root, &rel_path, &mut delta, touched).await?;
    }
    Ok(delta)
}

/// `git diff --name-status` against the stored commit for tracked changes,
/// plus a hash-mismatch sweep over the working tree for anything dirty —
/// covers both "committed since we last indexed" and "not committed yet".
async fn incremental_reindex(
    store: &IndexStore,
    repo_id: &str,
    repo_root: &Path,
    stored_commit: &str,
    current_commit: Option<&str>,
    touched_out: &mut TouchTracker,
) -> Result<IndexDelta> {
    let mut delta = IndexDelta::default();
    let mut candidates: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    if let Some(current_commit) = current_commit {
        if current_commit != stored_commit {
            for (status, rel_path) in git_diff_name_status(repo_root, stored_commit, current_commit)
            {
                candidates.insert(rel_path.clone());
                if status == 'D' {
                    if let Some(path_str) = rel_path.to_str() {
                        let removed = store.remove_file(repo_id, path_str).await?;
                        delta.files_removed += 1;
                        delta.symbols_removed += removed;
                    }
                }
            }
        }
    }

    // Dirty-working-tree sweep: only worth an O(repo) hash pass when the
    // tree actually has uncommitted changes — `git diff --name-status`
    // above already covers everything committed since `stored_commit`. On
    // a clean tree (the common case: reindex right after a commit) this
    // whole loop is skipped, which is what keeps a one-file, committed
    // change's reindex from paying to hash every other file in the repo.
    if git_is_dirty(repo_root) {
        for rel_path in walk_source_files(repo_root) {
            if candidates.contains(&rel_path) {
                continue; // already handled by the git-diff pass above
            }
            let Some(path_str) = rel_path.to_str() else {
                continue;
            };
            let abs = repo_root.join(&rel_path);
            let Ok(contents) = std::fs::read(&abs) else {
                continue;
            };
            let hash = hash_bytes(&contents);
            let unchanged = store
                .get_file_hash(repo_id, path_str)
                .await?
                .is_some_and(|stored| stored == hash);
            if !unchanged {
                candidates.insert(rel_path);
            }
        }
    }

    for rel_path in candidates {
        if let Some(path_str) = rel_path.to_str() {
            if git_diff_status_is_delete(repo_root, stored_commit, current_commit, path_str) {
                continue; // already handled above
            }
        }
        reindex_one_file(
            store,
            repo_id,
            repo_root,
            &rel_path,
            &mut delta,
            touched_out,
        )
        .await?;
    }
    Ok(delta)
}

async fn reindex_one_file(
    store: &IndexStore,
    repo_id: &str,
    repo_root: &Path,
    rel_path: &Path,
    delta: &mut IndexDelta,
    touched: &mut TouchTracker,
) -> Result<()> {
    let Some(path_str) = rel_path.to_str() else {
        return Ok(());
    };
    let Some(lang) = Language::from_path(path_str) else {
        return Ok(());
    };
    let abs = repo_root.join(rel_path);
    let Ok(bytes) = std::fs::read(&abs) else {
        // Removed since the walk enumerated it (race with a concurrent
        // agent editing the tree) — treat as a deletion, not a failure.
        let removed = store.remove_file(repo_id, path_str).await?;
        delta.files_removed += 1;
        delta.symbols_removed += removed;
        return Ok(());
    };
    let Ok(source) = std::str::from_utf8(&bytes) else {
        delta.parse_failures += 1;
        return Ok(());
    };
    let hash = hash_bytes(&bytes);
    let parsed = match parse_file(lang, source) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(path = path_str, error = %e, "lopi-index: parse failed, skipping file");
            delta.parse_failures += 1;
            return Ok(());
        }
    };

    let (removed, local_to_db) = store
        .replace_file_symbols(repo_id, path_str, lang, &hash, &parsed.symbols)
        .await
        .with_context(|| format!("replacing symbols for {path_str}"))?;
    let refs_added = store
        .insert_file_refs(repo_id, path_str, &parsed.refs, &local_to_db)
        .await
        .with_context(|| format!("inserting refs for {path_str}"))?;

    touched.paths.push(path_str.to_string());
    touched
        .new_names
        .extend(parsed.symbols.iter().map(|s| s.name.clone()));

    delta.files_indexed += 1;
    delta.symbols_removed += removed;
    delta.symbols_added += parsed.symbols.len();
    delta.refs_added += refs_added;
    Ok(())
}

fn git_diff_status_is_delete(
    repo_root: &Path,
    stored_commit: &str,
    current_commit: Option<&str>,
    path: &str,
) -> bool {
    let Some(current_commit) = current_commit else {
        return false;
    };
    git_diff_name_status(repo_root, stored_commit, current_commit)
        .into_iter()
        .any(|(status, p)| status == 'D' && p.to_str() == Some(path))
}

/// The extensions this crate has a grammar for — kept alongside
/// [`Language::from_path`] rather than duplicated logic; a new grammar
/// means a new `Language::from_path` arm, and this list stays in sync
/// automatically since it delegates to the same function per-candidate.
fn walk_source_files(repo_root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_dir(repo_root, repo_root, &mut out);
    out.sort();
    out
}

fn walk_dir(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            walk_dir(root, &path, out);
        } else if let Ok(rel) = path.strip_prefix(root) {
            if Language::from_path(&rel.to_string_lossy()).is_some() {
                out.push(rel.to_path_buf());
            }
        }
    }
}

/// Whether `repo_root` has any uncommitted change (tracked or untracked).
/// Fails open to `true` (do the slower but always-correct hash sweep) on
/// any error reading `git status` — silently trusting a clean-tree fast
/// path we couldn't actually verify would be the wrong direction to fail in.
fn git_is_dirty(repo_root: &Path) -> bool {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output();
    match output {
        Ok(o) if o.status.success() => !o.stdout.is_empty(),
        _ => true,
    }
}

fn git_head(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim().to_string())
}

/// `(status_char, repo-relative-path)` pairs from `git diff --name-status`.
/// Renames (`R100`) are reported as the new path with status `'R'` — the
/// caller's hash-mismatch sweep will reindex it under the new name; the old
/// path's stale row is left for the next full reindex to notice via
/// `is_none()` on a path that no longer exists (bounded blast radius: a
/// renamed-but-not-yet-fully-reindexed symbol is stale, not wrong).
fn git_diff_name_status(repo_root: &Path, from: &str, to: &str) -> Vec<(char, PathBuf)> {
    let output = Command::new("git")
        .args(["diff", "--name-status", from, to])
        .current_dir(repo_root)
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let status = parts.next()?.chars().next()?;
            let path = parts.next_back()?; // rename lines carry old\tnew; next_back() picks new
            Some((status, PathBuf::from(path)))
        })
        .collect()
}

#[cfg(test)]
#[path = "reindex_tests.rs"]
mod tests;
