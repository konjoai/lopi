use anyhow::Result;
use lopi_core::{AgentEvent, EventBus};
use lopi_memory::MemoryStore;

use crate::{
    remote,
    util::{db_path, fmt_status},
};

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

/// Cancel a task on the `lopi sail` server at `base_url`, returning the
/// user-facing outcome message (never printing directly) so this is
/// assertable in a test without capturing stdout — a mutation-testing kill
/// test needs the real message, not just "did this return `Ok`."
pub async fn cancel(base_url: &str, task_id: String) -> Result<String> {
    let token = lopi_ui::client::resolve_auth_token(None);
    Ok(
        match remote::cancel_task(base_url, token, &task_id).await {
            Ok(msg) => msg,
            Err(_) => {
                "⚠️  No running lopi sail server on :3000.\n   Start `lopi sail` first or use the web dashboard."
                    .to_string()
            }
        },
    )
}

/// P1.3 — `lopi resume --agent-id <uuid>`: load the most-recent checkpoint
/// for a task and print it. The checkpoint carries enough state for an
/// upstream operator to decide whether to re-queue, abort, or inspect the
/// `repo_path` directly. Full re-attach is a follow-up sprint.
pub async fn resume(agent_id: String) -> Result<()> {
    let store = MemoryStore::open(crate::util::db_path()).await?;
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Mutation-testing kill test: pins `cancel`'s exact "not found"
    /// message against a real server, so a mutant that stubs the return
    /// value (`Ok(())`) fails instead of silently surviving — `cancel`
    /// returns the message rather than printing it directly precisely so
    /// this is assertable without capturing stdout.
    #[tokio::test]
    async fn cancel_unknown_id_reports_not_found() {
        let base_url = crate::test_support::spawn_live_server(None).await;
        let msg = cancel(&base_url, "not-a-real-task-id".to_string())
            .await
            .unwrap();
        assert_eq!(msg, "ℹ️  task not found");
    }
}
