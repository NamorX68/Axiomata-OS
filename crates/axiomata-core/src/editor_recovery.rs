//! Unsaved editor work, kept aside so a crash or a closed window does not
//! lose it (`docs/plans/editor.md`, D9 and F8).
//!
//! The editor writes the unsaved text of a file here two seconds after the
//! last change, one JSON file per open file under
//! `~/.axiomata/editor-recovery/`, named after a hash of its root id and path.
//! Saving or discarding the changes deletes it; opening the file again offers
//! it back. Entries older than [`MAX_AGE_DAYS`] are swept at startup.
//!
//! Nothing here reaches the file itself: restoring puts the text back into
//! the editor, and saving then goes through the file service's guard like any
//! other save. A recovery entry only ever holds what the editor already had.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AxiomataError;
use crate::json_state;
use crate::paths;

/// Layout of an entry on disk.
pub const RECOVERY_VERSION: u64 = 1;
/// Entries untouched for this long are swept at startup.
pub const MAX_AGE_DAYS: i64 = 30;
/// Largest content kept: the editor never edits more than this.
pub const MAX_CONTENT_BYTES: usize = axiomata_files::MAX_WRITE_BYTES as usize;
/// Largest entry file read back — content plus JSON escaping headroom.
const MAX_ENTRY_BYTES: u64 = 4 * MAX_CONTENT_BYTES as u64;
/// Most entries kept at once. The editor has a handful of files open; the cap
/// is there so a caller inventing ever new keys cannot fill the disk (ED1.3
/// security audit) — a new entry beyond it pushes out the oldest.
pub const MAX_ENTRIES: usize = 256;
/// Most bytes all entries together may take on disk. With 16 MiB files
/// editable (ED5, T2) the entry count alone would allow gigabytes; this keeps
/// the total bounded whatever the write limit is (ED5.1 security audit) —
/// saving pushes out the oldest entries until the new one fits.
pub const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

/// One file's unsaved text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recovery {
    pub version: u64,
    /// The file service's root id (`workspace`, `project:3`, …).
    pub root: String,
    pub rel: String,
    /// The file's version the unsaved text was based on; `None` for a file
    /// that did not exist yet. Lets the editor say "the file changed since".
    pub base_version: Option<String>,
    pub content: String,
    pub saved_at: DateTime<Utc>,
}

/// Where an entry for `root` + `rel` lives in `dir`.
fn entry_path(dir: &Path, root: &str, rel: &str) -> PathBuf {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let key = format!("{root}\0{rel}");
    let hash = key.bytes().fold(FNV_OFFSET, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(FNV_PRIME)
    });
    dir.join(format!("{hash:016x}.json"))
}

fn invalid(path: &Path, reason: impl Into<String>) -> AxiomataError {
    AxiomataError::InvalidDashboardState {
        path: path.to_path_buf(),
        reason: reason.into(),
    }
}

/// Keeps `content` as the unsaved state of `root` + `rel`.
///
/// Errors:
///     [`AxiomataError::InvalidDashboardState`] for content over
///     [`MAX_CONTENT_BYTES`]; [`AxiomataError::Io`] if the write fails.
pub fn save_in(
    dir: &Path,
    root: &str,
    rel: &str,
    base_version: Option<String>,
    content: &str,
) -> Result<(), AxiomataError> {
    let path = entry_path(dir, root, rel);
    if content.len() > MAX_CONTENT_BYTES {
        return Err(invalid(
            &path,
            "unsaved content is larger than the editor edits",
        ));
    }
    let entry = Recovery {
        version: RECOVERY_VERSION,
        root: root.to_string(),
        rel: rel.to_string(),
        base_version,
        content: content.to_string(),
        saved_at: Utc::now(),
    };
    let json = serde_json::to_string(&entry).map_err(|err| invalid(&path, err.to_string()))?;
    // An entry `load_in` would refuse to read back is not worth writing.
    if json.len() as u64 > MAX_ENTRY_BYTES {
        return Err(invalid(
            &path,
            "unsaved content escapes to more than an entry holds",
        ));
    }
    make_room(dir, &path, json.len() as u64, MAX_TOTAL_BYTES);
    json_state::save(&path, &json)
}

/// Entry files in `dir` with their modification times and sizes (regular files only).
fn entries(dir: &Path) -> Vec<(PathBuf, std::time::SystemTime, u64)> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    read.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .filter_map(|p| {
            let meta = fs::symlink_metadata(&p)
                .ok()
                .filter(fs::Metadata::is_file)?;
            Some((p, meta.modified().ok()?, meta.len()))
        })
        .collect()
}

/// Removes the oldest entries until one of `incoming` bytes, written at
/// `target`, fits under both [`MAX_ENTRIES`] and `budget` bytes in total
/// ([`MAX_TOTAL_BYTES`]; a parameter so a test need not write 256 MiB). The
/// entry at `target` itself is about to be replaced, so it counts for neither.
fn make_room(dir: &Path, target: &Path, incoming: u64, budget: u64) {
    let mut others: Vec<_> = entries(dir)
        .into_iter()
        .filter(|(p, _, _)| p != target)
        .collect();
    others.sort_by_key(|(_, modified, _)| *modified);
    let mut count = others.len();
    let mut total: u64 = others.iter().map(|(_, _, len)| len).sum();
    for (path, _, len) in &others {
        if count < MAX_ENTRIES && total + incoming <= budget {
            break;
        }
        if fs::remove_file(path).is_ok() {
            count -= 1;
            total -= len;
        }
    }
}

/// The unsaved state of `root` + `rel`, if there is one. An entry that is a
/// symlink, oversized, unreadable or belongs to another file (a hash
/// collision) counts as none.
pub fn load_in(dir: &Path, root: &str, rel: &str) -> Option<Recovery> {
    let path = entry_path(dir, root, rel);
    let meta = fs::symlink_metadata(&path).ok()?;
    if !meta.is_file() || meta.len() > MAX_ENTRY_BYTES {
        return None;
    }
    let mut raw = String::new();
    fs::File::open(&path)
        .ok()?
        .take(MAX_ENTRY_BYTES)
        .read_to_string(&mut raw)
        .ok()?;
    let entry: Recovery = serde_json::from_str(&raw).ok()?;
    (entry.version == RECOVERY_VERSION && entry.root == root && entry.rel == rel).then_some(entry)
}

/// Forgets the unsaved state of `root` + `rel`. A missing entry is fine.
///
/// Errors:
///     [`AxiomataError::Io`] if an existing entry cannot be removed.
pub fn delete_in(dir: &Path, root: &str, rel: &str) -> Result<(), AxiomataError> {
    let path = entry_path(dir, root, rel);
    match fs::remove_file(&path) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            Err(AxiomataError::Io { path, source: err })
        }
        _ => Ok(()),
    }
}

/// Removes entries last written before `now - MAX_AGE_DAYS`, judged by the
/// file's modification time, and temp files a crashed save left behind.
/// Only regular files directly in `dir` are touched; a symlink is never
/// followed or removed. Returns how many went.
pub fn sweep_in(dir: &Path, now: DateTime<Utc>) -> usize {
    let cutoff = now - Duration::days(MAX_AGE_DAYS);
    let mut removed = 0;
    for (path, modified, _) in entries(dir) {
        if DateTime::<Utc>::from(modified) < cutoff && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    let Ok(read) = fs::read_dir(dir) else {
        return removed;
    };
    for path in read.flatten().map(|e| e.path()) {
        let is_leftover = path.extension().is_some_and(|e| e == "axiomata-tmp")
            && fs::symlink_metadata(&path).is_ok_and(|m| m.is_file());
        if is_leftover && fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// [`save_in`] under `~/.axiomata/editor-recovery/`.
pub fn save(
    root: &str,
    rel: &str,
    base_version: Option<String>,
    content: &str,
) -> Result<(), AxiomataError> {
    save_in(
        &paths::editor_recovery_dir(),
        root,
        rel,
        base_version,
        content,
    )
}

/// [`load_in`] under `~/.axiomata/editor-recovery/`.
pub fn load(root: &str, rel: &str) -> Option<Recovery> {
    load_in(&paths::editor_recovery_dir(), root, rel)
}

/// [`delete_in`] under `~/.axiomata/editor-recovery/`.
pub fn delete(root: &str, rel: &str) -> Result<(), AxiomataError> {
    delete_in(&paths::editor_recovery_dir(), root, rel)
}

/// [`sweep_in`] under `~/.axiomata/editor-recovery/`, as of now.
pub fn sweep() -> usize {
    sweep_in(&paths::editor_recovery_dir(), Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

    #[test]
    fn the_content_cap_stays_wired_to_the_file_service_write_limit() {
        // Recovery must never hold more than the editor can ever save (T2:
        // large files are edited, not read-only, up to 16 MiB) — pinned so a
        // future change to one side does not silently drift from the other.
        assert_eq!(MAX_CONTENT_BYTES, 16 * 1024 * 1024);
        assert_eq!(MAX_CONTENT_BYTES as u64, axiomata_files::MAX_WRITE_BYTES);
    }

    #[test]
    fn saves_loads_and_deletes_one_entry_per_file() {
        let dir = unique_temp_dir("editor-recovery");
        save_in(
            &dir,
            "workspace",
            "Notes/a.md",
            Some("3-abc".into()),
            "unsaved",
        )
        .unwrap();
        save_in(&dir, "project:1", "Notes/a.md", None, "other root").unwrap();

        let entry = load_in(&dir, "workspace", "Notes/a.md").unwrap();
        assert_eq!(entry.content, "unsaved");
        assert_eq!(entry.base_version.as_deref(), Some("3-abc"));
        assert_eq!(
            load_in(&dir, "project:1", "Notes/a.md").unwrap().content,
            "other root"
        );
        assert!(load_in(&dir, "workspace", "Notes/b.md").is_none());

        // Saving again replaces the entry.
        save_in(&dir, "workspace", "Notes/a.md", None, "newer").unwrap();
        assert_eq!(
            load_in(&dir, "workspace", "Notes/a.md").unwrap().content,
            "newer"
        );

        delete_in(&dir, "workspace", "Notes/a.md").unwrap();
        assert!(load_in(&dir, "workspace", "Notes/a.md").is_none());
        delete_in(&dir, "workspace", "Notes/a.md").unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = entry_path(&dir, "project:1", "Notes/a.md");
            assert_eq!(
                fs::metadata(path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn base_version_none_round_trips_as_none() {
        let dir = unique_temp_dir("editor-recovery-no-base");
        save_in(&dir, "workspace", "new.md", None, "unsaved draft").unwrap();
        let entry = load_in(&dir, "workspace", "new.md").unwrap();
        assert_eq!(entry.base_version, None);
        assert_eq!(entry.content, "unsaved draft");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_entry_written_by_a_future_schema_version_is_not_handed_out() {
        let dir = unique_temp_dir("editor-recovery-version");
        save_in(&dir, "workspace", "a.md", None, "mine").unwrap();
        assert!(load_in(&dir, "workspace", "a.md").is_some());

        let path = entry_path(&dir, "workspace", "a.md");
        let bumped = fs::read_to_string(&path)
            .unwrap()
            .replace("\"version\":1", "\"version\":2");
        fs::write(&path, bumped).unwrap();
        assert!(
            load_in(&dir, "workspace", "a.md").is_none(),
            "an entry from a newer/different schema version must not be trusted"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn refuses_oversized_content_and_ignores_a_foreign_or_symlinked_entry() {
        let dir = unique_temp_dir("editor-recovery-guard");
        let big = "x".repeat(MAX_CONTENT_BYTES + 1);
        assert!(save_in(&dir, "workspace", "a.md", None, &big).is_err());

        // An entry whose recorded file is another one (a collision) is not handed out.
        save_in(&dir, "workspace", "a.md", None, "mine").unwrap();
        let path = entry_path(&dir, "workspace", "a.md");
        let foreign = fs::read_to_string(&path)
            .unwrap()
            .replace("\"a.md\"", "\"b.md\"");
        fs::write(&path, foreign).unwrap();
        assert!(load_in(&dir, "workspace", "a.md").is_none());

        #[cfg(unix)]
        {
            fs::remove_file(&path).unwrap();
            let elsewhere = dir.join("elsewhere.json");
            fs::write(&elsewhere, "{}").unwrap();
            std::os::unix::fs::symlink(&elsewhere, &path).unwrap();
            assert!(load_in(&dir, "workspace", "a.md").is_none());
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_at_most_max_entries_by_pushing_out_the_oldest() {
        let dir = unique_temp_dir("editor-recovery-cap");
        for i in 0..MAX_ENTRIES {
            save_in(&dir, "workspace", &format!("f{i}.md"), None, "x").unwrap();
        }
        // Make f0 unmistakably the oldest.
        let oldest = entry_path(&dir, "workspace", "f0.md");
        let past = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&oldest)
            .unwrap()
            .set_modified(past)
            .unwrap();

        // Rewriting an existing entry evicts nothing; a new key evicts the oldest.
        save_in(&dir, "workspace", "f1.md", None, "again").unwrap();
        assert!(load_in(&dir, "workspace", "f0.md").is_some());
        save_in(&dir, "workspace", "new.md", None, "y").unwrap();
        assert!(load_in(&dir, "workspace", "f0.md").is_none());
        assert!(load_in(&dir, "workspace", "new.md").is_some());
        assert_eq!(entries(&dir).len(), MAX_ENTRIES);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn keeps_the_total_under_the_byte_budget_by_pushing_out_the_oldest() {
        let dir = unique_temp_dir("editor-recovery-total");
        let content = "x".repeat(1000);
        for i in 0..6u64 {
            save_in(&dir, "workspace", &format!("f{i}.md"), None, &content).unwrap();
            let path = entry_path(&dir, "workspace", &format!("f{i}.md"));
            let at = std::time::SystemTime::now() - std::time::Duration::from_secs(600 - i * 60);
            fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(at)
                .unwrap();
        }
        let each = entries(&dir)[0].2;
        let budget = 6 * each;
        // Replacing an entry frees its own bytes first: nothing else goes.
        make_room(&dir, &entry_path(&dir, "workspace", "f3.md"), each, budget);
        assert_eq!(entries(&dir).len(), 6);
        // A new entry of the same size needs one old one out — the oldest.
        make_room(&dir, &entry_path(&dir, "workspace", "new.md"), each, budget);
        assert_eq!(entries(&dir).len(), 5);
        assert!(load_in(&dir, "workspace", "f0.md").is_none());
        assert!(load_in(&dir, "workspace", "f1.md").is_some());
        // One twice the size pushes out the next oldest as well.
        make_room(
            &dir,
            &entry_path(&dir, "workspace", "big.md"),
            2 * each,
            budget,
        );
        assert_eq!(entries(&dir).len(), 4);
        assert!(load_in(&dir, "workspace", "f1.md").is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sweeps_a_leftover_temp_file_but_never_a_symlink() {
        let dir = unique_temp_dir("editor-recovery-tmp");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("abc.json.axiomata-tmp"), "partial").unwrap();
        #[cfg(unix)]
        {
            let outside = unique_temp_dir("editor-recovery-outside");
            fs::create_dir_all(&outside).unwrap();
            fs::write(outside.join("keep.json"), "{}").unwrap();
            std::os::unix::fs::symlink(outside.join("keep.json"), dir.join("link.json")).unwrap();
            assert_eq!(
                sweep_in(&dir, Utc::now() + Duration::days(MAX_AGE_DAYS + 1)),
                1
            );
            assert!(outside.join("keep.json").exists());
            assert!(fs::symlink_metadata(dir.join("link.json")).is_ok());
            let _ = fs::remove_dir_all(outside);
        }
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sweep_ignores_unrelated_files_and_subdirectories() {
        let dir = unique_temp_dir("editor-recovery-sweep-ignores");
        fs::create_dir_all(&dir).unwrap();
        // An old-looking regular file that is neither `.json` nor
        // `.axiomata-tmp` must survive — sweep only ever touches the two
        // extensions it knows about.
        fs::write(dir.join("notes.txt"), "unrelated").unwrap();
        // A subdirectory, even one shaped like a `.json` entry name, must
        // never be removed or descended into.
        fs::create_dir_all(dir.join("nested.json")).unwrap();
        fs::write(dir.join("nested.json").join("inner.json"), "{}").unwrap();

        let far_future = Utc::now() + Duration::days(MAX_AGE_DAYS + 1);
        assert_eq!(
            sweep_in(&dir, far_future),
            0,
            "nothing here matches either extension entries()/the temp sweep look for"
        );
        assert!(dir.join("notes.txt").exists());
        assert!(dir.join("nested.json").is_dir());
        assert!(dir.join("nested.json").join("inner.json").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sweeps_only_entries_older_than_the_cutoff() {
        let dir = unique_temp_dir("editor-recovery-sweep");
        save_in(&dir, "workspace", "a.md", None, "fresh").unwrap();
        assert_eq!(sweep_in(&dir, Utc::now()), 0);
        assert_eq!(
            sweep_in(&dir, Utc::now() + Duration::days(MAX_AGE_DAYS + 1)),
            1
        );
        assert!(load_in(&dir, "workspace", "a.md").is_none());
        assert_eq!(sweep_in(&dir.join("missing"), Utc::now()), 0);
        let _ = fs::remove_dir_all(dir);
    }
}
