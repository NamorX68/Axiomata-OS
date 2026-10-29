//! The database side of the maintenance: old rows of the history tables, then compacting the file
//! (`docs/plans/maintenance.md` §3).
//!
//! Ages are compared with `julianday`, not as text: the stamps are RFC 3339, but not all written with the same offset
//! or precision, so a text comparison could keep or drop the wrong rows. A stamp SQLite cannot read gives `NULL`,
//! which compares as neither older nor newer — such a row simply stays.

use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, ToSql};

use super::Item;
use crate::config::MaintenanceSettings;

/// Above this share of free pages the database file is compacted (`VACUUM`) after a clean-up; below it the few
/// kilobytes are not worth locking the database for.
const VACUUM_FREE_SHARE: f64 = 0.10;

/// One history table and which of its rows a clean-up removes.
struct Rule {
    key: &'static str,
    label: &'static str,
    table: &'static str,
    /// The rows to remove, with `:cutoff` (and `:keep` when [`Rule::keep`] is set) bound.
    doomed: &'static str,
    days: u32,
    keep: Option<u32>,
}

impl Rule {
    fn keep_text(&self) -> String {
        match (self.key, self.keep) {
            ("card_events", _) => format!("{} days after the card was archived", self.days),
            (_, Some(keep)) => format!("{} days, at least the newest {keep}", self.days),
            (_, None) => format!("{} days", self.days),
        }
    }
}

/// The history tables with their rules, in the order the report lists them.
fn rules(settings: &MaintenanceSettings) -> [Rule; 6] {
    [
        Rule {
            key: "runs",
            label: "Runs",
            table: "runs",
            // A run that is still `running` is left alone whatever its age: its process may be about to write it.
            doomed: "status <> 'running' AND julianday(started_at) < julianday(:cutoff) \
                     AND id NOT IN (SELECT id FROM runs ORDER BY started_at DESC, id DESC LIMIT :keep)",
            days: settings.runs_days,
            keep: Some(settings.runs_keep),
        },
        Rule {
            key: "routine_runs",
            label: "Routine firings",
            table: "routine_runs",
            doomed: "julianday(fired_at) < julianday(:cutoff)",
            days: settings.routine_runs_days,
            keep: None,
        },
        Rule {
            key: "chat_turns",
            label: "Chat turns",
            table: "chat_turns",
            doomed: "julianday(created_at) < julianday(:cutoff)",
            days: settings.chat_turns_days,
            keep: None,
        },
        Rule {
            key: "web_searches",
            label: "Web searches",
            table: "web_searches",
            doomed: "julianday(searched_at) < julianday(:cutoff)",
            days: settings.web_searches_days,
            keep: None,
        },
        Rule {
            key: "card_events",
            label: "Archived card history",
            table: "card_events",
            // Only cards archived that long ago; the history of a living card stays whole (a2a.md A19).
            doomed: "card_id IN (SELECT id FROM cards WHERE archived_at IS NOT NULL \
                     AND julianday(archived_at) < julianday(:cutoff))",
            days: settings.card_events_days,
            keep: None,
        },
        Rule {
            key: "session_spend",
            label: "Session spend",
            table: "session_spend",
            doomed: "julianday(at) < julianday(:cutoff)",
            days: settings.session_spend_days,
            keep: None,
        },
    ]
}

/// Measures every history table and, unless `dry_run`, removes its old rows; each table on its own, so a failing one
/// does not keep the others from being cleaned.
pub(super) fn history(
    db: &Connection,
    settings: &MaintenanceSettings,
    now: DateTime<Utc>,
    dry_run: bool,
) -> Vec<Item> {
    rules(settings)
        .into_iter()
        .map(|rule| {
            let mut item = Item::new(rule.key, rule.label, "rows", rule.keep_text());
            if let Err(err) = apply(db, &rule, now, dry_run, &mut item) {
                tracing::warn!(%err, table = rule.table, "maintenance could not clean a table");
                item.note_error(err.to_string());
            }
            item
        })
        .collect()
}

fn apply(
    db: &Connection,
    rule: &Rule,
    now: DateTime<Utc>,
    dry_run: bool,
    item: &mut Item,
) -> rusqlite::Result<()> {
    // Out of range only for an age no validated setting has; then nothing is old enough to go.
    let cutoff = now
        .checked_sub_signed(Duration::days(i64::from(rule.days)))
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
        .to_rfc3339();
    let keep = rule.keep.map(i64::from);
    let mut params: Vec<(&str, &dyn ToSql)> = vec![(":cutoff", &cutoff)];
    if let Some(keep) = &keep {
        params.push((":keep", keep));
    }
    let total: i64 = db.query_row(&format!("SELECT COUNT(*) FROM {}", rule.table), [], |row| {
        row.get(0)
    })?;
    item.count = total.max(0) as u64;
    let freeable = if dry_run {
        let sql = format!("SELECT COUNT(*) FROM {} WHERE {}", rule.table, rule.doomed);
        db.query_row(&sql, params.as_slice(), |row| row.get::<_, i64>(0))?
            .max(0) as u64
    } else {
        let sql = format!("DELETE FROM {} WHERE {}", rule.table, rule.doomed);
        db.execute(&sql, params.as_slice())? as u64
    };
    item.freeable_count = freeable;
    Ok(())
}

/// The database file: its size on disk, and what compacting would give back. Unless `dry_run`, moves the write-ahead
/// log into the file (`wal_checkpoint(TRUNCATE)`) and compacts the file when more than [`VACUUM_FREE_SHARE`] of it is
/// free.
pub(super) fn database(db: &Connection, path: &Path, dry_run: bool) -> Item {
    let mut item = Item::new(
        "database",
        "Database file",
        "",
        "compacted when over 10 % is free".to_owned(),
    );
    let pages = |db: &Connection| -> rusqlite::Result<(i64, i64, i64)> {
        let count = db.query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let free = db.query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        let size = db.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        Ok((count, free, size))
    };
    let before = file_bytes(path);
    match pages(db) {
        Ok((count, free, size)) => {
            item.freeable_bytes = Some((free.max(0) * size.max(0)) as u64);
            if !dry_run {
                if let Err(err) = compact(db, count, free) {
                    tracing::warn!(%err, "maintenance could not compact the database");
                    item.note_error(err.to_string());
                }
                item.freeable_bytes = Some(before.saturating_sub(file_bytes(path)));
            }
        }
        Err(err) => item.note_error(err.to_string()),
    }
    item.bytes = Some(file_bytes(path));
    item
}

fn compact(db: &Connection, page_count: i64, free_pages: i64) -> rusqlite::Result<()> {
    // The pragma answers with a row (busy, log, checkpointed), so it is read rather than executed.
    db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    if page_count > 0 && free_pages as f64 / page_count as f64 > VACUUM_FREE_SHARE {
        db.execute_batch("VACUUM")?;
        db.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
    }
    Ok(())
}

/// The database with its write-ahead log and shared-memory file, as they are on disk now.
fn file_bytes(path: &Path) -> u64 {
    ["", "-wal", "-shm"]
        .iter()
        .map(|side| {
            let mut name = path.as_os_str().to_owned();
            name.push(side);
            std::fs::metadata(&name).map_or(0, |meta| meta.len())
        })
        .sum()
}
