//! Self-modification automation: diagnosis and Telegram approval gate.
//!
//! Safety invariants enforced here (not by caller):
//! - `allowed_dirs` is always `["crates/", "src/"]` for every self-modify task.
//! - If approval times out (120 s), the task is NOT queued.
use anyhow::Result;
use lopi_core::{Priority, Task, TaskSource};
use lopi_memory::MemoryStore;
use lopi_orchestrator::TaskQueue;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use teloxide::{
    prelude::*,
    types::{InlineKeyboardButton, InlineKeyboardMarkup},
};
use tokio::sync::oneshot;
use tokio::sync::Mutex;
use tokio::time::timeout;

/// Pending self-modify approvals keyed by `chat_id` so concurrent proposals
/// from different chats cannot interfere with each other.
pub type PendingSelfModify = Arc<Mutex<HashMap<i64, (String, oneshot::Sender<bool>)>>>;

/// Minimum post-mortem pattern count required to trigger a self-improve proposal.
pub const POSTMORTEM_THRESHOLD: i64 = 3;

/// Look-back window (hours) when counting recent post-mortem patterns.
pub const POSTMORTEM_WINDOW_HOURS: i64 = 24;

/// Approval gate timeout in seconds.
const APPROVAL_TIMEOUT_SECS: u64 = 120;

/// Callback data prefix for self-modify approval.
pub const SELF_MODIFY_APPROVE: &str = "selfmod:yes";
/// Callback data prefix for self-modify rejection.
pub const SELF_MODIFY_REJECT: &str = "selfmod:no";

/// Examines the memory store and returns a goal string for a self-improvement
/// task when there are enough recent post-mortem patterns, or `None` otherwise.
///
/// # Errors
/// Returns an error if the memory store queries fail.
pub async fn self_diagnose(store: &MemoryStore) -> Result<Option<String>> {
    let count = store
        .recent_postmortem_count(POSTMORTEM_WINDOW_HOURS)
        .await?;
    if count < POSTMORTEM_THRESHOLD {
        return Ok(None);
    }

    let failures = store.recent_failures(5).await?;
    if failures.is_empty() {
        return Ok(None);
    }

    let summary = failures
        .iter()
        .enumerate()
        .map(|(i, g)| format!("{}. {}", i + 1, g))
        .collect::<Vec<_>>()
        .join("; ");

    let goal = format!(
        "Self-improve: address {} repeated failure pattern(s) detected in the last {} h. \
         Recent failures: {summary}",
        count, POSTMORTEM_WINDOW_HOURS
    );
    Ok(Some(goal))
}

/// Queues a self-modify task after building it with the required safety constraints.
///
/// `allowed_dirs` is hardcoded to `["crates/", "src/"]` regardless of what the
/// caller passes — this is intentional.
pub fn build_self_modify_task(goal: &str, approver_chat_id: i64) -> Task {
    let mut t = Task::new(goal);
    t.priority = Priority::Normal;
    t.source = TaskSource::SelfModify {
        approved_by: format!("telegram:{approver_chat_id}"),
    };
    t.allowed_dirs = vec!["crates/".into(), "src/".into()];
    t
}

/// Queue a self-modify task if the operator approves via Telegram.
///
/// This is the high-level entry point called by the `/self-improve` command
/// handler and by the post-mortem threshold trigger.
///
/// # Errors
/// Returns an error if sending the Telegram message fails.
pub async fn propose_and_await(
    bot: &Bot,
    chat_id: ChatId,
    goal: &str,
    queue: &Arc<TaskQueue>,
    pending: &PendingSelfModify,
) -> Result<()> {
    let (tx, rx) = oneshot::channel::<bool>();

    // Register pending approval keyed by chat_id so multi-chat races can't cross-resolve.
    {
        let mut guard = pending.lock().await;
        guard.insert(chat_id.0, (goal.to_string(), tx));
    }

    let kb = InlineKeyboardMarkup::new([[
        InlineKeyboardButton::callback("✅ Yes, self-improve", SELF_MODIFY_APPROVE),
        InlineKeyboardButton::callback("❌ No / cancel", SELF_MODIFY_REJECT),
    ]]);

    bot.send_message(
        chat_id,
        format!(
            "🤖 Self-Improvement Proposal\n\n\
             {goal}\n\n\
             Approve queuing a self-modify task?\n\
             Expires in {APPROVAL_TIMEOUT_SECS}s — allowed dirs: crates/, src/"
        ),
    )
    .reply_markup(kb)
    .await?;

    match timeout(Duration::from_secs(APPROVAL_TIMEOUT_SECS), rx).await {
        Ok(Ok(true)) => {
            let task = build_self_modify_task(goal, chat_id.0);
            queue.push(task).await;
            bot.send_message(chat_id, "✅ Self-modify task queued.")
                .await?;
        }
        Ok(Ok(false)) | Ok(Err(_)) => {
            bot.send_message(chat_id, "❌ Self-modify proposal rejected.")
                .await?;
        }
        Err(_) => {
            // Timeout — clear this chat's pending entry and inform the user.
            let mut guard = pending.lock().await;
            guard.remove(&chat_id.0);
            bot.send_message(
                chat_id,
                format!("⏰ Self-modify proposal expired after {APPROVAL_TIMEOUT_SECS}s."),
            )
            .await?;
            tracing::warn!("self-improve approval timed out for chat {}", chat_id.0);
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use lopi_memory::MemoryStore;

    #[tokio::test]
    async fn self_diagnose_empty_store_returns_none() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        let result = self_diagnose(&store).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn self_diagnose_with_few_patterns_returns_none() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        // Only 2 patterns — below threshold of 3
        for i in 0..2 {
            store
                .insert_postmortem_pattern(&format!("goal {i}"), "constraint")
                .await
                .unwrap();
        }
        let result = self_diagnose(&store).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn self_diagnose_with_patterns_but_no_failures_returns_none() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        // 3 patterns but no failed task records — recent_failures returns empty
        for i in 0..3 {
            store
                .insert_postmortem_pattern(&format!("goal {i}"), "fix constraint")
                .await
                .unwrap();
        }
        let result = self_diagnose(&store).await.unwrap();
        // patterns >= threshold but failures list is empty → None
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn self_diagnose_returns_some_when_threshold_met_and_failures_exist() {
        let store = MemoryStore::open_in_memory().await.unwrap();
        // Insert POSTMORTEM_THRESHOLD post-mortem patterns.
        for i in 0..POSTMORTEM_THRESHOLD {
            store
                .insert_postmortem_pattern(&format!("keyword goal {i}"), "constraint")
                .await
                .unwrap();
        }
        // Insert a failed task so recent_failures returns non-empty.
        let task = lopi_core::Task::new("some failing task unique-abc");
        store.save_task(&task, "queued").await.unwrap();
        store.mark_completed(&task.id, "failed").await.unwrap();

        let result = self_diagnose(&store).await.unwrap();
        assert!(
            result.is_some(),
            "expected Some when patterns >= threshold and failures exist"
        );
        let goal = result.unwrap();
        assert!(
            goal.starts_with("Self-improve:"),
            "goal should start with 'Self-improve:'"
        );
    }

    #[tokio::test]
    async fn build_self_modify_task_enforces_allowed_dirs() {
        let task = build_self_modify_task("test goal", 12345);
        assert_eq!(
            task.allowed_dirs,
            vec!["crates/".to_string(), "src/".to_string()]
        );
        assert!(matches!(task.source, TaskSource::SelfModify { .. }));
    }

    #[tokio::test]
    async fn build_self_modify_task_sets_approver() {
        let task = build_self_modify_task("improve thing", 99999);
        match &task.source {
            TaskSource::SelfModify { approved_by } => {
                assert_eq!(approved_by, "telegram:99999");
            }
            _ => panic!("expected SelfModify source"),
        }
    }
}
