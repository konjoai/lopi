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
    /// Phase 11 — require human approval of the plan before implementation.
    #[serde(default)]
    pub require_plan_approval: Option<bool>,
    /// Verifier as Explicit Gate — force the Konjo Verifier second-score pass
    /// for this task, independent of `autonomy_level`. Mirrors
    /// [`lopi_core::Task::verifier_required`].
    #[serde(default)]
    pub verifier_required: Option<bool>,
    /// Explicit verifier model override, e.g. `"claude-opus-4-7"`. Mirrors
    /// [`lopi_core::Task::verifier_model`].
    #[serde(default)]
    pub verifier_model: Option<String>,
    /// Reasoning-effort hint for the verifier's grading pass. Mirrors
    /// [`lopi_core::Task::verifier_effort`].
    #[serde(default)]
    pub verifier_effort: Option<String>,
    /// Report on Finish channel name (e.g. `"telegram"`). Validated via
    /// [`lopi_core::ReportChannel::parse`] at request time — an unknown or
    /// currently-unreachable channel (`"whatsapp"`) is rejected with a 422,
    /// never silently dropped. Mirrors [`lopi_core::Task::report`].
    #[serde(default)]
    pub report: Option<String>,
    /// Per-task override of the hard iteration ceiling, taking precedence
    /// over the repo's `.lopi/loop.toml`. `0` is the infinite-loop sentinel.
    /// Mirrors [`lopi_core::Task::max_iterations`].
    #[serde(default)]
    pub max_iterations: Option<u8>,
    /// Explicit worker-model override. Mirrors [`lopi_core::Task::model`].
    #[serde(default)]
    pub model: Option<String>,
    /// Reasoning-effort hint for the worker's planning pass. Stored for
    /// round-trip only — not yet folded into any planning prompt. Mirrors
    /// [`lopi_core::Task::effort`].
    #[serde(default)]
    pub effort: Option<String>,
    /// Guardrail precondition — a shell command that must exit `0` before
    /// the loop's first iteration. Mirrors [`lopi_core::Task::gate`].
    #[serde(default)]
    pub gate: Option<String>,
    /// Guardrail exit-condition — a shell command checked after each
    /// iteration; exiting `0` ends the loop early as a success. Mirrors
    /// [`lopi_core::Task::until`].
    #[serde(default)]
    pub until: Option<String>,
    /// On-fail policy override (`"stop"` / `"continue"` / `"backoff"`).
    /// Mirrors [`lopi_core::Task::on_fail`].
    #[serde(default)]
    pub on_fail: Option<lopi_core::loop_config::OnFail>,
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
