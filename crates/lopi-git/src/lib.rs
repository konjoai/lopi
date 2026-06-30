//! lopi-git — git branch management and path diff validation for agent runs.

/// Validates that a set of changed paths stays within configured allow/deny globs.
pub mod diff;
/// Manages git branches, worktrees, and rollbacks for isolated agent runs.
pub mod manager;
/// Pre-PR rebase onto the advanced default branch, with conflict reporting.
pub mod rebase;
/// True `git worktree` isolation: a dedicated checkout per parallel agent run.
pub mod worktree;

pub use diff::DiffChecker;
pub use manager::GitManager;
pub use worktree::{GcReport, Worktree, WorktreeManager};
