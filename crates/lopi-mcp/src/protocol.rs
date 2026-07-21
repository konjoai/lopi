//! MCP message construction + result parsing on top of [`jsonrpc`](crate::jsonrpc).
//!
//! These pure helpers build the requests of an MCP session — `initialize`, the
//! `notifications/initialized` handshake completion, `tools/list`, and
//! `tools/call` — and parse the results lopi cares about ([`McpTool`] entries
//! and a tool call's text output). Keeping them free of any transport makes the
//! protocol shape testable in isolation.

use crate::jsonrpc::{Notification, Request};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The MCP protocol revision lopi implements. `2024-11-05` predates the
/// extensions framework entirely — a host has no spec-compliant reason to
/// negotiate an extension (e.g. MCP Apps, SEP-1865) against a server still
/// declaring that revision. Bumped to the first stable revision the
/// extensions capabilities mechanism ships in.
pub const MCP_PROTOCOL_VERSION: &str = "2025-11-25";

/// A tool advertised by an MCP server — one entry of a `tools/list` result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpTool {
    /// The tool's unique name, used as the `tools/call` target.
    pub name: String,
    /// Human-readable description of what the tool does.
    #[serde(default)]
    pub description: String,
    /// JSON Schema describing the tool's arguments (the `inputSchema` member).
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Value,
    /// MCP Apps binding (SEP-1865) — e.g.
    /// `{"ui": {"resourceUri": "ui://lopi/stack-status"}}` when this tool has
    /// an inline widget a supporting host renders instead of plain text.
    /// `None` for every plain-text tool (the whole curated set except
    /// `lopi_get_stack_status`, MCPB-App-1).
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

/// A resource advertised by an MCP server — one entry of a `resources/list`
/// result. lopi's only resource today is the `ui://` widget bound to
/// `lopi_get_stack_status` (MCPB-App-1).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpResource {
    /// The resource's address, e.g. `"ui://lopi/stack-status"`.
    pub uri: String,
    /// Human-readable resource name.
    pub name: String,
    /// Optional longer description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// IANA media type of the resource's contents, e.g. `"text/html"`.
    #[serde(rename = "mimeType", default, skip_serializing_if = "String::is_empty")]
    pub mime_type: String,
}

/// One resource's contents, as returned by a `resources/read` call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpResourceContents {
    /// The resource's address, echoed back from the request.
    pub uri: String,
    /// IANA media type of `text`.
    #[serde(rename = "mimeType", default, skip_serializing_if = "String::is_empty")]
    pub mime_type: String,
    /// The resource's text contents (lopi only ever serves text/HTML
    /// resources — no binary `blob` variant is needed yet).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// Build the `initialize` request that opens an MCP session, advertising the
/// protocol revision and this client's identity.
#[must_use]
pub fn initialize_request(id: i64, client_name: &str, client_version: &str) -> Request {
    Request::new(
        id,
        "initialize",
        Some(json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": client_name, "version": client_version },
        })),
    )
}

/// The `notifications/initialized` notification a client sends after it has
/// processed the `initialize` response, completing the handshake.
#[must_use]
pub fn initialized_notification() -> Notification {
    Notification::new("notifications/initialized", None)
}

/// Build the `tools/list` request enumerating the server's tools.
#[must_use]
pub fn list_tools_request(id: i64) -> Request {
    Request::new(id, "tools/list", None)
}

/// Build a `tools/call` request invoking `name` with `arguments`.
#[must_use]
pub fn call_tool_request(id: i64, name: &str, arguments: Value) -> Request {
    Request::new(
        id,
        "tools/call",
        Some(json!({ "name": name, "arguments": arguments })),
    )
}

/// Parse a `tools/list` result value into the advertised [`McpTool`]s. Malformed
/// entries are skipped rather than failing the whole list.
#[must_use]
pub fn parse_tools(result: &Value) -> Vec<McpTool> {
    result
        .get("tools")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|t| serde_json::from_value(t.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Extract the text output of a `tools/call` result. MCP returns a `content`
/// array of typed blocks; the `text` blocks are concatenated in order.
#[must_use]
pub fn parse_tool_text(result: &Value) -> String {
    result
        .get("content")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn initialize_request_carries_protocol_and_client_info() {
        let req = initialize_request(1, "lopi", "0.1.0");
        assert_eq!(req.method, "initialize");
        let p = req.params.unwrap();
        assert_eq!(p["protocolVersion"], MCP_PROTOCOL_VERSION);
        assert_eq!(p["clientInfo"]["name"], "lopi");
        assert_eq!(p["clientInfo"]["version"], "0.1.0");
    }

    #[test]
    fn call_tool_request_wraps_name_and_arguments() {
        let req = call_tool_request(5, "search", json!({"q": "rust"}));
        assert_eq!(req.method, "tools/call");
        let p = req.params.unwrap();
        assert_eq!(p["name"], "search");
        assert_eq!(p["arguments"]["q"], "rust");
    }

    #[test]
    fn list_tools_request_has_no_params() {
        assert!(list_tools_request(2).params.is_none());
    }

    #[test]
    fn parse_tools_reads_entries_and_skips_malformed() {
        let result = json!({
            "tools": [
                { "name": "a", "description": "first", "inputSchema": {"type": "object"} },
                { "description": "no name — skipped" },
                { "name": "b" }
            ]
        });
        let tools = parse_tools(&result);
        assert_eq!(tools.len(), 2, "malformed (nameless) entry skipped");
        assert_eq!(tools[0].name, "a");
        assert_eq!(tools[0].description, "first");
        assert_eq!(tools[0].input_schema["type"], "object");
        // Missing optional fields default cleanly.
        assert_eq!(tools[1].name, "b");
        assert_eq!(tools[1].description, "");
    }

    #[test]
    fn parse_tools_empty_when_absent() {
        assert!(parse_tools(&json!({})).is_empty());
        assert!(parse_tools(&json!({"tools": "not-an-array"})).is_empty());
    }

    #[test]
    fn parse_tool_text_concatenates_text_blocks() {
        let result = json!({
            "content": [
                { "type": "text", "text": "Hello, " },
                { "type": "image", "data": "..." },
                { "type": "text", "text": "world" }
            ]
        });
        assert_eq!(parse_tool_text(&result), "Hello, world");
    }

    #[test]
    fn parse_tool_text_empty_when_no_content() {
        assert_eq!(parse_tool_text(&json!({})), "");
    }

    #[test]
    fn initialized_notification_shape() {
        let n = initialized_notification();
        assert_eq!(n.method, "notifications/initialized");
        assert!(n.params.is_none());
    }
}
