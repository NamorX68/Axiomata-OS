//! SQLite persistence for routines and their firing history.
//!
//! Tables `routines` and `routine_runs` (migration 0003). This module owns all
//! SQL for them; [`crate::routines::scheduler`] and the CLI/Tauri layers go
//! through these functions rather than issuing queries directly.
//!
//! `next_fire_at` is treated as authoritative state here: [`add`] computes the
//! first one, [`mark_fired`] advances it after a real firing, and
//! [`roll_forward`] advances it without firing (the startup catch-up path). It
//! is never recomputed implicitly on read.

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use rusqlite::types::Type;

use crate::error::AxiomataError;
use crate::routines::model::{NewRoutine, Routine, RoutineRun, RoutineRunStatus, RoutineTarget};
use crate::routines::schedule;

/// Hard upper bound on how many rows [`list_runs`] returns, mirroring
/// [`crate::skills::runlog::MAX_RUN_LIMIT`].
pub const MAX_ROUTINE_RUN_LIMIT: usize = 500;

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
/// - [`AxiomataError::InvalidRoutine`] if the cron expression is malformed or
///   the name is already taken.
/// - [`AxiomataError::Database`] on any other SQL failure.
pub fn add(db: &Connection, new: NewRoutine) -> Result<Routine, AxiomataError> {
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

    match result {
        Ok(_) => {}
        Err(rusqlite::Error::SqliteFailure(err, _))
            if err.code == rusqlite::ErrorCode::ConstraintViolation =>
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

/// Returns every routine, soonest `next_fire_at` first, routines with no next
/// fire (disabled-and-stale, or a past pinned year) last, ties broken by name.
pub fn list(db: &Connection) -> Result<Vec<Routine>, AxiomataError> {
    let mut stmt = db.prepare(&format!(
        "SELECT {COLUMNS} FROM routines \
         ORDER BY next_fire_at IS NULL, next_fire_at ASC, name ASC"
    ))?;
    let rows = stmt.query_map([], row_to_routine)?;
    collect(rows)
}

/// Fetches one routine by id, or `None` if there is no such row.
pub fn get(db: &Connection, id: i64) -> Result<Option<Routine>, AxiomataError> {
    let mut stmt = db.prepare(&format!("SELECT {COLUMNS} FROM routines WHERE id = ?1"))?;
    match stmt.query_row([id], row_to_routine) {
        Ok(routine) => Ok(Some(routine)),
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
    let rows = stmt.query_map([now.to_rfc3339()], row_to_routine)?;
    collect(rows)
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

/// Records that a routine fired: sets `last_fired_at` and advances
/// `next_fire_at` to `next`. Does not touch `updated_at` (that tracks
/// user-facing config changes, not firings).
pub fn mark_fired(
    db: &Connection,
    id: i64,
    fired_at: DateTime<Utc>,
    next: Option<DateTime<Utc>>,
) -> Result<(), AxiomataError> {
    db.execute(
        "UPDATE routines SET last_fired_at = ?1, next_fire_at = ?2 WHERE id = ?3",
        rusqlite::params![fired_at.to_rfc3339(), next.map(|dt| dt.to_rfc3339()), id],
    )?;
    Ok(())
}

/// Advances `next_fire_at` to `next` without recording a firing — the startup
/// catch-up path for a routine whose scheduled time passed while the app was
/// not running.
pub fn roll_forward(
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

/// Appends one row to `routine_runs` and returns it with its assigned id.
pub fn record_run(
    db: &Connection,
    routine_id: i64,
    run: NewRoutineRun,
) -> Result<RoutineRun, AxiomataError> {
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
            run.detail,
        ],
    )?;
    Ok(RoutineRun {
        id: db.last_insert_rowid(),
        routine_id,
        run_id: run.run_id,
        scheduled_for: run.scheduled_for,
        fired_at: run.fired_at,
        status: run.status,
        detail: run.detail,
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
    collect(rows)
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

/// Maps a `routines` row (selected as [`COLUMNS`]) onto a [`Routine`].
fn row_to_routine(row: &rusqlite::Row<'_>) -> rusqlite::Result<Routine> {
    let target_type: String = row.get(3)?;
    let target_value: String = row.get(4)?;
    let target = RoutineTarget::from_columns(&target_type, target_value).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(3, Type::Text, err.to_string().into())
    })?;

    Ok(Routine {
        id: row.get(0)?,
        name: row.get(1)?,
        cron_expr: row.get(2)?,
        target,
        backend: row.get(5)?,
        enabled: row.get::<_, i64>(6)? != 0,
        next_fire_at: opt_timestamp(row.get::<_, Option<String>>(7)?, 7)?,
        last_fired_at: opt_timestamp(row.get::<_, Option<String>>(8)?, 8)?,
        created_at: timestamp(&row.get::<_, String>(9)?, 9)?,
        updated_at: timestamp(&row.get::<_, String>(10)?, 10)?,
    })
}

/// Maps a `routine_runs` row onto a [`RoutineRun`].
fn row_to_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<RoutineRun> {
    let status: String = row.get(5)?;
    Ok(RoutineRun {
        id: row.get(0)?,
        routine_id: row.get(1)?,
        run_id: row.get(2)?,
        scheduled_for: timestamp(&row.get::<_, String>(3)?, 3)?,
        fired_at: timestamp(&row.get::<_, String>(4)?, 4)?,
        status: RoutineRunStatus::from_db_str(&status, "status")?,
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
            AxiomataError::InvalidRoutine { .. }
        ));

        add(&db, new_routine("dup")).unwrap();
        assert!(matches!(
            add(&db, new_routine("dup")).unwrap_err(),
            AxiomataError::InvalidRoutine { .. }
        ));

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
        roll_forward(&db, a.id, Some(past)).unwrap();
        roll_forward(&db, b.id, Some(past)).unwrap();
        set_enabled(&db, b.id, false).unwrap();
        roll_forward(&db, c.id, Some(future)).unwrap();

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
        roll_forward(&db, r.id, Some(stale)).unwrap();
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
    fn mark_fired_and_record_run_build_history_and_state() {
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
        let next = Utc::now() + chrono::Duration::minutes(2);
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
        mark_fired(&db, r.id, fired_at, Some(next)).unwrap();

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
                scheduled_for: next,
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
    fn list_orders_by_next_fire_with_nulls_last() {
        let (path, db) = temp_db();
        let soon = add(&db, new_routine("soon")).unwrap();
        let later = add(&db, new_routine("later")).unwrap();
        let never = add(&db, new_routine("never")).unwrap();

        roll_forward(
            &db,
            soon.id,
            Some(Utc::now() + chrono::Duration::minutes(1)),
        )
        .unwrap();
        roll_forward(&db, later.id, Some(Utc::now() + chrono::Duration::hours(1))).unwrap();
        roll_forward(&db, never.id, None).unwrap();

        let ordered: Vec<String> = list(&db).unwrap().into_iter().map(|r| r.name).collect();
        assert_eq!(ordered, vec!["soon", "later", "never"]);

        let _ = fs::remove_file(&path);
    }
}
