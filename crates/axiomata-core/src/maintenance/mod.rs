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
pub mod glut;
mod rows;
mod worktrees;

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

pub use worktrees::OrphanWorktree;

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
    /// The worktrees whose agent is gone (A3c): a clean one is removed by a clean-up (`removed`), one with changes
    /// waits for the owner.
    pub orphan_worktrees: Vec<OrphanWorktree>,
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
    /// Where the studio puts its agents' worktrees (`~/Worktrees/axiomata-agents`).
    pub worktrees: PathBuf,
}

impl Locations {
    /// The live places, from [`paths`].
    pub fn current() -> Self {
        Self {
            home: paths::axiomata_home(),
            db: paths::db_path(),
            worktrees: paths::worktrees_dir(),
        }
    }
}

/// Measures everything and says what a clean-up would free; deletes nothing.
pub fn report(core: &AxiomataCore) -> Report {
    let settings = core.config_read().maintenance.clone();
    clean_with(&core.db, &settings, true)
}

/// Cleans up by the rules of `[maintenance]`; with `dry_run` only says what it would remove.
pub fn clean(core: &AxiomataCore, dry_run: bool) -> Report {
    let settings = core.config_read().maintenance.clone();
    clean_with(&core.db, &settings, dry_run)
}

/// [`clean`] on a shared connection and a copy of the rules, for a caller that runs it on a thread of its own (the
/// app does, so the window never waits for a clean-up).
pub fn clean_with(db: &Mutex<Connection>, settings: &MaintenanceSettings, dry_run: bool) -> Report {
    run(db, settings, &Locations::current(), Utc::now(), dry_run)
}

/// How often the app cleans up by itself at start (A2).
pub const WEEKLY: chrono::TimeDelta = chrono::TimeDelta::days(7);

/// How long after the start the weekly clean-up waits, so it never competes with the app coming up.
pub const START_DELAY: std::time::Duration = std::time::Duration::from_secs(90);

/// Whether the weekly clean-up is due: never ran, or the last run lies a week back (or, with a clock set back, in the
/// future — a stamp that cannot be right does not hold the clean-up off).
pub fn due(last_run: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    last_run.is_none_or(|last| now - last >= WEEKLY || last > now)
}

/// Starts the weekly clean-up of the app's start on a thread of its own (MT3): after [`START_DELAY`], only when
/// `[maintenance] enabled` and [`due`]. Never holds the start up, and holds the database only while it counts and
/// deletes rows.
pub fn spawn_weekly(core: &AxiomataCore) {
    let config = std::sync::Arc::clone(&core.config);
    let db = std::sync::Arc::clone(&core.db);
    let started = std::thread::Builder::new()
        .name("maintenance".to_owned())
        .spawn(move || {
            std::thread::sleep(START_DELAY);
            let settings = config
                .read()
                .unwrap_or_else(|poison| poison.into_inner())
                .maintenance
                .clone();
            weekly(&db, &settings, &Locations::current(), Utc::now());
        });
    if let Err(err) = started {
        tracing::warn!(%err, "could not start the weekly maintenance");
    }
}

/// The weekly clean-up's decision and run; `None` when it is switched off or not due.
pub(crate) fn weekly(
    db: &Mutex<Connection>,
    settings: &MaintenanceSettings,
    locations: &Locations,
    now: DateTime<Utc>,
) -> Option<Report> {
    if !settings.enabled || !due(last_run(&lock(db)), now) {
        return None;
    }
    let report = run(db, settings, locations, now, false);
    match &report.error {
        Some(error) => tracing::warn!(%error, "the weekly maintenance did not run"),
        None => tracing::info!(
            freed = report.freeable_bytes,
            total = report.total_bytes,
            "the weekly maintenance cleaned up"
        ),
    }
    Some(report)
}

/// Removes the orphaned worktree at `path` with whatever changes it has — the owner's "Delete" in Settings → Storage,
/// after the app asked. `path` must be one the sweep finds right now; anything else is refused.
///
/// # Errors
///
/// A sentence: not an orphan (or the agents could not be read), or why git or the file system refused.
pub fn remove_orphan_worktree(
    db: &Mutex<Connection>,
    path: &std::path::Path,
) -> Result<(), String> {
    remove_orphan_at(db, &Locations::current(), path)
}

pub(crate) fn remove_orphan_at(
    db: &Mutex<Connection>,
    locations: &Locations,
    path: &std::path::Path,
) -> Result<(), String> {
    let agents = Agents::read(&lock(db)).map_err(|err| err.to_string())?;
    worktrees::remove_one(&locations.worktrees, &agents, path)
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
            orphan_worktrees: Vec::new(),
            error: Some(error),
        };
    }
    // The one long hold of the database: the row deletes and, when it is worth it, `VACUUM` of a file of some 16 MB
    // (plan §3 accepts the short lock). Files, git and Glut all run after the guard is gone.
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
    // Git runs here, never under the database lock.
    let (worktree_item, orphan_worktrees) =
        worktrees::sweep(&locations.worktrees, &agents, dry_run);
    items.push(worktree_item);

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
        orphan_worktrees,
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
    /// The worktrees the agents' rows name.
    worktrees: Vec<PathBuf>,
    /// The studio's project folders: the only repositories an orphan's `.git` file is believed to name.
    projects: Vec<PathBuf>,
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
        let mut stmt = db.prepare("SELECT id, worktree_path FROM ide_agents")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let ids = rows.iter().map(|(id, _)| *id).collect();
        let worktrees = rows
            .into_iter()
            .filter_map(|(_, path)| path.map(PathBuf::from))
            .collect();
        let mut stmt = db.prepare("SELECT repo_root FROM projects")?;
        let projects = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|root| root.map(PathBuf::from))
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Self {
            ids,
            worktrees,
            projects,
            issued,
        })
    }

    /// When the agents cannot be read, no folder may count as orphaned.
    fn unknown() -> Self {
        Self {
            ids: HashSet::new(),
            worktrees: Vec::new(),
            projects: Vec::new(),
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
