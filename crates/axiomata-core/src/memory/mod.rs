//! Memory router: keeps the generated router block in the Second-Brain
//! workspace's `CLAUDE.md` files in step with the actual folder contents.
//!
//! - [`walker`] scans the workspace and groups files by top-level area.
//! - [`router`] renders the delimited block and splices it into a file.
//! - [`sync`] ties those together: one block per area plus the root.
//! - [`status`] reports whether a re-sync is due (a tracked file is newer than
//!   the last-written root `CLAUDE.md`).
//! - [`watcher`] (M2 step 9) flips a stale flag reactively for the GUI.
//!
//! Sync is always explicit — a CLI command, the "Sync Now" button, or once at
//! app start. Nothing here rewrites the user's files on a timer.
//!
//! Implemented in M2.

use std::path::PathBuf;
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::config::Config;
use crate::error::AxiomataError;

pub mod router;
pub mod walker;
pub mod watcher;

/// Name of the router file Claude Code auto-loads from a directory.
const CLAUDE_MD: &str = "CLAUDE.md";

/// What a [`sync`] did.
#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    /// `CLAUDE.md` files created or updated this run.
    pub written: Vec<PathBuf>,
    /// `CLAUDE.md` files that already held the current block.
    pub unchanged: usize,
    /// Total tracked files the scan found.
    pub tracked_files: usize,
}

/// Whether the router is up to date with the workspace.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryStatus {
    /// The workspace being tracked.
    pub workspace_root: PathBuf,
    /// Last sync, taken as the modification time of the root `CLAUDE.md`.
    /// `None` if that file doesn't exist yet.
    pub last_sync: Option<DateTime<Utc>>,
    /// `true` if a tracked file has changed since `last_sync`, or the workspace
    /// has never been synced.
    pub stale: bool,
    /// Total tracked files.
    pub tracked_files: usize,
}

/// Regenerates every router block: one `<area>/CLAUDE.md` per top-level area and
/// the root `<workspace_root>/CLAUDE.md`.
///
/// The root file is written last, so its modification time marks "everything is
/// in sync as of now" for [`status`]. Files whose block is already current are
/// not touched.
///
/// Errors:
///     [`AxiomataError::Io`] on a filesystem failure;
///     [`AxiomataError::InvalidRouter`] if a `CLAUDE.md` has a start marker
///     with no matching end marker.
pub fn sync(config: &Config) -> Result<SyncReport, AxiomataError> {
    let scan = walker::scan(config)?;
    let root = &config.workspace_root;

    let mut written = Vec::new();
    let mut unchanged = 0usize;

    let mut record = |path: PathBuf, outcome: router::BlockWrite| match outcome {
        router::BlockWrite::Written => written.push(path),
        router::BlockWrite::Unchanged => unchanged += 1,
    };

    for (area, entries) in &scan.tree.areas {
        let path = root.join(area).join(CLAUDE_MD);
        let block = router::render_area_block(area, entries);
        record(path.clone(), router::upsert_block(&path, &block)?);
    }

    let root_path = root.join(CLAUDE_MD);
    let root_block = router::render_root_block(&scan.tree);
    record(
        root_path.clone(),
        router::upsert_block(&root_path, &root_block)?,
    );

    Ok(SyncReport {
        written,
        unchanged,
        tracked_files: scan.file_count,
    })
}

/// Reports whether the router is stale, without writing anything.
///
/// "Stale" means: the workspace has a tracked file whose modification time is
/// after the root `CLAUDE.md`'s (i.e. it changed since the last sync), or the
/// root `CLAUDE.md` doesn't exist yet.
///
/// Errors:
///     [`AxiomataError::Io`] if the workspace root cannot be read.
pub fn status(config: &Config) -> Result<MemoryStatus, AxiomataError> {
    let fresh = walker::freshness(config)?;
    let root_claude = config.workspace_root.join(CLAUDE_MD);
    let last_sync_mtime = std::fs::metadata(&root_claude)
        .and_then(|meta| meta.modified())
        .ok();

    let stale = match last_sync_mtime {
        None => true,
        Some(synced) => matches!(fresh.newest_mtime, Some(newest) if newest > synced),
    };

    Ok(MemoryStatus {
        workspace_root: config.workspace_root.clone(),
        last_sync: last_sync_mtime.map(system_time_to_utc),
        stale,
        tracked_files: fresh.file_count,
    })
}

/// Converts a `SystemTime` to a UTC `DateTime`, clamping a pre-epoch time to
/// the epoch (only reachable via a badly wrong system clock).
fn system_time_to_utc(t: SystemTime) -> DateTime<Utc> {
    match t.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(dur) => {
            DateTime::from_timestamp(dur.as_secs() as i64, dur.subsec_nanos()).unwrap_or_default()
        }
        Err(_) => DateTime::<Utc>::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;
    use std::fs;
    use std::path::Path;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn workspace(files: &[(&str, &str)]) -> (Scratch, Config) {
        let root = unique_temp_dir("axiomata-test-memory");
        for (rel, contents) in files {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };
        (Scratch(root), config)
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn sync_writes_root_and_area_router_files_then_is_a_no_op() {
        let (s, config) = workspace(&[
            ("inbox.md", "# Inbox\n"),
            ("projects/a.md", "---\ntitle: Project A\n---\nbody\n"),
            ("projects/b.md", "no title\n"),
        ]);

        let first = sync(&config).unwrap();
        assert_eq!(first.tracked_files, 3);
        assert_eq!(first.unchanged, 0);
        assert_eq!(first.written.len(), 2); // root + projects/

        let root_md = read(&s.0.join("CLAUDE.md"));
        assert!(root_md.contains("- **projects/** — 2 files · [index](projects/CLAUDE.md)"));
        assert!(root_md.contains("- `inbox.md` — Inbox"));

        let area_md = read(&s.0.join("projects/CLAUDE.md"));
        assert!(area_md.contains("## projects/ — index"));
        assert!(area_md.contains("- `a.md` — Project A"));
        assert!(area_md.contains("- `b.md`\n"));

        // A second sync with no changes writes nothing (determinism + idempotence).
        let second = sync(&config).unwrap();
        assert!(second.written.is_empty());
        assert_eq!(second.unchanged, 2);
        assert_eq!(read(&s.0.join("CLAUDE.md")), root_md);
    }

    #[test]
    fn sync_preserves_hand_written_content_outside_the_block() {
        let (s, config) = workspace(&[("note.md", "# Note\n")]);
        let root_path = s.0.join("CLAUDE.md");
        fs::write(&root_path, "# My workspace\n\nImportant hand notes.\n").unwrap();

        sync(&config).unwrap();
        let result = read(&root_path);
        assert!(result.starts_with("# My workspace\n\nImportant hand notes.\n"));
        assert!(result.contains(router::ROUTER_START));
        assert!(result.contains("- `note.md` — Note"));
    }

    #[test]
    fn status_is_fresh_right_after_sync_and_stale_after_an_edit() {
        let (s, config) = workspace(&[("a.md", "# A\n")]);

        assert!(status(&config).unwrap().stale, "never synced -> stale");

        sync(&config).unwrap();
        let after = status(&config).unwrap();
        assert!(!after.stale);
        assert_eq!(after.tracked_files, 1);
        assert!(after.last_sync.is_some());

        // Make a tracked file clearly newer than the root CLAUDE.md.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(s.0.join("a.md"), "# A edited\n").unwrap();
        assert!(status(&config).unwrap().stale, "edited file -> stale");

        // Re-syncing clears it again.
        sync(&config).unwrap();
        assert!(!status(&config).unwrap().stale);
    }
}
