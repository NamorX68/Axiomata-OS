//! Memory router: keeps the generated router block in the Second-Brain
//! workspace's `CLAUDE.md` files in step with the actual folder contents.
//!
//! - [`walker`] scans the workspace and groups files by top-level area.
//! - [`router`] renders the delimited block and splices it into a file.
//! - [`sync`] ties those together: one block per area plus the root.
//! - [`status`] reports whether a re-sync is due (a tracked file changed since
//!   the last sync).
//!
//! Sync is always explicit — a CLI command, the "Sync now" button, or once at
//! app start. Nothing here rewrites the user's files on a timer, and there is no
//! filesystem watcher: staleness is a cheap walk-and-compare on demand.
//!
//! Implemented in M2.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::config::Config;
use crate::error::AxiomataError;
use crate::paths;

pub(crate) mod router;
pub(crate) mod walker;

/// Name of the router file Claude Code auto-loads from a directory.
const CLAUDE_MD: &str = "CLAUDE.md";

/// What a [`sync`] did.
#[derive(Debug, Clone, Serialize)]
pub struct SyncReport {
    /// `CLAUDE.md` files created or updated this run.
    pub written: Vec<PathBuf>,
    /// Number of `CLAUDE.md` files that already held the current block.
    pub unchanged: usize,
    /// `CLAUDE.md` files that could not be written, with why. A bad file no
    /// longer aborts the whole sync — the rest still run.
    pub failed: Vec<(PathBuf, String)>,
    /// Total tracked files the scan found.
    pub tracked_files: usize,
}

/// Whether the router is up to date with the workspace.
#[derive(Debug, Clone, Serialize)]
pub struct MemoryStatus {
    /// The workspace being tracked.
    pub workspace_root: PathBuf,
    /// When `sync` last ran for this workspace, from the sync-marker file.
    /// `None` if it has never been synced.
    pub last_sync: Option<DateTime<Utc>>,
    /// `true` if a tracked file has changed since `last_sync`, or the workspace
    /// has never been synced.
    pub stale: bool,
    /// Total tracked files.
    pub tracked_files: usize,
}

/// Regenerates every router block: one `<area>/CLAUDE.md` per top-level area and
/// the root `<workspace_root>/CLAUDE.md`, then stamps the sync marker.
///
/// A `CLAUDE.md` that can't be written (a symlink, an ambiguous hand-edited
/// block, an I/O error) is collected into [`SyncReport::failed`] and the sync
/// continues with the rest.
///
/// Errors:
///     [`AxiomataError::UnsafeWorkspaceRoot`] if `workspace_root` resolves to a
///     dangerous location (`/`, the home directory);
///     [`AxiomataError::Io`] if the workspace root itself cannot be read.
pub fn sync(config: &Config) -> Result<SyncReport, AxiomataError> {
    let root = guarded_root(config)?;
    let scan = walker::scan(config)?;

    let mut written = Vec::new();
    let mut unchanged = 0usize;
    let mut failed = Vec::new();

    let mut apply = |path: PathBuf, block: String| match write_block_within(&root, &path, &block) {
        Ok(router::BlockWrite::Written) => written.push(path),
        Ok(router::BlockWrite::Unchanged) => unchanged += 1,
        Err(err) => failed.push((path, err.to_string())),
    };

    for (area, entries) in &scan.tree.areas {
        let path = root.join(area).join(CLAUDE_MD);
        apply(path, router::render_area_block(area, entries));
    }
    let root_path = root.join(CLAUDE_MD);
    apply(root_path, router::render_root_block(&scan.tree));

    stamp_marker(&root)?;

    Ok(SyncReport {
        written,
        unchanged,
        failed,
        tracked_files: scan.file_count,
    })
}

/// Reports whether the router is stale, without writing anything.
///
/// Stale = a tracked file's modification time is after this workspace's entry in
/// the sync-marker file, or the workspace has never been synced.
///
/// Errors:
///     [`AxiomataError::Io`] if the workspace root cannot be read.
pub fn status(config: &Config) -> Result<MemoryStatus, AxiomataError> {
    let fresh = walker::freshness(config)?;
    let last_sync = read_marker(&config.workspace_root);
    let newest = fresh.newest_mtime.map(system_time_to_utc);

    let stale = match (last_sync, newest) {
        (None, _) => true,
        (Some(synced), Some(newest)) => newest > synced,
        (Some(_), None) => false,
    };

    Ok(MemoryStatus {
        workspace_root: config.workspace_root.clone(),
        last_sync,
        stale,
        tracked_files: fresh.file_count,
    })
}

/// Canonicalizes `workspace_root` and rejects obviously wrong targets.
fn guarded_root(config: &Config) -> Result<PathBuf, AxiomataError> {
    let root = config
        .workspace_root
        .canonicalize()
        .map_err(|source| AxiomataError::Io {
            path: config.workspace_root.clone(),
            source,
        })?;

    let unsafe_target = root.parent().is_none() // filesystem root
        || home::home_dir().is_some_and(|home| {
            home.canonicalize().is_ok_and(|home| home == root)
        });
    if unsafe_target {
        return Err(AxiomataError::UnsafeWorkspaceRoot { path: root });
    }
    Ok(root)
}

/// `upsert_block`, but only if the resolved target stays inside `root` — a
/// planted `../` area name or a symlinked directory can't redirect the write.
fn write_block_within(
    root: &Path,
    target: &Path,
    block: &str,
) -> Result<router::BlockWrite, AxiomataError> {
    if let Some(parent) = target.parent() {
        // The parent directory must exist and canonicalize under the root.
        // `upsert_block` creates leaf dirs itself, so create `parent` first.
        fs::create_dir_all(parent).map_err(|source| AxiomataError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        let parent_canon = parent.canonicalize().map_err(|source| AxiomataError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        if !parent_canon.starts_with(root) {
            return Err(AxiomataError::InvalidRouter {
                path: target.to_path_buf(),
                reason: "resolved to a location outside the workspace root".to_owned(),
            });
        }
    }
    router::upsert_block(target, block)
}

// --- sync marker -----------------------------------------------------------

/// Reads this workspace's last-sync timestamp from
/// `~/.axiomata/memory-last-sync.json`, or `None` if unrecorded / unreadable.
fn read_marker(workspace_root: &Path) -> Option<DateTime<Utc>> {
    let key = marker_key(workspace_root);
    let raw = fs::read_to_string(paths::memory_last_sync_path()).ok()?;
    let map: BTreeMap<String, String> = serde_json::from_str(&raw).ok()?;
    let stamp = map.get(&key)?;
    DateTime::parse_from_rfc3339(stamp)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Records "synced now" for `root` in the marker file (created if missing).
fn stamp_marker(root: &Path) -> Result<(), AxiomataError> {
    let path = paths::memory_last_sync_path();
    let mut map: BTreeMap<String, String> = fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default();
    map.insert(marker_key(root), Utc::now().to_rfc3339());

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AxiomataError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let serialized = serde_json::to_string_pretty(&map).map_err(|err| AxiomataError::Io {
        path: path.clone(),
        source: std::io::Error::other(err),
    })?;
    fs::write(&path, serialized).map_err(|source| AxiomataError::Io { path, source })
}

/// The marker-file key for a workspace: its canonical path if resolvable,
/// otherwise the path as given (so `status` before the first `sync` still keys
/// consistently).
fn marker_key(workspace_root: &Path) -> String {
    workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf())
        .to_string_lossy()
        .into_owned()
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
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;

    struct Scratch {
        _guard: std::sync::MutexGuard<'static, ()>,
        home: PathBuf,
        ws: PathBuf,
        config: Config,
    }

    impl Scratch {
        /// Isolated `AXIOMATA_HOME` (so the marker file is scratch too) plus a
        /// scratch workspace pre-filled from `(rel, contents)` pairs.
        fn new(files: &[(&str, &str)]) -> Self {
            let guard = ENV_MUTEX.lock().unwrap();
            let home = unique_temp_dir("axiomata-test-memory-home");
            let ws = unique_temp_dir("axiomata-test-memory-ws");
            fs::create_dir_all(&home).unwrap();
            for (rel, contents) in files {
                let path = ws.join(rel);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, contents).unwrap();
            }
            // SAFETY: serialized by `guard`, see `paths::tests`.
            unsafe {
                env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
            }
            let config = Config {
                workspace_root: ws.clone(),
                ..Config::default()
            };
            Self {
                _guard: guard,
                home,
                ws,
                config,
            }
        }

        fn read(&self, rel: &str) -> String {
            fs::read_to_string(self.ws.join(rel)).unwrap()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            // SAFETY: still holding `_guard`.
            unsafe {
                env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
            }
            let _ = fs::remove_dir_all(&self.home);
            let _ = fs::remove_dir_all(&self.ws);
        }
    }

    #[test]
    fn sync_writes_root_and_area_router_files_then_is_a_no_op() {
        let s = Scratch::new(&[
            ("inbox.md", "# Inbox\n"),
            ("projects/a.md", "---\ntitle: Project A\n---\nbody\n"),
            ("projects/b.md", "no title\n"),
        ]);

        let first = sync(&s.config).unwrap();
        assert_eq!(first.tracked_files, 3);
        assert_eq!(first.unchanged, 0);
        assert!(first.failed.is_empty());
        assert_eq!(first.written.len(), 2); // root + projects/

        let root_md = s.read("CLAUDE.md");
        assert!(root_md.contains("- **projects/** — 2 files · [index](projects/CLAUDE.md)"));
        assert!(root_md.contains("- `inbox.md` — Inbox"));

        let area_md = s.read("projects/CLAUDE.md");
        assert!(area_md.contains("- `a.md` — Project A"));
        assert!(area_md.contains("- `b.md`\n"));

        let second = sync(&s.config).unwrap();
        assert!(second.written.is_empty());
        assert_eq!(second.unchanged, 2);
        assert_eq!(s.read("CLAUDE.md"), root_md);
    }

    #[test]
    fn sync_preserves_hand_written_content_outside_the_block() {
        let s = Scratch::new(&[("note.md", "# Note\n")]);
        let root_path = s.ws.join("CLAUDE.md");
        fs::write(&root_path, "# My workspace\n\nImportant hand notes.\n").unwrap();

        sync(&s.config).unwrap();
        let result = fs::read_to_string(&root_path).unwrap();
        assert!(result.starts_with("# My workspace\n\nImportant hand notes.\n"));
        assert!(result.contains(router::ROUTER_START));
        assert!(result.contains("- `note.md` — Note"));
    }

    #[test]
    fn status_clears_after_sync_even_when_the_block_is_byte_identical() {
        let s = Scratch::new(&[("a.md", "# A\n")]);
        assert!(status(&s.config).unwrap().stale, "never synced -> stale");

        sync(&s.config).unwrap();
        assert!(!status(&s.config).unwrap().stale);

        // Edit the *body* only — the rendered block is unchanged, so the old
        // "root CLAUDE.md mtime" design would have stayed stale forever.
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(s.ws.join("a.md"), "# A\n\nnew body text\n").unwrap();
        assert!(status(&s.config).unwrap().stale, "edited file -> stale");

        let report = sync(&s.config).unwrap();
        assert!(report.written.is_empty(), "block content did not change");
        assert!(
            !status(&s.config).unwrap().stale,
            "a no-op sync still stamps the marker and clears stale"
        );
    }

    #[test]
    fn sync_refuses_a_home_or_root_workspace() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = home::home_dir().unwrap();
        let config = Config {
            workspace_root: home,
            ..Config::default()
        };
        assert!(matches!(
            sync(&config),
            Err(AxiomataError::UnsafeWorkspaceRoot { .. })
        ));
    }

    #[test]
    fn a_symlinked_claude_md_is_reported_as_failed_not_followed() {
        let s = Scratch::new(&[("a.md", "# A\n")]);
        let outside = s.home.join("victim.txt");
        fs::write(&outside, "do not touch\n").unwrap();
        std::os::unix::fs::symlink(&outside, s.ws.join("CLAUDE.md")).unwrap();

        let report = sync(&s.config).unwrap();
        assert!(
            report.failed.iter().any(|(p, _)| p.ends_with("CLAUDE.md")),
            "the symlinked root CLAUDE.md should be a failure, not a write"
        );
        assert_eq!(fs::read_to_string(&outside).unwrap(), "do not touch\n");
    }
}
