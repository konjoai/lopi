use anyhow::Result;
use lopi_memory::MemoryStore;
use std::path::PathBuf;

use crate::LearnCmd;

pub async fn run(cmd: LearnCmd, db_path: PathBuf) -> Result<()> {
    /* mutants::skip — integration handler: requires live MemoryStore and stdout */
    match cmd {
        LearnCmd::List {
            limit,
            postmortem_only,
        } => {
            let store = MemoryStore::open(db_path).await?;
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
                        "  No post-mortem patterns yet. Enable with `lopi run --adaptive-retry`."
                    );
                } else {
                    println!("  No patterns yet. Patterns are mined after each completed task.");
                }
                return Ok(());
            }

            let h = ("Id", "Keywords", "Avg Att.", "Success%", "Source");
            println!("  {:<8}  {:<40}  {:>9}  {:>9}  {}", h.0, h.1, h.2, h.3, h.4);
            println!("  {}", "─".repeat(90));
            for p in filtered {
                let id_short = &p.id[..8.min(p.id.len())];
                let kw = if p.goal_keywords.len() > 40 {
                    format!("{}…", &p.goal_keywords[..39])
                } else {
                    p.goal_keywords.clone()
                };
                let avg = p
                    .avg_attempts
                    .map_or_else(|| "-".to_string(), |a| format!("{a:.1}"));
                let sr = p
                    .success_rate
                    .map_or_else(|| "-".to_string(), |s| format!("{:.0}%", s * 100.0));
                let source = if p.derived_from_postmortem == 1 {
                    "🧠 post-mortem"
                } else {
                    "📊 mined"
                };
                println!("  {id_short:<8}  {kw:<40}  {avg:>9}  {sr:>9}  {source}");
            }
        }

        LearnCmd::Show { id } => {
            let store = MemoryStore::open(db_path).await?;
            let Some(p) = store.find_pattern_by_id_prefix(&id).await? else {
                eprintln!("❌ no pattern matches id prefix '{id}'");
                std::process::exit(1);
            };

            println!("🧠 Pattern {}\n", p.id);
            println!("  Keywords:    {}", p.goal_keywords);
            println!(
                "  Source:      {}",
                if p.derived_from_postmortem == 1 {
                    "🧠 post-mortem-derived"
                } else {
                    "📊 mined from completed-task statistics"
                }
            );
            println!(
                "  Avg attempts: {}",
                p.avg_attempts
                    .map_or_else(|| "-".to_string(), |a| format!("{a:.2}"))
            );
            println!(
                "  Success:     {}",
                p.success_rate
                    .map_or_else(|| "-".to_string(), |s| format!("{:.0}%", s * 100.0))
            );
            println!("  Last seen:   {}", p.last_seen);
            match p.successful_constraints.as_deref() {
                Some(c) => {
                    println!("\n  Constraint:");
                    println!("    {c}");
                }
                None => println!("\n  Constraint:  (none captured yet)"),
            }
        }

        LearnCmd::Export { limit } => {
            let store = MemoryStore::open(db_path).await?;
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
        }

        LearnCmd::Annotate { id, annotation } => {
            let store = MemoryStore::open(db_path).await?;
            match store.find_pattern_by_id_prefix(&id).await? {
                Some(pattern) => {
                    store
                        .annotate_pattern(&pattern.id, Some(annotation.as_str()))
                        .await?;
                    println!(
                        "✅ pattern {} annotated as '{}'",
                        &pattern.id[..8],
                        annotation
                    );
                }
                None => {
                    eprintln!("❌ pattern not found for id prefix: {}", id);
                    std::process::exit(1);
                }
            }
        }
    }
    Ok(())
}
