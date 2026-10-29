//! Writes and reads the skill-run history.
//!
//! Every run is recorded in two places: a row in the SQLite `runs` table (what
//! the UI and CLI query) and an appended JSON line in
//! `~/.axiomata/logs/runs.log` (for `tail -f`-style inspection). The two are
//! kept in sync by [`record_run`].
//!
//! Implemented in M1.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::error::AxiomataError;
use crate::paths;
use crate::skills::model::{RunRecord, RunSource, RunStatus, RunSummary};

/// Hard cap on how many rows the `runs` table is allowed to accumulate.
/// [`MAX_RUN_LIMIT`] only bounds one *query*'s result — nothing previously
/// stopped the table itself from growing without limit (e.g. a `*/1 * * * *`
/// routine with an always-failing target). [`record_run`] prunes down to this
/// many rows, keeping the most recent, every time it writes one.
///
/// Several times [`MAX_RUN_LIMIT`] so pruning is never in tension with a
/// legitimate "show me the last `MAX_RUN_LIMIT`" query.
const RUNS_RETENTION_LIMIT: usize = MAX_RUN_LIMIT * 4;

/// The size from which `runs.log` is rotated before the next line is appended (`docs/plans/maintenance.md` §3): it is
/// a mirror for `tail -f`, not the record — the database is — so its old lines need not stay forever.
pub const RUNS_LOG_ROTATE_BYTES: u64 = 10 * 1024 * 1024;

/// How many rotated generations of `runs.log` stay beside it (`runs.log.1` the newer, `runs.log.2` the older).
pub const RUNS_LOG_GENERATIONS: u32 = 2;

/// Persists `record` to both the database and the JSONL log, then prunes
/// `runs` back down to [`RUNS_RETENTION_LIMIT`] rows if this write pushed it
/// over (see [`RUNS_RETENTION_LIMIT`]'s docs).
///
/// The database row is written first; on success the JSONL line is appended.
/// Returns the record with its assigned [`RunRecord::id`] set. The JSONL log
/// is an append-only trail the app never queries back; it is rotated at
/// [`RUNS_LOG_ROTATE_BYTES`] rather than pruned line by line.
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

/// Writes the row of a run that is starting (durable runs, CP3): status `running`, this process's id, and the id of
/// the journal a Glut run keeps. [`finish_run`] completes it; [`discard_run`] takes it back when the run never started.
///
/// Errors:
///     [`AxiomataError::Database`] if the insert fails.
pub fn begin_run(
    db: &Connection,
    skill_name: &str,
    source: RunSource,
    journal: Option<&str>,
) -> Result<i64, AxiomataError> {
    let now = Utc::now().to_rfc3339();
    db.execute(
        "INSERT INTO runs (skill_name, backend, status, started_at, finished_at, source, pid, journal) \
         VALUES (?1, '', 'running', ?2, ?2, ?3, ?4, ?5)",
        rusqlite::params![skill_name, now, source.as_str(), std::process::id(), journal],
    )?;
    let id = db.last_insert_rowid();
    prune_runs(db, RUNS_RETENTION_LIMIT)?;
    Ok(id)
}

/// Completes the row [`begin_run`] wrote with the run's outcome, and appends it to the log. The journal column is
/// cleared: a finished run has none to resume.
///
/// Errors:
///     [`AxiomataError::Database`] if the update fails; [`AxiomataError::Io`] if the log cannot be appended to.
pub fn finish_run(
    db: &Connection,
    id: i64,
    mut record: RunRecord,
) -> Result<RunRecord, AxiomataError> {
    db.execute(
        "UPDATE runs SET skill_name = ?2, backend = ?3, status = ?4, exit_code = ?5, duration_ms = ?6, \
         stdout = ?7, stderr = ?8, error = ?9, started_at = ?10, finished_at = ?11, source = ?12, provider = ?13, \
         cost_usd = ?14, input_tokens = ?15, output_tokens = ?16, num_turns = ?17, model = ?18, \
         cached_input_tokens = ?19, journal = NULL WHERE id = ?1",
        rusqlite::params![
            id,
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
            record.source.as_str(),
            record.provider,
            record.cost_usd,
            record.input_tokens,
            record.output_tokens,
            record.num_turns,
            record.model,
            record.cached_input_tokens,
        ],
    )?;
    record.id = Some(id);
    append_jsonl(&record)?;
    Ok(record)
}

/// Removes the row of a run that never started (it was refused as already running).
///
/// Errors:
///     [`AxiomataError::Database`] if the delete fails.
pub fn discard_run(db: &Connection, id: i64) -> Result<(), AxiomataError> {
    db.execute(
        "DELETE FROM runs WHERE id = ?1 AND status = 'running'",
        [id],
    )?;
    Ok(())
}

/// Marks every `running` row whose process is gone as `interrupted` (at the app's start): a run the app or the CLI
/// lost is never gone without a trace. Returns how many it marked.
///
/// Errors:
///     [`AxiomataError::Database`] if the query or an update fails.
pub fn mark_interrupted(db: &Connection) -> Result<usize, AxiomataError> {
    let running: Vec<(i64, Option<i64>)> = {
        let mut statement = db.prepare("SELECT id, pid FROM runs WHERE status = 'running'")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut marked = 0;
    for (id, pid) in running {
        if pid.is_some_and(process_alive) {
            continue;
        }
        marked += db.execute(
            "UPDATE runs SET status = 'interrupted', \
             error = 'the run stopped before it ended: its process was gone (an app quit, a crash, a rebuild)' \
             WHERE id = ?1 AND status = 'running'",
            [id],
        )?;
    }
    Ok(marked)
}

/// Whether the process `pid` still lives (signal 0 reaches it). This process counts as alive. Only a positive id is
/// one: a negative one would name a process group.
fn process_alive(pid: i64) -> bool {
    let Some(pid) = i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .and_then(rustix::process::Pid::from_raw)
    else {
        return false;
    };
    rustix::process::test_kill_process(pid).is_ok()
}

/// Clears the journal of every interrupted run whose journal is not among `kept` (the unfinished journals there are):
/// such a run is no longer offered for a resume.
///
/// Errors:
///     [`AxiomataError::Database`] if the query or an update fails.
pub fn forget_lost_journals(db: &Connection, kept: &[String]) -> Result<usize, AxiomataError> {
    let interrupted: Vec<(i64, String)> = {
        let mut statement = db.prepare(
            "SELECT id, journal FROM runs WHERE status = 'interrupted' AND journal IS NOT NULL",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut forgotten = 0;
    for (id, journal) in interrupted {
        if !kept.contains(&journal) {
            forgotten += db.execute("UPDATE runs SET journal = NULL WHERE id = ?1", [id])?;
        }
    }
    Ok(forgotten)
}

/// The journal id of an interrupted run, if it kept one.
///
/// Errors:
///     [`AxiomataError::Database`] if the query fails.
pub fn interrupted_journal(db: &Connection, id: i64) -> Result<Option<String>, AxiomataError> {
    match db.query_row(
        "SELECT journal FROM runs WHERE id = ?1 AND status = 'interrupted'",
        [id],
        |row| row.get::<_, Option<String>>(0),
    ) {
        Ok(journal) => Ok(journal),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(err.into()),
    }
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
          stdout, stderr, error, started_at, finished_at, source, \
          provider, cost_usd, input_tokens, output_tokens, num_turns, model, \
          cached_input_tokens) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, \
                 ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
            record.source.as_str(),
            record.provider,
            record.cost_usd,
            record.input_tokens,
            record.output_tokens,
            record.num_turns,
            record.model,
            record.cached_input_tokens,
        ],
    )?;
    Ok(db.last_insert_rowid())
}

/// Appends `record` as one JSON line to `~/.axiomata/logs/runs.log`, rotating the file first when it is full.
fn append_jsonl(record: &RunRecord) -> Result<(), AxiomataError> {
    let path = paths::runs_log_path();
    rotate_if_full(&path, RUNS_LOG_ROTATE_BYTES);
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

/// `runs.log.<n>` beside `log`.
pub fn rotated_log_path(log: &Path, generation: u32) -> PathBuf {
    let mut name = OsString::from(log.as_os_str());
    name.push(format!(".{generation}"));
    PathBuf::from(name)
}

/// Moves `log` to `log.1` (and `log.1` to `log.2`, dropping the oldest) once it holds `limit` bytes or more.
///
/// Best effort: a failed rename is logged and the line is appended to the full file anyway — losing a line of the
/// mirror would be worse than a file that grew past its limit. Two processes rotating at the same moment can drop one
/// generation; the database still has every run. A link in place of the log is left alone.
fn rotate_if_full(log: &Path, limit: u64) {
    match fs::symlink_metadata(log) {
        Ok(meta) if meta.is_file() && meta.len() >= limit => {}
        _ => return,
    }
    for generation in (1..RUNS_LOG_GENERATIONS).rev() {
        let older = rotated_log_path(log, generation);
        if fs::symlink_metadata(&older).is_ok_and(|meta| meta.is_file()) {
            let target = rotated_log_path(log, generation + 1);
            if let Err(err) = fs::rename(&older, &target) {
                tracing::warn!(%err, path = %older.display(), "could not rotate a run log generation");
            }
        }
    }
    if let Err(err) = fs::rename(log, rotated_log_path(log, 1)) {
        tracing::warn!(%err, path = %log.display(), "could not rotate the run log");
    }
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
        "SELECT id, skill_name, backend, status, exit_code, duration_ms, error, started_at, source, \
                provider, cost_usd, (status = 'interrupted' AND journal IS NOT NULL) \
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
         duration_ms, stdout, stderr, error, started_at, finished_at, source, \
         provider, cost_usd, input_tokens, output_tokens, num_turns, model, \
         cached_input_tokens \
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
    let source: String = row.get(8)?;
    Ok(RunSummary {
        id: row.get(0)?,
        skill_name: row.get(1)?,
        backend: row.get(2)?,
        status: RunStatus::from_db_str(&status, 3)?,
        exit_code: row.get(4)?,
        duration_ms: row.get::<_, i64>(5)? as u64,
        error: row.get(6)?,
        started_at: parse_timestamp(row, 7, &started_at)?,
        source: RunSource::from_db_str(&source, 8)?,
        provider: row.get(9)?,
        cost_usd: row.get(10)?,
        resumable: row.get(11)?,
    })
}

/// Maps one `runs` row onto a [`RunRecord`].
fn row_to_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    let status: String = row.get(3)?;
    let started_at: String = row.get(9)?;
    let finished_at: String = row.get(10)?;
    let source: String = row.get(11)?;

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
        source: RunSource::from_db_str(&source, 11)?,
        provider: row.get(12)?,
        cost_usd: row.get(13)?,
        input_tokens: row.get::<_, Option<i64>>(14)?.map(|n| n as u64),
        output_tokens: row.get::<_, Option<i64>>(15)?.map(|n| n as u64),
        num_turns: row.get::<_, Option<i64>>(16)?.map(|n| n as u32),
        model: row.get(17)?,
        cached_input_tokens: row.get::<_, Option<i64>>(18)?.map(|n| n as u64),
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
            provider: None,
            cost_usd: None,
            input_tokens: None,
            cached_input_tokens: None,
            output_tokens: None,
            num_turns: None,
            model: None,
            source: RunSource::Manual,
        }
    }

    /// Durable runs (CP3): a row from the start, completed at the end; a row whose process is gone becomes
    /// `interrupted`, one of a living process stays `running`; a run that never started leaves no row.
    #[test]
    fn a_run_is_recorded_from_its_start_and_a_lost_one_is_marked_interrupted() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-runlog-durable");
        fs::create_dir_all(home.join("logs")).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }
        let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();

        let finished = begin_run(&db, "news-web", RunSource::Routine, Some("run-1")).unwrap();
        assert_eq!(list_runs(&db, 10).unwrap()[0].status, RunStatus::Running);
        let stored = finish_run(&db, finished, sample_record("news-web")).unwrap();
        assert_eq!(stored.id, Some(finished));
        let row = get_run(&db, finished).unwrap().unwrap();
        assert_eq!(
            (row.status, row.stdout.as_str()),
            (RunStatus::Success, "done")
        );
        assert_eq!(interrupted_journal(&db, finished).unwrap(), None);

        let lost = begin_run(&db, "cleanup", RunSource::Manual, Some("run-2")).unwrap();
        let alive = begin_run(&db, "triage", RunSource::Manual, None).unwrap();
        let refused = begin_run(&db, "racer", RunSource::Manual, None).unwrap();
        discard_run(&db, refused).unwrap();
        // The lost run's process is gone (no process has pid 0 to signal); this process is alive.
        db.execute("UPDATE runs SET pid = 999999999 WHERE id = ?1", [lost])
            .unwrap();
        assert_eq!(mark_interrupted(&db).unwrap(), 1);
        assert_eq!(
            get_run(&db, lost).unwrap().unwrap().status,
            RunStatus::Interrupted
        );
        assert_eq!(
            get_run(&db, alive).unwrap().unwrap().status,
            RunStatus::Running
        );
        assert_eq!(
            interrupted_journal(&db, lost).unwrap().as_deref(),
            Some("run-2")
        );
        let listed = list_runs(&db, 10).unwrap();
        let resumable: Vec<bool> = listed.iter().map(|run| run.resumable).collect();
        assert_eq!(
            resumable.iter().filter(|r| **r).count(),
            1,
            "only the lost run with a journal"
        );
        assert!(get_run(&db, refused).unwrap().is_none());
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
    fn cost_provider_and_tokens_round_trip_through_list_runs_and_get_run() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-runlog-cost-home");
        fs::create_dir_all(home.join("logs")).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }
        let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();

        let mut paid = sample_record("mail-digest");
        paid.provider = Some("open_router".to_owned());
        paid.cost_usd = Some(0.0731);
        paid.input_tokens = Some(9000);
        paid.output_tokens = Some(450);
        paid.num_turns = Some(6);
        record_run(&db, paid).unwrap();

        let summary = &list_runs(&db, 5).unwrap()[0];
        assert_eq!(summary.provider.as_deref(), Some("open_router"));
        assert_eq!(summary.cost_usd, Some(0.0731));

        let full = get_run(&db, 1).unwrap().unwrap();
        assert_eq!(full.cost_usd, Some(0.0731));
        assert_eq!(full.input_tokens, Some(9000));
        assert_eq!(full.output_tokens, Some(450));
        assert_eq!(full.num_turns, Some(6));
        assert_eq!(full.provider.as_deref(), Some("open_router"));

        unsafe {
            env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn source_round_trips_through_both_list_runs_and_get_run() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-runlog-source-home");
        fs::create_dir_all(home.join("logs")).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }

        let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();

        let manual = sample_record("by-hand");
        record_run(&db, manual).unwrap();
        let mut routine_fired = sample_record("by-routine");
        routine_fired.source = RunSource::Routine;
        record_run(&db, routine_fired).unwrap();

        let summaries = list_runs(&db, 10).unwrap();
        let by_name = |name: &str| summaries.iter().find(|s| s.skill_name == name).unwrap();
        assert_eq!(by_name("by-hand").source, RunSource::Manual);
        assert_eq!(by_name("by-routine").source, RunSource::Routine);

        assert_eq!(get_run(&db, 1).unwrap().unwrap().source, RunSource::Manual);
        assert_eq!(get_run(&db, 2).unwrap().unwrap().source, RunSource::Routine);

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

    #[test]
    fn run_source_from_db_str_rejects_unknown_values() {
        assert_eq!(
            RunSource::from_db_str("manual", 8).unwrap(),
            RunSource::Manual
        );
        assert_eq!(
            RunSource::from_db_str("routine", 8).unwrap(),
            RunSource::Routine
        );
        assert!(RunSource::from_db_str("scheduled", 8).is_err());
        assert!(RunSource::from_db_str("", 8).is_err());
    }

    #[test]
    fn a_corrupted_source_column_surfaces_as_an_error_not_a_panic_or_silent_default() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-runlog-corrupt-source-home");
        fs::create_dir_all(home.join("logs")).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }

        let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();
        // Only a hand-edit (or a future format change) could put an
        // unrecognised value in this column — `record_run` only ever stores
        // `RunSource::as_str()`'s own tokens.
        db.execute(
            "INSERT INTO runs \
             (skill_name, backend, status, exit_code, duration_ms, \
              started_at, finished_at, source) \
             VALUES ('probe', 'ollama', 'success', 0, 5, \
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'scheduled')",
            [],
        )
        .unwrap();

        assert!(list_runs(&db, 10).is_err());
        assert!(get_run(&db, 1).is_err());

        unsafe {
            env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }
    #[test]
    fn a_full_run_log_rotates_into_two_generations_and_drops_the_oldest() {
        let dir = unique_temp_dir("axiomata-test-runlog-rotate");
        fs::create_dir_all(&dir).unwrap();
        let log = dir.join("runs.log");

        // Below the limit nothing moves.
        fs::write(&log, "small\n").unwrap();
        rotate_if_full(&log, 100);
        assert!(log.exists());
        assert!(!rotated_log_path(&log, 1).exists());

        fs::write(&log, "first\n".repeat(20)).unwrap();
        rotate_if_full(&log, 100);
        assert!(!log.exists(), "the full log moved away");
        assert!(rotated_log_path(&log, 1).exists());

        fs::write(&log, "second\n".repeat(20)).unwrap();
        rotate_if_full(&log, 100);
        fs::write(&log, "third\n".repeat(20)).unwrap();
        rotate_if_full(&log, 100);
        assert!(
            fs::read_to_string(rotated_log_path(&log, 1))
                .unwrap()
                .starts_with("third")
        );
        assert!(
            fs::read_to_string(rotated_log_path(&log, 2))
                .unwrap()
                .starts_with("second")
        );
        assert!(
            !rotated_log_path(&log, 3).exists(),
            "only two generations stay"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
