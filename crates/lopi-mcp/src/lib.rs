//! lopi-mcp — a Model Context Protocol (MCP) client for lopi agents.
//!
//! MCP is how loop-engineering's "plugins & connectors" let an agent **act in
//! your environment**: lopi spawns an MCP server, discovers its tools, and calls
//! them. This crate builds that up in transport-agnostic layers so each is
//! testable on its own:
//!
//! - [`jsonrpc`] — the JSON-RPC 2.0 envelope types and newline framing.
//! - [`protocol`] — MCP message construction (`initialize`, `tools/list`,
//!   `tools/call`) and result parsing ([`McpTool`]).
//!
//! The stdio transport and the session client that drives a live server are
//! layered on top of these in later sprints.

/// Bridge discovered MCP tools into lopi's `ToolRegistry`.
pub mod bridge;
/// The stdio transport + session client that drives a live MCP server.
pub mod client;
/// MCP server configuration parsed from `.lopi/loop.toml`.
pub mod config;
/// JSON-RPC 2.0 envelope types and the MCP newline framing.
pub mod jsonrpc;
/// MCP message construction and result parsing.
pub mod protocol;
/// Expose lopi as an MCP server — answer `initialize`/`tools/list`/`tools/call`.
pub mod server;

pub use bridge::{discover_and_register, register_server_tools, register_tools, tool_spec};
pub use client::{McpClient, StdioClient};
pub use config::{load_servers, parse_servers, McpServerSpec};
pub use jsonrpc::{encode_line, IdGen, Notification, Request, Response, RpcError};
pub use protocol::{
    call_tool_request, initialize_request, initialized_notification, list_tools_request,
    parse_tool_text, parse_tools, McpTool, MCP_PROTOCOL_VERSION,
};
pub use server::{serve, ToolHandler};
