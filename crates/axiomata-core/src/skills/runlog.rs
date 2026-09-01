//! Writes and reads the skill-run history.
//!
//! Every run is recorded in two places: a row in the SQLite `runs` table (what
//! the UI and CLI query) and an appended JSON line in
//! `~/.axiomata/logs/runs.log` (for `tail -f`-style inspection). The two are
//! kept in sync by [`record_run`].
//!
//! Implemented in M1.

use std::fs::OpenOptions;
use std::io::Write;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::AxiomataError;
use crate::paths;

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
    fn from_db_str(raw: &str, column: usize) -> rusqlite::Result<Self> {
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
}

/// The slim projection of a [`RunRecord`] for history-list views (the dashboard
/// card, `axiomata-cli list-runs`). Deliberately omits the potentially large
/// captured `stdout` / `stderr` — fetch the full record with [`get_run`] to
/// show a single run's output.
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
}

/// Persists `record` to both the database and the JSONL log.
///
/// The database row is written first; on success the JSONL line is appended.
/// Returns the record with its assigned [`RunRecord::id`] set.
///
/// Errors:
///     [`AxiomataError::Database`] if the row insert fails; [`AxiomataError::Io`]
///     if the log file cannot be appended to (the database row is already
///     committed in that case).
pub fn record_run(db: &Connection, mut record: RunRecord) -> Result<RunRecord, AxiomataError> {
    let id = insert_row(db, &record)?;
    record.id = Some(id);
    append_jsonl(&record)?;
    Ok(record)
}

/// Inserts one row into `runs` and returns its id.
fn insert_row(db: &Connection, record: &RunRecord) -> Result<i64, AxiomataError> {
    db.execute(
        "INSERT INTO runs \
         (skill_name, backend, status, exit_code, duration_ms, \
          stdout, stderr, error, started_at, finished_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            record.skill_name,
            record.backend,
            record.status.as_str(),
            record.exit_code,
            record.duration_ms,
            record.stdout,
            record.stderr,
            record.error,
            record.started_at.to_rfc3339(),
            record.finished_at.to_rfc3339(),
        ],
    )?;
    Ok(db.last_insert_rowid())
}

/// Appends `record` as one JSON line to `~/.axiomata/logs/runs.log`.
fn append_jsonl(record: &RunRecord) -> Result<(), AxiomataError> {
    let path = paths::runs_log_path();
    let line = serde_json::to_string(record).map_err(|err| AxiomataError::Io {
        path: path.clone(),
        source: std::io::Error::other(err),
    })?;

    let mut options = OpenOptions::new();
    options.create(true).append(true);
    // The log holds captured agent output; keep it owner-only on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|source| AxiomataError::Io {
        path: path.clone(),
        source,
    })?;
    writeln!(file, "{line}").map_err(|source| AxiomataError::Io { path, source })
}

/// Hard upper bound on how many rows [`list_runs`] will return, whatever
/// `limit` the caller asks for — a history list never needs more, and it keeps
/// an unbounded request from pulling the whole table.
pub const MAX_RUN_LIMIT: usize = 500;

/// Returns the most recent runs as [`RunSummary`] values, newest first, capped
/// at `min(limit, MAX_RUN_LIMIT)`.
///
/// Errors:
///     [`AxiomataError::Database`] if the query fails.
pub fn list_runs(db: &Connection, limit: usize) -> Result<Vec<RunSummary>, AxiomataError> {
    let limit = limit.min(MAX_RUN_LIMIT);
    let mut stmt = db.prepare(
        "SELECT id, skill_name, backend, status, exit_code, duration_ms, error, started_at \
         FROM runs ORDER BY started_at DESC, id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit as i64], row_to_summary)?;

    let mut summaries = Vec::new();
    for row in rows {
        summaries.push(row?);
    }
    Ok(summaries)
}

/// Fetches one full [`RunRecord`] by id, or `None` if there is no such row.
///
/// Errors:
///     [`AxiomataError::Database`] if the query fails.
pub fn get_run(db: &Connection, id: i64) -> Result<Option<RunRecord>, AxiomataError> {
    let mut stmt = db.prepare(
        "SELECT id, skill_name, backend, status, exit_code, \
         duration_ms, stdout, stderr, error, started_at, finished_at \
         FROM runs WHERE id = ?1",
    )?;
    match stmt.query_row([id], row_to_record) {
        Ok(record) => Ok(Some(record)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// Maps one summary-projection row onto a [`RunSummary`].
fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunSummary> {
    let status: String = row.get(3)?;
    let started_at: String = row.get(7)?;
    Ok(RunSummary {
        id: row.get(0)?,
        skill_name: row.get(1)?,
        backend: row.get(2)?,
        status: RunStatus::from_db_str(&status, 3)?,
        exit_code: row.get(4)?,
        duration_ms: row.get::<_, i64>(5)? as u64,
        error: row.get(6)?,
        started_at: parse_timestamp(row, 7, &started_at)?,
    })
}

/// Maps one `runs` row onto a [`RunRecord`].
fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    let status: String = row.get(3)?;
    let started_at: String = row.get(9)?;
    let finished_at: String = row.get(10)?;

    Ok(RunRecord {
        id: Some(row.get(0)?),
        skill_name: row.get(1)?,
        backend: row.get(2)?,
        status: RunStatus::from_db_str(&status, 3)?,
        exit_code: row.get(4)?,
        duration_ms: row.get::<_, i64>(5)? as u64,
        stdout: row.get(6)?,
        stderr: row.get(7)?,
        error: row.get(8)?,
        started_at: parse_timestamp(row, 9, &started_at)?,
        finished_at: parse_timestamp(row, 10, &finished_at)?,
    })
}

/// Parses an RFC 3339 timestamp column into a UTC `DateTime`, turning a bad
/// value into a column decode error rather than a panic.
fn parse_timestamp(
    _row: &rusqlite::Row<'_>,
    index: usize,
    raw: &str,
) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(err),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;
    use std::fs;

    /// A representative successful record.
    fn sample_record(skill: &str) -> RunRecord {
        let now = Utc::now();
        RunRecord {
            id: None,
            skill_name: skill.to_owned(),
            backend: "ollama".to_owned(),
            status: RunStatus::Success,
            exit_code: Some(0),
            duration_ms: 42,
            stdout: "done".to_owned(),
            stderr: String::new(),
            error: None,
            started_at: now,
            finished_at: now,
        }
    }

    #[test]
    fn record_run_writes_db_row_and_jsonl_line_and_reads_back() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-runlog-home");
        fs::create_dir_all(home.join("logs")).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }

        let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();

        let stored = record_run(&db, sample_record("cleanup")).unwrap();
        assert_eq!(stored.id, Some(1));
        let stored2 = record_run(&db, sample_record("triage")).unwrap();
        assert_eq!(stored2.id, Some(2));

        // JSONL: one line per run, valid JSON, newest run present.
        let log = fs::read_to_string(crate::paths::runs_log_path()).unwrap();
        let lines: Vec<&str> = log.lines().collect();
        assert_eq!(lines.len(), 2);
        let parsed: RunRecord = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(parsed.skill_name, "triage");

        // DB list: summaries, newest first, limit respected.
        let recent = list_runs(&db, 10).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].skill_name, "triage");
        assert_eq!(recent[1].skill_name, "cleanup");
        assert_eq!(recent[0].status, RunStatus::Success);
        assert_eq!(recent[0].id, 2);

        let capped = list_runs(&db, 1).unwrap();
        assert_eq!(capped.len(), 1);
        assert_eq!(capped[0].skill_name, "triage");

        // A too-large `limit` is clamped, not honoured literally.
        assert!(list_runs(&db, usize::MAX).unwrap().len() <= MAX_RUN_LIMIT);

        // The full record (with captured output) comes back via get_run.
        let full = get_run(&db, 2).unwrap().unwrap();
        assert_eq!(full.skill_name, "triage");
        assert_eq!(full.stdout, "done");
        assert!(get_run(&db, 999).unwrap().is_none());

        unsafe {
            env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn failed_record_round_trips_error_and_null_exit_code() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-runlog-fail-home");
        fs::create_dir_all(home.join("logs")).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }

        let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();
        let mut record = sample_record("broken");
        record.status = RunStatus::Failed;
        record.exit_code = None;
        record.stdout = String::new();
        record.error = Some("ollama API error: connection refused".to_owned());

        record_run(&db, record).unwrap();
        let recent = list_runs(&db, 5).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].status, RunStatus::Failed);
        assert_eq!(recent[0].exit_code, None);
        assert_eq!(
            recent[0].error.as_deref(),
            Some("ollama API error: connection refused")
        );

        unsafe {
            env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn run_status_from_db_str_rejects_unknown_values() {
        assert_eq!(
            RunStatus::from_db_str("success", 4).unwrap(),
            RunStatus::Success
        );
        assert_eq!(
            RunStatus::from_db_str("failed", 4).unwrap(),
            RunStatus::Failed
        );
        assert!(RunStatus::from_db_str("pending", 4).is_err());
        assert!(RunStatus::from_db_str("", 4).is_err());
    }
}
