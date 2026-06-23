//! The stdio transport + session client that drives a live MCP server.
//!
//! [`McpClient`] speaks the [`protocol`](crate::protocol) over a server's
//! stdin/stdout: it writes newline-framed JSON-RPC requests and reads responses,
//! correlating them by id (interleaved notifications and log lines are skipped).
//! It is generic over the async reader/writer so it can be exercised over
//! in-memory pipes in tests; [`McpClient::spawn`] is the production constructor
//! that launches a server process and wires its piped stdio.
//!
//! Calls are serial — one request in flight at a time — which is all an agent's
//! tool-discovery and tool-call traffic needs, and keeps correlation trivial.

use crate::jsonrpc::{encode_line, IdGen, Response};
use crate::protocol::{
    call_tool_request, initialize_request, initialized_notification, list_tools_request,
    parse_tool_text, parse_tools, McpTool,
};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};

/// A client connected to a spawned server over its piped stdio — the concrete
/// type returned by [`McpClient::spawn`].
pub type StdioClient = McpClient<ChildStdin, BufReader<ChildStdout>>;

/// A connected MCP client over an async writer (server stdin) and buffered
/// reader (server stdout).
pub struct McpClient<W, R> {
    writer: W,
    reader: R,
    ids: IdGen,
    /// The server process, kept alive for the client's lifetime when spawned.
    /// `None` for clients built over arbitrary IO (e.g. test pipes).
    child: Option<tokio::process::Child>,
}

impl<W, R> McpClient<W, R>
where
    W: AsyncWriteExt + Unpin,
    R: AsyncBufReadExt + Unpin,
{
    /// Build a client over an arbitrary writer/reader pair (server stdin/stdout).
    pub fn new(writer: W, reader: R) -> Self {
        Self {
            writer,
            reader,
            ids: IdGen::default(),
            child: None,
        }
    }

    /// The OS process id of the spawned server, or `None` for a client built
    /// over arbitrary IO (e.g. test pipes) or once the server has exited.
    #[must_use]
    pub fn server_pid(&self) -> Option<u32> {
        self.child.as_ref().and_then(tokio::process::Child::id)
    }

    /// Complete the MCP handshake: `initialize`, then the
    /// `notifications/initialized` confirmation. Must be called before listing
    /// or calling tools.
    ///
    /// # Errors
    /// Returns `Err` if the server errors on `initialize` or the IO fails.
    pub async fn initialize(&mut self, client_name: &str, client_version: &str) -> Result<()> {
        let id = self.ids.next();
        self.request(initialize_request(id, client_name, client_version))
            .await
            .context("MCP initialize")?;
        let line = encode_line(&initialized_notification())?;
        self.write_line(&line).await?;
        Ok(())
    }

    /// Enumerate the server's tools (`tools/list`).
    ///
    /// # Errors
    /// Returns `Err` if the server errors or the IO fails.
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>> {
        let id = self.ids.next();
        let result = self.request(list_tools_request(id)).await?;
        Ok(parse_tools(&result))
    }

    /// Call tool `name` with `arguments`, returning its concatenated text output.
    ///
    /// # Errors
    /// Returns `Err` if the server reports a tool error or the IO fails.
    pub async fn call_tool(&mut self, name: &str, arguments: Value) -> Result<String> {
        let id = self.ids.next();
        let result = self.request(call_tool_request(id, name, arguments)).await?;
        Ok(parse_tool_text(&result))
    }

    /// Send a request and read until the response with the matching id, skipping
    /// interleaved notifications/log lines.
    async fn request(&mut self, req: crate::jsonrpc::Request) -> Result<Value> {
        let want = req.id;
        let line = encode_line(&req)?;
        self.write_line(&line).await?;
        let mut buf = String::new();
        loop {
            buf.clear();
            let n = self
                .reader
                .read_line(&mut buf)
                .await
                .context("reading MCP response")?;
            if n == 0 {
                bail!("MCP server closed the connection before responding to id {want}");
            }
            let trimmed = buf.trim();
            match serde_json::from_str::<Response>(trimmed) {
                // A response for our id — return its result or propagate the error.
                Ok(resp) if resp.id == want => return resp.into_result().map_err(Into::into),
                // A response for some other id, or a non-response line
                // (notification / log) — keep reading.
                _ => continue,
            }
        }
    }

    /// Write a pre-framed line to the server and flush.
    async fn write_line(&mut self, line: &str) -> Result<()> {
        self.writer
            .write_all(line.as_bytes())
            .await
            .context("writing to MCP server")?;
        self.writer.flush().await.context("flushing MCP server")?;
        Ok(())
    }
}

impl McpClient<ChildStdin, BufReader<ChildStdout>> {
    /// Launch `program args…` as an MCP server and connect to its piped stdio.
    /// The process is killed when the client is dropped.
    ///
    /// # Errors
    /// Returns `Err` if the process cannot be spawned or its stdio cannot be
    /// captured.
    pub fn spawn(program: &str, args: &[String]) -> Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("spawning MCP server `{program}`"))?;
        let stdin = child
            .stdin
            .take()
            .context("MCP server stdin was not piped")?;
        let stdout = child
            .stdout
            .take()
            .context("MCP server stdout was not piped")?;
        Ok(Self {
            writer: stdin,
            reader: BufReader::new(stdout),
            ids: IdGen::default(),
            child: Some(child),
        })
    }
}

#[cfg(test)]
mod tests;
