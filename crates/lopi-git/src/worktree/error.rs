use std::path::PathBuf;
use thiserror::Error;

/// Errors from [`WorktreeManager`](super::WorktreeManager) and
/// [`Worktree`](super::Worktree) operations (Track C -- the error-taxonomy pass
/// finishing what Sprint S13R, Phase E started in `diff.rs`).
#[derive(Debug, Error)]
pub enum WorktreeError {
    /// Opening the repository at `path` failed.
    #[error("opening git repo at {path}: {source}")]
    OpenRepo {
        /// The path that failed to open as a git repository.
        path: PathBuf,
        #[source]
        source: git2::Error,
    },
    /// Creating the worktree root directory failed.
    #[error("creating worktree root {path}: {source}")]
    CreateRoot {
        /// The directory that could not be created.
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A `git` subprocess could not be spawned or its output read.
    #[error("invoking git: {0}")]
    Spawn(#[source] std::io::Error),
    /// A `git` subprocess ran but exited non-zero.
    #[error("git {args:?} failed: {stderr}")]
    CommandFailed {
        /// The `git` argument vector that failed.
        args: Vec<String>,
        /// Its captured stderr.
        stderr: String,
    },
    /// A named operation wrapping an inner `WorktreeError`, giving the
    /// original message a caller-relevant label (e.g. which branch or task
    /// an `add` was for) without needing a bespoke variant per call site.
    #[error("{context}: {source}")]
    Context {
        /// What this repo was doing when `source` occurred.
        context: String,
        #[source]
        source: Box<WorktreeError>,
    },
}
