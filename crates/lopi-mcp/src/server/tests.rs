#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{handle_request, serve, ToolHandler};
use crate::jsonrpc::Request;
use crate::protocol::McpTool;
use crate::McpClient;
use anyhow::Result;
use serde_json::{json, Value};
use std::future::Future;
use tokio::io::BufReader;

/// A tiny handler: one `echo` tool, plus an error for anything else.
struct MockHandler;

impl ToolHandler for MockHandler {
    fn tools(&self) -> Vec<McpTool> {
        vec![McpTool {
            name: "echo".into(),
            description: "echoes its msg".into(),
            input_schema: json!({ "type": "object" }),
        }]
    }

    fn call(&self, name: &str, arguments: Value) -> impl Future<Output = Result<String>> + Send {
        let name = name.to_string();
        async move {
            if name == "echo" {
                Ok(arguments
                    .get("msg")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string())
            } else {
                anyhow::bail!("unknown tool: {name}")
            }
        }
    }
}

#[tokio::test]
async fn client_drives_served_handler_end_to_end() {
    let (client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);
    tokio::spawn(async move {
        let handler = MockHandler;
        serve(&handler, BufReader::new(server_r), server_w)
            .await
            .ok();
    });

    let mut client = McpClient::new(client_w, BufReader::new(client_r));
    client.initialize("test-client", "0.1.0").await.unwrap();

    let tools = client.list_tools().await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");

    let out = client
        .call_tool("echo", json!({ "msg": "hi" }))
        .await
        .unwrap();
    assert_eq!(out, "hi");

    // A failing tool call surfaces as a JSON-RPC error on the client.
    let err = client.call_tool("nope", json!({})).await.unwrap_err();
    assert!(err.to_string().contains("unknown tool"), "got: {err}");
}

#[tokio::test]
async fn unknown_method_is_method_not_found() {
    let resp = handle_request(&MockHandler, &Request::new(9, "frobnicate", None)).await;
    let e = resp.into_result().unwrap_err();
    assert_eq!(e.code, -32601);
    assert!(e.message.contains("frobnicate"));
}

#[tokio::test]
async fn tools_call_without_a_name_is_invalid_params() {
    let req = Request::new(3, "tools/call", Some(json!({ "arguments": {} })));
    let resp = handle_request(&MockHandler, &req).await;
    let e = resp.into_result().unwrap_err();
    assert_eq!(e.code, -32602);
}

#[tokio::test]
async fn initialize_advertises_lopi_server_info() {
    let resp = handle_request(&MockHandler, &Request::new(1, "initialize", None)).await;
    let result = resp.into_result().unwrap();
    assert_eq!(result["serverInfo"]["name"], "lopi");
    assert!(result["capabilities"]["tools"].is_object());
}
