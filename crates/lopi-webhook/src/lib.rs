//! GitHub webhook receiver — ingests CI-failure, PR-review, and issue events,
//! runs automated triage, and injects tasks into the lopi queue.

/// GitHub webhook axum router and event handlers.
pub mod github;
pub(crate) mod issue;
pub(crate) mod issue_triage;

pub use github::{serve, TriageConfig};
