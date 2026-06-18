//! Monitoring and display handlers — fleet, dock, tail, cost, schedules.
use anyhow::Result;
use lopi_core::{Priority, ScheduleEntry, Task, TaskSource};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use teloxide::prelude::*;
use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use tracing::warn;

use super::handlers::arg_n;
use crate::telegram::format::{
    format_uptime, priority_badge, relative_time, short_id, status_emoji,
};

/// Send a fleet overview: running agents, queued tasks, stats, and daily cost.
pub async fn handle_fleet(
    bot: &Bot,
    msg: &Message,
    queue: &TaskQueue,
    pool: &AgentPool,
    store: &MemoryStore,
) -> Result<()> {
    let stats = pool.stats();
    let running_agents = pool.running_agents();
    let queued_items = queue.peek_queued();
    let (tokens, cost) = store.daily_token_totals().await.unwrap_or((0, 0.0));
    let uptime = format_uptime(stats.uptime_secs);
    let now = chrono::Utc::now().format("%H:%M UTC");
    let mut lines = vec![format!("⛵ lopi fleet — {now}\n")];

    lines.push(format!("🔥 RUNNING ({})", running_agents.len()));
    if running_agents.is_empty() {
        lines.push("  (none)".to_string());
    } else {
        for a in &running_agents {
            let id = short_id(&a.task_id);
            let goal_preview = &a.goal[..a.goal.len().min(35)];
            lines.push(format!("  • {id}  {goal_preview}  attempt {}", a.attempt));
        }
    }

    lines.push(format!("\n📬 QUEUED ({})", queued_items.len()));
    if queued_items.is_empty() {
        lines.push("  (none)".to_string());
    } else {
        for (i, (prio, goal)) in queued_items.iter().take(5).enumerate() {
            let preview = &goal[..goal.len().min(40)];
            lines.push(format!("  {}. {}  {preview}", i + 1, priority_badge(*prio)));
        }
        if queued_items.len() > 5 {
            lines.push(format!("  … and {} more", queued_items.len() - 5));
        }
    }

    lines.push(format!(
        "\n📊 TOTALS\n  ✅ succeeded: {}    ❌ failed: {}    ⏱ uptime: {uptime}",
        stats.succeeded, stats.failed
    ));
    lines.push(format!(
        "\n💰 TODAY\n  tokens: {tokens}    cost: ${cost:.2}"
    ));

    let kb = InlineKeyboardMarkup::new([[
        InlineKeyboardButton::callback("🔄 Refresh", "fleet_refresh"),
        InlineKeyboardButton::callback("🚢 Dock", "fleet_dock"),
    ]]);
    bot.send_message(msg.chat.id, lines.join("\n"))
        .reply_markup(kb)
        .await?;
    Ok(())
}

/// Show recent task history; `arg` is an optional count (defaults to 8, max 20).
pub async fn handle_dock(bot: &Bot, msg: &Message, store: &MemoryStore, arg: &str) -> Result<()> {
    let n = arg_n(arg, 8).min(20) as i64;
    match store.load_history(n).await {
        Ok(rows) if rows.is_empty() => {
            bot.send_message(msg.chat.id, "🚢 no tasks recorded yet.")
                .await?;
        }
        Ok(rows) => {
            let mut lines = vec![format!("🚢 recent tasks ({})\n", rows.len())];
            for row in &rows {
                let emoji = status_emoji(&row.status);
                let goal_preview = &row.goal[..row.goal.len().min(40)];
                let when = row
                    .created_at
                    .parse::<chrono::DateTime<chrono::Utc>>()
                    .map(relative_time)
                    .unwrap_or_else(|_| row.created_at.clone());
                let id_short = short_id(&row.id);
                let verdict = verdict_marker(store, &row.id).await;
                lines.push(format!(
                    "{emoji}  {goal_preview:<40}  {when}  {id_short}{verdict}"
                ));
            }
            bot.send_message(msg.chat.id, lines.join("\n")).await?;
        }
        Err(e) => {
            warn!("dock load_history error: {e}");
            bot.send_message(msg.chat.id, format!("❌ failed to load history: {e}"))
                .await?;
        }
    }
    Ok(())
}

/// Konjo Verifier marker for a task's latest verdict, for inline display in
/// `/dock`. Empty when no verdict exists or the lookup fails — the dock listing
/// must stay resilient to a missing verifier row.
async fn verdict_marker(store: &MemoryStore, task_id: &str) -> &'static str {
    match store.load_verifier_verdicts(task_id).await {
        Ok(rows) => match rows.last() {
            Some(row) if row.passed != 0 => "  🔬✅",
            Some(_) => "  🔬❌",
            None => "",
        },
        Err(e) => {
            warn!("dock verdict lookup error for {task_id}: {e}");
            ""
        }
    }
}

/// Show the last N log lines for a task identified by ID prefix.
pub async fn handle_tail(bot: &Bot, msg: &Message, arg: &str, store: &MemoryStore) -> Result<()> {
    let (id_prefix, n) = parse_tail_arg(arg);
    if id_prefix.is_empty() {
        bot.send_message(msg.chat.id, "Usage: /tail <id-prefix> [N]")
            .await?;
        return Ok(());
    }
    let rows = match store.load_history(200).await {
        Ok(r) => r,
        Err(e) => {
            warn!("tail load_history error: {e}");
            bot.send_message(msg.chat.id, format!("❌ error: {e}"))
                .await?;
            return Ok(());
        }
    };
    let Some(row) = rows.into_iter().find(|r| r.id.starts_with(id_prefix)) else {
        bot.send_message(
            msg.chat.id,
            format!("❓ task {id_prefix} not found. Use /dock to list tasks."),
        )
        .await?;
        return Ok(());
    };
    match store.load_task_logs(&row.id, n as i64).await {
        Ok(logs) if logs.is_empty() => {
            bot.send_message(
                msg.chat.id,
                format!("📭 no logs for task {id_prefix} — it may not have started yet."),
            )
            .await?;
        }
        Ok(logs) => {
            let id_short = short_id(&row.id);
            let goal_preview = &row.goal[..row.goal.len().min(30)];
            let mut lines = vec![format!(
                "📜 last {} lines — {goal_preview} ({id_short})\n",
                logs.len()
            )];
            for log in &logs {
                lines.push(format!(
                    "[{}] {}  {}",
                    &log.ts[..19.min(log.ts.len())],
                    log.level.to_uppercase(),
                    log.line
                ));
            }
            bot.send_message(msg.chat.id, lines.join("\n")).await?;
        }
        Err(e) => {
            warn!("tail load_task_logs error: {e}");
            bot.send_message(msg.chat.id, format!("❌ error loading logs: {e}"))
                .await?;
        }
    }
    Ok(())
}

/// Send the daily token usage and cost summary.
pub async fn handle_cost(bot: &Bot, msg: &Message, store: &MemoryStore) -> Result<()> {
    let (tokens, cost) = store.daily_token_totals().await.unwrap_or((0, 0.0));
    let total_tasks = store.task_count().await.unwrap_or(0);
    let text = format!(
        "💰 lopi cost summary\n\nTODAY\n  tokens:  {tokens}\n  cost:    ${cost:.2}\n\nALL TIME\n  tasks:   {total_tasks} completed\n\nBudget limits:\n  fleet:  $25.00/hr    agent:  $5.00/hr    task:  $1.50/hr"
    );
    bot.send_message(msg.chat.id, text).await?;
    Ok(())
}

/// List all configured cron schedules with their next-run times.
pub async fn handle_schedules(bot: &Bot, msg: &Message, schedules: &[ScheduleEntry]) -> Result<()> {
    if schedules.is_empty() {
        bot.send_message(msg.chat.id, "🗓 no schedules configured.")
            .await?;
        return Ok(());
    }
    let mut lines = vec![format!("🗓 schedules ({})\n", schedules.len())];
    for s in schedules {
        let next = lopi_orchestrator::next_run_times(&s.cron, 1);
        let next_str = next.first().map_or_else(
            || "unknown".to_string(),
            |t| format!("{}", t.format("%a %H:%M UTC")),
        );
        let goal_preview = &s.goal[..s.goal.len().min(50)];
        lines.push(format!(
            "📅 {}\n   goal: {goal_preview}\n   cron: {}\n   next: {next_str}\n   priority: {}\n",
            s.name,
            s.cron,
            s.priority.to_uppercase()
        ));
    }
    bot.send_message(msg.chat.id, lines.join("\n")).await?;
    Ok(())
}

/// Trigger a named schedule immediately, pushing its task onto the queue.
pub async fn handle_run_schedule(
    bot: &Bot,
    msg: &Message,
    name: &str,
    schedules: &[ScheduleEntry],
    queue: &TaskQueue,
) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        bot.send_message(msg.chat.id, "Usage: /run <schedule-name>")
            .await?;
        return Ok(());
    }
    if let Some(entry) = schedules.iter().find(|s| s.name == name) {
        let mut t = Task::new(entry.goal.clone());
        t.source = TaskSource::Telegram {
            chat_id: msg.chat.id.0,
            message_id: msg.id.0,
        };
        t.priority = match entry.priority.as_str() {
            "low" => Priority::Low,
            "high" => Priority::High,
            "critical" => Priority::Critical,
            _ => Priority::Normal,
        };
        let id_short = short_id(&t.id.to_string()).to_string();
        queue.push(t).await;
        let goal_preview = &entry.goal[..entry.goal.len().min(50)];
        bot.send_message(
            msg.chat.id,
            format!(
                "▶️ {name} triggered now\n{goal_preview}\nID: {id_short}  ·  priority: {}",
                entry.priority.to_uppercase()
            ),
        )
        .await?;
    } else {
        bot.send_message(
            msg.chat.id,
            format!("❓ no schedule named \"{name}\"\nUse /schedules to see configured schedules."),
        )
        .await?;
    }
    Ok(())
}

/// Parse `/tail <id> [N]` argument string.
pub fn parse_tail_arg(arg: &str) -> (&str, usize) {
    let parts: Vec<&str> = arg.trim().splitn(2, ' ').collect();
    let id_prefix = parts.first().copied().unwrap_or("");
    let n = parts
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_usize)
        .min(30);
    (id_prefix, n)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_tail_id_only() {
        let (id, n) = parse_tail_arg("a3f2c1");
        assert_eq!(id, "a3f2c1");
        assert_eq!(n, 10);
    }

    #[test]
    fn test_parse_tail_id_and_n() {
        let (id, n) = parse_tail_arg("a3f2c1 25");
        assert_eq!(id, "a3f2c1");
        assert_eq!(n, 25);
    }

    #[test]
    fn test_parse_tail_empty() {
        let (id, _n) = parse_tail_arg("");
        assert_eq!(id, "");
    }

    #[test]
    fn test_parse_run_schedule_name() {
        let name = "  nightly-lint  ";
        assert_eq!(name.trim(), "nightly-lint");
    }
}
