//! Multi-line task draft: `/draft` accumulates lines, `/submit` queues them
//! as one task, `/cancel_draft` discards.
use anyhow::Result;
use lopi_core::{Task, TaskSource};
use lopi_orchestrator::TaskQueue;
use std::collections::HashMap;
use std::sync::Arc;
use teloxide::prelude::*;
use tokio::sync::Mutex;

use super::budget::{take_pending_budget, PendingBudgetMap};
use crate::telegram::format::short_id;

/// Shared draft state: maps chat_id → accumulated lines.
pub type DraftMap = Arc<Mutex<HashMap<i64, Vec<String>>>>;

pub(super) async fn handle_draft(bot: &Bot, msg: &Message, drafts: &DraftMap) -> Result<()> {
    drafts.lock().await.insert(msg.chat.id.0, Vec::new());
    bot.send_message(
        msg.chat.id,
        "📝 Draft mode started.\nSend lines one by one. Each message adds a line.\nSend /submit when done, or /cancel_draft to discard.",
    )
    .await?;
    Ok(())
}

pub(super) async fn handle_submit(
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

pub(super) async fn handle_cancel_draft(bot: &Bot, msg: &Message, drafts: &DraftMap) -> Result<()> {
    drafts.lock().await.remove(&msg.chat.id.0);
    bot.send_message(msg.chat.id, "🗑 Draft discarded.").await?;
    Ok(())
}
