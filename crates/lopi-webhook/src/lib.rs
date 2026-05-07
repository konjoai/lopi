//! GitHub webhook receiver — ingests CI-failure and PR-review events and injects tasks into the lopi queue.

/// GitHub webhook axum router and event handlers.
pub mod github;
/// Start the GitHub webhook HTTP server on the given address.
pub use github::serve;
