//! [`TaskRow`] / [`TaskStatusCounts`] — split out of `mod.rs` purely to keep
//! that file under the 500-line CI file-size gate as Egress-Allowlist-1 added
//! the `source`/`provenance()` field and method; same rationale as
//! `lopi-core`'s `task_source.rs` split from `task.rs`.

/// Task counts by lifecycle bucket, returned by [`super::MemoryStore::status_counts`].
///
/// Computed from the shared durable store so the totals are correct across
/// every repo/pool (see the method's docs for the multi-repo undercount this
/// avoids).
#[derive(Debug, Default, Clone, Copy)]
pub struct TaskStatusCounts {
    /// Tasks currently executing.
    pub running: usize,
    /// Tasks queued but not yet started.
    pub queued: usize,
    /// Tasks that reached a successful terminal state.
    pub succeeded: usize,
    /// Tasks that reached a failed terminal state.
    pub failed: usize,
}

/// Flat view of a task record returned by [`super::MemoryStore::load_history`].
#[derive(Debug, sqlx::FromRow)]
pub struct TaskRow {
    /// Stringified UUID — primary key matching the `tasks` table.
    pub id: String,
    /// Human-readable goal text submitted with the task.
    pub goal: String,
    /// Current lifecycle status string (e.g. `"pending"`, `"done"`, `"failed"`).
    pub status: String,
    /// ISO-8601 timestamp when the task was created.
    pub created_at: String,
    /// ISO-8601 timestamp when the task reached a terminal state, if any.
    pub completed_at: Option<String>,
    /// Backend-1 — the caller-supplied [`lopi_core::Task::client_ref`], if any.
    pub client_ref: Option<String>,
    /// MCPB-App-1 — the git branch this task's most recent attempt runs (or
    /// ran) on, `None` until the first `TaskStarted` event fires.
    pub branch: Option<String>,
    /// macOS-Web-Parity-5 — the effective repo (task override, or the pool
    /// default) this task's most recent attempt runs (or ran) against,
    /// `None` until the first `TaskStarted` event fires.
    pub repo: Option<String>,
    /// Sprint Successor-1 — stringified UUID of the task this one was
    /// derived from, `None` for anything not created by
    /// `derive_successor_task`.
    pub parent_task: Option<String>,
    /// Sprint Successor-1 — successor hops from the root of this task's
    /// chain; `0` for anything not derived.
    pub chain_depth: i64,
    /// Egress-Allowlist-1 — the task's `source` column, a JSON-serialized
    /// [`lopi_core::TaskSource`] written by `save_task`. Kept raw (not
    /// deserialized eagerly) since most callers only need the derived
    /// [`Self::provenance`] label, not the full enum. Use `provenance()`
    /// rather than matching on this directly.
    pub source: String,
    /// Sprint F4 Phase 4 — the CLI's own resumable session id for this
    /// task's most recent attempt, `None` until `AgentRunner::persist_cli_session`
    /// first writes it.
    pub cli_session_id: Option<String>,
}

impl TaskRow {
    /// Whether this run originated from untrusted input (a GitHub webhook) or
    /// an operator-initiated path (CLI, API, Telegram, self-modify,
    /// self-authored successor) — foundation for a future human-approval
    /// gate on outbound notification. This sprint only records and surfaces
    /// the marker; see `docs/security/EGRESS_SURFACE.md`.
    ///
    /// Deliberately narrower than [`lopi_core::is_untrusted_source`], which
    /// also classifies `TaskSource::Telegram` as untrusted for a different
    /// purpose (Sprint Successor-1's chain-extension caution). Sprint S2's
    /// own trifecta gate (`require_plan_approval`) never extended to
    /// Telegram — it's inbound-authenticated via `allowed_chat_ids`, a
    /// different threat model than an unauthenticated webhook payload (see
    /// `docs/security/TRIFECTA_PATHS.md` §1, row E). This marker mirrors
    /// that same operational judgment rather than the broader predicate.
    /// Falls back to `"unknown"` (logged, not silent) if `source` predates
    /// this column or fails to parse — never guesses in either safety
    /// direction, since nothing gates on this value yet.
    #[must_use]
    pub fn provenance(&self) -> &'static str {
        match serde_json::from_str::<lopi_core::TaskSource>(&self.source) {
            Ok(lopi_core::TaskSource::Webhook { .. }) => "untrusted",
            Ok(_) => "operator",
            Err(e) => {
                tracing::warn!("TaskRow::provenance: failed to parse source column: {e}");
                "unknown"
            }
        }
    }
}
