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

/// JSON-RPC 2.0 envelope types and the MCP newline framing.
pub mod jsonrpc;
/// MCP message construction and result parsing.
pub mod protocol;

pub use jsonrpc::{encode_line, IdGen, Notification, Request, Response, RpcError};
pub use protocol::{
    call_tool_request, initialize_request, initialized_notification, list_tools_request,
    parse_tool_text, parse_tools, McpTool, MCP_PROTOCOL_VERSION,
};
