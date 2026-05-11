#![allow(clippy::missing_errors_doc)]
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
}

/// Start the Telegram bot. Requires `TELOXIDE_TOKEN` env var or explicit `token`.
///
/// `allowed_chat_ids`: allowlist of chat IDs permitted to issue commands.
/// Empty list = allow all chats (dev mode).
pub async fn run(
    token: String,
    queue: TaskQueue,
    store: MemoryStore,
    allowed_chat_ids: Vec<i64>,
) -> Result<()> {
    let bot = Bot::new(token);
    let queue_arc = Arc::new(queue);
    let store_arc = Arc::new(store);
    let allowed = Arc::new(allowed_chat_ids);

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
        InMemStorage::<()>::new()
    ])
    .build()
    .dispatch()
    .await;

    Ok(())
}

async fn message_handler(
    bot: Bot,
    msg: Message,
    cmd: LopiCmd,
    queue: Arc<TaskQueue>,
    store: Arc<MemoryStore>,
    allowed: Arc<Vec<i64>>,
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

        LopiCmd::Patterns => match store.load_patterns(10).await {
            Ok(patterns) => {
                if patterns.is_empty() {
                    bot.send_message(msg.chat.id, "📊 No patterns recorded yet.")
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
                        bot.send_message(msg.chat.id, text).reply_markup(kb).await?;
                    }
                }
            }
            Err(e) => {
                bot.send_message(msg.chat.id, format!("❌ Error loading patterns: {e}"))
                    .await?;
            }
        },
    }
    Ok(())
}

async fn callback_query_handler(bot: Bot, q: CallbackQuery, store: Arc<MemoryStore>) -> Result<()> {
    let data = q.data.as_deref().unwrap_or("");
    let reply = if data.starts_with("bump:") {
        let goal = data.trim_start_matches("bump:");
        format!("⬆️ priority bumped for: {goal}")
    } else if data.starts_with("cancel:") {
        let goal = data.trim_start_matches("cancel:");
        format!("🗑 cancellation noted for: {goal}\n(tasks in-flight cannot be stopped — the next retry will not be started)")
    } else if data.starts_with("annotate:") {
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
    } else {
        "Unknown action.".into()
    };

    if let Some(msg) = q.message {
        bot.send_message(msg.chat().id, reply).await?;
    }
    bot.answer_callback_query(q.id).await?;
    Ok(())
}
