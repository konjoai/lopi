use tokio::sync::broadcast;
use crate::task::{TaskId, TaskStatus};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Rich event emitted by agents and consumed by TUI, WebSocket, and log panels.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    TaskQueued {
        task_id: TaskId,
        goal: String,
        priority: crate::task::Priority,
    },
    TaskStarted {
        task_id: TaskId,
        attempt: u8,
        branch: String,
    },
    StatusChanged {
        task_id: TaskId,
        status: TaskStatus,
        attempt: u8,
    },
    LogLine {
        task_id: TaskId,
        line: String,
        level: LogLevel,
        ts: DateTime<Utc>,
    },
    ScoreUpdated {
        task_id: TaskId,
        test_pass_rate: f32,
        lint_errors: u32,
        diff_lines: u32,
    },
    TaskCompleted {
        task_id: TaskId,
        outcome: TaskStatus,
        total_attempts: u8,
    },
    TaskCancelled {
        task_id: TaskId,
    },
    PoolStats {
        running: usize,
        queued: usize,
        succeeded: usize,
        failed: usize,
        uptime_secs: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
    Debug,
}

impl AgentEvent {
    pub fn log(task_id: TaskId, line: impl Into<String>, level: LogLevel) -> Self {
        Self::LogLine {
            task_id,
            line: line.into(),
            level,
            ts: Utc::now(),
        }
    }

    pub fn info(task_id: TaskId, line: impl Into<String>) -> Self {
        Self::log(task_id, line, LogLevel::Info)
    }

    pub fn warn(task_id: TaskId, line: impl Into<String>) -> Self {
        Self::log(task_id, line, LogLevel::Warn)
    }

    pub fn error(task_id: TaskId, line: impl Into<String>) -> Self {
        Self::log(task_id, line, LogLevel::Error)
    }
}

/// Thin wrapper around `tokio::sync::broadcast` for workspace-wide event fanout.
#[derive(Clone)]
pub struct EventBus<T: Clone> {
    tx: broadcast::Sender<T>,
}

impl<T: Clone + Send + 'static> EventBus<T> {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn send(&self, event: T) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.tx.subscribe()
    }

    pub fn sender(&self) -> broadcast::Sender<T> {
        self.tx.clone()
    }
}
