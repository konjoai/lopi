//! Bridge discovered MCP tools into lopi's `ToolRegistry`.
//!
//! This is where an MCP server's capabilities become tools the agent loop can
//! invoke: each [`McpTool`] is converted to a `ToolSpec` (namespaced by its
//! server so two servers can expose the same tool name) and registered. The
//! conversion is pure; the discovery+register flow is generic over the client's
//! IO, so it is exercised over in-memory pipes rather than a live server.

use crate::config::McpServerSpec;
use crate::protocol::McpTool;
use crate::McpClient;
use anyhow::Result;
use lopi_tools::{ToolRegistry, ToolSpec};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

/// Convert a discovered MCP tool into a registry [`ToolSpec`], namespacing the
/// name as `"<server>.<tool>"`. MCP's `inputSchema` becomes the spec's
/// parameters; a non-object schema is coerced to an empty object so it always
/// satisfies the registry's "parameters must be a JSON object" contract.
#[must_use]
pub fn tool_spec(server_name: &str, tool: &McpTool) -> ToolSpec {
    let name = format!("{server_name}.{}", tool.name);
    let parameters = if tool.input_schema.is_object() {
        tool.input_schema.clone()
    } else {
        serde_json::json!({})
    };
    ToolSpec::new(name, tool.description.clone(), parameters)
}

/// Register `tools` (from `server_name`) into `registry`, returning the
/// namespaced names registered.
///
/// # Errors
/// Returns `Err` if a tool fails the registry's validation or its disk flush.
pub async fn register_tools(
    registry: &ToolRegistry,
    server_name: &str,
    tools: &[McpTool],
) -> Result<Vec<String>> {
    let mut names = Vec::with_capacity(tools.len());
    for tool in tools {
        let spec = tool_spec(server_name, tool);
        let name = spec.name.clone();
        registry.register(spec).await?;
        names.push(name);
    }
    Ok(names)
}

/// Discover `client`'s tools (`tools/list`) and register them under
/// `server_name`. The client must already be initialized.
///
/// # Errors
/// Returns `Err` if discovery or registration fails.
pub async fn discover_and_register<W, R>(
    client: &mut McpClient<W, R>,
    server_name: &str,
    registry: &ToolRegistry,
) -> Result<Vec<String>>
where
    W: AsyncWriteExt + Unpin,
    R: AsyncBufReadExt + Unpin,
{
    let tools = client.list_tools().await?;
    register_tools(registry, server_name, &tools).await
}

/// Spawn `spec`'s server, complete the handshake, and register its tools into
/// `registry` — the end-to-end "connect a configured server" entry point.
///
/// # Errors
/// Returns `Err` if the server cannot be spawned, the handshake fails, or
/// registration fails.
pub async fn register_server_tools(
    spec: &McpServerSpec,
    registry: &ToolRegistry,
    client_name: &str,
    client_version: &str,
) -> Result<Vec<String>> {
    let mut client = spec.connect()?;
    client.initialize(client_name, client_version).await?;
    discover_and_register(&mut client, &spec.name, registry).await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::{discover_and_register, register_tools, tool_spec};
    use crate::McpClient;
    use lopi_tools::ToolRegistry;
    use serde_json::{json, Value};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    fn mctool(name: &str, schema: Value) -> crate::protocol::McpTool {
        crate::protocol::McpTool {
            name: name.into(),
            description: format!("the {name} tool"),
            input_schema: schema,
            meta: None,
        }
    }

    #[test]
    fn tool_spec_namespaces_and_keeps_object_schema() {
        let schema = json!({ "type": "object", "properties": { "q": { "type": "string" } } });
        let spec = tool_spec("github", &mctool("search", schema.clone()));
        assert_eq!(spec.name, "github.search");
        assert_eq!(spec.description, "the search tool");
        assert_eq!(spec.parameters, schema);
    }

    #[test]
    fn tool_spec_coerces_non_object_schema_to_empty_object() {
        // A tool with no inputSchema (Null) still produces a valid object spec.
        let spec = tool_spec("fs", &mctool("noop", Value::Null));
        assert_eq!(spec.name, "fs.noop");
        assert_eq!(spec.parameters, json!({}));
    }

    #[tokio::test]
    async fn register_tools_writes_namespaced_specs() {
        let dir = tempfile::TempDir::new().unwrap();
        let registry = ToolRegistry::new(dir.path().join("tools.json"));
        let tools = vec![
            mctool("read", json!({ "type": "object" })),
            mctool("write", Value::Null),
        ];

        let names = register_tools(&registry, "fs", &tools).await.unwrap();
        assert_eq!(names, vec!["fs.read", "fs.write"]);
        assert!(registry.get("fs.read").await.is_some());
        let write = registry.get("fs.write").await.unwrap();
        assert_eq!(write.parameters, json!({}), "coerced schema persisted");
    }

    #[tokio::test]
    async fn discover_and_register_over_pipes() {
        // Mock server answers a single tools/list with two tools.
        let (client_w, server_r) = tokio::io::duplex(8192);
        let (mut server_w, client_r) = tokio::io::duplex(8192);
        tokio::spawn(async move {
            let mut lines = BufReader::new(server_r).lines();
            let line = lines.next_line().await.unwrap().unwrap();
            let id = serde_json::from_str::<Value>(&line).unwrap()["id"]
                .as_i64()
                .unwrap();
            let resp = json!({
                "jsonrpc": "2.0", "id": id,
                "result": { "tools": [
                    { "name": "a", "description": "A" },
                    { "name": "b", "description": "B", "inputSchema": { "type": "object" } }
                ] }
            });
            let mut s = serde_json::to_string(&resp).unwrap();
            s.push('\n');
            server_w.write_all(s.as_bytes()).await.unwrap();
            server_w.flush().await.unwrap();
        });

        let mut client = McpClient::new(client_w, BufReader::new(client_r));
        let dir = tempfile::TempDir::new().unwrap();
        let registry = ToolRegistry::new(dir.path().join("tools.json"));

        let names = discover_and_register(&mut client, "srv", &registry)
            .await
            .unwrap();
        assert_eq!(names, vec!["srv.a", "srv.b"]);
        assert!(registry.get("srv.a").await.is_some());
        assert!(registry.get("srv.b").await.is_some());
    }
}
