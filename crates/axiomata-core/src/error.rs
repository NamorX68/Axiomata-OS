//! Shared error type for the Axiomata-OS core engine.

use std::path::PathBuf;

use thiserror::Error;

/// Errors that can occur in the Axiomata-OS core engine.
#[derive(Debug, Error)]
pub enum AxiomataError {
    /// A filesystem operation (read, write, or directory creation) failed.
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// `~/.axiomata/config.toml` exists but could not be parsed as TOML.
    #[error("failed to parse config at {path}: {source}")]
    ConfigParse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    /// The in-memory config could not be serialized back to TOML.
    #[error("failed to serialize config: {source}")]
    ConfigSerialize {
        #[source]
        source: toml::ser::Error,
    },

    /// A SQLite operation failed.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// Applying a specific schema migration failed.
    #[error("migration {version} failed: {source}")]
    Migration {
        version: u32,
        #[source]
        source: rusqlite::Error,
    },
}
