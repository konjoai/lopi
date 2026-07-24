//! lopi-ui: ratatui TUI dashboard and axum web/JSON API for the lopi orchestrator.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

/// Ratatui terminal UI dashboard for live agent monitoring.
pub mod tui;
/// Axum-based HTTP API and WebSocket/SSE streaming layer.
pub mod web;
