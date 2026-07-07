#![allow(clippy::unwrap_used, clippy::panic)]
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
        AgentEvent::TurnMetrics {
            pressure, activity, ..
        } => {
            assert!((pressure - 0.5).abs() < f32::EPSILON);
            assert!((activity - 0.5).abs() < f32::EPSILON);
        }
        _ => panic!("decoded into wrong variant"),
    }
}
