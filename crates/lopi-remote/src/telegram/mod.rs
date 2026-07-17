//! Telegram bot for lopi — full remote control surface.
#![allow(clippy::missing_errors_doc)]

use anyhow::Result;
use lopi_core::{AgentEvent, EventBus, ScheduleEntry};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use std::collections::HashMap;
use std::sync::Arc;
use teloxide::{dispatching::UpdateHandler, prelude::*, utils::command::BotCommands};
use tokio::sync::Mutex;

pub mod budget;
pub mod callbacks;
pub mod format;
pub mod handlers;
pub mod monitor;
pub mod notify;

use budget::PendingBudgetMap;
use handlers::{message_handler, text_message_handler, DraftMap};

/// Bundled bot dependencies injected into every command handler.
///
/// teloxide's `dptree::Injectable` only supports a bounded number of
/// separately-injected parameters — bundling everything the handlers share
/// (queue/store/pool/allowed-list/schedules/draft state/pending-budget
/// state) into one `Clone` struct keeps `message_handler` well under that
/// bound regardless of how many more remote-control features add shared
/// state later. Every field is already cheap to clone (`Arc`/`Arc<Mutex<_>>`).
#[derive(Clone)]
pub struct BotDeps {
    pub(crate) queue: Arc<TaskQueue>,
    pub(crate) store: Arc<MemoryStore>,
    pub(crate) pool: Arc<AgentPool>,
    pub(crate) allowed: Arc<Vec<i64>>,
    pub(crate) schedules: Arc<Vec<ScheduleEntry>>,
    pub(crate) drafts: DraftMap,
    pub(crate) pending_budgets: PendingBudgetMap,
}

/// All commands accepted by the lopi Telegram bot.
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "lopi commands:")]
pub enum LopiCmd {
    /// Show the full command reference.
    #[command(description = "show command reference")]
    Help,
    /// Queue a normal-priority task.
    #[command(description = "queue a task: /task <goal>")]
    Task(String),
    /// Queue a high-priority task.
    #[command(description = "high-priority task: /urgent <goal>")]
    Urgent(String),
    /// Queue a critical-priority task.
    #[command(description = "critical priority task: /critical <goal>")]
    Critical(String),
    /// Show queue depth.
    #[command(description = "show queue depth")]
    Status,
    /// Fleet overview.
    #[command(description = "fleet overview — agents, queue, costs")]
    Fleet,
    /// Recent task history.
    #[command(description = "recent tasks: /dock or /dock 15")]
    Dock(String),
    /// Cancel a running task.
    #[command(description = "cancel a task: /cancel <id>")]
    Cancel(String),
    /// Retry a failed task.
    #[command(description = "retry a failed task: /retry <id>")]
    Retry(String),
    /// List configured schedules.
    #[command(description = "list cron schedules")]
    Schedules,
    /// Trigger a schedule immediately.
    #[command(description = "trigger a schedule: /run <name>", rename = "run")]
    RunSchedule(String),
    /// Last N log lines for a task.
    #[command(description = "last log lines: /tail <id> [N]")]
    Tail(String),
    /// Learned patterns.
    #[command(description = "learned patterns: /learn or /learn 5")]
    Learn(String),
    /// Approve a pending PR.
    #[command(description = "approve a pending PR: /approve <task-id>")]
    Approve(String),
    /// List patterns (alias for /learn).
    #[command(description = "list recent patterns")]
    Patterns,
    /// Token and cost summary.
    #[command(description = "token usage and cost summary")]
    Cost,
    /// Start a multi-line task draft.
    #[command(description = "start a multi-line task draft")]
    Draft,
    /// Submit the current draft as a task.
    #[command(description = "submit draft as a task")]
    Submit,
    /// Discard the current draft.
    #[command(description = "discard current draft", rename = "cancel_draft")]
    CancelDraft,
    /// Set a one-off budget override for the next queued card, or show it.
    #[command(
        description = "budget override for the next card: /budget <preset|usd> or /budget status"
    )]
    Budget(String),
}

/// Start the Telegram bot.
///
/// `allowed_chat_ids`: allowlist of chat IDs permitted to issue commands.
/// Empty = allow all chats (dev mode).
///
/// `notify_chat_id`: single chat ID to receive completion notifications.
/// `None` disables outbound notifications.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    token: String,
    queue: TaskQueue,
    store: MemoryStore,
    pool: AgentPool,
    bus: EventBus<AgentEvent>,
    schedules: Vec<ScheduleEntry>,
    notify_chat_id: Option<i64>,
    allowed_chat_ids: Vec<i64>,
) -> Result<()> {
    let bot = Bot::new(token);
    let queue_arc = Arc::new(queue);
    let store_arc = Arc::new(store);
    let pool_arc = Arc::new(pool);
    let allowed = Arc::new(allowed_chat_ids);
    let schedules_arc = Arc::new(schedules);
    let drafts: DraftMap = Arc::new(Mutex::new(HashMap::new()));
    let pending_budgets: PendingBudgetMap = Arc::new(Mutex::new(HashMap::new()));

    // Spawn the completion notifier task.
    let notify_bot = bot.clone();
    let bus_rx = bus.subscribe();
    tokio::spawn(notify::notify_loop(notify_bot, bus_rx, notify_chat_id));

    // `message_handler` takes the bundled `BotDeps` (see its doc comment for
    // why); `callback_query_handler`/`text_message_handler` still take their
    // own individual types, so both the bundle and its constituent pieces
    // are registered — dptree matches whichever type each handler asks for.
    let deps = BotDeps {
        queue: queue_arc.clone(),
        store: store_arc.clone(),
        pool: pool_arc.clone(),
        allowed: allowed.clone(),
        schedules: schedules_arc.clone(),
        drafts: drafts.clone(),
        pending_budgets: pending_budgets.clone(),
    };

    let handler = build_handler();

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![
            deps,
            queue_arc,
            store_arc,
            pool_arc,
            allowed,
            schedules_arc,
            drafts,
            pending_budgets
        ])
        .build()
        .dispatch()
        .await;

    Ok(())
}

fn build_handler() -> UpdateHandler<anyhow::Error> {
    let command_handler = Update::filter_message()
        .filter_command::<LopiCmd>()
        .endpoint(message_handler);

    let text_handler = Update::filter_message()
        .filter(|msg: Message| msg.text().is_some_and(|t| !t.starts_with('/')))
        .endpoint(text_message_handler);

    let callback_handler =
        Update::filter_callback_query().endpoint(callbacks::callback_query_handler);

    dptree::entry()
        .branch(command_handler)
        .branch(text_handler)
        .branch(callback_handler)
}
