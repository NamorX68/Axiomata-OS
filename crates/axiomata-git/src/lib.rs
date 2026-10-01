//! Git for Axiomata-OS, as a standalone layer: runs `git` ([`run`]) and reads its machine formats
//! ([`diff`], and the working tree's status). No Tauri, no database, no macOS-only code.
//!
//! The only thing that publishes is [`repo::push`]: the checked-out branch, to its upstream, never forced.

pub mod diff;
pub mod repo;
pub mod run;

/// A failed git operation.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// A `git` invocation failed, or git could not be run at all. Carries the command and git's
    /// own stderr, which together are what makes a git failure diagnosable.
    #[error("{command} failed: {reason}")]
    Git { command: String, reason: String },

    /// Caller-supplied input was rejected before it reached git.
    #[error("invalid {field}: {reason}")]
    Invalid { field: &'static str, reason: String },
}

pub type Result<T> = std::result::Result<T, GitError>;
