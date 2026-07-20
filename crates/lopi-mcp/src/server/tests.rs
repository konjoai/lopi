#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::{handle_request, serve, ToolHandler};
use crate::jsonrpc::Request;
use crate::protocol::{McpResource, McpResourceContents, McpTool};
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
            meta: None,
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

/// A tiny tool whose output is a JSON object, for asserting `structuredContent`.
struct JsonToolHandler;

impl ToolHandler for JsonToolHandler {
    fn tools(&self) -> Vec<McpTool> {
        vec![McpTool {
            name: "status".into(),
            description: "returns a JSON object".into(),
            input_schema: json!({ "type": "object" }),
            meta: Some(json!({ "ui": { "resourceUri": "ui://mock/widget" } })),
        }]
    }

    async fn call(&self, _name: &str, _arguments: Value) -> Result<String> {
        Ok(json!({ "running": 2, "queued": 1 }).to_string())
    }

    fn resources(&self) -> Vec<McpResource> {
        vec![McpResource {
            uri: "ui://mock/widget".into(),
            name: "mock widget".into(),
            description: String::new(),
            mime_type: "text/html".into(),
        }]
    }

    fn read_resource(&self, uri: &str) -> impl Future<Output = Result<McpResourceContents>> + Send {
        let uri = uri.to_string();
        async move {
            if uri == "ui://mock/widget" {
                Ok(McpResourceContents {
                    uri,
                    mime_type: "text/html".into(),
                    text: "<html>mock</html>".into(),
                })
            } else {
                anyhow::bail!("unknown resource: {uri}")
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
    assert!(result["capabilities"]["resources"].is_object());
}

#[tokio::test]
async fn resources_list_returns_the_handlers_resources() {
    let resp = handle_request(&JsonToolHandler, &Request::new(1, "resources/list", None)).await;
    let result = resp.into_result().unwrap();
    let resources = result["resources"].as_array().unwrap();
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0]["uri"], "ui://mock/widget");
    assert_eq!(resources[0]["mimeType"], "text/html");
}

#[tokio::test]
async fn resources_list_is_empty_by_default() {
    let resp = handle_request(&MockHandler, &Request::new(1, "resources/list", None)).await;
    let result = resp.into_result().unwrap();
    assert!(result["resources"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn resources_read_returns_the_matching_contents() {
    let req = Request::new(
        2,
        "resources/read",
        Some(json!({ "uri": "ui://mock/widget" })),
    );
    let resp = handle_request(&JsonToolHandler, &req).await;
    let result = resp.into_result().unwrap();
    let contents = &result["contents"][0];
    assert_eq!(contents["uri"], "ui://mock/widget");
    assert_eq!(contents["mimeType"], "text/html");
    assert_eq!(contents["text"], "<html>mock</html>");
}

#[tokio::test]
async fn resources_read_unknown_uri_is_a_server_error() {
    let req = Request::new(2, "resources/read", Some(json!({ "uri": "ui://nope" })));
    let resp = handle_request(&JsonToolHandler, &req).await;
    let e = resp.into_result().unwrap_err();
    assert_eq!(e.code, -32001);
}

#[tokio::test]
async fn resources_read_without_a_uri_is_invalid_params() {
    let req = Request::new(2, "resources/read", Some(json!({})));
    let resp = handle_request(&JsonToolHandler, &req).await;
    let e = resp.into_result().unwrap_err();
    assert_eq!(e.code, -32602);
}

#[tokio::test]
async fn tools_call_surfaces_json_output_as_structured_content_too() {
    let req = Request::new(
        3,
        "tools/call",
        Some(json!({ "name": "status", "arguments": {} })),
    );
    let resp = handle_request(&JsonToolHandler, &req).await;
    let result = resp.into_result().unwrap();
    assert_eq!(result["structuredContent"]["running"], 2);
    assert_eq!(result["structuredContent"]["queued"], 1);
    // The plain-text content block is still present for non-MCP-Apps hosts.
    assert!(result["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("\"running\":2"));
}

#[tokio::test]
async fn tools_call_omits_structured_content_for_non_json_text() {
    let req = Request::new(
        4,
        "tools/call",
        Some(json!({ "name": "echo", "arguments": { "msg": "hi" } })),
    );
    let resp = handle_request(&MockHandler, &req).await;
    let result = resp.into_result().unwrap();
    assert!(result.get("structuredContent").is_none());
}

#[tokio::test]
async fn tools_list_advertises_the_ui_meta_binding() {
    let resp = handle_request(&JsonToolHandler, &Request::new(1, "tools/list", None)).await;
    let result = resp.into_result().unwrap();
    let tool = &result["tools"][0];
    assert_eq!(tool["_meta"]["ui"]["resourceUri"], "ui://mock/widget");
}
