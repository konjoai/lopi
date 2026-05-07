use crate::task::{TaskId, TaskStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

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
    /// Periodic per-agent cognition metrics emitted during a run.
    /// Drives the Forge's live shader uniforms in lopi-ui.
    /// `pressure` and `activity` are normalized to `[0.0, 1.0]`.
    TurnMetrics {
        task_id: TaskId,
        /// Context window fill — `ContextWindow::token_pressure()`.
        pressure: f32,
        /// Generation intensity (tokens/sec normalized against a soft cap).
        activity: f32,
        /// Raw output tokens per second.
        tokens_per_sec: f32,
        /// Accumulated cost in USD for this run.
        cost_usd: f32,
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

#[cfg(test)]
mod wire_format_tests {
    //! These tests pin the on-wire JSON shape that the lopi-ui WebSocket client
    //! (web/src/lib/parser.ts) parses. If a Rust-side change here breaks the
    //! shape, this test fails first — before any UI regression ships.
    use super::*;
    use serde_json::json;

    #[test]
    fn turn_metrics_serializes_with_snake_case_tag() {
        let id = TaskId::new();
        let ev = AgentEvent::TurnMetrics {
            task_id: id,
            pressure: 0.42,
            activity: 0.65,
            tokens_per_sec: 52.4,
            cost_usd: 0.0124,
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["type"], "turn_metrics");
        assert_eq!(v["task_id"], json!(id));
        assert!(v["pressure"].as_f64().unwrap() > 0.41 && v["pressure"].as_f64().unwrap() < 0.43);
        assert!(v["activity"].is_number());
        assert!(v["tokens_per_sec"].is_number());
        assert!(v["cost_usd"].is_number());
    }

    #[test]
    fn pool_stats_wire_shape() {
        let ev = AgentEvent::PoolStats {
            running: 3,
            queued: 2,
            succeeded: 12,
            failed: 1,
            uptime_secs: 1820,
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["type"], "pool_stats");
        assert_eq!(v["running"], 3);
        assert_eq!(v["uptime_secs"], 1820);
    }

    #[test]
    fn log_line_uses_lowercase_level() {
        let ev = AgentEvent::log(TaskId::new(), "hello", LogLevel::Warn);
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["type"], "log_line");
        assert_eq!(v["level"], "warn");
    }

    #[test]
    fn task_completed_with_struct_outcome_serializes() {
        let id = TaskId::new();
        let ev = AgentEvent::TaskCompleted {
            task_id: id,
            outcome: crate::task::TaskStatus::Success {
                branch: "feat/x".to_string(),
                pr_url: None,
            },
            total_attempts: 2,
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["type"], "task_completed");
        assert_eq!(v["outcome"]["Success"]["branch"], "feat/x");
        assert!(v["outcome"]["Success"]["pr_url"].is_null());
    }

    #[test]
    fn turn_metrics_round_trip() {
        let original = AgentEvent::TurnMetrics {
            task_id: TaskId::new(),
            pressure: 0.5,
            activity: 0.5,
            tokens_per_sec: 25.0,
            cost_usd: 0.001,
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: AgentEvent = serde_json::from_str(&json).unwrap();
        match decoded {
            AgentEvent::TurnMetrics { pressure, activity, .. } => {
                assert!((pressure - 0.5).abs() < f32::EPSILON);
                assert!((activity - 0.5).abs() < f32::EPSILON);
            }
            _ => panic!("decoded into wrong variant"),
        }
    }
}

/// Thin wrapper around `tokio::sync::broadcast` for workspace-wide event fanout.
#[derive(Clone)]
pub struct EventBus<T: Clone> {
    tx: broadcast::Sender<T>,
}

impl<T: Clone + Send + 'static> EventBus<T> {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn send(&self, event: T) {
        let _ = self.tx.send(event);
    }

    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.tx.subscribe()
    }

    #[must_use]
    pub fn sender(&self) -> broadcast::Sender<T> {
        self.tx.clone()
    }
}
