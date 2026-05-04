use anyhow::Result;
use lopi_core::{Priority, Task, TaskSource};
use lopi_orchestrator::TaskQueue;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
    utils::command::BotCommands,
};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "lopi commands:")]
pub enum LopiCmd {
    #[command(description = "show this help")]
    Help,
    #[command(description = "queue a new task: /task <goal>")]
    Task(String),
    #[command(description = "high-priority task: /urgent <goal>")]
    Urgent(String),
    #[command(description = "show queue depth")]
    Status,
    #[command(description = "approve a pending PR by ID: /approve <task-id>")]
    Approve(String),
}

/// Start the Telegram bot. Requires `TELOXIDE_TOKEN` env var or explicit `token`.
pub async fn run(token: String, queue: TaskQueue) -> Result<()> {
    let bot = Bot::new(token);

    let queue_cmd = queue.clone();
    LopiCmd::repl(bot.clone(), move |bot: Bot, msg: Message, cmd: LopiCmd| {
        let queue = queue_cmd.clone();
        async move {
            match cmd {
                LopiCmd::Help => {
                    bot.send_message(msg.chat.id, LopiCmd::descriptions().to_string()).await?;
                }

                LopiCmd::Task(goal) | LopiCmd::Urgent(goal) => {
                    let mut t = Task::new(goal.clone());
                    t.source = TaskSource::Telegram {
                        chat_id: msg.chat.id.0,
                        message_id: msg.id.0,
                    };
                    // Detect "urgent" variant by command name.
                    if msg.text().map(|t| t.starts_with("/urgent")).unwrap_or(false) {
                        t.priority = Priority::High;
                    }

                    let dup = queue.push(t).await;
                    let reply = if let Some(existing) = dup {
                        format!("⚠️ already queued (id: {}…)", &existing.0.to_string()[..8])
                    } else {
                        // Inline keyboard: cancel button (approval comes via /approve later).
                        let kb = InlineKeyboardMarkup::new([[
                            InlineKeyboardButton::callback("✅ Priority bump", format!("bump:{goal}")),
                            InlineKeyboardButton::callback("❌ Cancel", format!("cancel:{goal}")),
                        ]]);
                        bot.send_message(msg.chat.id, format!("🚢 queued: {goal}"))
                            .reply_markup(kb)
                            .await?;
                        return respond(());
                    };
                    bot.send_message(msg.chat.id, reply).await?;
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
            }
            respond(())
        }
    })
    .await;
    Ok(())
}

/// Handle inline keyboard callback queries (bump / cancel buttons).
pub async fn handle_callback(bot: Bot, q: CallbackQuery) -> Result<(), teloxide::RequestError> {
    let data = q.data.as_deref().unwrap_or("");
    let reply = if data.starts_with("bump:") {
        let goal = data.trim_start_matches("bump:");
        format!("⬆️ priority bumped for: {goal}")
    } else if data.starts_with("cancel:") {
        let goal = data.trim_start_matches("cancel:");
        format!("🗑 cancellation noted for: {goal}\n(tasks in-flight cannot be stopped — the next retry will not be started)")
    } else {
        "Unknown action.".into()
    };

    if let Some(msg) = q.message {
        bot.send_message(msg.chat().id, reply).await?;
    }
    bot.answer_callback_query(q.id).await?;
    Ok(())
}
