#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::McpClient;
use serde_json::{json, Value};
use std::collections::VecDeque;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

/// A canned MCP server over duplex pipes: for each request line (one carrying an
/// `id`) it replies with the next queued response body — its `jsonrpc`/`id`
/// filled in — while notifications (no `id`) get no reply.
async fn mock_server(
    reader: DuplexStream,
    mut writer: DuplexStream,
    mut responses: VecDeque<Value>,
) {
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let Ok(v) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(id) = v.get("id").and_then(Value::as_i64) else {
            continue; // notification — no response
        };
        let mut resp = responses
            .pop_front()
            .expect("a queued response for each request");
        resp["jsonrpc"] = json!("2.0");
        resp["id"] = json!(id);
        let mut s = serde_json::to_string(&resp).unwrap();
        s.push('\n');
        writer.write_all(s.as_bytes()).await.unwrap();
        writer.flush().await.unwrap();
    }
}

/// Connect a client to a fresh mock server preloaded with `responses`.
fn connect(responses: Vec<Value>) -> McpClient<DuplexStream, BufReader<DuplexStream>> {
    let (client_w, server_r) = tokio::io::duplex(8192);
    let (server_w, client_r) = tokio::io::duplex(8192);
    tokio::spawn(mock_server(server_r, server_w, responses.into()));
    McpClient::new(client_w, BufReader::new(client_r))
}

#[tokio::test]
async fn full_session_handshake_list_and_call() {
    let mut client = connect(vec![
        json!({ "result": { "protocolVersion": "2024-11-05" } }),
        json!({ "result": { "tools": [{ "name": "echo", "description": "echoes" }] } }),
        json!({ "result": { "content": [{ "type": "text", "text": "hello" }] } }),
    ]);

    // A pipe-backed client owns no process.
    assert!(client.server_pid().is_none());

    client.initialize("lopi", "0.1.0").await.unwrap();
    let tools = client.list_tools().await.unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "echo");

    let out = client
        .call_tool("echo", json!({ "msg": "hi" }))
        .await
        .unwrap();
    assert_eq!(out, "hello");
}

#[tokio::test]
async fn tool_error_propagates() {
    let mut client = connect(vec![
        json!({ "result": {} }),                                   // initialize
        json!({ "error": { "code": -32000, "message": "boom" } }), // tools/call
    ]);
    client.initialize("lopi", "0.1.0").await.unwrap();
    let err = client.call_tool("x", json!({})).await.unwrap_err();
    assert!(
        err.to_string().contains("boom"),
        "server error surfaced: {err}"
    );
}

#[tokio::test]
async fn skips_noise_before_the_matching_response() {
    let (client_w, server_r) = tokio::io::duplex(8192);
    let (mut server_w, client_r) = tokio::io::duplex(8192);
    tokio::spawn(async move {
        let mut lines = BufReader::new(server_r).lines();
        let line = lines.next_line().await.unwrap().unwrap();
        let id = serde_json::from_str::<Value>(&line).unwrap()["id"]
            .as_i64()
            .unwrap();
        // A log notification (no id) lands before the real response.
        let noise = b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\",\"params\":{}}\n";
        server_w.write_all(noise).await.unwrap();
        let resp = format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{{\"tools\":[]}}}}\n");
        server_w.write_all(resp.as_bytes()).await.unwrap();
        server_w.flush().await.unwrap();
    });

    let mut client = McpClient::new(client_w, BufReader::new(client_r));
    let tools = client.list_tools().await.unwrap();
    assert!(tools.is_empty(), "notification skipped, real response read");
}

#[tokio::test]
async fn errors_when_server_closes_without_responding() {
    let (client_w, server_r) = tokio::io::duplex(64);
    let (server_w, client_r) = tokio::io::duplex(64);
    drop(server_r); // the server is gone
    drop(server_w);
    let mut client = McpClient::new(client_w, BufReader::new(client_r));
    assert!(client.list_tools().await.is_err());
}

/// Regression test: previously `Response.id` was a plain `i64`, so a
/// JSON-RPC 2.0 `"id": null` response (sent when the server couldn't
/// correlate its error to any request) failed to deserialize as a
/// `Response` at all — it fell through to the "skip non-response lines"
/// branch and the client looped forever waiting for a response that could
/// never match. It must now be surfaced as an error immediately.
#[tokio::test]
async fn null_id_error_response_is_surfaced_not_looped() {
    let (client_w, server_r) = tokio::io::duplex(8192);
    let (mut server_w, client_r) = tokio::io::duplex(8192);
    tokio::spawn(async move {
        let mut lines = BufReader::new(server_r).lines();
        let _ = lines.next_line().await; // consume the request line
        let resp = b"{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{\"code\":-32700,\"message\":\"Parse error\"}}\n";
        server_w.write_all(resp).await.unwrap();
        server_w.flush().await.unwrap();
    });

    let mut client = McpClient::new(client_w, BufReader::new(client_r));
    let err = tokio::time::timeout(std::time::Duration::from_secs(5), client.list_tools())
        .await
        .expect("must not hang waiting for an id that can never match")
        .expect_err("id:null carries an error and must surface as one");
    assert!(err.to_string().contains("Parse error"), "{err}");
}

/// Regression test: a server that never responds must not hang the caller
/// forever — the request has to be bounded by REQUEST_TIMEOUT.
#[tokio::test(start_paused = true)]
async fn request_times_out_when_server_never_responds() {
    let (client_w, server_r) = tokio::io::duplex(64);
    let (server_w, client_r) = tokio::io::duplex(64);
    // Keep the server side of the pipe open (but silent) so reads block
    // waiting for data rather than hitting EOF immediately.
    let _keep_alive = (server_r, server_w);

    let mut client = McpClient::new(client_w, BufReader::new(client_r));
    let err = client.list_tools().await.unwrap_err();
    assert!(err.to_string().contains("timed out"), "{err}");
}
