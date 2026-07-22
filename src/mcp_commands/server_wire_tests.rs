//! `lopi_cancel_task`'s regression coverage at the `crates/lopi-mcp` JSON-RPC
//! surface — the exact path a widget's `callServerTool()` exercises, not
//! `dispatch()` called directly (already covered by `mod_tests.rs`). Per
//! `MCPB-App-2`'s Phase 2: `mod_tests.rs`'s existing `cancel_task_*` tests
//! prove the function is correct in isolation, but never drove a real
//! `tools/call` request through `lopi_mcp::serve()`'s newline-framed
//! JSON-RPC loop the way a real MCP host actually would — the same class of
//! gap `KT-B3-Live`'s three findings exposed (green unit tests, broken
//! host-facing contract).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::tests::test_state;
use super::LopiToolHandler;
use lopi_mcp::McpClient;
use serde_json::{json, Value};
use tokio::io::BufReader;

/// Spawn a real `lopi_mcp::serve()` loop over an in-memory duplex pipe,
/// wrapping the real `LopiToolHandler` (not a mock) around a fresh
/// `test_state()`, and return a connected, initialized client.
async fn connected_client() -> McpClient<tokio::io::DuplexStream, BufReader<tokio::io::DuplexStream>>
{
    let (client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);
    let handler = LopiToolHandler {
        state: test_state().await,
    };
    tokio::spawn(async move {
        lopi_mcp::serve(&handler, BufReader::new(server_r), server_w)
            .await
            .ok();
    });
    let mut client = McpClient::new(client_w, BufReader::new(client_r));
    client.initialize("mcpb-app-2-test", "0.1.0").await.unwrap();
    client
}

#[tokio::test]
async fn cancel_task_via_real_tools_call_deletes_the_task() {
    let mut client = connected_client().await;

    let submitted: Value = serde_json::from_str(
        &client
            .call_tool(
                "lopi_submit_task",
                json!({ "goal": "cancel me over the wire" }),
            )
            .await
            .unwrap(),
    )
    .unwrap();
    let task_id = submitted["id"].as_str().unwrap().to_string();

    let cancelled: Value = serde_json::from_str(
        &client
            .call_tool("lopi_cancel_task", json!({ "task_id": task_id }))
            .await
            .unwrap(),
    )
    .unwrap();
    // Same shape a real widget's `callServerTool()` resolves to — mirrors
    // `mod_tests.rs::cancel_task_deletes_a_queued_task`'s in-process
    // assertion, now proven over the real JSON-RPC transport too.
    assert_eq!(cancelled["deleted"], true);
    assert_eq!(cancelled["cancelled"], false);

    let after: Value = serde_json::from_str(
        &client
            .call_tool("lopi_get_task", json!({ "task_id": task_id }))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(after["error"], "task not found");
}

#[tokio::test]
async fn cancel_task_via_real_tools_call_reports_not_found() {
    let mut client = connected_client().await;

    let resp: Value = serde_json::from_str(
        &client
            .call_tool("lopi_cancel_task", json!({ "task_id": "does-not-exist" }))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(resp["error"], "task not found");
}
