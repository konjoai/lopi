//! `lopi stability` subcommand handlers.
use anyhow::Result;
use lopi_memory::MemoryStore;

use crate::util::db_path;

/// List the most recent stability assessments from the ledger.
pub async fn list(limit: i64, unstable_only: bool) -> Result<()> {
    let store = MemoryStore::open(db_path()).await?;
    let entries = store.load_stability_entries(limit).await?;
    let filtered: Vec<_> = if unstable_only {
        entries
            .into_iter()
            .filter(|e| e.verdict == "unstable")
            .collect()
    } else {
        entries
    };

    println!("🔬 lopi stability — {} assessment(s)\n", filtered.len());
    if filtered.is_empty() {
        if unstable_only {
            println!("  No unstable assessments in the ledger.");
        } else {
            println!("  No stability assessments yet.");
            println!(
                "  Enable with `AgentRunner::with_stability_gate()` or `lopi run --stability-gate`."
            );
        }
        return Ok(());
    }

    println!(
        "  {:<8}  {:<36}  {:<9}  {:>8}  {:>8}  Verdict",
        "Id", "Goal prefix", "Model", "Variance", "Samples"
    );
    println!("  {}", "─".repeat(90));
    for e in &filtered {
        let id = &e.id[..8.min(e.id.len())];
        let goal = if e.task_goal_pfx.len() > 36 {
            format!("{}…", &e.task_goal_pfx[..35])
        } else {
            e.task_goal_pfx.clone()
        };
        let model_short = e.model.split('-').next_back().unwrap_or(&e.model);
        let verdict_icon = match e.verdict.as_str() {
            "stable" => "✅ stable",
            "warning" => "⚠️  warning",
            "unstable" => "🚫 UNSTABLE",
            other => other,
        };
        println!(
            "  {id:<8}  {goal:<36}  {model_short:<9}  {:>8.3}  {:>8}  {verdict_icon}",
            e.variance_score, e.n_samples
        );
    }
    Ok(())
}

/// Show an aggregate summary of all-time stability verdict counts.
pub async fn summary() -> Result<()> {
    let store = MemoryStore::open(db_path()).await?;
    let (stable, warning, unstable) = store.stability_verdict_counts().await?;
    let total = stable + warning + unstable;
    println!("🔬 lopi stability summary\n");
    println!("  Total assessments:  {total}");
    println!("  ✅ Stable:          {stable}");
    println!("  ⚠️  Warning:         {warning}");
    println!("  🚫 Unstable:        {unstable}");
    if total > 0 {
        let block_rate = unstable as f64 / total as f64 * 100.0;
        println!("  Block rate:         {block_rate:.1}%");
    }
    Ok(())
}
