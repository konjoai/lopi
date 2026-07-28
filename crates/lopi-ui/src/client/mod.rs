//! Sprint T0 — the TUI's write-capable client layer.
//!
//! Every prior TUI/CLI path into a running `lopi sail` server was
//! read-only: `lopi watch` only ever pumped `AgentEvent`s into the same
//! dashboard (`src/remote.rs`), and the sole write operation anywhere was a
//! bespoke `reqwest::Client::new().delete(url)` call used by `lopi cancel`
//! (`reqwest_cancel`, now `RemoteClient::cancel_task`). `TuiClient` is the
//! trait every future TUI widget (T1's input bar, T2's card editor, T3's
//! stack board, T4's loop-config editor) talks to instead of a transport
//! directly, so nothing in the render layer needs to know whether it's
//! driving an in-process `LocalClient` or a networked `RemoteClient`.
//!
//! The method set below covers exactly the route table T0's grounding
//! confirmed (`crates/lopi-ui/src/web/mod.rs`) — no invented endpoints.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod auth;
mod local;
mod remote;
pub mod stack_payload;
#[cfg(test)]
mod test_support;

pub use auth::resolve_auth_token;
pub use local::LocalClient;
pub use remote::RemoteClient;

/// A task as returned by `GET /api/tasks` / `GET /api/tasks/:id` — the ad
/// hoc `json!` shape both handlers build (`crates/lopi-ui/src/web/
/// handlers.rs`), given a stable Rust type here so callers don't each
/// re-parse raw `serde_json::Value`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    /// Task id (UUID string).
    pub id: String,
    /// The goal text.
    pub goal: String,
    /// Current status, as the server's `TaskStatus` serializes it.
    pub status: serde_json::Value,
    /// Creation timestamp (RFC 3339).
    #[serde(default)]
    pub created_at: Option<String>,
    /// Completion timestamp (RFC 3339), once finished.
    #[serde(default)]
    pub completed_at: Option<String>,
    /// Caller-supplied correlation id, echoed back.
    #[serde(default)]
    pub client_ref: Option<String>,
    /// Cost in USD so far, if known.
    #[serde(default)]
    pub cost: Option<f64>,
    /// Repo the task ran against.
    #[serde(default)]
    pub repo: Option<String>,
    /// Provenance metadata (source, lineage).
    #[serde(default)]
    pub provenance: Option<serde_json::Value>,
}

/// A scheduled chain, as `GET /api/schedule-chains(/:id)` returns it
/// (`chain_to_json`, `crates/lopi-ui/src/web/schedule_chain_handlers.rs`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainSummary {
    /// Chain id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Cron expression.
    pub cron: String,
    /// Target repo, if set.
    #[serde(default)]
    pub repo: Option<String>,
    /// Priority.
    #[serde(default)]
    pub priority: Option<String>,
    /// Autonomy level override for the chain.
    #[serde(default)]
    pub autonomy_level: Option<String>,
    /// Policy applied when a step fails.
    #[serde(default)]
    pub on_fail: Option<String>,
    /// Whether the chain is currently enabled.
    pub enabled: bool,
    /// The chain's ordered steps, as raw JSON (server-side `ChainStepBody`
    /// shape — not re-typed here since T0 doesn't build/edit chains, only
    /// reads them; T3 owns the `ChainStepBody` port).
    pub steps: serde_json::Value,
    /// Creation timestamp.
    #[serde(default)]
    pub created_at: Option<String>,
    /// Last-updated timestamp.
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// Everything that can go wrong driving a [`TuiClient`] call. Distinguishes
/// "no token configured," "401 from server," and "network/transport
/// failure" as separate variants so KT-T0.2's fail-closed behavior is
/// assertable in a unit test, not just observable by eye.
#[derive(Debug, Error)]
pub enum ClientError {
    /// The server requires a bearer token and none was configured
    /// client-side. Distinct from `Unauthorized` (a token was sent but
    /// rejected) — this means the client never tried.
    #[error("no auth token configured for {0}")]
    NoToken(String),
    /// The server rejected the request with `401 Unauthorized`.
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    /// The requested task/chain was not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// The request conflicted with server-side state (e.g. a task not
    /// awaiting plan approval).
    #[error("conflict: {0}")]
    Conflict(String),
    /// A network/transport failure (connection refused, DNS, TLS, timeout).
    #[error("transport error: {0}")]
    Transport(String),
    /// This client implementation doesn't support the requested operation
    /// (e.g. [`LocalClient`]'s chain methods without a reachable
    /// `ChainScheduleManager` — see that module's doc comment).
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// Any other server-reported failure, carrying the server's message.
    #[error("request failed: {0}")]
    Other(String),
}

/// A write-capable client for lopi's task/stack/loop API — the trait every
/// TUI widget talks to instead of a transport directly.
#[async_trait]
pub trait TuiClient: Send + Sync {
    /// List tasks (`GET /api/tasks`).
    async fn list_tasks(&self) -> Result<Vec<TaskSummary>, ClientError>;

    /// Get one task by id or unambiguous id prefix (`GET /api/tasks/:id`).
    async fn get_task(&self, id: &str) -> Result<TaskSummary, ClientError>;

    /// Submit a new task (`POST /api/tasks`). Returns the created task's id;
    /// may be an existing task's id if the server deduplicated on goal.
    async fn create_task(
        &self,
        request: &crate::web::types::CreateTaskRequest,
    ) -> Result<String, ClientError>;

    /// Cancel a task by id or unambiguous id prefix (`DELETE /api/tasks/:id`).
    /// Returns `true` if a task was actually cancelled.
    async fn cancel_task(&self, id: &str) -> Result<bool, ClientError>;

    /// Approve a task's proposed plan
    /// (`POST /api/tasks/:id/plan/approve`).
    async fn approve_plan(&self, id: &str) -> Result<(), ClientError>;

    /// Reject a task's proposed plan (`POST /api/tasks/:id/plan/reject`).
    async fn reject_plan(&self, id: &str) -> Result<(), ClientError>;

    /// List scheduled chains (`GET /api/schedule-chains`).
    async fn list_chains(&self) -> Result<Vec<ChainSummary>, ClientError>;

    /// Get one scheduled chain (`GET /api/schedule-chains/:id`).
    async fn get_chain(&self, id: &str) -> Result<ChainSummary, ClientError>;

    /// Create a scheduled chain (`POST /api/schedule-chains`). `body` is
    /// the raw JSON `ScheduleChainBody` shape (T3 owns the typed port).
    async fn create_chain(&self, body: serde_json::Value) -> Result<ChainSummary, ClientError>;

    /// Enable a scheduled chain (`POST /api/schedule-chains/:id/enable`).
    async fn enable_chain(&self, id: &str) -> Result<(), ClientError>;

    /// Disable a scheduled chain (`POST /api/schedule-chains/:id/disable`).
    async fn disable_chain(&self, id: &str) -> Result<(), ClientError>;

    /// Fire a scheduled chain immediately
    /// (`POST /api/schedule-chains/:id/run-now`).
    async fn run_chain_now(&self, id: &str) -> Result<(), ClientError>;

    /// Fetch the repo's loop-engineering snapshot
    /// (`GET /api/loop-engineering`).
    async fn get_loop_config(&self) -> Result<serde_json::Value, ClientError>;
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
