//! Deterministic, seeded synthetic-store generator behind `lopi demo`.
//!
//! Fabricates a complete, self-consistent lopi store — repos, tasks across
//! every status, agent traffic, token counts, a quality trend, patterns,
//! lessons, and at least one honest failure — so someone can see a fully
//! alive dashboard with zero setup. Nothing here reads the real machine:
//! no environment inspection, no git calls, no filesystem scans of real
//! repos. See `docs/adr/0001-demo-mode-and-measurement.md` for the design
//! rationale and `docs/MEASUREMENT.md` for how synthetic data is marked.
//!
//! Heavy lifting is split across two sibling modules: `generator_content`
//! (the pure, IO-free task-plan builder shared with [`crate::scenario`]) and
//! `generator_seed` (the private DB-writing helpers for attempts,
//! turn metrics, logs, DAG nodes, patterns, lessons, quality runs, and
//! dead-letters) — kept as separate files purely to stay under this repo's
//! file-size CI gate.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use lopi_core::Task;
use lopi_memory::{DemoRepoRow, MemoryStore};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::content;
use crate::generator_content::{build_task_plans, TaskOutcome, TaskPlan};
use crate::generator_seed::{
    attempt_count_for, seeded_uuid, write_attempts, write_dag, write_dead_letter, write_lessons,
    write_patterns, write_quality_trend, write_task_logs, write_turn_metrics,
};

/// Filename of the demo store, a sibling of the real store's `lopi.db`
/// (`~/.lopi/demo.db`) — see the ADR for why not an XDG data dir.
pub const DEMO_DB_FILENAME: &str = "demo.db";

/// Fixed default seed for a bare `lopi demo` (no `--seed`), so a
/// no-flags run stays reproducible.
pub const DEFAULT_DEMO_SEED: u64 = 1337;

/// `~/.lopi/demo.db` — mirrors the real store's own `$HOME`-based
/// convention (see `src/util.rs::db_path` in the binary crate; this crate
/// can't depend on the binary, so it resolves `$HOME` itself the same way).
#[must_use]
pub fn default_demo_store_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".lopi").join(DEMO_DB_FILENAME)
}

/// Summary of what [`generate`] produced, for CLI startup banners and tests.
#[derive(Debug, Clone)]
pub struct GeneratedDemo {
    /// The seed used to generate this store.
    pub seed: u64,
    /// Number of synthetic repos written (always 4).
    pub repo_count: usize,
    /// Number of synthetic tasks written.
    pub task_count: usize,
}

/// Lexically normalize `.`/`..` components without touching the filesystem
/// — used before canonicalization so a nonexistent path's `..` components
/// don't end up littering the climb-to-nearest-existing-ancestor fallback
/// below with literal `..` segments.
fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolve `path` to a canonical form suitable for equality comparison,
/// even when `path` (or intermediate directories on it) doesn't exist yet:
/// canonicalize the nearest existing ancestor and rejoin the (lexically
/// normalized) remaining components. This is what lets the isolation guard
/// below prove two paths differ — or collide — before either file exists.
fn resolve_for_compare(path: &Path) -> Result<PathBuf> {
    let normalized = normalize_lexically(path);
    if let Ok(canon) = std::fs::canonicalize(&normalized) {
        return Ok(canon);
    }

    let mut existing = normalized;
    let mut suffix: Vec<std::ffi::OsString> = Vec::new();
    loop {
        let popped = existing.file_name().map(std::ffi::OsStr::to_os_string);
        match popped {
            Some(name) => {
                existing.pop();
                suffix.push(name);
            }
            None => {
                anyhow::bail!(
                    "cannot resolve path for comparison — no existing ancestor found: {}",
                    path.display()
                )
            }
        }
        if let Ok(canon) = std::fs::canonicalize(&existing) {
            let mut result = canon;
            for part in suffix.iter().rev() {
                result.push(part);
            }
            return Ok(result);
        }
    }
}

/// Refuse when `dest` and `real_store_path` resolve to the same file — the
/// hard safety boundary from the ADR. Provable even when neither file
/// exists yet.
fn guard_not_real_store(dest: &Path, real_store_path: &Path) -> Result<()> {
    let dest_resolved = resolve_for_compare(dest)
        .with_context(|| format!("resolving demo dest path {}", dest.display()))?;
    let real_resolved = resolve_for_compare(real_store_path)
        .with_context(|| format!("resolving real store path {}", real_store_path.display()))?;
    if dest_resolved == real_resolved {
        anyhow::bail!(
            "refusing to generate demo store at {} — it resolves to the same file as the real store {}",
            dest.display(),
            real_store_path.display()
        );
    }
    Ok(())
}

/// Insert all 4 entries from `content::REPOS`.
async fn write_repos(store: &MemoryStore) -> Result<()> {
    for (i, repo) in content::REPOS.iter().enumerate() {
        store
            .insert_demo_repo(&DemoRepoRow {
                name: repo.name.to_string(),
                stack: repo.stack.to_string(),
                path: repo.path.to_string(),
                description: repo.description.to_string(),
                sort_order: i64::try_from(i).unwrap_or(0),
            })
            .await?;
    }
    Ok(())
}

/// Generate a complete synthetic store at `dest`. Refuses — returns `Err`,
/// writes nothing — if `dest` resolves to the same file as `real_store_path`
/// (the hard safety boundary from the ADR: this must be provable even when
/// neither file exists yet, e.g. a fresh install with no real store created
/// yet — canonicalize what exists, and for a path that doesn't exist,
/// canonicalize its parent directory and rejoin the file name).
///
/// Deterministic: the same `seed` produces byte-identical goals, task
/// counts, repo assignments, pattern/lesson text, and status distribution
/// across repeated calls (to different `dest` paths — verify this with a
/// test). Timestamps are real (`chrono::Utc::now()` at call time) — only
/// the *offsets* between them are seeded, per the ADR.
///
/// # Errors
/// Returns `Err` if `dest` resolves to `real_store_path`, or if any
/// database write fails.
pub async fn generate(dest: &Path, real_store_path: &Path, seed: u64) -> Result<GeneratedDemo> {
    guard_not_real_store(dest, real_store_path)?;

    let store = MemoryStore::open(dest)
        .await
        .with_context(|| format!("opening demo store at {}", dest.display()))?;
    let mut rng = StdRng::seed_from_u64(seed);

    store.set_metadata("synthetic", "true").await?;
    store.set_metadata("demo_seed", &seed.to_string()).await?;
    store
        .set_metadata("demo_generated_at", &Utc::now().to_rfc3339())
        .await?;

    write_repos(&store).await?;

    let plans = build_task_plans(seed);
    let task_count = plans.len();

    let mut dag_written_success = false;
    let mut dag_written_running = false;
    let mut failed_task_ids: Vec<String> = Vec::new();

    for (idx, plan) in plans.iter().enumerate() {
        let task_id_str = write_one_task(&store, &mut rng, plan, idx).await?;

        if plan.outcome == TaskOutcome::Success && !dag_written_success {
            write_dag(&store, &task_id_str, false).await?;
            dag_written_success = true;
        }
        if plan.outcome == TaskOutcome::Running && !dag_written_running {
            write_dag(&store, &task_id_str, true).await?;
            dag_written_running = true;
        }
        if plan.outcome == TaskOutcome::Failed {
            failed_task_ids.push(task_id_str);
        }
    }

    write_patterns(&store, &mut rng).await?;
    write_lessons(&store, &mut rng).await?;
    write_quality_trend(&store, content::REPOS[0].path).await?;
    write_dead_letters(&store, &failed_task_ids).await?;

    Ok(GeneratedDemo {
        seed,
        repo_count: content::REPOS.len(),
        task_count,
    })
}

/// Persist one task plan — the row itself, its repo, its attempts, and (for
/// anything past `Queued`) its turn metrics and log lines. Returns the
/// task's stringified id, for the DAG/dead-letter bookkeeping the caller
/// does across the whole plan list. Split out of [`generate`] purely to
/// keep that function's cognitive complexity under this repo's CI gate.
async fn write_one_task(
    store: &MemoryStore,
    rng: &mut StdRng,
    plan: &TaskPlan,
    idx: usize,
) -> Result<String> {
    let mut task = Task::new(plan.goal.clone());
    task.id = plan.id;
    task.source = plan.source.clone();
    task.max_retries = 3;
    let offset_minutes: i64 = rng.gen_range(5..(60 * 24 * 10));
    task.created_at = Utc::now() - Duration::minutes(offset_minutes);

    store.save_task(&task, "queued").await?;
    if plan.outcome != TaskOutcome::Queued {
        store.mark_running(&task.id).await?;
    }
    let repo = &content::REPOS[plan.repo_index];
    store.set_task_repo(&task.id, repo.path).await?;
    if plan.outcome.is_terminal() {
        store
            .mark_completed(&task.id, plan.outcome.db_status())
            .await?;
    }

    let attempt_count = attempt_count_for(plan.outcome, task.max_retries, rng);
    if attempt_count > 0 {
        write_attempts(store, rng, &task, plan.outcome, attempt_count).await?;
    }

    let task_id_str = task.id.0.to_string();
    if plan.outcome != TaskOutcome::Queued {
        let session_id = seeded_uuid(rng);
        let today_bias = plan.outcome == TaskOutcome::Running
            || (plan.outcome == TaskOutcome::Success && idx.is_multiple_of(3));
        write_turn_metrics(store, rng, task.id, session_id, today_bias).await?;
        write_task_logs(store, rng, &task_id_str, task.created_at, plan.outcome).await?;
    }

    Ok(task_id_str)
}

/// Write dead-letter audit entries for up to 2 failed tasks — the demo's
/// "at least one honest failure story" (per the ADR). Split out of
/// [`generate`] for the same cognitive-complexity reason as
/// [`write_one_task`].
async fn write_dead_letters(store: &MemoryStore, failed_task_ids: &[String]) -> Result<()> {
    for (i, task_id) in failed_task_ids.iter().take(2).enumerate() {
        let blocker = content::BLOCKERS[i % content::BLOCKERS.len()];
        write_dead_letter(store, task_id, blocker).await?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "generator_tests.rs"]
mod tests;
