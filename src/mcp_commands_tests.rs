//! Unit tests for `mcp_commands.rs` — split out to keep that module within
//! the 500-line budget.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

/// A fresh in-memory-store `AppState` with its own pool/queue, no
/// dispatch loop spawned — submitted tasks stay queued (never picked up
/// by a runner), which is exactly what lets these tests inspect queue
/// state directly instead of racing a real `AgentRunner`.
async fn test_state() -> AppState {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(8);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(1, PathBuf::from("."), queue.clone(), bus.clone()).with_store(store.clone()),
    );
    AppState::new_with_repo(store, bus, queue, pool, None, PathBuf::from("."))
}

#[test]
fn tool_defs_advertises_exactly_the_curated_seven() {
    let tools = tool_defs();
    let names: Vec<_> = tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "lopi_submit_task",
            "lopi_list_tasks",
            "lopi_get_task",
            "lopi_cancel_task",
            "lopi_get_logs",
            "lopi_get_agent_dag",
            "lopi_get_stats",
        ]
    );
}

#[test]
fn required_str_returns_the_actual_value() {
    let args = json!({ "task_id": "abc123" });
    assert_eq!(required_str(&args, "task_id").unwrap(), "abc123");
}

#[test]
fn required_str_errors_on_missing_key() {
    assert!(required_str(&json!({}), "task_id").is_err());
}

#[tokio::test]
async fn submit_task_queues_with_default_priority() {
    let state = test_state().await;
    let resp = submit_task(&state, &json!({ "goal": "default priority goal" }))
        .await
        .unwrap();
    assert_eq!(resp["goal"], "default priority goal");
    assert_eq!(resp["queued"], true);
    assert!(resp["id"].as_str().is_some());
    let queued = state.queue.peek_queued();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].0, Priority::Normal);
}

#[tokio::test]
async fn submit_task_honors_each_priority_value() {
    let state = test_state().await;
    for (input, expected) in [
        ("low", Priority::Low),
        ("high", Priority::High),
        ("critical", Priority::Critical),
    ] {
        let goal = format!("{input} priority goal");
        submit_task(&state, &json!({ "goal": goal, "priority": input }))
            .await
            .unwrap();
        let queued = state.queue.peek_queued();
        let entry = queued
            .iter()
            .find(|(_, g)| g == &goal)
            .expect("submitted goal missing from queue");
        assert_eq!(entry.0, expected, "priority {input} mapped incorrectly");
    }
}

#[tokio::test]
async fn submit_task_requires_goal() {
    let state = test_state().await;
    assert!(submit_task(&state, &json!({})).await.is_err());
}

#[tokio::test]
async fn list_tasks_reflects_a_submitted_task() {
    let state = test_state().await;
    submit_task(&state, &json!({ "goal": "listed goal" }))
        .await
        .unwrap();
    let resp = list_tasks(&state).await;
    let tasks = resp["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["goal"], "listed goal");
}

#[tokio::test]
async fn get_task_finds_by_id_prefix() {
    let state = test_state().await;
    let submitted = submit_task(&state, &json!({ "goal": "prefix lookup goal" }))
        .await
        .unwrap();
    let id = submitted["id"].as_str().unwrap().to_string();
    let resp = get_task(&state, &json!({ "task_id": id[..8] }))
        .await
        .unwrap();
    assert_eq!(resp["id"], id);
    assert_eq!(resp["goal"], "prefix lookup goal");
}

#[tokio::test]
async fn get_task_reports_not_found() {
    let state = test_state().await;
    let resp = get_task(&state, &json!({ "task_id": "does-not-exist" }))
        .await
        .unwrap();
    assert_eq!(resp["error"], "task not found");
}

#[tokio::test]
async fn cancel_task_deletes_a_queued_task() {
    let state = test_state().await;
    let submitted = submit_task(&state, &json!({ "goal": "cancel me" }))
        .await
        .unwrap();
    let id = submitted["id"].as_str().unwrap().to_string();
    let resp = cancel_task(&state, &json!({ "task_id": id }))
        .await
        .unwrap();
    assert_eq!(resp["deleted"], true);
    // No dispatch loop ran, so there's no live handle to signal — this
    // is the documented cross-process limitation (LEDGER.md), exercised
    // here in-process instead.
    assert_eq!(resp["cancelled"], false);
    let after = get_task(&state, &json!({ "task_id": id })).await.unwrap();
    assert_eq!(after["error"], "task not found");
}

#[tokio::test]
async fn cancel_task_reports_not_found() {
    let state = test_state().await;
    let resp = cancel_task(&state, &json!({ "task_id": "nope" }))
        .await
        .unwrap();
    assert_eq!(resp["error"], "task not found");
}

#[tokio::test]
async fn get_logs_unknown_task_is_an_error() {
    let state = test_state().await;
    let resp = get_logs(&state, &json!({ "task_id": "nope" }))
        .await
        .unwrap();
    assert_eq!(resp["error"], "unknown task id");
}

#[tokio::test]
async fn get_logs_known_task_returns_empty_history() {
    let state = test_state().await;
    let submitted = submit_task(&state, &json!({ "goal": "log me" }))
        .await
        .unwrap();
    let id = submitted["id"].as_str().unwrap().to_string();
    let resp = get_logs(&state, &json!({ "task_id": id.clone() }))
        .await
        .unwrap();
    assert_eq!(resp["task_id"], id);
    assert!(resp["logs"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_agent_dag_unknown_task_is_an_error() {
    let state = test_state().await;
    let resp = get_agent_dag(&state, &json!({ "task_id": "nope" }))
        .await
        .unwrap();
    assert_eq!(resp["error"], "unknown task id");
}

#[tokio::test]
async fn get_agent_dag_known_task_returns_empty_graph() {
    let state = test_state().await;
    let submitted = submit_task(&state, &json!({ "goal": "dag me" }))
        .await
        .unwrap();
    let id = submitted["id"].as_str().unwrap().to_string();
    let resp = get_agent_dag(&state, &json!({ "task_id": id.clone() }))
        .await
        .unwrap();
    assert_eq!(resp["task_id"], id);
    assert!(resp["nodes"].as_array().unwrap().is_empty());
    assert!(resp["edges"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn get_stats_reflects_queue_state() {
    let state = test_state().await;
    submit_task(&state, &json!({ "goal": "stats goal" }))
        .await
        .unwrap();
    let resp = get_stats(&state).await;
    assert_eq!(resp["queued"], 1);
    assert_eq!(resp["running"], 0);
    assert_eq!(resp["succeeded"], 0);
    assert_eq!(resp["failed"], 0);
}

#[tokio::test]
async fn dispatch_routes_get_stats_to_real_json_not_a_placeholder() {
    let state = test_state().await;
    let text = dispatch(&state, "lopi_get_stats", json!({})).await.unwrap();
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed.get("running").is_some());
    assert_ne!(text, "xyzzy");
    assert_ne!(text, "");
}

#[tokio::test]
async fn dispatch_rejects_unknown_tool_name() {
    let state = test_state().await;
    assert!(dispatch(&state, "not_a_real_tool", json!({}))
        .await
        .is_err());
}

#[tokio::test]
async fn tool_handler_call_round_trips_through_dispatch() {
    let handler = LopiToolHandler {
        state: test_state().await,
    };
    assert_eq!(handler.tools().len(), 7);
    let text = handler.call("lopi_list_tasks", json!({})).await.unwrap();
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed["tasks"].as_array().unwrap().is_empty());
}
