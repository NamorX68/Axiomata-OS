//! The files the maintenance looks after: the app log, the run log, backups, orphaned status folders and the app's
//! own Claude Code sessions (`docs/plans/maintenance.md` §3). Every function measures; with `dry_run` false it also
//! deletes, through [`fsops::remove_entry`] only.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime};

use super::fsops::{self, Entry};
use super::{Item, Locations};
use crate::skills::runlog::{RUNS_LOG_GENERATIONS, RUNS_LOG_ROTATE_BYTES, rotated_log_path};

/// What the app log's files start with (`app.log.<date>`, the Tauri shell's rolling appender).
const APP_LOG_PREFIX: &str = "app.log.";

/// The marker between a file's name and the tag of a backup copy of it (`axiomata.db.backup-2026-10-07`).
const BACKUP_MARKER: &str = ".backup-";

/// The parts of a SQLite database that belong to one backup copy: `<db>.backup-<tag>-shm`/`-wal` go with
/// `<db>.backup-<tag>`.
const SQLITE_SIDE_FILES: [&str; 2] = ["-shm", "-wal"];

/// The folders of the app's Claude Code login folder the maintenance may clean (`~/.axiomata/claude`), with how deep
/// their entries lie: `projects/<folder>/<session>` holds one folder per working directory, the others hold their
/// entries directly. **Nothing else** in that folder is touched — it holds the login.
const CLAUDE_SESSION_DIRS: [(&str, usize); 4] = [
    ("projects", 2),
    ("runs", 1),
    ("shell-snapshots", 1),
    ("session-env", 1),
];

/// The moment `days` before `now`.
fn cutoff(now: SystemTime, days: u32) -> SystemTime {
    now.checked_sub(Duration::from_secs(u64::from(days) * 24 * 60 * 60))
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Deletes `doomed` (each listed from the folder beside it) unless `dry_run`, and counts on `item` what goes.
///
/// One failure never stops the others: it is logged, noted on the item, and its bytes are not counted as freed.
fn drop_entries(item: &mut Item, base: &Path, doomed: Vec<(&Path, Entry, u64)>, dry_run: bool) {
    for (dir, entry, bytes) in doomed {
        if !dry_run && let Err(err) = fsops::remove_entry(base, dir, &entry) {
            tracing::warn!(%err, path = %entry.path.display(), "maintenance could not remove an entry");
            item.note_error(format!("{}: {err}", entry.path.display()));
            continue;
        }
        item.freeable_count += 1;
        *item.freeable_bytes.get_or_insert(0) += bytes;
    }
}

/// The daily app log files older than `days` (the appender keeps 14 files besides; this bounds them by age too).
pub(super) fn app_log(locations: &Locations, days: u32, now: SystemTime, dry_run: bool) -> Item {
    let mut item = Item::new("app_log", "App log", "files", format!("{days} days"));
    item.bytes = Some(0);
    item.freeable_bytes = Some(0);
    let Some(dir) = fsops::real_dir(&locations.home, &["logs"]) else {
        return item;
    };
    let listed = match fsops::entries(&dir) {
        Ok(listed) => listed,
        Err(err) => {
            item.note_error(format!("{}: {err}", dir.display()));
            return item;
        }
    };
    let oldest_kept = cutoff(now, days);
    let mut doomed = Vec::new();
    for entry in listed {
        if !entry.is_file() || !entry.name.starts_with(APP_LOG_PREFIX) {
            continue;
        }
        let bytes = entry.meta.len();
        item.count += 1;
        *item.bytes.get_or_insert(0) += bytes;
        if entry.modified() < oldest_kept {
            doomed.push((dir.as_path(), entry, bytes));
        }
    }
    drop_entries(&mut item, &locations.home, doomed, dry_run);
    item
}

/// The run log and its rotated generations. Only measured: `runlog` rotates it itself when it is full.
pub(super) fn runs_log(locations: &Locations) -> Item {
    let mut item = Item::new(
        "runs_log",
        "Run log",
        "files",
        format!(
            "rotated at {} MB, the last {RUNS_LOG_GENERATIONS} kept",
            RUNS_LOG_ROTATE_BYTES / (1024 * 1024)
        ),
    );
    item.bytes = Some(0);
    item.freeable_bytes = Some(0);
    let Some(dir) = fsops::real_dir(&locations.home, &["logs"]) else {
        return item;
    };
    let log = dir.join("runs.log");
    let generations = (1..=RUNS_LOG_GENERATIONS).map(|n| rotated_log_path(&log, n));
    for path in std::iter::once(log.clone()).chain(generations) {
        if let Ok(meta) = std::fs::symlink_metadata(&path)
            && meta.file_type().is_file()
        {
            item.count += 1;
            *item.bytes.get_or_insert(0) += meta.len();
        }
    }
    item
}

/// Splits a backup's name into the file it copies and the tag of the copy: `axiomata.db.backup-2026-10-07-wal` is
/// (`axiomata.db`, `2026-10-07`), so a database's side files stay with it.
fn backup_of(name: &str) -> Option<(&str, &str)> {
    let (base, tag) = name.split_once(BACKUP_MARKER)?;
    let tag = SQLITE_SIDE_FILES
        .iter()
        .find_map(|side| tag.strip_suffix(side))
        .unwrap_or(tag);
    (!base.is_empty() && !tag.is_empty()).then_some((base, tag))
}

/// The `*.backup-*` copies directly in `~/.axiomata`: the newest `keep` copies of each file stay.
///
/// "Newest" is by the copy's last modification (its newest part), not by its tag: a tag is whatever name the copy got
/// by hand.
pub(super) fn backups(locations: &Locations, keep: u32, dry_run: bool) -> Item {
    let mut item = Item::new(
        "backups",
        "Backups",
        "files",
        format!("the newest {keep} of each file"),
    );
    item.bytes = Some(0);
    item.freeable_bytes = Some(0);
    let home = &locations.home;
    let listed = match fsops::entries(home) {
        Ok(listed) => listed,
        Err(err) => {
            item.note_error(format!("{}: {err}", home.display()));
            return item;
        }
    };
    // file → tag → the parts of that copy.
    let mut copies: BTreeMap<String, BTreeMap<String, Vec<Entry>>> = BTreeMap::new();
    for entry in listed {
        if !entry.is_file() {
            continue;
        }
        let Some((base, tag)) = backup_of(&entry.name) else {
            continue;
        };
        let (base, tag) = (base.to_owned(), tag.to_owned());
        item.count += 1;
        *item.bytes.get_or_insert(0) += entry.meta.len();
        copies
            .entry(base)
            .or_default()
            .entry(tag)
            .or_default()
            .push(entry);
    }
    let mut doomed = Vec::new();
    for by_tag in copies.into_values() {
        let mut ordered: Vec<(SystemTime, String, Vec<Entry>)> = by_tag
            .into_iter()
            .map(|(tag, parts)| {
                let newest = parts.iter().map(Entry::modified).max();
                (newest.unwrap_or(SystemTime::UNIX_EPOCH), tag, parts)
            })
            .collect();
        // Newest first; copies written at the same moment by their tag, so a dated tag decides between them.
        ordered.sort_by(|a, b| (&b.0, &b.1).cmp(&(&a.0, &a.1)));
        for (_, _, parts) in ordered.into_iter().skip(keep as usize) {
            for entry in parts {
                let bytes = entry.meta.len();
                doomed.push((home.as_path(), entry, bytes));
            }
        }
    }
    drop_entries(&mut item, &locations.home, doomed, dry_run);
    item
}

/// The status folders of agents that no longer exist (`agent-events/<id>`).
///
/// `known` are the agents' ids and `issued` the highest id the database had handed out when they were read: a folder
/// with a higher number belongs to an agent created since, never to a deleted one, so it stays.
pub(super) fn agent_events(
    locations: &Locations,
    known: &HashSet<i64>,
    issued: i64,
    dry_run: bool,
) -> Item {
    let mut item = Item::new(
        "agent_events",
        "Agent status folders",
        "folders",
        "while their agent exists".to_owned(),
    );
    item.bytes = Some(0);
    item.freeable_bytes = Some(0);
    let Some(dir) = fsops::real_dir(&locations.home, &["agent-events"]) else {
        return item;
    };
    let listed = match fsops::entries(&dir) {
        Ok(listed) => listed,
        Err(err) => {
            item.note_error(format!("{}: {err}", dir.display()));
            return item;
        }
    };
    let mut doomed = Vec::new();
    for entry in listed {
        // Only `<id>` folders are channels; anything else in there is not this rule's to judge.
        let Some(id) = entry
            .is_dir()
            .then(|| entry.name.parse::<i64>().ok())
            .flatten()
        else {
            continue;
        };
        let (bytes, _) = fsops::tree_size(&entry.path);
        item.count += 1;
        *item.bytes.get_or_insert(0) += bytes;
        if id > 0 && id <= issued && !known.contains(&id) {
            doomed.push((dir.as_path(), entry, bytes));
        }
    }
    drop_entries(&mut item, &locations.home, doomed, dry_run);
    item
}

/// The app's own Claude Code sessions older than `days`, in the four folders of [`CLAUDE_SESSION_DIRS`] only.
///
/// An entry is old when nothing inside it changed for `days` (a folder counts by its newest file), so a session that
/// is still being written to is never cut apart. A project folder emptied this way goes too.
pub(super) fn claude_sessions(
    locations: &Locations,
    days: u32,
    now: SystemTime,
    dry_run: bool,
) -> Item {
    let mut item = Item::new(
        "claude_sessions",
        "Claude sessions",
        "entries",
        format!("{days} days"),
    );
    item.bytes = Some(0);
    item.freeable_bytes = Some(0);
    let oldest_kept = cutoff(now, days);
    for (name, depth) in CLAUDE_SESSION_DIRS {
        let Some(dir) = fsops::real_dir(&locations.home, &["claude", name]) else {
            continue;
        };
        if depth == 1 {
            clean_level(&mut item, &locations.home, &dir, oldest_kept, dry_run);
            continue;
        }
        let folders = match fsops::entries(&dir) {
            Ok(listed) => listed,
            Err(err) => {
                item.note_error(format!("{}: {err}", dir.display()));
                continue;
            }
        };
        for folder in folders.into_iter().filter(Entry::is_dir) {
            clean_level(
                &mut item,
                &locations.home,
                &folder.path,
                oldest_kept,
                dry_run,
            );
            // Only an empty folder goes; `remove_dir` refuses one with anything left in it.
            if !dry_run
                && fsops::entries(&folder.path).is_ok_and(|left| left.is_empty())
                && fsops::real_dir(&dir, &[&folder.name]).is_some()
            {
                let _ = std::fs::remove_dir(&folder.path);
            }
        }
    }
    item
}

/// Measures the entries directly in `dir` and removes those older than `oldest_kept`.
fn clean_level(item: &mut Item, base: &Path, dir: &Path, oldest_kept: SystemTime, dry_run: bool) {
    let listed = match fsops::entries(dir) {
        Ok(listed) => listed,
        Err(err) => {
            item.note_error(format!("{}: {err}", dir.display()));
            return;
        }
    };
    let mut doomed = Vec::new();
    for entry in listed {
        if !entry.is_file() && !entry.is_dir() {
            continue;
        }
        let (bytes, newest) = fsops::tree_size(&entry.path);
        item.count += 1;
        *item.bytes.get_or_insert(0) += bytes;
        if newest < oldest_kept {
            doomed.push((dir, entry, bytes));
        }
    }
    drop_entries(item, base, doomed, dry_run);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backup_name_names_its_file_and_a_database_keeps_its_side_files() {
        assert_eq!(
            backup_of("axiomata.db.backup-2026-10-07"),
            Some(("axiomata.db", "2026-10-07"))
        );
        assert_eq!(
            backup_of("axiomata.db.backup-2026-10-07-wal"),
            Some(("axiomata.db", "2026-10-07"))
        );
        assert_eq!(
            backup_of("axiomata.db.backup-2026-10-07-shm"),
            Some(("axiomata.db", "2026-10-07"))
        );
        assert_eq!(
            backup_of("config.toml.backup-2026-10-08"),
            Some(("config.toml", "2026-10-08"))
        );
        assert_eq!(backup_of("config.toml"), None);
        assert_eq!(backup_of(".backup-x"), None);
        assert_eq!(backup_of("a.backup-"), None);
    }
}
