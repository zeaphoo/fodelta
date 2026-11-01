//! Core library for fodelta.
//!
//! The crate is intentionally split into small modules: the CLI only parses
//! arguments and renders results, while repository, object storage, worktree
//! scanning and history logic live here so they can be tested without a
//! terminal.

use std::path::PathBuf;

use thiserror::Error;

pub mod cli;

/// Convenient result alias used across the library.
pub type Result<T> = std::result::Result<T, FodeltaError>;

/// Process exit status. `Diff` is reserved for `diff --exit-code` when the two
/// sides differ, so scripts can distinguish "differences found" from errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitCode {
    Success = 0,
    Diff = 1,
    Error = 2,
}

impl ExitCode {
    pub const fn code(self) -> i32 {
        self as i32
    }
}

/// All errors surfaced by fodelta. Every variant maps to exit code 2; the only
/// exit code 1 is produced by `diff --exit-code` through [`ExitCode::Diff`].
#[derive(Debug, Error)]
pub enum FodeltaError {
    #[error("not a fodelta repository (or any parent up to {start})")]
    NotARepository { start: PathBuf },

    #[error("a fodelta repository already exists at {root}")]
    RepositoryExists { root: PathBuf },

    #[error("unsupported repository format version {found} (this binary supports {supported})")]
    UnsupportedFormat { found: u32, supported: u32 },

    #[error("object not found: {oid}")]
    ObjectNotFound { oid: String },

    #[error("corrupt object {oid}: {reason}")]
    CorruptObject { oid: String, reason: String },

    #[error("path escapes repository: {path}")]
    PathEscapes { path: PathBuf },

    #[error("path is ignored: {path}")]
    PathIgnored { path: PathBuf },

    #[error("path did not match any files: {path}")]
    PathNotMatched { path: PathBuf },

    #[error("path does not exist in {rev}: {path}")]
    PathMissingInRev { rev: String, path: PathBuf },

    #[error("nothing to commit (working tree matches HEAD)")]
    NothingToCommit,

    #[error("nothing to rollback (HEAD already matches {rev})")]
    NothingToRollback { rev: String },

    #[error("another fodelta process is running (lock: {path})")]
    Locked { path: PathBuf },

    #[error("invalid revision '{rev}': {reason}")]
    InvalidRevision { rev: String, reason: String },

    #[error("ambiguous revision '{rev}': candidates {candidates}")]
    AmbiguousRevision { rev: String, candidates: String },

    #[error("HEAD does not point to a commit yet")]
    UnbornHead,

    #[error("invalid configuration: {0}")]
    Config(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),
}

impl FodeltaError {
    /// Every [`FodeltaError`] is a failure, therefore exit code 2.
    pub const fn exit_code(&self) -> i32 {
        ExitCode::Error.code()
    }
}

/// Parse arguments and run the requested command.
pub fn run(cli: cli::Cli) -> Result<ExitCode> {
    cli::dispatch(cli)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_map_to_exit_code_two() {
        let err = FodeltaError::ObjectNotFound {
            oid: "deadbeef".to_string(),
        };
        assert_eq!(err.exit_code(), 2);
        assert_eq!(ExitCode::Error.code(), 2);
    }

    #[test]
    fn diff_exit_code_is_one() {
        assert_eq!(ExitCode::Diff.code(), 1);
        assert_eq!(ExitCode::Success.code(), 0);
    }
}
