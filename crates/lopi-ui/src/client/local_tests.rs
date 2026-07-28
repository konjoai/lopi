#![allow(clippy::unwrap_used)]

use super::*;
use crate::client::test_support::bare_create_task_request as bare_request;
use lopi_core::{AgentEvent, EventBus};
use lopi_orchestrator::TaskQueue;

async fn build_client() -> LocalClient {
    let store = MemoryStore::open_in_memory().await.unwrap();
    let bus: EventBus<AgentEvent> = EventBus::new(64);
    let queue = TaskQueue::new();
    let pool =
        Arc::new(AgentPool::new(1, std::env::temp_dir(), queue, bus).with_store(store.clone()));
    LocalClient::new(pool, store)
}

#[tokio::test]
async fn create_then_get_then_cancel_round_trips() {
    let client = build_client().await;
    let id = client
        .create_task(&bare_request("local client goal"))
        .await
        .unwrap();
    assert!(!id.is_empty());

    let fetched = client.get_task(&id).await.unwrap();
    assert_eq!(fetched.goal, "local client goal");

    let listed = client.list_tasks().await.unwrap();
    assert!(listed.iter().any(|t| t.id == id));

    // This test never spawns `pool.run()`'s dispatch loop (that would spin
    // up a real `claude` subprocess), so the task never leaves the queue —
    // `cancel_by_prefix` correctly finds nothing dispatched yet and returns
    // `false`. What matters here is that the call itself succeeds.
    let cancel_result = client.cancel_task(&id).await;
    assert!(cancel_result.is_ok());
}

#[tokio::test]
async fn get_task_unknown_id_is_not_found() {
    let client = build_client().await;
    let err = client.get_task("does-not-exist").await.unwrap_err();
    assert!(matches!(err, ClientError::NotFound(_)), "got {err:?}");
}

#[tokio::test]
async fn approve_plan_on_non_awaiting_task_is_conflict() {
    let client = build_client().await;
    let id = client
        .create_task(&bare_request("not awaiting plan"))
        .await
        .unwrap();
    let err = client.approve_plan(&id).await.unwrap_err();
    assert!(matches!(err, ClientError::Conflict(_)), "got {err:?}");
}

#[tokio::test]
async fn invalid_permission_mode_is_rejected_before_submission() {
    let client = build_client().await;
    let mut req = bare_request("bad permission mode");
    req.permission_mode = Some("not-a-real-mode".to_string());
    let err = client.create_task(&req).await.unwrap_err();
    assert!(matches!(err, ClientError::Other(_)), "got {err:?}");
}

#[tokio::test]
async fn chain_methods_are_unsupported() {
    let client = build_client().await;
    assert!(matches!(
        client.list_chains().await.unwrap_err(),
        ClientError::Unsupported(_)
    ));
    assert!(matches!(
        client.get_chain("x").await.unwrap_err(),
        ClientError::Unsupported(_)
    ));
    assert!(matches!(
        client
            .create_chain(serde_json::json!({}))
            .await
            .unwrap_err(),
        ClientError::Unsupported(_)
    ));
    assert!(matches!(
        client.enable_chain("x").await.unwrap_err(),
        ClientError::Unsupported(_)
    ));
    assert!(matches!(
        client.disable_chain("x").await.unwrap_err(),
        ClientError::Unsupported(_)
    ));
    assert!(matches!(
        client.run_chain_now("x").await.unwrap_err(),
        ClientError::Unsupported(_)
    ));
    assert!(matches!(
        client.get_loop_config().await.unwrap_err(),
        ClientError::Unsupported(_)
    ));
}
