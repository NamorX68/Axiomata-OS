//! SQLite persistence for routines and their firing history.
//!
//! Tables `routines` and `routine_runs` (migration 0003). This module owns all
//! SQL for them; [`crate::routines::scheduler`] and the CLI/Tauri layers go
//! through these functions rather than issuing queries directly.
//!
//! `next_fire_at` is authoritative state and it moves forward in exactly one
//! place: [`advance`], which reads the routine's own `cron_expr` and computes
//! the next occurrence itself. [`add`] sets the first value; nothing recomputes
//! it implicitly on read.

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use rusqlite::types::Type;

use crate::error::AxiomataError;
use crate::routines::model::{NewRoutine, Routine, RoutineRun, RoutineRunStatus, RoutineTarget};
use crate::routines::schedule;

/// Hard upper bound on how many rows [`list_runs`] returns, mirroring
/// [`crate::skills::runlog::MAX_RUN_LIMIT`].
pub const MAX_ROUTINE_RUN_LIMIT: usize = 500;

/// Max length of a routine `name` (also its UNIQUE key).
const MAX_NAME_LEN: usize = 128;
/// Max byte length of a `prompt` target stored in the `routines` row.
const MAX_TARGET_LEN: usize = 64 * 1024;
/// `routine_runs.detail` is free text from lower layers; cap it before storing
/// so a pathological error message can't bloat the table.
const MAX_DETAIL_LEN: usize = 2000;

/// The fields needed to record one firing attempt in `routine_runs`.
#[derive(Debug, Clone)]
pub struct NewRoutineRun {
    /// The `runs.id` of the underlying execution, if one happened.
    pub run_id: Option<i64>,
    /// The `next_fire_at` value this attempt was satisfying.
    pub scheduled_for: DateTime<Utc>,
    /// When the scheduler acted on it.
    pub fired_at: DateTime<Utc>,
    pub status: RoutineRunStatus,
    pub detail: Option<String>,
}

/// Creates a routine.
///
/// Validates the cron expression, computes the initial `next_fire_at` relative
/// to now, stamps `created_at`/`updated_at`, inserts the row, and returns the
/// stored [`Routine`].
///
/// # Errors
///
/// - [`AxiomataError::InvalidRoutine`] if the name / target / backend fail
///   validation, or the name is already taken.
/// - [`AxiomataError::InvalidCron`] if the cron expression is malformed.
/// - [`AxiomataError::Database`] on any other SQL failure.
pub fn add(db: &Connection, new: NewRoutine) -> Result<Routine, AxiomataError> {
    validate_new(&new)?;
    schedule::validate(&new.cron_expr)?;

    let now = Utc::now();
    let next_fire_at = schedule::next_after(&new.cron_expr, now)?;
    let (target_type, target) = new.target.to_columns();

    let result = db.execute(
        "INSERT INTO routines \
         (name, cron_expr, target_type, target, backend, enabled, \
          next_fire_at, last_fired_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?8)",
        rusqlite::params![
            new.name,
            new.cron_expr,
            target_type,
            target,
            new.backend,
            new.enabled as i64,
            next_fire_at.map(|dt| dt.to_rfc3339()),
            now.to_rfc3339(),
        ],
    );

    // 2067 == SQLITE_CONSTRAINT_UNIQUE — the only constraint `add` can hit
    // (the `name` UNIQUE index), since every other column is validated above.
    const SQLITE_CONSTRAINT_UNIQUE: i32 = 2067;
    match result {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.extended_code == SQLITE_CONSTRAINT_UNIQUE =>
        {
            return Err(AxiomataError::InvalidRoutine {
                reason: format!("a routine named {:?} already exists", new.name),
            });
        }
        Err(err) => return Err(err.into()),
    }

    let id = db.last_insert_rowid();
    get(db, id)?.ok_or_else(|| AxiomataError::InvalidRoutine {
        reason: "routine vanished immediately after insert".to_owned(),
    })
}

/// Rejects a routine whose name, target, or backend is malformed — enforced
/// here so both the CLI and the Tauri command are covered.
///
/// - `name`: 1–[`MAX_NAME_LEN`] chars of `[A-Za-z0-9 _-]` (also its UNIQUE key).
/// - `RoutineTarget::Skill`: a non-empty `[A-Za-z0-9_-]` slug, so it cannot
///   traverse out of `~/.axiomata/skills/`.
/// - `RoutineTarget::Prompt`: 1–[`MAX_TARGET_LEN`] bytes.
/// - `backend`: `None`, `"claude-code"`, or `"ollama"`.
fn validate_new(new: &NewRoutine) -> Result<(), AxiomataError> {
    let bad = |reason: String| Err(AxiomataError::InvalidRoutine { reason });

    let name_ok = (1..=MAX_NAME_LEN).contains(&new.name.chars().count())
        && new
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '_' | '-'));
    if !name_ok {
        return bad(format!(
            "routine name must be 1–{MAX_NAME_LEN} characters of [A-Za-z0-9 _-]"
        ));
    }

    match &new.target {
        RoutineTarget::Skill(skill) => {
            let slug_ok = !skill.is_empty()
                && skill
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
            if !slug_ok {
                return bad(format!(
                    "skill target {skill:?} must be a non-empty [A-Za-z0-9_-] name"
                ));
            }
        }
        RoutineTarget::Prompt(prompt) => {
            if prompt.is_empty() || prompt.len() > MAX_TARGET_LEN {
                return bad(format!(
                    "prompt target must be 1–{MAX_TARGET_LEN} bytes (got {})",
                    prompt.len()
                ));
            }
        }
    }

    match new.backend.as_deref() {
        None | Some("claude-code") | Some("ollama") => Ok(()),
        Some(other) => bad(format!(
            "backend must be \"claude-code\" or \"ollama\", got {other:?}"
        )),
    }
}

/// Returns every routine, soonest `next_fire_at` first, routines with no next
/// fire (disabled-and-stale, or a past pinned year) last, ties broken by name.
pub fn list(db: &Connection) -> Result<Vec<Routine>, AxiomataError> {
    let mut stmt = db.prepare(&format!(
        "SELECT {COLUMNS} FROM routines \
         ORDER BY next_fire_at IS NULL, next_fire_at ASC, name ASC"
    ))?;
    let rows = stmt.query_map([], row_to_raw_routine)?;
    collect(rows)?
        .into_iter()
        .map(RawRoutine::into_routine)
        .collect()
}

/// Fetches one routine by id, or `None` if there is no such row.
pub fn get(db: &Connection, id: i64) -> Result<Option<Routine>, AxiomataError> {
    let mut stmt = db.prepare(&format!("SELECT {COLUMNS} FROM routines WHERE id = ?1"))?;
    match stmt.query_row([id], row_to_raw_routine) {
        Ok(raw) => raw.into_routine().map(Some),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

/// Returns enabled routines whose `next_fire_at` is at or before `now`,
/// soonest first.
///
/// This is the scheduler's hot query: both the per-tick "what is due" check
/// and the startup catch-up sweep use it.
pub fn due_routines(db: &Connection, now: DateTime<Utc>) -> Result<Vec<Routine>, AxiomataError> {
    let mut stmt = db.prepare(&format!(
        "SELECT {COLUMNS} FROM routines \
         WHERE enabled = 1 AND next_fire_at IS NOT NULL AND next_fire_at <= ?1 \
         ORDER BY next_fire_at ASC"
    ))?;
    let rows = stmt.query_map([now.to_rfc3339()], row_to_raw_routine)?;
    collect(rows)?
        .into_iter()
        .map(RawRoutine::into_routine)
        .collect()
}

/// Enables or disables a routine.
///
/// Re-enabling recomputes `next_fire_at` from now, so a routine that was
/// disabled for a long time does not immediately fire for a schedule slot it
/// slept through. Disabling leaves `next_fire_at` untouched.
///
/// Returns `false` if there is no routine with that id.
pub fn set_enabled(db: &Connection, id: i64, enabled: bool) -> Result<bool, AxiomataError> {
    let Some(routine) = get(db, id)? else {
        return Ok(false);
    };

    let now = Utc::now();
    let next_fire_at = if enabled {
        schedule::next_after(&routine.cron_expr, now)?.map(|dt| dt.to_rfc3339())
    } else {
        routine.next_fire_at.map(|dt| dt.to_rfc3339())
    };

    let changed = db.execute(
        "UPDATE routines SET enabled = ?1, next_fire_at = ?2, updated_at = ?3 WHERE id = ?4",
        rusqlite::params![enabled as i64, next_fire_at, now.to_rfc3339(), id],
    )?;
    Ok(changed > 0)
}

/// Why [`advance`] is moving a routine's `next_fire_at` forward.
#[derive(Debug, Clone, Copy)]
pub enum Advance {
    /// The routine just fired at this instant; also stamps `last_fired_at`.
    Fired(DateTime<Utc>),
    /// The routine was past-due at startup and is being skipped forward
    /// without firing (the catch-up path). `last_fired_at` is left as-is.
    Skipped,
}

/// Moves a routine's `next_fire_at` to the next occurrence of its **own**
/// stored `cron_expr`, strictly after now. This is the single place
/// `next_fire_at` advances after creation — the scheduler never computes it.
///
/// Returns the new `next_fire_at` (`None` if the cron has no further
/// occurrence). A no-op — returning `Ok(None)` — if there is no such routine.
/// Does not touch `updated_at` (that tracks user config changes, not firings).
///
/// # Errors
///
/// [`AxiomataError::CorruptRoutineRow`] if the stored `cron_expr` no longer
/// parses (a corrupted or externally-edited row — [`add`] only ever stores a
/// validated one); the row is left untouched so the caller can decide what to
/// do with a routine that can't be scheduled. [`AxiomataError::Database`] on
/// a write failure.
pub fn advance(
    db: &Connection,
    id: i64,
    how: Advance,
) -> Result<Option<DateTime<Utc>>, AxiomataError> {
    let Some(routine) = get(db, id)? else {
        return Ok(None);
    };
    // A stored, previously-valid expression that no longer parses is a
    // data-integrity problem, not a fresh user mistake — translate
    // `next_after`'s `InvalidCron` (its only ever meaning: "this string
    // doesn't parse") into `CorruptRoutineRow` here, where the routine's id
    // is available to attach to it.
    let next = schedule::next_after(&routine.cron_expr, Utc::now()).map_err(|err| match err {
        AxiomataError::InvalidCron { expr, reason } => AxiomataError::CorruptRoutineRow {
            id,
            reason: format!("stored cron expression {expr:?} no longer parses: {reason}"),
        },
        other => other,
    })?;
    let next_str = next.map(|dt| dt.to_rfc3339());

    match how {
        Advance::Fired(at) => db.execute(
            "UPDATE routines SET last_fired_at = ?1, next_fire_at = ?2 WHERE id = ?3",
            rusqlite::params![at.to_rfc3339(), next_str, id],
        )?,
        Advance::Skipped => db.execute(
            "UPDATE routines SET next_fire_at = ?1 WHERE id = ?2",
            rusqlite::params![next_str, id],
        )?,
    };
    Ok(next)
}

/// Sets a routine's `next_fire_at` to an explicit value, bypassing cron
/// computation. This is a low-level override — normal forward advancement goes
/// through [`advance`]. Used by tests to force a routine due, and available for
/// a future "edit schedule" path.
pub fn set_next_fire_at(
    db: &Connection,
    id: i64,
    next: Option<DateTime<Utc>>,
) -> Result<(), AxiomataError> {
    db.execute(
        "UPDATE routines SET next_fire_at = ?1 WHERE id = ?2",
        rusqlite::params![next.map(|dt| dt.to_rfc3339()), id],
    )?;
    Ok(())
}

/// Appends one row to `routine_runs`, returns it with its assigned id, and
/// prunes that routine's history back down to [`MAX_ROUTINE_RUN_LIMIT`] rows
/// if this write pushed it over. `run.detail` is truncated to
/// [`MAX_DETAIL_LEN`] before storage.
///
/// Without this, a fast-firing routine with an always-failing target (e.g.
/// `*/1 * * * * *` against a missing skill) would grow its history
/// unboundedly — [`MAX_ROUTINE_RUN_LIMIT`] previously only capped how many
/// rows [`list_runs`] would *return*, not how many actually accumulated.
pub fn record_run(
    db: &Connection,
    routine_id: i64,
    run: NewRoutineRun,
) -> Result<RoutineRun, AxiomataError> {
    record_run_with_retention(db, routine_id, run, MAX_ROUTINE_RUN_LIMIT)
}

/// [`record_run`] with a caller-chosen retention limit — used by tests to
/// exercise the auto-prune behaviour without needing
/// [`MAX_ROUTINE_RUN_LIMIT`] (500) real rows for one routine.
fn record_run_with_retention(
    db: &Connection,
    routine_id: i64,
    run: NewRoutineRun,
    retention_limit: usize,
) -> Result<RoutineRun, AxiomataError> {
    let detail = clamp_detail(run.detail);
    db.execute(
        "INSERT INTO routine_runs \
         (routine_id, run_id, scheduled_for, fired_at, status, detail) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            routine_id,
            run.run_id,
            run.scheduled_for.to_rfc3339(),
            run.fired_at.to_rfc3339(),
            run.status.as_str(),
            detail,
        ],
    )?;
    let stored = RoutineRun {
        id: db.last_insert_rowid(),
        routine_id,
        run_id: run.run_id,
        scheduled_for: run.scheduled_for,
        fired_at: run.fired_at,
        status: run.status,
        detail,
    };
    prune_routine_runs(db, routine_id, retention_limit)?;
    Ok(stored)
}

/// Deletes every `routine_runs` row for `routine_id` except the `keep` most
/// recent (by `fired_at`, ties broken by `id`). A no-op once that routine is
/// at or under `keep` rows.
///
/// Errors:
///     [`AxiomataError::Database`] if the delete fails.
fn prune_routine_runs(db: &Connection, routine_id: i64, keep: usize) -> Result<(), AxiomataError> {
    db.execute(
        "DELETE FROM routine_runs WHERE routine_id = ?1 AND id NOT IN \
         (SELECT id FROM routine_runs WHERE routine_id = ?1 \
          ORDER BY fired_at DESC, id DESC LIMIT ?2)",
        rusqlite::params![routine_id, keep as i64],
    )?;
    Ok(())
}

/// Truncates `raw` to [`MAX_DETAIL_LEN`] characters on a char boundary.
fn clamp_detail(raw: Option<String>) -> Option<String> {
    raw.map(|text| match text.char_indices().nth(MAX_DETAIL_LEN) {
        Some((cut, _)) => {
            let mut short = text[..cut].to_owned();
            short.push('…');
            short
        }
        None => text,
    })
}

/// Returns the firing history of one routine, newest first, capped at
/// `min(limit, MAX_ROUTINE_RUN_LIMIT)`.
pub fn list_runs(
    db: &Connection,
    routine_id: i64,
    limit: usize,
) -> Result<Vec<RoutineRun>, AxiomataError> {
    let limit = limit.min(MAX_ROUTINE_RUN_LIMIT);
    let mut stmt = db.prepare(
        "SELECT id, routine_id, run_id, scheduled_for, fired_at, status, detail \
         FROM routine_runs WHERE routine_id = ?1 \
         ORDER BY fired_at DESC, id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![routine_id, limit as i64], row_to_run)?;
    collect(rows)?
        .into_iter()
        .map(RawRoutineRun::into_routine_run)
        .collect()
}

/// Returns the most recent firing of one routine, or `None` if it has never
/// fired. Used to derive [`crate::routines::model::RoutineState`].
pub fn latest_run(db: &Connection, routine_id: i64) -> Result<Option<RoutineRun>, AxiomataError> {
    Ok(list_runs(db, routine_id, 1)?.into_iter().next())
}

/// The `routines` column list shared by every `SELECT` here, so the row mapper
/// and the queries cannot drift apart.
const COLUMNS: &str = "id, name, cron_expr, target_type, target, backend, \
     enabled, next_fire_at, last_fired_at, created_at, updated_at";

/// Drains a `query_map` iterator into a `Vec`, surfacing the first row error.
fn collect<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> Result<Vec<T>, AxiomataError> {
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// [`Routine`] with its target still the two raw stored columns, not yet
/// reconstructed into a [`RoutineTarget`].
///
/// A row-mapping closure passed to `query_map`/`query_row` is constrained to
/// return `rusqlite::Result<T>`, so it has no way to produce
/// [`AxiomataError::CorruptRoutineRow`] (which needs the row's `id` and isn't
/// a `rusqlite::Error` at all) if the stored `target_type` turns out to be
/// unrecognised. [`row_to_raw_routine`] does only the parts that are
/// genuinely rusqlite's business (column types, timestamp format);
/// [`RawRoutine::into_routine`] does the domain-level reconstruction
/// afterwards, as ordinary Rust code free to return whatever error fits.
struct RawRoutine {
    id: i64,
    name: String,
    cron_expr: String,
    target_type: String,
    target_value: String,
    backend: Option<String>,
    enabled: bool,
    next_fire_at: Option<DateTime<Utc>>,
    last_fired_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl RawRoutine {
    /// Reconstructs the [`RoutineTarget`], turning an unrecognised
    /// `target_type` into [`AxiomataError::CorruptRoutineRow`] rather than a
    /// generic `rusqlite`/`Database` error — only a hand-edited or otherwise
    /// corrupted row should ever hit this, since [`add`] validates the
    /// target before it is ever stored.
    fn into_routine(self) -> Result<Routine, AxiomataError> {
        let target = RoutineTarget::from_columns(&self.target_type, self.target_value).map_err(
            |reason| AxiomataError::CorruptRoutineRow {
                id: self.id,
                reason,
            },
        )?;
        Ok(Routine {
            id: self.id,
            name: self.name,
            cron_expr: self.cron_expr,
            target,
            backend: self.backend,
            enabled: self.enabled,
            next_fire_at: self.next_fire_at,
            last_fired_at: self.last_fired_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

/// Maps a `routines` row (selected as [`COLUMNS`]) onto a [`RawRoutine`] —
/// see its docs for why the target isn't reconstructed here.
fn row_to_raw_routine(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRoutine> {
    Ok(RawRoutine {
        id: row.get(0)?,
        name: row.get(1)?,
        cron_expr: row.get(2)?,
        target_type: row.get(3)?,
        target_value: row.get(4)?,
        backend: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        next_fire_at: opt_timestamp(row.get::<_, Option<String>>(7)?, 7)?,
        last_fired_at: opt_timestamp(row.get::<_, Option<String>>(8)?, 8)?,
        created_at: timestamp(&row.get::<_, String>(9)?, 9)?,
        updated_at: timestamp(&row.get::<_, String>(10)?, 10)?,
    })
}

/// [`RoutineRun`] with its `status` still the raw stored token — the same
/// "a row-mapping closure can't produce `CorruptRoutineRow`" reason as
/// [`RawRoutine`].
struct RawRoutineRun {
    id: i64,
    routine_id: i64,
    run_id: Option<i64>,
    scheduled_for: DateTime<Utc>,
    fired_at: DateTime<Utc>,
    status: String,
    detail: Option<String>,
}

impl RawRoutineRun {
    fn into_routine_run(self) -> Result<RoutineRun, AxiomataError> {
        let status = RoutineRunStatus::from_db_str(&self.status, "status").map_err(|reason| {
            AxiomataError::CorruptRoutineRow {
                id: self.id,
                reason,
            }
        })?;
        Ok(RoutineRun {
            id: self.id,
            routine_id: self.routine_id,
            run_id: self.run_id,
            scheduled_for: self.scheduled_for,
            fired_at: self.fired_at,
            status,
            detail: self.detail,
        })
    }
}

/// Maps a `routine_runs` row onto a [`RawRoutineRun`].
fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRoutineRun> {
    Ok(RawRoutineRun {
        id: row.get(0)?,
        routine_id: row.get(1)?,
        run_id: row.get(2)?,
        scheduled_for: timestamp(&row.get::<_, String>(3)?, 3)?,
        fired_at: timestamp(&row.get::<_, String>(4)?, 4)?,
        status: row.get(5)?,
        detail: row.get(6)?,
    })
}

/// Parses an RFC 3339 timestamp column, turning a bad value into a column
/// decode error rather than a panic.
fn timestamp(raw: &str, index: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(err)))
}

/// [`timestamp`] for a nullable column.
fn opt_timestamp(raw: Option<String>, index: usize) -> rusqlite::Result<Option<DateTime<Utc>>> {
    raw.map(|value| timestamp(&value, index)).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routines::model::RoutineState;
    use crate::test_support::unique_temp_dir;
    use std::fs;

    fn temp_db() -> (std::path::PathBuf, Connection) {
        let path = unique_temp_dir("axiomata-test-routines").with_extension("db");
        let conn = crate::db::open_and_migrate_at(&path).unwrap();
        (path, conn)
    }

    fn new_routine(name: &str) -> NewRoutine {
        NewRoutine {
            name: name.to_owned(),
            cron_expr: "0 */2 * * * *".to_owned(),
            target: RoutineTarget::Skill("example-skill".to_owned()),
            backend: None,
            enabled: true,
        }
    }

    #[test]
    fn add_computes_next_fire_and_round_trips() {
        let (path, db) = temp_db();

        let created = add(&db, new_routine("every-two-min")).unwrap();
        assert!(created.id > 0);
        assert!(created.enabled);
        assert!(created.next_fire_at.is_some());
        assert!(created.next_fire_at.unwrap() > Utc::now());
        assert!(created.last_fired_at.is_none());

        let fetched = get(&db, created.id).unwrap().unwrap();
        assert_eq!(fetched, created);
        assert!(get(&db, 9999).unwrap().is_none());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn add_rejects_bad_cron_and_duplicate_name() {
        let (path, db) = temp_db();

        let mut bad = new_routine("bad");
        bad.cron_expr = "*/2 * * * *".to_owned(); // 5-field crontab, unsupported
        assert!(matches!(
            add(&db, bad).unwrap_err(),
            AxiomataError::InvalidCron { .. }
        ));

        add(&db, new_routine("dup")).unwrap();
        assert!(matches!(
            add(&db, new_routine("dup")).unwrap_err(),
            AxiomataError::InvalidRoutine { .. }
        ));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn add_rejects_malformed_name_target_and_backend() {
        let (path, db) = temp_db();

        let cases: Vec<NewRoutine> = vec![
            NewRoutine {
                name: "bad/name".to_owned(),
                ..new_routine("_")
            },
            NewRoutine {
                name: "x".repeat(200),
                ..new_routine("_")
            },
            NewRoutine {
                target: RoutineTarget::Skill("../etc/passwd".to_owned()),
                ..new_routine("trav")
            },
            NewRoutine {
                target: RoutineTarget::Prompt(String::new()),
                ..new_routine("empty-prompt")
            },
            NewRoutine {
                backend: Some("opencode".to_owned()),
                ..new_routine("weird-backend")
            },
        ];
        for case in cases {
            assert!(
                matches!(
                    add(&db, case.clone()).unwrap_err(),
                    AxiomataError::InvalidRoutine { .. }
                ),
                "expected {case:?} to be rejected"
            );
        }

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn record_run_truncates_an_oversized_detail() {
        let (path, db) = temp_db();
        let r = add(&db, new_routine("verbose")).unwrap();
        let huge = "e".repeat(MAX_DETAIL_LEN * 3);
        record_run(
            &db,
            r.id,
            NewRoutineRun {
                run_id: None,
                scheduled_for: Utc::now(),
                fired_at: Utc::now(),
                status: RoutineRunStatus::Failed,
                detail: Some(huge),
            },
        )
        .unwrap();
        let stored = list_runs(&db, r.id, 1).unwrap()[0].detail.clone().unwrap();
        assert!(stored.chars().count() <= MAX_DETAIL_LEN + 1);
        assert!(stored.ends_with('…'));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn record_run_prunes_a_routines_history_automatically() {
        let (path, db) = temp_db();
        let r = add(&db, new_routine("noisy")).unwrap();

        fn a_run() -> NewRoutineRun {
            NewRoutineRun {
                run_id: None,
                scheduled_for: Utc::now(),
                fired_at: Utc::now(),
                status: RoutineRunStatus::Failed,
                detail: None,
            }
        }

        // A tiny retention limit so the test doesn't need
        // MAX_ROUTINE_RUN_LIMIT (500) real rows to see it kick in.
        for _ in 0..5 {
            record_run_with_retention(&db, r.id, a_run(), 3).unwrap();
        }
        assert_eq!(
            list_runs(&db, r.id, 100).unwrap().len(),
            3,
            "record_run should prune this routine's history on every write"
        );

        // Pruning is per-routine — a second routine's history is untouched.
        let other = add(&db, new_routine("quiet")).unwrap();
        record_run_with_retention(&db, other.id, a_run(), 3).unwrap();
        assert_eq!(list_runs(&db, other.id, 100).unwrap().len(), 1);
        assert_eq!(list_runs(&db, r.id, 100).unwrap().len(), 3);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn due_routines_selects_only_enabled_past_due_rows() {
        let (path, db) = temp_db();

        let a = add(&db, new_routine("due-enabled")).unwrap();
        let b = add(&db, new_routine("due-disabled")).unwrap();
        let c = add(&db, new_routine("not-yet")).unwrap();

        let past = Utc::now() - chrono::Duration::minutes(5);
        let future = Utc::now() + chrono::Duration::hours(1);
        set_next_fire_at(&db, a.id, Some(past)).unwrap();
        set_next_fire_at(&db, b.id, Some(past)).unwrap();
        set_enabled(&db, b.id, false).unwrap();
        set_next_fire_at(&db, c.id, Some(future)).unwrap();

        let due = due_routines(&db, Utc::now()).unwrap();
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, a.id);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn set_enabled_toggles_and_reenable_refreshes_next_fire() {
        let (path, db) = temp_db();
        let r = add(&db, new_routine("toggle")).unwrap();

        // Force a stale past next-fire, then disable.
        let stale = Utc::now() - chrono::Duration::days(3);
        set_next_fire_at(&db, r.id, Some(stale)).unwrap();
        assert!(set_enabled(&db, r.id, false).unwrap());
        assert!(!get(&db, r.id).unwrap().unwrap().enabled);

        // Re-enabling must move next_fire_at back into the future.
        assert!(set_enabled(&db, r.id, true).unwrap());
        let reenabled = get(&db, r.id).unwrap().unwrap();
        assert!(reenabled.enabled);
        assert!(reenabled.next_fire_at.unwrap() > Utc::now());

        assert!(!set_enabled(&db, 4242, true).unwrap());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn advance_and_record_run_build_history_and_state() {
        let (path, db) = temp_db();
        let r = add(&db, new_routine("history")).unwrap();

        // A real `runs` row for the successful firing to link to (the
        // `routine_runs.run_id` foreign key is enforced).
        db.execute(
            "INSERT INTO runs \
             (skill_name, backend, status, exit_code, duration_ms, started_at, finished_at) \
             VALUES ('example-skill', 'ollama', 'success', 0, 5, \
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let run_id = db.last_insert_rowid();

        assert_eq!(
            RoutineState::derive(&r, latest_run(&db, r.id).unwrap().as_ref()),
            RoutineState::Scheduled
        );

        let fired_at = Utc::now();
        record_run(
            &db,
            r.id,
            NewRoutineRun {
                run_id: Some(run_id),
                scheduled_for: fired_at,
                fired_at,
                status: RoutineRunStatus::Success,
                detail: None,
            },
        )
        .unwrap();
        let next = advance(&db, r.id, Advance::Fired(fired_at)).unwrap();
        assert!(
            next.unwrap() > Utc::now(),
            "advance recomputes from the cron"
        );

        let after = get(&db, r.id).unwrap().unwrap();
        assert_eq!(after.last_fired_at.unwrap(), fired_at.with_timezone(&Utc));
        assert_eq!(
            RoutineState::derive(&after, latest_run(&db, r.id).unwrap().as_ref()),
            RoutineState::Fired
        );

        // Second, failed firing becomes the latest.
        record_run(
            &db,
            r.id,
            NewRoutineRun {
                run_id: None,
                scheduled_for: next.unwrap(),
                fired_at: Utc::now(),
                status: RoutineRunStatus::Failed,
                detail: Some("skill vanished".to_owned()),
            },
        )
        .unwrap();

        let history = list_runs(&db, r.id, 10).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].status, RoutineRunStatus::Failed);
        assert_eq!(history[0].detail.as_deref(), Some("skill vanished"));
        assert_eq!(
            RoutineState::derive(&after, latest_run(&db, r.id).unwrap().as_ref()),
            RoutineState::Failed
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn advance_reports_a_corrupted_stored_cron_as_corrupt_row_not_invalid_cron() {
        let (path, db) = temp_db();
        let r = add(&db, new_routine("was-fine")).unwrap();

        // Only a hand-edit (or a future format change) could put an
        // unparseable value in an already-stored `cron_expr` — `add` only
        // ever stores one that already passed `schedule::validate`.
        db.execute(
            "UPDATE routines SET cron_expr = 'not a cron' WHERE id = ?1",
            [r.id],
        )
        .unwrap();

        let err = advance(&db, r.id, Advance::Fired(Utc::now())).unwrap_err();
        match err {
            AxiomataError::CorruptRoutineRow { id, reason } => {
                assert_eq!(id, r.id);
                assert!(reason.contains("not a cron"));
            }
            other => panic!("expected CorruptRoutineRow, got {other:?}"),
        }

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn get_reports_a_corrupted_target_type_as_corrupt_row() {
        let (path, db) = temp_db();
        let r = add(&db, new_routine("gets-corrupted")).unwrap();

        db.execute(
            "UPDATE routines SET target_type = 'webhook' WHERE id = ?1",
            [r.id],
        )
        .unwrap();

        let err = get(&db, r.id).unwrap_err();
        match err {
            AxiomataError::CorruptRoutineRow { id, reason } => {
                assert_eq!(id, r.id);
                assert!(reason.contains("webhook"));
            }
            other => panic!("expected CorruptRoutineRow, got {other:?}"),
        }

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn list_orders_by_next_fire_with_nulls_last() {
        let (path, db) = temp_db();
        let soon = add(&db, new_routine("soon")).unwrap();
        let later = add(&db, new_routine("later")).unwrap();
        let never = add(&db, new_routine("never")).unwrap();

        set_next_fire_at(
            &db,
            soon.id,
            Some(Utc::now() + chrono::Duration::minutes(1)),
        )
        .unwrap();
        set_next_fire_at(&db, later.id, Some(Utc::now() + chrono::Duration::hours(1))).unwrap();
        set_next_fire_at(&db, never.id, None).unwrap();

        let ordered: Vec<String> = list(&db).unwrap().into_iter().map(|r| r.name).collect();
        assert_eq!(ordered, vec!["soon", "later", "never"]);

        let _ = fs::remove_file(&path);
    }
}
