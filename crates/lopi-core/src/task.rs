use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// Unique identifier for a task — a newtype over `Uuid`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TaskId(pub Uuid);

impl TaskId {
    /// Generate a new random task ID.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Current lifecycle status of a [`Task`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    /// Task is waiting in the queue for an available agent.
    Queued,
    /// Agent is generating an implementation plan.
    Planning,
    /// Agent is applying code changes.
    Implementing,
    /// Agent is running the test suite.
    Testing,
    /// Agent is evaluating test and lint results.
    Scoring,
    /// Task is being retried after a previous failed attempt.
    Retrying {
        /// Attempt number of the upcoming retry.
        attempt: u8,
    },
    /// Task completed successfully and a branch (and optionally PR) was created.
    Success {
        /// Branch containing the successful changes.
        branch: String,
        /// URL of the opened pull request, if auto-PR is enabled.
        pr_url: Option<String>,
    },
    /// Task failed after exhausting all retry attempts.
    Failed {
        /// Human-readable description of why the task failed.
        reason: String,
    },
    /// Changes were rolled back after a failure.
    RolledBack,
}

/// Scheduling priority for a [`Task`] in the agent queue.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Lowest priority — processed only when no higher-priority tasks are queued.
    Low = 0,
    /// Default priority for tasks submitted without an explicit override.
    Normal = 1,
    /// Elevated priority — processed before `Normal` tasks.
    High = 2,
    /// Highest priority — pre-empts all other queued tasks.
    Critical = 3,
}

/// Rubric used by the Konjo Verifier to grade an agent's output.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Rubric {
    /// Display name for this rubric (e.g. `"refactor_safety"`).
    pub name: String,
    /// Ordered criteria the verifier checks. Each entry is an imperative statement
    /// such as `"All existing tests still pass"`.
    pub criteria: Vec<String>,
}

/// Verdict returned by the Konjo Verifier second-score pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierVerdict {
    /// Whether the agent's output satisfies all rubric criteria.
    pub passed: bool,
    /// Criteria not met — one sentence each explaining the gap.
    pub gaps: Vec<String>,
    /// Imperative fix hints ready for injection into the next retry's planning prompt.
    pub fix_hints: Vec<String>,
    /// Verifier confidence in the verdict, normalised to `[0.0, 1.0]`.
    pub confidence: f64,
}

/// A unit of work submitted to the lopi agent pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique identifier for this task.
    pub id: TaskId,
    /// Natural-language description of what the agent must achieve.
    pub goal: String,
    /// Additional hard constraints injected into the planning prompt.
    pub constraints: Vec<String>,
    /// Directories the agent is allowed to modify.
    pub allowed_dirs: Vec<String>,
    /// Directories the agent must never touch.
    pub forbidden_dirs: Vec<String>,
    /// Scheduling priority relative to other queued tasks.
    pub priority: Priority,
    /// Maximum number of retry attempts before the task is marked failed.
    pub max_retries: u8,
    /// Timestamp when the task was created.
    pub created_at: DateTime<Utc>,
    /// Origin that submitted this task.
    pub source: TaskSource,
    /// Override repository path for this task. Pool default is used when None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_path: Option<PathBuf>,
    /// P2 — capabilities every dispatcher (pool, constellation router)
    /// must satisfy before this task can be picked up. Empty (default)
    /// means "any agent can run this". Compared against
    /// `AgentPool::register_capabilities` and constellation member
    /// tags via [`crate::Task::capabilities_satisfied_by`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_capabilities: Vec<String>,
    /// P1.4 — Optional JSON Schema the agent's structured output must
    /// satisfy. Stored as raw `serde_json::Value` so callers can supply
    /// any schema shape; the validator in `lopi_core::schema` enforces a
    /// pragmatic subset (`type`, `required`, `properties`, `enum`).
    /// Validation failures are counted via `schema_violations_inc` and
    /// trigger an adaptive-retry cycle so the next plan prompt includes
    /// the violation message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<serde_json::Value>,
    /// P2 — Allowlist of tool names this task may call. The names are
    /// looked up in `lopi-tools::ToolRegistry` at dispatch time. An empty
    /// vec means "no tools" — the agent stays in pure-CLI/API mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<String>,
    /// Sprint S — optional rubric for the Konjo Verifier second-score pass.
    /// When set, the verifier grades the diff against these criteria after
    /// the heuristic score passes. Falls back to a workspace default rubric
    /// when `None` and verifier mode is enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rubric: Option<Rubric>,
    /// Base branch the runner checks out before creating the per-attempt
    /// `lopi/{id}-attempt-N` working branch. `None` (default) uses current
    /// `HEAD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    /// Explicit Claude model override (full model id, e.g. `"claude-opus-4-7"`).
    /// When `Some`, the runner uses this instead of the complexity-based
    /// `select_model` heuristic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Effort hint: `"low"` / `"medium"` / `"high"` / `"max"`. Drives the
    /// retry budget when `max_retries` is left at its default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

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
}

impl Task {
    /// Create a new `Task` with sensible defaults and `Normal` priority.
    #[must_use]
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            id: TaskId::new(),
            goal: goal.into(),
            constraints: vec![],
            allowed_dirs: vec!["src/".into(), "tests/".into()],
            forbidden_dirs: vec![".github/".into(), "infra/".into(), "Cargo.toml".into()],
            priority: Priority::Normal,
            max_retries: 3,
            created_at: Utc::now(),
            source: TaskSource::Cli,
            repo_path: None,
            output_schema: None,
            tools: Vec::new(),
            required_capabilities: Vec::new(),
            rubric: None,
            base_branch: None,
            model: None,
            effort: None,
        }
    }

    /// True when every entry in `required_capabilities` appears in
    /// `provided`. Empty requirements vacuously satisfy.
    ///
    /// Used by `AgentPool::submit` and the constellation router to filter
    /// candidate agents before dispatch.
    #[must_use]
    pub fn capabilities_satisfied_by(&self, provided: &[String]) -> bool {
        self.required_capabilities
            .iter()
            .all(|req| provided.iter().any(|p| p == req))
    }
}
