//! Unit + KT-B2 fixture tests for `lopi_get_stack_status` and its bound
//! `ui://` resource.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use lopi_core::{AgentEvent, EventBus, Task};
use lopi_memory::MemoryStore;
use lopi_orchestrator::{AgentPool, TaskQueue};
use std::path::PathBuf;
use std::sync::Arc;

async fn test_state() -> (AppState, MemoryStore) {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(8);
    let queue = TaskQueue::new();
    let pool = Arc::new(
        AgentPool::new(1, PathBuf::from("."), queue.clone(), bus.clone()).with_store(store.clone()),
    );
    let state = AppState::new_with_repo(store.clone(), bus, queue, pool, None, PathBuf::from("."));
    (state, store)
}

/// KT-B2 — a real multi-task fixture with two tasks concurrently in
/// different stages, one `Planning`-shaped, one `Testing`-shaped, each on
/// its own branch. Asserts the join returns each task's *real* field
/// values, not just that the query runs without erroring — the same
/// mutation-testing-precedent bar `MCP-Serve-1`'s G3 gate set.
#[tokio::test]
async fn get_stack_status_joins_roster_branch_and_stage_for_concurrent_tasks() {
    let (state, store) = test_state().await;

    let planning_task = Task::new("add a widget");
    let planning_id = planning_task.id;
    store.save_task(&planning_task, "running").await.unwrap();
    store
        .set_task_branch(&planning_id, "lopi/aaa-attempt-1")
        .await
        .unwrap();
    store
        .upsert_dag_node(
            &planning_id.0.to_string(),
            "plan",
            "running",
            "[]",
            None,
            None,
        )
        .await
        .unwrap();

    let testing_task = Task::new("fix a bug");
    let testing_id = testing_task.id;
    store.save_task(&testing_task, "running").await.unwrap();
    store
        .set_task_branch(&testing_id, "lopi/bbb-attempt-1")
        .await
        .unwrap();
    let testing_id_str = testing_id.0.to_string();
    store
        .upsert_dag_node(&testing_id_str, "plan", "done", "[]", None, None)
        .await
        .unwrap();
    store
        .upsert_dag_node(
            &testing_id_str,
            "implement",
            "done",
            "[\"plan\"]",
            None,
            None,
        )
        .await
        .unwrap();
    store
        .upsert_dag_node(
            &testing_id_str,
            "test",
            "running",
            "[\"implement\"]",
            None,
            None,
        )
        .await
        .unwrap();

    let result = get_stack_status(&state).await;
    let tasks = result["tasks"].as_array().unwrap();
    assert_eq!(
        tasks.len(),
        2,
        "both concurrent tasks present in the roster"
    );

    let planning = tasks
        .iter()
        .find(|t| t["id"] == planning_id.0.to_string())
        .expect("planning task in roster");
    assert_eq!(planning["goal"], "add a widget");
    assert_eq!(planning["branch"], "lopi/aaa-attempt-1");
    assert_eq!(planning["stage"], "plan");
    assert_eq!(planning["status"], "running");

    let testing = tasks
        .iter()
        .find(|t| t["id"] == testing_id_str)
        .expect("testing task in roster");
    assert_eq!(testing["goal"], "fix a bug");
    assert_eq!(testing["branch"], "lopi/bbb-attempt-1");
    assert_eq!(testing["stage"], "test");
    assert_eq!(testing["status"], "running");
}

#[tokio::test]
async fn get_stack_status_reports_queued_before_any_dag_node_or_branch() {
    let (state, store) = test_state().await;
    let task = Task::new("just queued");
    store.save_task(&task, "queued").await.unwrap();

    let result = get_stack_status(&state).await;
    let tasks = result["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0]["stage"], "queued");
    assert_eq!(tasks[0]["status"], "queued");
    assert!(
        tasks[0]["branch"].is_null(),
        "no TaskStarted has fired yet, so no branch is set"
    );
}

#[tokio::test]
async fn get_stack_status_empty_roster_is_an_empty_array_not_an_error() {
    let (state, _store) = test_state().await;
    let result = get_stack_status(&state).await;
    assert!(result["tasks"].as_array().unwrap().is_empty());
}

#[test]
fn ui_resources_advertises_the_stack_status_widget() {
    let resources = ui_resources();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].uri, RESOURCE_URI);
    assert_eq!(resources[0].mime_type, "text/html");
}

#[test]
fn ui_resource_contents_serves_the_widget_html() {
    let contents = ui_resource_contents(RESOURCE_URI).unwrap();
    assert_eq!(contents.uri, RESOURCE_URI);
    assert_eq!(contents.mime_type, "text/html");
    // The three MCP Apps lifecycle methods the sprint brief specified must
    // actually be present in the shipped widget, not just planned.
    assert!(contents.text.contains("ui/initialize"));
    assert!(contents.text.contains("ui/notifications/initialized"));
    assert!(contents.text.contains("ui/notifications/tool-result"));
}

#[test]
fn ui_resource_contents_errors_for_an_unknown_uri() {
    assert!(ui_resource_contents("ui://nope").is_err());
}

#[test]
fn tool_def_binds_the_resource_uri() {
    let tool = tool_def();
    assert_eq!(tool.name, "lopi_get_stack_status");
    assert_eq!(tool.meta.unwrap()["ui"]["resourceUri"], RESOURCE_URI);
}
