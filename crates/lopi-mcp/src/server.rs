//! Expose lopi *as* an MCP server: answer `initialize`, `tools/list`, and
//! `tools/call` over a transport's read/write halves.
//!
//! The lopi-specific behavior lives behind the [`ToolHandler`] trait — it
//! advertises the tools and executes a call — so this module stays a pure
//! protocol engine, testable over in-memory pipes with a mock handler. The real
//! handler (submitting tasks, reading status, …) is wired in at the binary
//! layer, where the agent pool and store are in reach.

use crate::jsonrpc::{encode_line, Request, Response, RpcError};
use crate::protocol::{McpTool, MCP_PROTOCOL_VERSION};
use anyhow::Result;
use serde_json::{json, Value};
use std::future::Future;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

/// The lopi-side behavior an [`serve`] loop exposes over MCP.
pub trait ToolHandler {
    /// The tools this server advertises in `tools/list`.
    fn tools(&self) -> Vec<McpTool>;

    /// Invoke tool `name` with `arguments`, returning its text output. An `Err`
    /// is surfaced to the caller as a JSON-RPC error.
    fn call(&self, name: &str, arguments: Value) -> impl Future<Output = Result<String>> + Send;
}

/// Serve MCP requests from `reader`, writing responses to `writer`, until the
/// peer closes the connection. Each line is one JSON-RPC message; notifications
/// (no `id`) and unparsable lines draw no response.
///
/// # Errors
/// Returns `Err` only on an IO failure reading or writing the transport.
pub async fn serve<H, R, W>(handler: &H, mut reader: R, mut writer: W) -> Result<()>
where
    H: ToolHandler,
    R: AsyncBufReadExt + Unpin,
    W: AsyncWriteExt + Unpin,
{
    let mut buf = String::new();
    loop {
        buf.clear();
        if reader.read_line(&mut buf).await? == 0 {
            return Ok(()); // peer closed
        }
        let trimmed = buf.trim();
        let Ok(req) = serde_json::from_str::<Request>(trimmed) else {
            continue; // notification / non-request line — no reply
        };
        let response = handle_request(handler, &req).await;
        let line = encode_line(&response)?;
        writer.write_all(line.as_bytes()).await?;
        writer.flush().await?;
    }
}

/// Route one request to its handler, producing the response to send back.
async fn handle_request<H: ToolHandler>(handler: &H, req: &Request) -> Response {
    match req.method.as_str() {
        "initialize" => ok(req.id, initialize_result()),
        "tools/list" => ok(req.id, json!({ "tools": handler.tools() })),
        "tools/call" => handle_call(handler, req).await,
        other => err(req.id, -32601, format!("Method not found: {other}")),
    }
}

/// Execute a `tools/call`, wrapping the result as MCP text content.
async fn handle_call<H: ToolHandler>(handler: &H, req: &Request) -> Response {
    let params = req.params.clone().unwrap_or(Value::Null);
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if name.is_empty() {
        return err(req.id, -32602, "Invalid params: missing tool name".into());
    }
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    match handler.call(name, arguments).await {
        Ok(text) => ok(
            req.id,
            json!({ "content": [{ "type": "text", "text": text }] }),
        ),
        Err(e) => err(req.id, -32000, e.to_string()),
    }
}

/// The `initialize` result advertising lopi's server identity + tool capability.
fn initialize_result() -> Value {
    json!({
        "protocolVersion": MCP_PROTOCOL_VERSION,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "lopi", "version": env!("CARGO_PKG_VERSION") },
    })
}

/// A success response carrying `result`.
fn ok(id: i64, result: Value) -> Response {
    Response {
        jsonrpc: crate::jsonrpc::JSONRPC_VERSION.to_string(),
        id: Some(id),
        result: Some(result),
        error: None,
    }
}

/// An error response with `code` and `message`.
fn err(id: i64, code: i64, message: String) -> Response {
    Response {
        jsonrpc: crate::jsonrpc::JSONRPC_VERSION.to_string(),
        id: Some(id),
        result: None,
        error: Some(RpcError {
            code,
            message,
            data: None,
        }),
    }
}

#[cfg(test)]
mod tests;
