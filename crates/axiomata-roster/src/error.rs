//! Errors of the roster.

use std::path::PathBuf;

/// Everything that can go wrong reading, checking or writing engines and roles.
#[derive(Debug, thiserror::Error)]
pub enum RosterError {
    /// Caller-supplied or stored input was rejected. Names the field so a form can point at it.
    #[error("invalid {field}: {reason}")]
    Invalid { field: &'static str, reason: String },

    /// An `AGENT.md` could not be used. Names the file, because one bad file is skipped, not fatal.
    #[error("{path}: {reason}")]
    BadFile { path: PathBuf, reason: String },

    /// A file system operation failed.
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    /// The project's role overrides changed after the owner looked at them.
    #[error("the project's agent files changed since they were shown; look at them again")]
    Changed,
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, RosterError>;

impl RosterError {
    pub(crate) fn invalid(field: &'static str, reason: impl Into<String>) -> Self {
        RosterError::Invalid {
            field,
            reason: reason.into(),
        }
    }
}
