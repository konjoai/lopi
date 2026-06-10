use serde::{Deserialize, Serialize};

pub(super) const MAX_GOAL_LENGTH: usize = 2000;

/// Request body for `POST /api/tasks`.
#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    /// Natural-language goal for the agent (max 2 000 chars).
    pub goal: String,
    /// Path to the git repository the agent should work in.
    pub repo: Option<String>,
    /// Task priority: `"low"`, `"normal"` (default), `"high"`, or `"critical"`.
    #[serde(default)]
    pub priority: Option<String>,
    /// Additional constraints appended to the agent's system prompt.
    #[serde(default)]
    pub constraints: Option<Vec<String>>,
    /// Directories the agent is permitted to read and write.
    #[serde(default)]
    pub allowed_dirs: Option<Vec<String>>,
    /// Directories the agent must not touch.
    #[serde(default)]
    pub forbidden_dirs: Option<Vec<String>>,
    /// Maximum retry attempts before the task is marked failed.
    #[serde(default)]
    pub max_retries: Option<u8>,
    /// P2 — capabilities every dispatcher (pool, constellation router)
    /// must satisfy before this task can be picked up. Empty (default)
    /// means "any agent can run this".
    #[serde(default)]
    pub required_capabilities: Option<Vec<String>>,
    /// Base branch the runner checks out before creating the per-attempt
    /// `lopi/{id}-attempt-N` working branch. Empty / missing → current `HEAD`.
    #[serde(default)]
    pub base_branch: Option<String>,
    /// Explicit Claude model override (full model id, e.g. `"claude-opus-4-7"`).
    /// Empty / missing / `"auto"` → complexity-based auto-select.
    #[serde(default)]
    pub model: Option<String>,
    /// Effort hint: `"low"` / `"medium"` / `"high"` / `"max"`. Used as a
    /// fallback for `max_retries` when the request leaves it unset.
    #[serde(default)]
    pub effort: Option<String>,
}

/// Response body for `POST /api/tasks`.
#[derive(Debug, Serialize)]
pub struct CreateTaskResponse {
    /// UUID of the created (or existing) task.
    pub id: String,
    /// The goal string as stored.
    pub goal: String,
    /// `true` if the task was newly queued; `false` if it was a duplicate.
    pub queued: bool,
    /// If this was a duplicate, the ID of the existing task.
    pub duplicate_of: Option<String>,
}
