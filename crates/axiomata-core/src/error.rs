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

    /// A second trigger for the same skill/prompt name arrived while an
    /// earlier one was still running (a mount-time auto-refresh racing a
    /// manual ↻, a Routine firing mid-refresh, a dev hot-reload remounting
    /// before the previous run finished). Deliberately `Err`, not an
    /// `Ok(Failed RunRecord)` — this never reaches `runlog::record_run` /
    /// the routine-run history, so it can't shadow the real (in-flight or
    /// already-successful) run as the "latest run" a connector module's
    /// `list_runs` lookup would otherwise find.
    #[error("{name} is already running (triggered elsewhere) — try again once it finishes")]
    AlreadyRunning { name: String },

    /// A backend that talks to an HTTP API (Ollama) returned an error.
    #[error("the {backend} agent API returned an error: {message}")]
    AgentApi {
        backend: &'static str,
        message: String,
    },

    /// The model to run with is missing or malformed for the active provider.
    /// The Claude Code CLI is *not* spawned in this case: with a non-Anthropic
    /// `ANTHROPIC_BASE_URL` in effect, letting the CLI fall back to its own
    /// built-in default model would route that default through the paid proxy
    /// and bill it — a config typo must fail loudly, not silently cost money.
    #[error("{reason}")]
    InvalidAgentModel { reason: String },

    /// Today's spend through a paid model-routing provider has reached the
    /// configured daily cap. The turn is *not* spawned — see
    /// [`crate::spend::guard_redirected_turn`] and provider-hardening
    /// checkpoint 5.
    #[error(
        "{provider} daily spend cap of ${cap_usd:.2} reached (${spent_usd:.2} so far today) — \
         raise it in Settings or wait until tomorrow"
    )]
    SpendCapReached {
        provider: String,
        cap_usd: f64,
        spent_usd: f64,
    },

    /// A `SKILL.md` file could not be read, or its frontmatter was missing or
    /// malformed.
    #[error("invalid skill at {path}: {reason}")]
    InvalidSkill { path: PathBuf, reason: String },

    /// A skill was requested by name but not found in either skill directory.
    #[error("no skill named {name:?} found")]
    SkillNotFound { name: String },

    /// A `CLAUDE.md` file has a start router marker but no matching end marker,
    /// so its generated block cannot be replaced unambiguously.
    #[error("malformed router block in {path}: {reason}")]
    InvalidRouter { path: PathBuf, reason: String },

    /// `workspace_root` resolves to a location the memory router refuses to
    /// write into (the filesystem root, the home directory).
    #[error("refusing to run the memory router on {path} — set a dedicated workspace folder")]
    UnsafeWorkspaceRoot { path: PathBuf },

    /// A module-action request or response in the file queue could not be
    /// parsed, or the dashboard did not answer within the timeout.
    #[error("module action {id}: {reason}")]
    ModuleAction { id: String, reason: String },

    /// A workspace-relative path handed to `crate::workspace` is absolute,
    /// climbs out of the workspace (`..`, symlink), is a directory, a symlink,
    /// too large, or not UTF-8.
    #[error("invalid workspace file {path}: {reason}")]
    InvalidWorkspacePath { path: PathBuf, reason: String },

    /// `~/.axiomata/dashboard.json` (or the state handed in to save) is not a
    /// JSON object with a numeric `version`, or the file is a symlink.
    #[error("invalid dashboard state at {path}: {reason}")]
    InvalidDashboardState { path: PathBuf, reason: String },

    /// A new (or edited) routine's `name`, `target`, or `backend` failed
    /// field validation, or its `name` is already taken — user input, always
    /// fixable by supplying different values. Not for a cron expression
    /// specifically (see [`Self::InvalidCron`]) or a value that came back
    /// corrupted from storage (see [`Self::CorruptRoutineRow`]).
    #[error("invalid routine: {reason}")]
    InvalidRoutine { reason: String },

    /// A cron expression could not be parsed. Raised for a *freshly supplied*
    /// expression (creating or editing a routine) — user input, distinct from
    /// [`Self::CorruptRoutineRow`], which is for a *previously valid, already
    /// stored* expression that no longer parses.
    #[error("invalid cron expression {expr:?}: {reason}")]
    InvalidCron { expr: String, reason: String },

    /// A row read back from the `routines` or `routine_runs` table holds a
    /// value the routines module does not understand (an unknown
    /// `target_type` or run `status`, or a stored cron expression that no
    /// longer parses) — a data-integrity problem, not user input; only a
    /// hand-edited row or a schema/format change should ever produce this.
    #[error("routine row {id} is corrupt: {reason}")]
    CorruptRoutineRow { id: i64, reason: String },
}
