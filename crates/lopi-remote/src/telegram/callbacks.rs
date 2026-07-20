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
    } else if let Some(rest) = data.strip_prefix("bump:") {
        handle_bump(rest, pool).await
    } else if data == "fleet_refresh" {
        "🔄 use /fleet to refresh the fleet view.".to_string()
    } else if data == "fleet_dock" {
        super::monitor::format_dock(store, 8).await
    } else {
        "Unknown action.".to_string()
    }
}

/// Handle a `bump:<level>:<task_id>` callback, where `<level>` is `critical`
/// or `high` (matching the levels [`build_priority_keyboard`] offers).
async fn handle_bump(rest: &str, pool: &AgentPool) -> String {
    let (priority, level, task_id, short) = match parse_bump(rest) {
        Ok(parsed) => parsed,
        Err(msg) => return msg,
    };
    if pool.queue().bump_priority(&task_id, priority).await {
        format!("⬆️ task {short} bumped to {level}")
    } else {
        format!("❓ no queued task found with ID {short}\nUse /dock to see recent tasks.")
    }
}

/// Parse a `<level>:<task_id>` bump payload into its priority, the level
/// string (for the reply text), the task ID, and its short display prefix —
/// or a user-facing error message.
fn parse_bump(rest: &str) -> Result<(lopi_core::Priority, &str, lopi_core::TaskId, &str), String> {
    use lopi_core::{Priority, TaskId};
    use std::str::FromStr;

    let Some((level, task_id_str)) = rest.split_once(':') else {
        return Err("❓ malformed bump action.".to_string());
    };
    let priority = match level {
        "critical" => Priority::Critical,
        "high" => Priority::High,
        _ => return Err(format!("❓ unknown priority level: {level}")),
    };
    let Ok(uuid) = uuid::Uuid::from_str(task_id_str) else {
        return Err(format!("❓ could not parse task ID: {task_id_str}"));
    };
    let short = &task_id_str[..task_id_str.len().min(8)];
    Ok((priority, level, TaskId(uuid), short))
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn parse_bump_accepts_critical_and_high() {
        let id = lopi_core::TaskId::new();
        let critical_payload = format!("critical:{id}");
        let (priority, level, parsed_id, short) =
            parse_bump(&critical_payload).expect("valid bump payload");
        assert_eq!(priority, lopi_core::Priority::Critical);
        assert_eq!(level, "critical");
        assert_eq!(parsed_id, id);
        assert_eq!(short.len(), 8);

        let high_payload = format!("high:{id}");
        let (priority, level, ..) = parse_bump(&high_payload).expect("valid bump payload");
        assert_eq!(priority, lopi_core::Priority::High);
        assert_eq!(level, "high");
    }

    #[test]
    fn parse_bump_rejects_unknown_level() {
        let id = lopi_core::TaskId::new();
        assert!(parse_bump(&format!("urgent:{id}")).is_err());
    }

    #[test]
    fn parse_bump_rejects_malformed_payload() {
        assert!(parse_bump("no-colon-here").is_err());
    }

    #[test]
    fn parse_bump_rejects_unparseable_task_id() {
        assert!(parse_bump("critical:not-a-uuid").is_err());
    }
}
