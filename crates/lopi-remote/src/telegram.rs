#![allow(clippy::missing_errors_doc)]
use crate::self_modify;
use crate::self_modify::PendingSelfModify;
use anyhow::Result;
use lopi_core::{Priority, Task, TaskSource};
use lopi_memory::MemoryStore;
use lopi_orchestrator::TaskQueue;
use std::sync::Arc;
use teloxide::{
    dispatching::dialogue::InMemStorage,
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup, Update},
    utils::command::BotCommands,
};
use tokio::sync::Mutex;

/// Commands accepted by the lopi Telegram bot.
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "lopi commands:")]
pub enum LopiCmd {
    /// Display available commands.
    #[command(description = "show this help")]
    Help,
    /// Queue a new task with the given goal.
    #[command(description = "queue a new task: /task <goal>")]
    Task(String),
    /// Queue a high-priority task.
    #[command(description = "high-priority task: /urgent <goal>")]
    Urgent(String),
    /// Show the current queue depth.
    #[command(description = "show queue depth")]
    Status,
    /// Approve a pending PR by task ID.
    #[command(description = "approve a pending PR by ID: /approve <task-id>")]
    Approve(String),
    /// List and annotate recent patterns.
    #[command(description = "list recent patterns with approve/reject buttons")]
    Patterns,
    /// Propose a self-improvement task (requires allow_self_modify = true).
    #[command(description = "propose a self-improvement task")]
    SelfImprove,
}

/// Start the Telegram bot. Requires `TELOXIDE_TOKEN` env var or explicit `token`.
///
/// `allowed_chat_ids`: allowlist of chat IDs permitted to issue commands.
/// Empty list = allow all chats (dev mode).
/// `allow_self_modify`: mirrors the `allow_self_modify` config flag.
pub async fn run(
    token: String,
    queue: TaskQueue,
    store: MemoryStore,
    allowed_chat_ids: Vec<i64>,
    allow_self_modify: bool,
) -> Result<()> {
    let bot = Bot::new(token);
    let queue_arc = Arc::new(queue);
    let store_arc = Arc::new(store);
    let allowed = Arc::new(allowed_chat_ids);
    let allow_sm = Arc::new(allow_self_modify);
    let pending_sm: PendingSelfModify = Arc::new(Mutex::new(std::collections::HashMap::new()));

    let handler = Update::filter_message()
        .filter_command::<LopiCmd>()
        .endpoint(message_handler);

    let callback_handler = Update::filter_callback_query().endpoint(callback_query_handler);

    Dispatcher::builder(
        bot,
        dptree::entry().branch(handler).branch(callback_handler),
    )
    .dependencies(dptree::deps![
        queue_arc,
        store_arc,
        allowed,
        allow_sm,
        pending_sm,
        InMemStorage::<()>::new()
    ])
    .build()
    .dispatch()
    .await;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn message_handler(
    bot: Bot,
    msg: Message,
    cmd: LopiCmd,
    queue: Arc<TaskQueue>,
    store: Arc<MemoryStore>,
    allowed: Arc<Vec<i64>>,
    allow_sm: Arc<bool>,
    pending_sm: PendingSelfModify,
) -> Result<()> {
    if !allowed.is_empty() && !allowed.contains(&msg.chat.id.0) {
        tracing::warn!(
            "telegram: rejected command from unauthorized chat {}",
            msg.chat.id.0
        );
        return Ok(());
    }

    match cmd {
        LopiCmd::Help => {
            bot.send_message(msg.chat.id, LopiCmd::descriptions().to_string())
                .await?;
        }

        LopiCmd::Task(goal) | LopiCmd::Urgent(goal) => {
            let mut t = Task::new(goal.clone());
            t.source = TaskSource::Telegram {
                chat_id: msg.chat.id.0,
                message_id: msg.id.0,
            };
            if msg.text().is_some_and(|t| t.starts_with("/urgent")) {
                t.priority = Priority::High;
            }

            let dup = queue.push(t).await;
            if let Some(existing) = dup {
                bot.send_message(
                    msg.chat.id,
                    format!("⚠️ already queued (id: {}…)", &existing.0.to_string()[..8]),
                )
                .await?;
            } else {
                let kb = InlineKeyboardMarkup::new([[
                    InlineKeyboardButton::callback("✅ Priority bump", format!("bump:{goal}")),
                    InlineKeyboardButton::callback("❌ Cancel", format!("cancel:{goal}")),
                ]]);
                bot.send_message(msg.chat.id, format!("🚢 queued: {goal}"))
                    .reply_markup(kb)
                    .await?;
            }
        }

        LopiCmd::Status => {
            let n = queue.len();
            bot.send_message(
                msg.chat.id,
                format!("📊 queue depth: {n}\nUse /task <goal> to add work."),
            )
            .await?;
        }

        LopiCmd::Approve(id) => {
            bot.send_message(
                msg.chat.id,
                format!("✅ approval recorded for task {id}\n(PR merge requires manual action via gh/GitHub)"),
            )
            .await?;
        }

        LopiCmd::Patterns => {
            handle_patterns(&bot, msg.chat.id, &store).await?;
        }

        LopiCmd::SelfImprove => {
            handle_self_improve(&bot, msg.chat.id, &store, &queue, &allow_sm, &pending_sm).await?;
        }
    }
    Ok(())
}

async fn handle_patterns(bot: &Bot, chat_id: ChatId, store: &MemoryStore) -> Result<()> {
    match store.load_patterns(10).await {
        Ok(patterns) => {
            if patterns.is_empty() {
                bot.send_message(chat_id, "📊 No patterns recorded yet.")
                    .await?;
            } else {
                for p in patterns {
                    let id_short = &p.id[..8.min(p.id.len())];
                    let annotation = match p.user_annotation.as_deref() {
                        Some("approved") => "✅ Approved",
                        Some("rejected") => "❌ Rejected",
                        _ => "⭕ Unannotated",
                    };
                    let success = p.success_rate.unwrap_or(0.0) * 100.0;
                    let text = format!(
                        "**Pattern {}**\nKeywords: {}\nSuccess: {:.0}%\nStatus: {}\nConstraint: {}",
                        id_short,
                        &p.goal_keywords[..p.goal_keywords.len().min(40)],
                        success,
                        annotation,
                        p.successful_constraints.as_deref().unwrap_or("(none)")
                    );
                    let kb = InlineKeyboardMarkup::new([[
                        InlineKeyboardButton::callback(
                            "✅ Approve",
                            format!("annotate:approved:{}", &p.id),
                        ),
                        InlineKeyboardButton::callback(
                            "❌ Reject",
                            format!("annotate:rejected:{}", &p.id),
                        ),
                    ]]);
                    bot.send_message(chat_id, text).reply_markup(kb).await?;
                }
            }
        }
        Err(e) => {
            bot.send_message(chat_id, format!("❌ Error loading patterns: {e}"))
                .await?;
        }
    }
    Ok(())
}

async fn handle_self_improve(
    bot: &Bot,
    chat_id: ChatId,
    store: &MemoryStore,
    queue: &Arc<TaskQueue>,
    allow_sm: &bool,
    pending_sm: &PendingSelfModify,
) -> Result<()> {
    if !allow_sm {
        bot.send_message(
            chat_id,
            "⛔ Self-modification is disabled. Set `allow_self_modify = true` in lopi.toml.",
        )
        .await?;
        return Ok(());
    }

    let goal = match self_modify::self_diagnose(store).await {
        Ok(Some(g)) => g,
        Ok(None) => {
            bot.send_message(
                chat_id,
                "ℹ️ No significant issues detected — self-improvement not needed right now.",
            )
            .await?;
            return Ok(());
        }
        Err(e) => {
            bot.send_message(chat_id, format!("❌ Diagnosis failed: {e}"))
                .await?;
            return Ok(());
        }
    };

    self_modify::propose_and_await(bot, chat_id, &goal, queue, pending_sm).await
}

async fn callback_query_handler(
    bot: Bot,
    q: CallbackQuery,
    store: Arc<MemoryStore>,
    pending_sm: PendingSelfModify,
    allowed: Arc<Vec<i64>>,
) -> Result<()> {
    let caller_id = q.from.id.0 as i64;
    if !allowed.is_empty() && !allowed.contains(&caller_id) {
        tracing::warn!(
            "telegram: rejected callback from unauthorized user {}",
            caller_id
        );
        bot.answer_callback_query(q.id).await?;
        return Ok(());
    }

    let data = q.data.as_deref().unwrap_or("");

    let reply = if data.starts_with("bump:") {
        let goal = data.trim_start_matches("bump:");
        format!("⬆️ priority bumped for: {goal}")
    } else if data.starts_with("cancel:") {
        let goal = data.trim_start_matches("cancel:");
        format!("🗑 cancellation noted for: {goal}\n(tasks in-flight cannot be stopped — the next retry will not be started)")
    } else if data.starts_with("annotate:") {
        handle_annotate_callback(data, &store).await
    } else if data == self_modify::SELF_MODIFY_APPROVE || data == self_modify::SELF_MODIFY_REJECT {
        handle_selfmod_callback(data, caller_id, pending_sm).await
    } else {
        "Unknown action.".into()
    };

    if let Some(msg) = q.message {
        bot.send_message(msg.chat().id, reply).await?;
    }
    bot.answer_callback_query(q.id).await?;
    Ok(())
}

async fn handle_annotate_callback(data: &str, store: &MemoryStore) -> String {
    let parts: Vec<&str> = data.splitn(3, ':').collect();
    if parts.len() == 3 {
        let annotation = parts[1];
        let pattern_id = parts[2];
        store
            .annotate_pattern(pattern_id, Some(annotation))
            .await
            .ok();
        format!(
            "✓ Pattern {}… marked as {}.",
            &pattern_id[..8.min(pattern_id.len())],
            annotation
        )
    } else {
        "Invalid annotate format.".into()
    }
}

async fn handle_selfmod_callback(
    data: &str,
    caller_id: i64,
    pending_sm: PendingSelfModify,
) -> String {
    let approved = data == self_modify::SELF_MODIFY_APPROVE;
    let mut guard = pending_sm.lock().await;
    if let Some((_, tx)) = guard.remove(&caller_id) {
        let _ = tx.send(approved);
        if approved {
            "✅ Approved — self-modify task will be queued.".into()
        } else {
            "❌ Rejected — self-modify proposal cancelled.".into()
        }
    } else {
        "⚠️ No pending self-modify proposal for this chat (may have expired).".into()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use lopi_memory::MemoryStore;
    use std::collections::HashMap;
    use tokio::sync::{oneshot, Mutex};

    // ── handle_annotate_callback ────────────────────────────────────────────

    #[tokio::test]
    async fn annotate_callback_valid_format_stores_annotation() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        store
            .insert_postmortem_pattern("test goal", "constraint")
            .await
            .unwrap();
        let patterns = store.load_patterns(10).await.unwrap();
        let id = &patterns[0].id;
        let data = format!("annotate:reject:{id}");
        let reply = handle_annotate_callback(&data, &store).await;
        assert!(
            reply.contains("reject"),
            "reply should mention annotation: {reply}"
        );
        assert!(
            reply.contains(&id[..8.min(id.len())]),
            "reply should include id prefix: {reply}"
        );
    }

    #[tokio::test]
    async fn annotate_callback_invalid_format_returns_error() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let reply = handle_annotate_callback("bad_data", &store).await;
        assert_eq!(reply, "Invalid annotate format.");
    }

    #[tokio::test]
    async fn annotate_callback_two_part_format_returns_error() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let reply = handle_annotate_callback("annotate:only_two", &store).await;
        assert_eq!(reply, "Invalid annotate format.");
    }

    // ── handle_selfmod_callback ─────────────────────────────────────────────

    #[tokio::test]
    async fn selfmod_callback_no_pending_returns_expired_message() {
        let pending: PendingSelfModify = Arc::new(Mutex::new(HashMap::new()));
        let reply = handle_selfmod_callback(self_modify::SELF_MODIFY_APPROVE, 42, pending).await;
        assert!(
            reply.contains("No pending"),
            "expected 'No pending' in: {reply}"
        );
    }

    #[tokio::test]
    async fn selfmod_callback_approve_sends_true_and_returns_approved_message() {
        let pending: PendingSelfModify = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel::<bool>();
        pending.lock().await.insert(99, ("goal".into(), tx));
        let reply =
            handle_selfmod_callback(self_modify::SELF_MODIFY_APPROVE, 99, Arc::clone(&pending))
                .await;
        assert!(reply.contains("Approved"), "expected approval in: {reply}");
        assert!(rx.await.unwrap(), "expected approved=true on channel");
        // Entry should be removed.
        assert!(pending.lock().await.get(&99).is_none());
    }

    #[tokio::test]
    async fn selfmod_callback_reject_sends_false_and_returns_rejected_message() {
        let pending: PendingSelfModify = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel::<bool>();
        pending.lock().await.insert(77, ("goal".into(), tx));
        let reply =
            handle_selfmod_callback(self_modify::SELF_MODIFY_REJECT, 77, Arc::clone(&pending))
                .await;
        assert!(reply.contains("Rejected"), "expected rejection in: {reply}");
        assert!(!rx.await.unwrap(), "expected approved=false on channel");
    }

    #[tokio::test]
    async fn selfmod_callback_wrong_caller_does_not_consume_other_chat_entry() {
        let pending: PendingSelfModify = Arc::new(Mutex::new(HashMap::new()));
        let (tx, _rx) = oneshot::channel::<bool>();
        pending.lock().await.insert(11, ("goal".into(), tx));
        // Different caller_id — should NOT consume chat 11's entry.
        let reply =
            handle_selfmod_callback(self_modify::SELF_MODIFY_APPROVE, 22, Arc::clone(&pending))
                .await;
        assert!(
            reply.contains("No pending"),
            "wrong caller should get expired msg: {reply}"
        );
        // Entry for chat 11 must still be present.
        assert!(pending.lock().await.get(&11).is_some());
    }
}
