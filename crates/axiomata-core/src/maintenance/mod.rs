//! Axiomata keeps its own storage clean (`docs/plans/maintenance.md`).
//!
//! Only Axiomata's own system is looked after (A1): `~/.axiomata` — history rows in the database, the logs, the
//! backups, orphaned status folders and the app's Claude Code sessions. Never a project, its build folders, the vault,
//! `~/.glut` (Glut cleans that itself, A5) or the owner's own things (`wallpaper/`, `theme.css`, skills).
//!
//! Two calls: [`report`] measures every item and says what a clean-up would free, [`clean`] does the same and deletes.
//! Every item is handled on its own — one that fails is noted on its line and in the app log, and the others still run.

mod files;
mod fsops;
mod rows;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::AxiomataCore;
use crate::config::MaintenanceSettings;
use crate::paths;

/// The `app_meta` key under which the moment of the last real clean-up is kept (RFC 3339).
pub const LAST_RUN_KEY: &str = "maintenance.last_run";

/// One line of the report: a kind of data, how much of it there is, and what a clean-up frees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// A stable name (`runs`, `app_log`, …) the Settings page maps its retention fields onto.
    pub key: String,
    /// What the owner reads.
    pub label: String,
    /// What [`Item::count`] counts (`rows`, `files`, …); empty when there is nothing to count.
    pub unit: String,
    /// Size on disk; `None` for database rows, whose bytes cannot be told apart in the file.
    pub bytes: Option<u64>,
    /// How many there are now (before a clean-up: in a clean's report, the count it started from).
    pub count: u64,
    /// What a clean-up frees — in a real clean's report, what it did free.
    pub freeable_bytes: Option<u64>,
    /// The number of things a clean-up removes (or removed).
    pub freeable_count: u64,
    /// The rule, in words (`90 days, at least the newest 200`).
    pub keep: String,
    /// Why this item could not (wholly) be measured or cleaned; the others are unaffected.
    pub error: Option<String>,
}

impl Item {
    fn new(key: &str, label: &str, unit: &str, keep: String) -> Self {
        Self {
            key: key.to_owned(),
            label: label.to_owned(),
            unit: unit.to_owned(),
            bytes: None,
            count: 0,
            freeable_bytes: None,
            freeable_count: 0,
            keep,
            error: None,
        }
    }

    /// Keeps the first error and counts the rest, so a folder of a thousand unreadable files is one short line.
    fn note_error(&mut self, error: String) {
        match &mut self.error {
            None => self.error = Some(error),
            Some(first) if !first.ends_with(" more)") => first.push_str(" (and more)"),
            Some(_) => {}
        }
    }
}

/// What the maintenance found — and, after a real clean-up, what it removed.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// `true` for a report or a `--dry-run`: nothing was deleted, `freeable_*` is what a clean-up would free.
    pub dry_run: bool,
    /// Axiomata's data folder (`~/.axiomata`).
    pub home: PathBuf,
    /// Its whole size on disk, after the clean-up when one ran.
    pub total_bytes: u64,
    /// The sum of the items' `freeable_bytes`.
    pub freeable_bytes: u64,
    /// When the last real clean-up ran (RFC 3339); `None` before the first.
    pub last_run: Option<DateTime<Utc>>,
    /// One line per kind of data, in a fixed order; empty when the pass was refused.
    pub items: Vec<Item>,
    /// Why nothing was measured or cleaned at all (rules out of range, a folder that is not Axiomata's).
    pub error: Option<String>,
}

/// The places the maintenance works in, resolved once — tests hand in a scratch folder instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locations {
    /// `~/.axiomata` (or `$AXIOMATA_HOME`).
    pub home: PathBuf,
    /// The database file inside it.
    pub db: PathBuf,
}

impl Locations {
    /// The live places, from [`paths`].
    pub fn current() -> Self {
        Self {
            home: paths::axiomata_home(),
            db: paths::db_path(),
        }
    }
}

/// Measures everything and says what a clean-up would free; deletes nothing.
pub fn report(core: &AxiomataCore) -> Report {
    let settings = core.config_read().maintenance.clone();
    run(&core.db, &settings, &Locations::current(), Utc::now(), true)
}

/// Cleans up by the rules of `[maintenance]`; with `dry_run` only says what it would remove.
pub fn clean(core: &AxiomataCore, dry_run: bool) -> Report {
    let settings = core.config_read().maintenance.clone();
    run(
        &core.db,
        &settings,
        &Locations::current(),
        Utc::now(),
        dry_run,
    )
}

/// Locks the database, recovering a poisoned guard like [`AxiomataCore::db_lock`].
fn lock(db: &Mutex<Connection>) -> std::sync::MutexGuard<'_, Connection> {
    db.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// The whole pass. The database is locked only while rows are counted or removed, never while files are walked, so
/// the app stays responsive during a clean-up.
pub(crate) fn run(
    db: &Mutex<Connection>,
    settings: &MaintenanceSettings,
    locations: &Locations,
    now: DateTime<Utc>,
    dry_run: bool,
) -> Report {
    if let Err(error) = settings.validate().and_then(|()| check_home(locations)) {
        tracing::warn!(%error, "maintenance refused to run");
        return Report {
            dry_run: true,
            home: locations.home.clone(),
            total_bytes: 0,
            freeable_bytes: 0,
            last_run: last_run(&lock(db)),
            items: Vec::new(),
            error: Some(error),
        };
    }
    let (mut items, agents, last_run) = {
        let db = lock(db);
        let mut items = rows::history(&db, settings, now, dry_run);
        items.push(rows::database(&db, &locations.db, dry_run));
        let agents = Agents::read(&db).unwrap_or_else(|err| {
            tracing::warn!(%err, "maintenance could not read the agents");
            Agents::unknown()
        });
        (items, agents, last_run(&db))
    };

    let clock = SystemTime::from(now);
    items.push(files::app_log(
        locations,
        settings.app_log_days,
        clock,
        dry_run,
    ));
    items.push(files::runs_log(locations));
    items.push(files::backups(locations, settings.backups_keep, dry_run));
    items.push(files::agent_events(
        locations,
        &agents.ids,
        agents.issued,
        dry_run,
    ));
    items.push(files::claude_sessions(
        locations,
        settings.claude_sessions_days,
        clock,
        dry_run,
    ));

    let last_run = if dry_run {
        last_run
    } else {
        match record_last_run(&lock(db), now) {
            Ok(()) => Some(now),
            Err(err) => {
                tracing::warn!(%err, "maintenance could not record its run");
                last_run
            }
        }
    };
    let freeable_bytes = items.iter().filter_map(|item| item.freeable_bytes).sum();
    Report {
        dry_run,
        home: locations.home.clone(),
        total_bytes: fsops::tree_size(&locations.home).0,
        freeable_bytes,
        last_run,
        items,
        error: None,
    }
}

/// Refuses a home that is not Axiomata's: the rules delete by name and age, so they must never run in the user's
/// home folder, the root, or a folder without Axiomata's database (an `$AXIOMATA_HOME` pointing somewhere else).
fn check_home(locations: &Locations) -> Result<(), String> {
    let home = &locations.home;
    let user_home = home::home_dir();
    let foreign = home.parent().is_none()
        || user_home.is_some_and(|user| user == *home)
        || locations.db.parent() != Some(home.as_path())
        || !locations.db.is_file();
    if foreign {
        return Err(format!(
            "{} does not look like Axiomata's data folder (no axiomata.db in it), so nothing was cleaned",
            home.display()
        ));
    }
    Ok(())
}

/// Which agents exist, read in one go so the files can be judged without the database.
struct Agents {
    ids: HashSet<i64>,
    /// The highest id the database has handed out so far: anything above it is newer than this read.
    issued: i64,
}

impl Agents {
    fn read(db: &Connection) -> rusqlite::Result<Self> {
        // The counter first, the ids after: an agent the CLI adds in between is then above `issued` or among the
        // ids — never below `issued` and missing, which would make its fresh folders look orphaned.
        let issued = db
            .query_row(
                "SELECT seq FROM sqlite_sequence WHERE name = 'ide_agents'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0);
        let mut stmt = db.prepare("SELECT id FROM ide_agents")?;
        let ids = stmt
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        Ok(Self { ids, issued })
    }

    /// When the agents cannot be read, no folder may count as orphaned.
    fn unknown() -> Self {
        Self {
            ids: HashSet::new(),
            issued: 0,
        }
    }
}

/// When the last real clean-up ran; `None` before the first, or when the stamp cannot be read.
pub fn last_run(db: &Connection) -> Option<DateTime<Utc>> {
    let stamp: Option<String> = db
        .query_row(
            "SELECT value FROM app_meta WHERE key = ?1",
            [LAST_RUN_KEY],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    stamp
        .and_then(|stamp| DateTime::parse_from_rfc3339(&stamp).ok())
        .map(|when| when.with_timezone(&Utc))
}

fn record_last_run(db: &Connection, now: DateTime<Utc>) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO app_meta (key, value) VALUES (?1, ?2) \
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        [LAST_RUN_KEY, &now.to_rfc3339()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
