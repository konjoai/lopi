//! CLI command handler implementations extracted from main() to stay under
//! the 500-line file size gate.

use anyhow::Result;
use lopi_core::LopiConfig;
use lopi_memory::MemoryStore;
use lopi_orchestrator::next_run_times;
use std::path::PathBuf;

pub(crate) async fn handle_learn_list(
    limit: i64,
    postmortem_only: bool,
    db: PathBuf,
) -> Result<()> {
    let store = MemoryStore::open(db).await?;
    let patterns = store.load_patterns(limit).await?;
    let filtered: Vec<_> = if postmortem_only {
        patterns
            .into_iter()
            .filter(|p| p.derived_from_postmortem == 1)
            .collect()
    } else {
        patterns
    };

    println!("🧠 lopi learn — {} pattern(s)\n", filtered.len());
    if filtered.is_empty() {
        if postmortem_only {
            println!(
                "  No post-mortem patterns yet. Enable with `lopi run --adaptive-retry` on a task that fails."
            );
        } else {
            println!("  No patterns yet. Patterns are mined after each completed task.");
        }
        return Ok(());
    }

    println!(
        "  {:<8}  {:<40}  {:>9}  {:>9}  Source",
        "Id", "Keywords", "Avg Att.", "Success%"
    );
    println!("  {}", "─".repeat(90));
    for p in filtered {
        let id_short = &p.id[..8.min(p.id.len())];
        let kw = if p.goal_keywords.len() > 40 {
            format!("{}…", &p.goal_keywords[..39])
        } else {
            p.goal_keywords.clone()
        };
        let avg = p.avg_attempts.map_or_else(|| "-".to_string(), |a| format!("{a:.1}"));
        let sr = p.success_rate.map_or_else(|| "-".to_string(), |s| format!("{:.0}%", s * 100.0));
        let source = if p.derived_from_postmortem == 1 { "🧠 post-mortem" } else { "📊 mined" };
        println!("  {id_short:<8}  {kw:<40}  {avg:>9}  {sr:>9}  {source}");
    }
    Ok(())
}

pub(crate) async fn handle_learn_show(id: &str, db: PathBuf) -> Result<()> {
    let store = MemoryStore::open(db).await?;
    let Some(p) = store.find_pattern_by_id_prefix(id).await? else {
        eprintln!("❌ no pattern matches id prefix '{id}'");
        std::process::exit(1);
    };

    println!("🧠 Pattern {}\n", p.id);
    println!("  Keywords:    {}", p.goal_keywords);
    println!(
        "  Source:      {}",
        if p.derived_from_postmortem == 1 {
            "🧠 post-mortem-derived (Claude reflection over a failed run)"
        } else {
            "📊 mined from completed-task statistics"
        }
    );
    println!(
        "  Avg attempts: {}",
        p.avg_attempts.map_or_else(|| "-".to_string(), |a| format!("{a:.2}"))
    );
    println!(
        "  Success:     {}",
        p.success_rate.map_or_else(|| "-".to_string(), |s| format!("{:.0}%", s * 100.0))
    );
    println!("  Last seen:   {}", p.last_seen);
    if let Some(c) = p.successful_constraints.as_deref() {
        println!("\n  Constraint:");
        println!("    {c}");
    } else {
        println!("\n  Constraint:  (none captured yet)");
    }
    Ok(())
}

pub(crate) async fn handle_learn_export(limit: i64, db: PathBuf) -> Result<()> {
    let store = MemoryStore::open(db).await?;
    let patterns = store.load_patterns(limit).await?;
    let json = serde_json::json!({
        "exported_at": chrono::Utc::now().to_rfc3339(),
        "count": patterns.len(),
        "patterns": patterns.iter().map(|p| serde_json::json!({
            "id": p.id,
            "goal_keywords": p.goal_keywords,
            "successful_constraints": p.successful_constraints,
            "avg_attempts": p.avg_attempts,
            "success_rate": p.success_rate,
            "last_seen": p.last_seen,
            "derived_from_postmortem": p.derived_from_postmortem == 1,
        })).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&json)?);
    Ok(())
}

pub(crate) fn handle_schedules(cfg: &Option<LopiConfig>) -> Result<()> {
    let schedules = cfg.as_ref().map(|c| c.schedules.clone()).unwrap_or_default();
    if schedules.is_empty() {
        println!("⏰ lopi schedules — none configured");
        println!();
        println!("  Add [[schedules]] entries to lopi.toml:");
        println!();
        println!("  [[schedules]]");
        println!("  name = \"nightly-lint\"");
        println!("  repo = \"/path/to/repo\"");
        println!("  goal = \"Fix all clippy warnings\"");
        println!("  cron = \"0 2 * * *\"");
        return Ok(());
    }

    println!("⏰ lopi schedules — {} configured\n", schedules.len());
    let w = 30usize;
    println!("  {:<20}  {:<w$}  {:<14}  Next run (UTC)", "Name", "Goal", "Cron");
    println!("  {}", "─".repeat(20 + 2 + w + 2 + 14 + 2 + 26));
    for s in &schedules {
        let goal = if s.goal.len() > w {
            format!("{}…", &s.goal[..w - 1])
        } else {
            s.goal.clone()
        };
        let next = next_run_times(&s.cron, 1)
            .into_iter()
            .next()
            .map_or_else(|| "invalid cron".to_string(), |t| t.format("%Y-%m-%d %H:%M UTC").to_string());
        println!("  {:<20}  {:<w$}  {:<14}  {}", s.name, goal, s.cron, next);
    }
    Ok(())
}
