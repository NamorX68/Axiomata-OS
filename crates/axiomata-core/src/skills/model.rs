//! Data types for skill runs.
//!
//! The persistence for these lives in [`crate::skills::runlog`]; they are split
//! out here so the shape of a recorded run is separate from the code that
//! writes and reads it (mirrors [`crate::routines::model`] vs
//! [`crate::routines::store`]).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Outcome of a skill run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    /// The agent ran and reported success (exit code `0`).
    Success,
    /// The agent reported a non-zero exit code, or could not be run at all.
    Failed,
}

impl RunStatus {
    /// The lowercase string form stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed => "failed",
        }
    }

    /// Parses the database string form, erroring on any unexpected value rather
    /// than silently coercing it (schema drift / a future third status should
    /// surface, not be swallowed).
    pub fn from_db_str(raw: &str, column: usize) -> rusqlite::Result<Self> {
        match raw {
            "success" => Ok(Self::Success),
            "failed" => Ok(Self::Failed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                format!("unknown run status {other:?}").into(),
            )),
        }
    }
}

/// What triggered a run: a person, or a routine firing unattended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunSource {
    /// The CLI's `run-skill`, the Tauri dashboard's "run now", or a
    /// chat/instruct turn — someone asked for it directly, right now.
    #[default]
    Manual,
    /// `routines::scheduler` fired it unattended, on its own schedule.
    Routine,
}

impl RunSource {
    /// The lowercase string form stored in the database.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Routine => "routine",
        }
    }

    /// Parses the database string form, erroring on any unexpected value
    /// rather than silently coercing it — mirrors [`RunStatus::from_db_str`].
    pub fn from_db_str(raw: &str, column: usize) -> rusqlite::Result<Self> {
        match raw {
            "manual" => Ok(Self::Manual),
            "routine" => Ok(Self::Routine),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                column,
                rusqlite::types::Type::Text,
                format!("unknown run source {other:?}").into(),
            )),
        }
    }
}

/// One recorded skill run: a row in the `runs` table and a line in `runs.log`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    /// Database row id. `None` before the record is persisted, `Some` after.
    pub id: Option<i64>,
    /// Skill name as resolved at run time.
    pub skill_name: String,
    /// `"claude-code"` or `"ollama"`.
    pub backend: String,
    /// Overall outcome.
    pub status: RunStatus,
    /// Process/synthetic exit code. `None` when the agent produced no result at
    /// all (spawn failure, timeout, API error).
    pub exit_code: Option<i32>,
    /// Wall-clock duration of the run, in milliseconds.
    pub duration_ms: u64,
    /// Captured agent stdout / completion text.
    pub stdout: String,
    /// Captured agent stderr.
    pub stderr: String,
    /// Failure message for the case where no agent result was produced;
    /// `None` otherwise.
    pub error: Option<String>,
    /// When the run started.
    pub started_at: DateTime<Utc>,
    /// When the run finished (or failed).
    pub finished_at: DateTime<Utc>,
    /// Who/what triggered this run. `runner::record_from_result` and
    /// `runner::failure_record` (the only two places a `RunRecord` is built)
    /// have no way to know this themselves — they default to
    /// [`RunSource::Manual`], and `routines::scheduler::fire_one` overrides it
    /// to [`RunSource::Routine`] right before recording a routine's firing.
    #[serde(default)]
    pub source: RunSource,
}

/// The slim projection of a [`RunRecord`] for history-list views (the dashboard
/// card, `axiomata-cli list-runs`). Deliberately omits the potentially large
/// captured `stdout` / `stderr` — fetch the full record with
/// [`crate::skills::runlog::get_run`] to show a single run's output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    /// Database row id.
    pub id: i64,
    /// Skill name as resolved at run time.
    pub skill_name: String,
    /// `"claude-code"` or `"ollama"`.
    pub backend: String,
    /// Overall outcome.
    pub status: RunStatus,
    /// Process/synthetic exit code, or `None` when no agent result was produced.
    pub exit_code: Option<i32>,
    /// Wall-clock duration of the run, in milliseconds.
    pub duration_ms: u64,
    /// Short failure message when no agent result was produced; `None` otherwise.
    pub error: Option<String>,
    /// When the run started.
    pub started_at: DateTime<Utc>,
    /// Who/what triggered this run — see [`RunRecord::source`].
    pub source: RunSource,
}
