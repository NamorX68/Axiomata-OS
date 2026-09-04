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

use crate::error::AxiomataError;
use crate::paths;
use crate::skills::model::{RunRecord, RunStatus, RunSummary};

/// Hard cap on how many rows the `runs` table is allowed to accumulate.
/// [`MAX_RUN_LIMIT`] only bounds one *query*'s result — nothing previously
/// stopped the table itself from growing without limit (e.g. a `*/1 * * * *`
/// routine with an always-failing target). [`record_run`] prunes down to this
/// many rows, keeping the most recent, every time it writes one.
///
/// Several times [`MAX_RUN_LIMIT`] so pruning is never in tension with a
/// legitimate "show me the last `MAX_RUN_LIMIT`" query.
const RUNS_RETENTION_LIMIT: usize = MAX_RUN_LIMIT * 4;

/// Persists `record` to both the database and the JSONL log, then prunes
/// `runs` back down to [`RUNS_RETENTION_LIMIT`] rows if this write pushed it
/// over (see [`RUNS_RETENTION_LIMIT`]'s docs).
///
/// The database row is written first; on success the JSONL line is appended.
/// Returns the record with its assigned [`RunRecord::id`] set. The JSONL log
/// itself is intentionally never pruned — it is an append-only audit trail,
/// not something the app queries back.
///
/// Errors:
///     [`AxiomataError::Database`] if the row insert or the prune fails;
///     [`AxiomataError::Io`] if the log file cannot be appended to (the
///     database row is already committed in that case).
pub fn record_run(db: &Connection, record: RunRecord) -> Result<RunRecord, AxiomataError> {
    record_run_with_retention(db, record, RUNS_RETENTION_LIMIT)
}

/// [`record_run`] with a caller-chosen retention limit — used by tests to
/// exercise the auto-prune behaviour without needing
/// [`RUNS_RETENTION_LIMIT`] (2000) real rows.
fn record_run_with_retention(
    db: &Connection,
    mut record: RunRecord,
    retention_limit: usize,
) -> Result<RunRecord, AxiomataError> {
    let id = insert_row(db, &record)?;
    record.id = Some(id);
    prune_runs(db, retention_limit)?;
    append_jsonl(&record)?;
    Ok(record)
}

/// Deletes every `runs` row except the `keep` most recent (by `started_at`,
/// ties broken by `id`). A no-op once the table is at or under `keep` rows —
/// the common case once retention has kicked in once.
///
/// Errors:
///     [`AxiomataError::Database`] if the delete fails.
fn prune_runs(db: &Connection, keep: usize) -> Result<(), AxiomataError> {
    db.execute(
        "DELETE FROM runs WHERE id NOT IN \
         (SELECT id FROM runs ORDER BY started_at DESC, id DESC LIMIT ?1)",
        [keep as i64],
    )?;
    Ok(())
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
    fn prune_runs_keeps_only_the_most_recent_rows() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-runlog-prune-home");
        fs::create_dir_all(home.join("logs")).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }

        let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();
        for i in 0..10 {
            insert_row(&db, &sample_record(&format!("run-{i}"))).unwrap();
        }

        // A `keep` at or above the row count is a no-op.
        prune_runs(&db, 10).unwrap();
        assert_eq!(list_runs(&db, 100).unwrap().len(), 10);

        // Pruning to 3 keeps the 3 most recently inserted (highest ids, since
        // `sample_record` gives them all the same `started_at`).
        prune_runs(&db, 3).unwrap();
        let remaining = list_runs(&db, 100).unwrap();
        assert_eq!(remaining.len(), 3);
        assert_eq!(
            remaining
                .iter()
                .map(|r| r.skill_name.as_str())
                .collect::<Vec<_>>(),
            ["run-9", "run-8", "run-7"]
        );

        unsafe {
            env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn record_run_prunes_automatically_once_over_the_retention_limit() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-runlog-auto-prune-home");
        fs::create_dir_all(home.join("logs")).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }

        let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();
        // A tiny retention limit so the test doesn't need
        // `RUNS_RETENTION_LIMIT` (2000) real rows to see it kick in.
        for i in 0..5 {
            record_run_with_retention(&db, sample_record(&format!("run-{i}")), 3).unwrap();
        }
        let remaining = list_runs(&db, 100).unwrap();
        assert_eq!(remaining.len(), 3, "record_run should prune on every write");
        assert_eq!(
            remaining
                .iter()
                .map(|r| r.skill_name.as_str())
                .collect::<Vec<_>>(),
            ["run-4", "run-3", "run-2"]
        );

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
