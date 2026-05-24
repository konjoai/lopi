use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TaskId(pub Uuid);

impl TaskId {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Queued,
    Planning,
    Implementing,
    Testing,
    Scoring,
    Retrying {
        attempt: u8,
    },
    Success {
        branch: String,
        pr_url: Option<String>,
    },
    Failed {
        reason: String,
    },
    RolledBack,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    pub goal: String,
    pub constraints: Vec<String>,
    pub allowed_dirs: Vec<String>,
    pub forbidden_dirs: Vec<String>,
    pub priority: Priority,
    pub max_retries: u8,
    pub created_at: DateTime<Utc>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskSource {
    Cli,
    Telegram { chat_id: i64, message_id: i32 },
    Webhook { repo: String, event: String },
    Api,
    SelfModify { approved_by: String },
}

impl Task {
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
