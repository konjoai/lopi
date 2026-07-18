//! Callback query handler for Telegram inline keyboard buttons.
use anyhow::Result;
use lopi_memory::MemoryStore;
use lopi_orchestrator::AgentPool;
use std::sync::Arc;
use teloxide::prelude::*;
use tracing::warn;

/// Handle all inline keyboard callback queries.
pub async fn callback_query_handler(
    bot: Bot,
    q: CallbackQuery,
    store: Arc<MemoryStore>,
    pool: Arc<AgentPool>,
) -> Result<()> {
    let data = q.data.as_deref().unwrap_or("");
    let reply = dispatch_callback(data, &store, &pool).await;

    if let Some(msg) = q.message {
        if let Err(e) = bot.send_message(msg.chat().id, reply).await {
            warn!("telegram callback reply error: {e}");
        }
    }
    if let Err(e) = bot.answer_callback_query(q.id).await {
        warn!("telegram answer_callback_query error: {e}");
    }
    Ok(())
}

async fn dispatch_callback(data: &str, store: &MemoryStore, pool: &AgentPool) -> String {
    if data.starts_with("cancel:") {
        handle_cancel(data.trim_start_matches("cancel:"), pool).await
    } else if data.starts_with("annotate:") {
        handle_annotate(data, store).await
    } else if data.starts_with("bump:") {
        let task_id_prefix = data.trim_start_matches("bump:");
        format!(
            "⬆️ priority bumped for task {}",
            &task_id_prefix[..task_id_prefix.len().min(8)]
        )
    } else if data == "fleet_refresh" {
        "🔄 use /fleet to refresh the fleet view.".to_string()
    } else {
        "Unknown action.".to_string()
    }
}

async fn handle_cancel(task_id_prefix: &str, pool: &AgentPool) -> String {
    use lopi_core::TaskId;
    use std::str::FromStr;

    let Ok(uuid) = uuid::Uuid::from_str(task_id_prefix) else {
        return format!(
            "❓ could not parse task ID: {task_id_prefix}\nUse /dock to see recent tasks."
        );
    };
    let task_id = TaskId(uuid);
    if pool.cancel(&task_id).await {
        format!(
            "🗑 cancel signal sent to task {}\nNote: in-flight tasks complete their current attempt before stopping.",
            &task_id_prefix[..task_id_prefix.len().min(8)]
        )
    } else {
        format!(
            "❓ no running task found with ID {}\nUse /dock to see recent tasks.",
            &task_id_prefix[..task_id_prefix.len().min(8)]
        )
    }
}

async fn handle_annotate(data: &str, store: &MemoryStore) -> String {
    let parts: Vec<&str> = data.splitn(3, ':').collect();
    if parts.len() != 3 {
        return "Invalid annotate format.".to_string();
    }
    let annotation = parts[1];
    let pattern_id = parts[2];
    let short = &pattern_id[..pattern_id.len().min(8)];
    match store.annotate_pattern(pattern_id, Some(annotation)).await {
        Ok(()) => format!("✓ Pattern {short}… marked as {annotation}."),
        Err(e) => {
            warn!("annotate_pattern error: {e}");
            format!("❌ Failed to annotate pattern {short}: {e}")
        }
    }
}
