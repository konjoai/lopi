use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TaskId(pub Uuid);

impl TaskId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}

impl Default for TaskId {
    fn default() -> Self { Self::new() }
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(PartialEq)]
pub enum TaskStatus {
    Queued,
    Planning,
    Implementing,
    Testing,
    Scoring,
    Retrying { attempt: u8 },
    Success { branch: String, pr_url: Option<String> },
    Failed { reason: String },
    RolledBack,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority { Low = 0, Normal = 1, High = 2, Critical = 3 }

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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskSource {
    Cli,
    Telegram { chat_id: i64, message_id: i32 },
    Webhook { repo: String, event: String },
    Api,
}

impl Task {
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
        }
    }
}
