//! EventBus subscriber loop — pushes Telegram messages when tasks complete.
use lopi_core::{AgentEvent, TaskStatus};
use std::collections::HashMap;
use teloxide::prelude::*;
use tokio::sync::broadcast;
use tracing::warn;

use crate::egress::check_egress;
use crate::telegram::format::short_id;

/// Spawn the notification loop. Returns immediately if `chat_id` is `None`,
/// or — Sprint S2, Phase 4 — if `chat_id` is set but not present in
/// `egress_allowed_chat_ids`: an unset destination and a denied one both
/// mean "no automated sends," logged as a security event in the latter case
/// rather than silently behaving like the former.
///
/// Maintains a local `goal_cache` seeded from `TaskQueued` events so that
/// completion messages can include the task goal even though `TaskCompleted`
/// does not carry it.
pub async fn notify_loop(
    bot: Bot,
    mut rx: broadcast::Receiver<AgentEvent>,
    chat_id: Option<i64>,
    egress_allowed_chat_ids: Vec<i64>,
) {
    let Some(cid) = chat_id else { return };
    if !check_egress(&egress_allowed_chat_ids, cid, "notify_loop") {
        return;
    }
    let tg_chat = ChatId(cid);
    let mut goal_cache: HashMap<String, String> = HashMap::new();

    loop {
        match rx.recv().await {
            Ok(event) => handle_event(&bot, &event, tg_chat, &mut goal_cache).await,
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("telegram notifier lagged {n} events");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn handle_event(
    bot: &Bot,
    event: &AgentEvent,
    chat_id: ChatId,
    goal_cache: &mut HashMap<String, String>,
) {
    match event {
        AgentEvent::TaskQueued { task_id, goal, .. } => {
            goal_cache.insert(task_id.to_string(), goal.clone());
        }

        AgentEvent::TaskStarted {
            task_id,
            attempt,
            branch,
            ..
        } => {
            if *attempt == 1 {
                let id_str = task_id.to_string();
                let goal = goal_cache.get(&id_str).map_or("(unknown)", String::as_str);
                let text = format!("🚀 task started\n{goal}\nBranch: {branch}");
                send_msg(bot, chat_id, &text).await;
            }
        }

        AgentEvent::StatusChanged {
            task_id,
            status,
            attempt,
        } => {
            let id_str = task_id.to_string();
            let goal = goal_cache.get(&id_str).map_or("(unknown)", String::as_str);
            match status {
                TaskStatus::Implementing => {
                    let text = format!("🔧 implementing  ·  {goal}  ·  attempt {attempt}");
                    send_msg(bot, chat_id, &text).await;
                }
                TaskStatus::Testing => {
                    let text = format!("🧪 testing  ·  {goal}");
                    send_msg(bot, chat_id, &text).await;
                }
                _ => {}
            }
        }

        AgentEvent::ScoreUpdated {
            task_id,
            test_pass_rate,
            lint_errors,
            diff_lines,
        } => {
            if *test_pass_rate >= 0.75 {
                let pass_pct = (*test_pass_rate * 100.0) as u32;
                let lint_str = if *lint_errors == 0 {
                    "clean".to_string()
                } else {
                    format!("{lint_errors} errors")
                };
                let _ = task_id; // goal not needed for score notification
                let text = format!(
                    "📊 scored {:.2}  ·  tests: {pass_pct}%  ·  lint: {lint_str}  ·  diff: {diff_lines} lines",
                    test_pass_rate
                );
                send_msg(bot, chat_id, &text).await;
            }
        }

        AgentEvent::TaskCompleted {
            task_id,
            outcome,
            total_attempts,
            ..
        } => {
            let id_str = task_id.to_string();
            let id_short = short_id(&id_str);
            let goal = goal_cache.get(&id_str).map_or("(unknown)", String::as_str);
            let s = if *total_attempts == 1 { "" } else { "s" };
            match outcome {
                TaskStatus::Success { branch, pr_url } => {
                    let text = format!(
                        "✅ task complete — {total_attempts} attempt{s}\n\n{goal}\n\nBranch: {branch}"
                    );
                    send_msg(bot, chat_id, &text).await;
                    if let Some(url) = pr_url {
                        send_msg(bot, chat_id, &format!("🔗 Pull Request\n{url}")).await;
                    } else {
                        send_msg(
                            bot,
                            chat_id,
                            &format!("📦 changes committed to branch {branch}"),
                        )
                        .await;
                    }
                }
                TaskStatus::Failed { reason } => {
                    let text = format!(
                        "❌ task failed after {total_attempts} attempt{s}\n\n{goal}\n\nReason: {reason}\nUse /retry {id_short} to requeue with higher priority"
                    );
                    send_msg(bot, chat_id, &text).await;
                }
                _ => {}
            }
        }

        AgentEvent::TaskCancelled { task_id } => {
            let id_str = task_id.to_string();
            let id_short = short_id(&id_str);
            let goal = goal_cache.get(&id_str).map_or("(unknown)", String::as_str);
            let text = format!("🗑 task cancelled\n{goal}  ·  ID: {id_short}");
            send_msg(bot, chat_id, &text).await;
        }

        AgentEvent::BudgetExceeded {
            scope,
            limit_usd,
            burned_usd,
            ..
        } => {
            let text = format!(
                "⚠️ budget limit hit\nScope: {}  ·  Limit: ${limit_usd:.2}/hr  ·  Burned: ${burned_usd:.2}\nUse /fleet to see current agent costs",
                scope.as_str()
            );
            send_msg(bot, chat_id, &text).await;
        }

        // Budget & Guardrail Controls Part 4.2 — a streamed session's
        // estimated cost crossed 80% of its resolved --max-budget-usd cap.
        // A heads-up, not a refusal: the CLI's own hard stop still applies.
        AgentEvent::BudgetSoftWarn {
            task_id,
            estimated_usd,
            cap_usd,
        } => {
            let id_str = task_id.to_string();
            let goal = goal_cache.get(&id_str).map_or("(unknown)", String::as_str);
            let text = format!(
                "🟡 approaching budget cap\n{goal}\nEstimated: ${estimated_usd:.2}  ·  Cap: ${cap_usd:.2}"
            );
            send_msg(bot, chat_id, &text).await;
        }

        // Report on Finish (Loop Engineering primitive 6) — the summary text
        // is fully rendered by `emit_report`; this just delivers it.
        AgentEvent::ReportReady {
            channel, summary, ..
        } => {
            route_report_ready(bot, chat_id, channel, summary).await;
        }

        // TurnMetrics, LogLine, PoolStats intentionally suppressed — too noisy.
        _ => {}
    }
}

/// Deliver a [`AgentEvent::ReportReady`] summary. Reuses `send_msg`, not a
/// second sender. Known limitation: this always targets the one global
/// `chat_id` this loop was booted with, not a per-task destination — see
/// `LEDGER.md`'s Sprint 3 entry. An unsupported channel is dropped loudly
/// (`warn!`), never silently sent.
async fn route_report_ready(bot: &Bot, chat_id: ChatId, channel: &str, summary: &str) {
    if channel == "telegram" {
        send_msg(bot, chat_id, summary).await;
    } else {
        warn!("report-on-finish: dropping report for unsupported channel `{channel}`");
    }
}

async fn send_msg(bot: &Bot, chat_id: ChatId, text: &str) {
    if let Err(e) = bot.send_message(chat_id, text).await {
        warn!("telegram notify send error: {e}");
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Sprint S2, Phase 4 — a destination outside `egress_allowed_chat_ids`
    /// must never reach the event loop at all: `tx` is kept alive so if
    /// `notify_loop` entered the loop it would block on `rx.recv()`, and the
    /// short timeout would fire instead of the function returning.
    #[tokio::test]
    async fn denied_egress_destination_never_enters_the_loop() {
        let bot = Bot::new("test-token");
        let (_tx, rx) = broadcast::channel::<AgentEvent>(1);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            notify_loop(bot, rx, Some(999), vec![]),
        )
        .await;
        assert!(
            result.is_ok(),
            "denied destination must return immediately, not block on the event loop"
        );
    }

    /// The contrasting case: an allowlisted destination does enter the
    /// event loop and blocks on `rx.recv()` — the timeout fires because
    /// nothing was sent, proving the function didn't just return early.
    #[tokio::test]
    async fn allowed_egress_destination_enters_the_loop() {
        let bot = Bot::new("test-token");
        let (_tx, rx) = broadcast::channel::<AgentEvent>(1);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            notify_loop(bot, rx, Some(111), vec![111]),
        )
        .await;
        assert!(
            result.is_err(),
            "allowed destination should enter the event loop and block on rx.recv()"
        );
    }

    /// `chat_id: None` is the pre-existing "notifications disabled" case —
    /// must still short-circuit the same way regardless of the allowlist.
    #[tokio::test]
    async fn no_chat_id_returns_immediately_regardless_of_allowlist() {
        let bot = Bot::new("test-token");
        let (_tx, rx) = broadcast::channel::<AgentEvent>(1);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            notify_loop(bot, rx, None, vec![111]),
        )
        .await;
        assert!(result.is_ok());
    }

    fn task_completed_success_msg(goal: &str, attempts: u8) -> String {
        let s = if attempts == 1 { "" } else { "s" };
        let branch = "lopi/feature/abc".to_string();
        format!("✅ task complete — {attempts} attempt{s}\n\n{goal}\n\nBranch: {branch}")
    }

    fn task_completed_no_pr_msg(goal: &str, attempts: u8) -> String {
        task_completed_success_msg(goal, attempts)
    }

    fn task_failed_msg(goal: &str, reason: &str, id_short: &str, attempts: u8) -> String {
        let s = if attempts == 1 { "" } else { "s" };
        format!(
            "❌ task failed after {attempts} attempt{s}\n\n{goal}\n\nReason: {reason}\nUse /retry {id_short} to requeue with higher priority"
        )
    }

    fn budget_exceeded_msg(scope: &str, limit: f64, burned: f64) -> String {
        format!(
            "⚠️ budget limit hit\nScope: {scope}  ·  Limit: ${limit:.2}/hr  ·  Burned: ${burned:.2}\nUse /fleet to see current agent costs"
        )
    }

    #[test]
    fn test_task_completed_success_message() {
        let msg = task_completed_success_msg("fix the auth bug", 2);
        assert!(msg.contains("✅ task complete — 2 attempts"));
        assert!(msg.contains("fix the auth bug"));
    }

    #[test]
    fn test_task_completed_no_pr_message() {
        let msg = task_completed_no_pr_msg("fix the auth bug", 1);
        assert!(msg.contains("1 attempt"));
        assert!(!msg.contains("attempts")); // singular
    }

    #[test]
    fn test_task_failed_message() {
        let msg = task_failed_msg("fix the bug", "max retries exceeded", "a3f2c1b8", 3);
        assert!(msg.contains("❌ task failed after 3 attempts"));
        assert!(msg.contains("max retries exceeded"));
        assert!(msg.contains("/retry a3f2c1b8"));
    }

    #[test]
    fn test_budget_exceeded_message() {
        let msg = budget_exceeded_msg("agent", 5.00, 5.12);
        assert!(msg.contains("⚠️ budget limit hit"));
        assert!(msg.contains("agent"));
        assert!(msg.contains("5.00"));
        assert!(msg.contains("5.12"));
    }

    fn budget_soft_warn_msg(goal: &str, estimated: f64, cap: f64) -> String {
        format!("🟡 approaching budget cap\n{goal}\nEstimated: ${estimated:.2}  ·  Cap: ${cap:.2}")
    }

    #[test]
    fn test_budget_soft_warn_message() {
        let msg = budget_soft_warn_msg("fix the auth bug", 4.50, 5.00);
        assert!(msg.contains("🟡 approaching budget cap"));
        assert!(msg.contains("fix the auth bug"));
        assert!(msg.contains("4.50"));
        assert!(msg.contains("5.00"));
    }
}
