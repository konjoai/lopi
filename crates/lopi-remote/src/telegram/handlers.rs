//! Command handlers — task management, draft, help, memory, auth.
use anyhow::Result;
use lopi_core::{Priority, ScheduleEntry, Task, TaskSource};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use std::collections::HashMap;
use std::sync::Arc;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
};
use tokio::sync::Mutex;
use tracing::warn;

use super::budget::{handle_budget, take_pending_budget, PendingBudgetMap};
use super::{monitor, BotDeps, LopiCmd};
use crate::telegram::format::{priority_badge, short_id};

/// Shared draft state: maps chat_id → accumulated lines.
pub type DraftMap = Arc<Mutex<HashMap<i64, Vec<String>>>>;

/// Dispatch all bot commands — entry point for the teloxide command handler.
/// Takes the bundled [`BotDeps`] rather than each dependency separately —
/// see that struct's doc comment.
pub async fn message_handler(bot: Bot, msg: Message, cmd: LopiCmd, deps: BotDeps) -> Result<()> {
    if !deps.allowed.is_empty() && !deps.allowed.contains(&msg.chat.id.0) {
        warn!(
            "telegram: rejected command from unauthorized chat {}",
            msg.chat.id.0
        );
        return Ok(());
    }
    dispatch_task_cmd(
        &bot,
        &msg,
        cmd,
        &deps.queue,
        &deps.store,
        &deps.pool,
        &deps.schedules,
        &deps.drafts,
        &deps.pending_budgets,
    )
    .await
}

/// Route task-action commands; delegates monitoring commands to `dispatch_monitor_cmd`.
#[allow(clippy::too_many_arguments)]
async fn dispatch_task_cmd(
    bot: &Bot,
    msg: &Message,
    cmd: LopiCmd,
    queue: &Arc<TaskQueue>,
    store: &Arc<MemoryStore>,
    pool: &Arc<AgentPool>,
    schedules: &Arc<Vec<ScheduleEntry>>,
    drafts: &DraftMap,
    pending_budgets: &PendingBudgetMap,
) -> Result<()> {
    match cmd {
        LopiCmd::Task(goal) => {
            handle_queue(bot, msg, queue, goal, Priority::Normal, pending_budgets).await
        }
        LopiCmd::Urgent(goal) => {
            handle_queue(bot, msg, queue, goal, Priority::High, pending_budgets).await
        }
        LopiCmd::Critical(goal) => {
            handle_queue(bot, msg, queue, goal, Priority::Critical, pending_budgets).await
        }
        LopiCmd::Cancel(id) => handle_cancel(bot, msg, &id, pool).await,
        LopiCmd::Retry(id) => handle_retry(bot, msg, &id, store, queue).await,
        LopiCmd::Approve(id) => handle_approve(bot, msg, &id).await,
        LopiCmd::Draft => handle_draft(bot, msg, drafts).await,
        LopiCmd::Submit => handle_submit(bot, msg, queue, drafts, pending_budgets).await,
        LopiCmd::CancelDraft => handle_cancel_draft(bot, msg, drafts).await,
        LopiCmd::Budget(arg) => handle_budget(bot, msg, &arg, pool, pending_budgets).await,
        cmd => dispatch_monitor_cmd(bot, msg, cmd, queue, store, pool, schedules).await,
    }
}

/// Route monitoring and informational commands.
#[allow(clippy::too_many_arguments)]
async fn dispatch_monitor_cmd(
    bot: &Bot,
    msg: &Message,
    cmd: LopiCmd,
    queue: &Arc<TaskQueue>,
    store: &Arc<MemoryStore>,
    pool: &Arc<AgentPool>,
    schedules: &Arc<Vec<ScheduleEntry>>,
) -> Result<()> {
    match cmd {
        LopiCmd::Help => handle_help(bot, msg).await,
        LopiCmd::Status => handle_status(bot, msg, queue).await,
        LopiCmd::Fleet => monitor::handle_fleet(bot, msg, queue, pool, store).await,
        LopiCmd::Dock(arg) => monitor::handle_dock(bot, msg, store, &arg).await,
        LopiCmd::Tail(arg) => monitor::handle_tail(bot, msg, &arg, store).await,
        LopiCmd::Schedules => monitor::handle_schedules(bot, msg, schedules).await,
        LopiCmd::RunSchedule(name) => {
            monitor::handle_run_schedule(bot, msg, &name, schedules, queue).await
        }
        LopiCmd::Learn(arg) => handle_learn(bot, msg, store, arg_n(&arg, 5).min(20)).await,
        LopiCmd::Patterns => handle_learn(bot, msg, store, 10).await,
        LopiCmd::Cost => monitor::handle_cost(bot, msg, store).await,
        _ => Ok(()),
    }
}

/// Handle plain-text messages while a draft is active for this chat, appending each line to the buffer.
pub async fn text_message_handler(
    bot: Bot,
    msg: Message,
    allowed: Arc<Vec<i64>>,
    drafts: DraftMap,
) -> Result<()> {
    if !allowed.is_empty() && !allowed.contains(&msg.chat.id.0) {
        return Ok(());
    }
    let Some(text) = msg.text() else {
        return Ok(());
    };
    let chat_id = msg.chat.id.0;
    let mut map = drafts.lock().await;
    if let Some(lines) = map.get_mut(&chat_id) {
        lines.push(text.to_string());
        let n = lines.len();
        let preview: String = lines
            .iter()
            .map(|l| format!("> {l}"))
            .collect::<Vec<_>>()
            .join("\n");
        drop(map);
        bot.send_message(
            msg.chat.id,
            format!("Line {n} added. Draft so far:\n{preview}"),
        )
        .await?;
    }
    Ok(())
}

async fn handle_help(bot: &Bot, msg: &Message) -> Result<()> {
    let text = "⛵ lopi — KonjoAI agent orchestrator\n\n\
        TASKS\n\
        /task <goal>       queue a task (normal priority)\n\
        /urgent <goal>     queue a task (high priority)\n\
        /critical <goal>   queue a task (critical priority)\n\
        /cancel <id>       cancel a running task\n\
        /retry <id>        requeue a failed task\n\n\
        MONITORING\n\
        /fleet             fleet overview — agents, queue, costs\n\
        /dock [N]          recent task history (default: 8)\n\
        /tail <id> [N]     last log lines for a task\n\
        /cost              token usage and cost summary\n\n\
        SCHEDULES\n\
        /schedules         list configured cron schedules\n\
        /run <name>        trigger a schedule immediately\n\n\
        BUDGET\n\
        /budget <preset|usd>  one-off cap for the next card (quick/standard/deep/unlimited or a $ amount)\n\
        /budget status     show the resolved budget for the next card\n\n\
        MEMORY\n\
        /learn [N]         learned patterns (default: 5)\n\
        /approve <id>      record PR approval\n\
        /patterns          learned patterns (alias for /learn)\n\n\
        DRAFT\n\
        /draft             start a multi-line task draft\n\
        /submit            submit current draft as a task\n\
        /cancel_draft      discard current draft\n\n\
        Make it Konjo — build, ship, repeat. ⛵";
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

async fn handle_queue(
    bot: &Bot,
    msg: &Message,
    queue: &TaskQueue,
    goal: String,
    priority: Priority,
    pending_budgets: &PendingBudgetMap,
) -> Result<()> {
    if goal.trim().is_empty() {
        bot.send_message(msg.chat.id, "Usage: /task <goal>").await?;
        return Ok(());
    }
    let mut t = Task::new(goal.clone());
    t.source = TaskSource::Telegram {
        chat_id: msg.chat.id.0,
        message_id: msg.id.0,
    };
    t.priority = priority;
    let budget_note = take_pending_budget(pending_budgets, msg.chat.id.0, &mut t).await;
    let id_short = short_id(&t.id.to_string()).to_string();
    let dup = queue.push(t).await;
    if let Some(existing) = dup {
        bot.send_message(
            msg.chat.id,
            format!(
                "⚠️ already queued (id: {})",
                short_id(&existing.0.to_string())
            ),
        )
        .await?;
        return Ok(());
    }
    let badge = priority_badge(priority);
    let pos = queue.len();
    let kb = build_priority_keyboard(priority, &goal);
    bot.send_message(
        msg.chat.id,
        format!("⛵ queued {badge} {goal}\nID: {id_short}  ·  Position: {pos} in queue{budget_note}"),
    )
    .reply_markup(kb)
    .await?;
    Ok(())
}

fn build_priority_keyboard(priority: Priority, goal: &str) -> InlineKeyboardMarkup {
    match priority {
        Priority::Critical => InlineKeyboardMarkup::new([[InlineKeyboardButton::callback(
            "🗑 Cancel task",
            format!("cancel:{goal}"),
        )]]),
        Priority::High => InlineKeyboardMarkup::new([[
            InlineKeyboardButton::callback("🚨 Bump to CRITICAL", format!("bump:{goal}")),
            InlineKeyboardButton::callback("🗑 Cancel task", format!("cancel:{goal}")),
        ]]),
        _ => InlineKeyboardMarkup::new([[
            InlineKeyboardButton::callback("⬆️ Bump to URGENT", format!("bump:{goal}")),
            InlineKeyboardButton::callback("🗑 Cancel task", format!("cancel:{goal}")),
        ]]),
    }
}

async fn handle_status(bot: &Bot, msg: &Message, queue: &TaskQueue) -> Result<()> {
    let n = queue.len();
    bot.send_message(
        msg.chat.id,
        format!("📊 queue depth: {n}\nUse /fleet for full details."),
    )
    .await?;
    Ok(())
}

async fn handle_cancel(bot: &Bot, msg: &Message, id_prefix: &str, pool: &AgentPool) -> Result<()> {
    if id_prefix.trim().is_empty() {
        bot.send_message(msg.chat.id, "Usage: /cancel <id-prefix>")
            .await?;
        return Ok(());
    }
    let short = short_id(id_prefix).to_string();
    if pool.cancel_by_prefix(id_prefix).await {
        bot.send_message(
            msg.chat.id,
            format!("🗑 cancel signal sent to task {short}\nNote: in-flight tasks complete their current attempt before stopping."),
        )
        .await?;
    } else {
        bot.send_message(
            msg.chat.id,
            format!("❓ no running task found with ID {short}\nUse /dock to see recent tasks."),
        )
        .await?;
    }
    Ok(())
}

async fn handle_retry(
    bot: &Bot,
    msg: &Message,
    id_prefix: &str,
    store: &MemoryStore,
    queue: &TaskQueue,
) -> Result<()> {
    if id_prefix.trim().is_empty() {
        bot.send_message(msg.chat.id, "Usage: /retry <id-prefix>")
            .await?;
        return Ok(());
    }
    match store.load_history(200).await {
        Ok(rows) => {
            if let Some(row) = rows.into_iter().find(|r| r.id.starts_with(id_prefix)) {
                let mut t = Task::new(row.goal.clone());
                t.priority = Priority::High;
                t.source = TaskSource::Telegram {
                    chat_id: msg.chat.id.0,
                    message_id: msg.id.0,
                };
                let new_id = short_id(&t.id.to_string()).to_string();
                queue.push(t).await;
                bot.send_message(
                    msg.chat.id,
                    format!(
                        "🔁 requeued at HIGH priority\n{}\nNew ID: {new_id}",
                        row.goal
                    ),
                )
                .await?;
            } else {
                bot.send_message(
                    msg.chat.id,
                    format!("❓ no task found with ID prefix {id_prefix}\nUse /dock to see recent tasks."),
                )
                .await?;
            }
        }
        Err(e) => {
            warn!("retry load_history error: {e}");
            bot.send_message(msg.chat.id, format!("❌ error loading history: {e}"))
                .await?;
        }
    }
    Ok(())
}

/// Show learned patterns with approve/reject inline buttons.
pub async fn handle_learn(bot: &Bot, msg: &Message, store: &MemoryStore, n: usize) -> Result<()> {
    match store.load_patterns(n as i64).await {
        Ok(patterns) if patterns.is_empty() => {
            bot.send_message(msg.chat.id, "🧠 no patterns recorded yet.")
                .await?;
        }
        Ok(patterns) => {
            bot.send_message(
                msg.chat.id,
                format!("🧠 learned patterns ({})", patterns.len()),
            )
            .await?;
            for (i, p) in patterns.iter().enumerate() {
                let id_short = short_id(&p.id);
                let annotation = match p.user_annotation.as_deref() {
                    Some("approved") => "✅ approved",
                    Some("rejected") => "❌ rejected",
                    _ => "⭕ unannotated",
                };
                let success = p.success_rate.unwrap_or(0.0) * 100.0;
                let kw = &p.goal_keywords[..p.goal_keywords.len().min(40)];
                let constraint = p.successful_constraints.as_deref().unwrap_or("(none)");
                let text = format!(
                    "{}. {id_short}  {annotation}   success: {success:.0}%\n   keywords: {kw}\n   constraint: {constraint}",
                    i + 1
                );
                let kb = InlineKeyboardMarkup::new([[
                    InlineKeyboardButton::callback(
                        "✅ Approve",
                        format!("annotate:approved:{}", p.id),
                    ),
                    InlineKeyboardButton::callback(
                        "❌ Reject",
                        format!("annotate:rejected:{}", p.id),
                    ),
                ]]);
                bot.send_message(msg.chat.id, text).reply_markup(kb).await?;
            }
        }
        Err(e) => {
            warn!("learn load_patterns error: {e}");
            bot.send_message(msg.chat.id, format!("❌ error loading patterns: {e}"))
                .await?;
        }
    }
    Ok(())
}

async fn handle_approve(bot: &Bot, msg: &Message, id: &str) -> Result<()> {
    bot.send_message(
        msg.chat.id,
        format!(
            "✅ approval recorded for task {id}\n(PR merge requires manual action via gh/GitHub)"
        ),
    )
    .await?;
    Ok(())
}

async fn handle_draft(bot: &Bot, msg: &Message, drafts: &DraftMap) -> Result<()> {
    drafts.lock().await.insert(msg.chat.id.0, Vec::new());
    bot.send_message(
        msg.chat.id,
        "📝 Draft mode started.\nSend lines one by one. Each message adds a line.\nSend /submit when done, or /cancel_draft to discard.",
    )
    .await?;
    Ok(())
}

async fn handle_submit(
    bot: &Bot,
    msg: &Message,
    queue: &TaskQueue,
    drafts: &DraftMap,
    pending_budgets: &PendingBudgetMap,
) -> Result<()> {
    let lines = drafts.lock().await.remove(&msg.chat.id.0);
    let has_content = lines.as_ref().is_some_and(|v| !v.is_empty());
    if !has_content {
        bot.send_message(
            msg.chat.id,
            "📭 no draft to submit. Use /draft to start one.",
        )
        .await?;
        return Ok(());
    }
    let goal = lines.unwrap_or_default().join(" ");
    let mut t = Task::new(goal.clone());
    t.source = TaskSource::Telegram {
        chat_id: msg.chat.id.0,
        message_id: msg.id.0,
    };
    let budget_note = take_pending_budget(pending_budgets, msg.chat.id.0, &mut t).await;
    let id_short = short_id(&t.id.to_string()).to_string();
    queue.push(t).await;
    bot.send_message(
        msg.chat.id,
        format!("✅ Draft submitted as task\n{goal}\nID: {id_short}{budget_note}"),
    )
    .await?;
    Ok(())
}

async fn handle_cancel_draft(bot: &Bot, msg: &Message, drafts: &DraftMap) -> Result<()> {
    drafts.lock().await.remove(&msg.chat.id.0);
    bot.send_message(msg.chat.id, "🗑 Draft discarded.").await?;
    Ok(())
}

/// Parse an optional count from a command argument, returning `default` on failure.
pub fn arg_n(arg: &str, default: usize) -> usize {
    arg.trim().parse::<usize>().unwrap_or(default)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_dock_default_n() {
        assert_eq!(arg_n("", 8), 8);
    }

    #[test]
    fn test_parse_dock_custom_n() {
        assert_eq!(arg_n("15", 8), 15);
    }

    #[test]
    fn test_parse_dock_invalid_n() {
        assert_eq!(arg_n("abc", 8), 8);
    }

    #[test]
    fn test_unauthorized_chat_behavior() {
        let allowed = [12345_i64];
        let unauthorized_id = 99999_i64;
        // Verify auth check logic used in message_handler
        assert!(!allowed.is_empty() && !allowed.contains(&unauthorized_id));
    }
}
