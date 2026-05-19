use anyhow::Result;
use lopi_core::{AgentEvent, EventBus};
use lopi_memory::MemoryStore;

use crate::{db_path, fmt_status, remote};

pub async fn watch(ws_url: Option<String>, local: bool) -> Result<()> {
    if local {
        let bus: EventBus<AgentEvent> = EventBus::new(512);
        println!("👁  lopi watch (local bus — no running sail server)");
        lopi_ui::tui::run(bus).await?;
    } else {
        let url = ws_url.unwrap_or_else(|| "ws://127.0.0.1:3000/ws".into());
        println!("👁  lopi watch — connecting to {url}");
        remote::watch_remote(url).await?;
    }
    Ok(())
}

pub async fn tail(task_id: Option<String>, history: bool) -> Result<()> {
    let store = MemoryStore::open(db_path()).await?;
    if history || task_id.is_some() {
        let rows = store.load_history(50).await?;
        println!("⚓ lopi tail — {} task(s) in history", rows.len());
        for t in rows
            .iter()
            .filter(|t| task_id.as_deref().is_none_or(|id| t.id.starts_with(id)))
        {
            println!(
                "  [{}] {}… — {}",
                fmt_status(&t.status),
                &t.id[..8.min(t.id.len())],
                t.goal
            );
        }
    } else {
        println!("📋 lopi tail — use --history or run `lopi sail` for a live server");
        tokio::signal::ctrl_c().await?;
    }
    Ok(())
}

pub async fn dock() -> Result<()> {
    let store = MemoryStore::open(db_path()).await?;
    let history = store.load_history(50).await?;
    println!("⚓ lopi dock — {} task(s)\n", history.len());
    if history.is_empty() {
        println!("  No tasks yet. Try: lopi run --goal \"write a test\"");
        return Ok(());
    }
    let w = 50usize;
    println!("  {:<8}  {:<w$}  Status", "ID", "Goal");
    println!("  {}", "─".repeat(8 + 2 + w + 2 + 20));
    for t in history {
        let goal = if t.goal.len() > w {
            format!("{}…", &t.goal[..w - 1])
        } else {
            t.goal.clone()
        };
        println!(
            "  {:<8}  {:<w$}  {}",
            &t.id[..8.min(t.id.len())],
            goal,
            fmt_status(&t.status)
        );
    }
    Ok(())
}

pub async fn cancel(task_id: String) -> Result<()> {
    let url = format!("http://127.0.0.1:3000/api/tasks/{task_id}");
    if let Ok(msg) = remote::reqwest_cancel(&url).await {
        println!("{msg}");
    } else {
        println!("⚠️  No running lopi sail server on :3000.");
        println!("   Start `lopi sail` first or use the web dashboard.");
    }
    Ok(())
}

/// P1.3 — `lopi resume --agent-id <uuid>`: load the most-recent checkpoint
/// for a task and print it. The checkpoint carries enough state for an
/// upstream operator to decide whether to re-queue, abort, or inspect the
/// `repo_path` directly. Full re-attach is a follow-up sprint.
pub async fn resume(agent_id: String) -> Result<()> {
    let store = MemoryStore::open(crate::db_path()).await?;
    let task_id = match agent_id.parse::<uuid::Uuid>() {
        Ok(u) => lopi_core::TaskId(u),
        Err(_) => {
            anyhow::bail!("agent-id must be a uuid; got `{agent_id}`");
        }
    };
    match store.latest_checkpoint(&task_id).await? {
        Some(cp) => {
            println!("⛵ checkpoint for {agent_id}:");
            println!("   attempt:    {}", cp.attempt);
            println!("   state:      {}", cp.state);
            println!("   created_at: {}", cp.created_at);
            if let Some(p) = cp.repo_path {
                println!("   repo_path:  {p}");
            }
            if let Some(h) = cp.context_hash {
                println!("   ctx_hash:   {h}");
            }
            if let Some(plan) = cp.last_plan {
                let preview: String = plan.chars().take(160).collect();
                println!(
                    "   plan:       {preview}{}",
                    if plan.chars().count() > 160 {
                        "…"
                    } else {
                        ""
                    }
                );
            }
            if let Some(score) = cp.last_score {
                println!("   score:      {score}");
            }
        }
        None => {
            println!("no checkpoints recorded for {agent_id}");
        }
    }
    Ok(())
}
