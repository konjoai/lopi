//! lopi-ui: ratatui TUI dashboard and axum web/JSON API for the lopi orchestrator.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// Sprint T0 — the `TuiClient` trait plus its `RemoteClient`/`LocalClient`
/// implementations, giving the TUI a write-capable client for the same
/// stack/task API the web/macOS/iOS clients already use.
pub mod client;
/// Ratatui terminal UI dashboard for live agent monitoring.
pub mod tui;
/// Axum-based HTTP API and WebSocket/SSE streaming layer.
pub mod web;
