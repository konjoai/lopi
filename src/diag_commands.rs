//! `lopi diag` — export a point-in-time snapshot of the local SQLite store
//! (tasks, task logs, audit trail, stability ledger, quota observations) as
//! committable JSON.
//!
//! The store normally lives at `~/.lopi/lopi.db`, outside the repo and
//! covered by `.gitignore` — invisible to Claude chat or any agent without
//! access to this machine. This command gives that data a repo-relative,
//! human- and agent-readable home so it can be shared deliberately.
use anyhow::{Context, Result};
use lopi_memory::{AuditQuery, MemoryStore};
use std::path::{Path, PathBuf};

use crate::util::db_path;

/// Row limits for each exported table.
pub struct DiagLimits {
    /// Max task rows (most recent first, written oldest-first).
    pub tasks: i64,
    /// Max task-log lines across all tasks.
    pub logs: i64,
    /// Max audit-log rows.
    pub audit: i64,
}

struct DiagCounts {
    tasks: usize,
    logs: usize,
    audit: usize,
    stability: usize,
    quota: usize,
}

/// Write a full diagnostic snapshot into `<out>/<UTC timestamp>/`.
///
/// # Errors
/// Returns `Err` if the store can't be opened, a query fails, or the
/// output directory can't be created/written.
pub async fn export(out: PathBuf, limits: DiagLimits) -> Result<()> {
    let store = MemoryStore::open(db_path()).await?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let dir = out.join(&stamp);
    std::fs::create_dir_all(&dir).context("creating diagnostics directory")?;

    let counts = DiagCounts {
        tasks: export_tasks(&store, &dir, limits.tasks).await?,
        logs: export_logs(&store, &dir, limits.logs).await?,
        audit: export_audit(&store, &dir, limits.audit).await?,
        stability: export_stability(&store, &dir).await?,
        quota: export_quota(&store, &dir).await?,
    };
    write_readme(&dir, &stamp, &counts)?;

    println!("🩺 lopi diag — snapshot written to {}", dir.display());
    println!(
        "   tasks={} logs={} audit={} stability={} quota={}",
        counts.tasks, counts.logs, counts.audit, counts.stability, counts.quota
    );
    Ok(())
}

async fn export_tasks(store: &MemoryStore, dir: &Path, limit: i64) -> Result<usize> {
    let tasks = store.load_history(limit).await?;
    let json: Vec<_> = tasks
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "goal": t.goal,
                "status": t.status,
                "created_at": t.created_at,
                "completed_at": t.completed_at,
                "client_ref": t.client_ref,
            })
        })
        .collect();
    let n = json.len();
    write_json(dir, "tasks.json", &json)?;
    Ok(n)
}

async fn export_logs(store: &MemoryStore, dir: &Path, limit: i64) -> Result<usize> {
    let logs = store.load_recent_task_logs(limit).await?;
    let n = logs.len();
    write_json(dir, "task_logs.json", &logs)?;
    Ok(n)
}

async fn export_audit(store: &MemoryStore, dir: &Path, limit: i64) -> Result<usize> {
    let audit = store
        .query_audit(&AuditQuery {
            limit,
            ..AuditQuery::default()
        })
        .await?;
    let n = audit.len();
    write_json(dir, "audit.json", &audit)?;
    Ok(n)
}

async fn export_stability(store: &MemoryStore, dir: &Path) -> Result<usize> {
    let entries = store.load_stability_entries(200).await?;
    let json: Vec<_> = entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "task_goal_pfx": e.task_goal_pfx,
                "model": e.model,
                "n_samples": e.n_samples,
                "variance_score": e.variance_score,
                "verdict": e.verdict,
            })
        })
        .collect();
    let n = json.len();
    write_json(dir, "stability.json", &json)?;
    Ok(n)
}

async fn export_quota(store: &MemoryStore, dir: &Path) -> Result<usize> {
    let quota = store.list_quota_observations().await?;
    let n = quota.len();
    write_json(dir, "quota.json", &quota)?;
    Ok(n)
}

fn write_json<T: serde::Serialize>(dir: &Path, name: &str, data: &T) -> Result<()> {
    let path = dir.join(name);
    let json = serde_json::to_string_pretty(data).with_context(|| format!("serializing {name}"))?;
    std::fs::write(&path, json).with_context(|| format!("writing {}", path.display()))
}

fn write_readme(dir: &Path, stamp: &str, counts: &DiagCounts) -> Result<()> {
    let body = format!(
        "# lopi diagnostic snapshot — {stamp}\n\n\
Exported from the local SQLite store (`~/.lopi/lopi.db`) with `lopi diag`, so \
task/log/audit state that normally only exists on this machine can be shared \
with Claude chat or other agents that don't have local filesystem access.\n\n\
| File | Rows |\n\
|---|---|\n\
| tasks.json | {} |\n\
| task_logs.json | {} |\n\
| audit.json | {} |\n\
| stability.json | {} |\n\
| quota.json | {} |\n\n\
Contents may include task goal text, log lines, and file paths from whatever \
repos lopi has run against. This snapshot is committed to the repo, not \
gitignored — review before sharing further.\n",
        counts.tasks, counts.logs, counts.audit, counts.stability, counts.quota
    );
    std::fs::write(dir.join("README.md"), body).context("writing README.md")
}
