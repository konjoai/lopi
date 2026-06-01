//! lopi-git — git branch management and path diff validation for agent runs.

/// Validates that a set of changed paths stays within configured allow/deny globs.
pub mod diff;
/// Manages git branches, worktrees, and rollbacks for isolated agent runs.
pub mod manager;

pub use diff::DiffChecker;
pub use manager::GitManager;
