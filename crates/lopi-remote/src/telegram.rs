use anyhow::Result;
use lopi_core::{Task, TaskSource};
use lopi_orchestrator::TaskQueue;
use teloxide::{prelude::*, utils::command::BotCommands};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "lopi commands:")]
pub enum LopiCmd {
    #[command(description = "show this help")]
    Help,
    #[command(description = "queue a new task: /task <goal>")]
    Task(String),
    #[command(description = "show queue depth and recent activity")]
    Status,
    #[command(description = "approve a pending PR by id")]
    Approve(String),
}

/// Start the Telegram bot. Requires `TELOXIDE_TOKEN` (or pass `token`).
pub async fn run(token: String, queue: TaskQueue) -> Result<()> {
    let bot = Bot::new(token);
    LopiCmd::repl(bot, move |bot: Bot, msg: Message, cmd: LopiCmd| {
        let queue = queue.clone();
        async move {
            match cmd {
                LopiCmd::Help => {
                    bot.send_message(msg.chat.id, LopiCmd::descriptions().to_string()).await?;
                }
                LopiCmd::Task(goal) => {
                    let mut t = Task::new(goal.clone());
                    t.source = TaskSource::Telegram {
                        chat_id: msg.chat.id.0,
                        message_id: msg.id.0,
                    };
                    queue.push(t).await;
                    bot.send_message(msg.chat.id, format!("🚢 queued: {goal}")).await?;
                }
                LopiCmd::Status => {
                    let n = queue.len();
                    bot.send_message(msg.chat.id, format!("queue depth: {n}")).await?;
                }
                LopiCmd::Approve(id) => {
                    bot.send_message(msg.chat.id, format!("approval recorded for {id}")).await?;
                }
            }
            respond(())
        }
    })
    .await;
    Ok(())
}
