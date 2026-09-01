//! Shared error type for the Axiomata-OS core engine.

use std::path::PathBuf;
use std::time::Duration;

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

    /// A skill's frontmatter or a routine referenced an agent backend
    /// identifier that is neither `"claude-code"` nor `"ollama"`.
    #[error("unknown agent backend {backend:?} (expected \"claude-code\" or \"ollama\")")]
    UnknownAgentBackend { backend: String },

    /// The agent child process could not be spawned or waited on.
    #[error("failed to run the {backend} agent ({program}): {source}")]
    AgentSpawn {
        backend: &'static str,
        program: &'static str,
        #[source]
        source: std::io::Error,
    },

    /// The agent did not finish within its configured timeout.
    #[error("the {backend} agent timed out after {timeout:?}")]
    AgentTimeout {
        backend: &'static str,
        timeout: Duration,
    },

    /// A backend that talks to an HTTP API (Ollama) returned an error.
    #[error("the {backend} agent API returned an error: {message}")]
    AgentApi {
        backend: &'static str,
        message: String,
    },

    /// A `SKILL.md` file could not be read, or its frontmatter was missing or
    /// malformed.
    #[error("invalid skill at {path}: {reason}")]
    InvalidSkill { path: PathBuf, reason: String },

    /// A skill was requested by name but not found in either skill directory.
    #[error("no skill named {name:?} found")]
    SkillNotFound { name: String },
}
