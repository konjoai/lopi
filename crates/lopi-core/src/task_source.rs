//! [`TaskSource`] — split out of `task.rs` purely to keep that file under the
//! 500-line CI file-size gate as Sprint Successor-1 added lineage fields;
//! same rationale as `autonomy.rs`'s split from `loop_config.rs`. Re-exported
//! from `task.rs` unchanged so every existing `task::TaskSource` path stays
//! valid.

use crate::task::TaskId;
use serde::{Deserialize, Serialize};

/// Where a task originated — used for routing replies and audit logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskSource {
    /// Submitted via the `lopi run` command-line interface.
    Cli,
    /// Submitted by a Telegram bot message.
    Telegram {
        /// Telegram chat that sent the command.
        chat_id: i64,
        /// Message ID of the originating Telegram message.
        message_id: i32,
    },
    /// Injected by the GitHub webhook handler in response to a CI event.
    Webhook {
        /// Repository full name (e.g. `"org/repo"`).
        repo: String,
        /// GitHub event type that triggered the task (e.g. `"check_run"`).
        event: String,
    },
    /// Submitted via the REST API.
    Api,
    /// Approved self-modification task targeting lopi's own codebase.
    SelfModify {
        /// Identity or mechanism that approved the self-modification.
        approved_by: String,
    },
    /// Sprint Successor-1 — derived from a parent task by
    /// [`crate::successor::derive_successor_task`], rather than submitted by
    /// a human, webhook, or API caller. Distinct from [`SelfModify`](Self::SelfModify),
    /// which is about *what* a task targets (lopi's own codebase); this is
    /// about *who* created the task (the agent that ran `parent`, not a
    /// human/external system).
    SelfAuthored {
        /// The task this one was derived from.
        parent: TaskId,
    },
}
