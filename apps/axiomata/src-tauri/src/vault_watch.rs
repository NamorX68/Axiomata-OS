//! Tells the frontend which notes of the vault just changed (`docs/plans/ui-ux-round.md` G9): one recursive,
//! debounced watch over the whole workspace, for the Orbit's agent sparks and its live ember ageing. It reports a
//! file and when, never who wrote it — the frontend pairs a change with the skill or agent at work at that moment,
//! and lets it come from the hub when there is none.
//!
//! Bursts are folded: events are collected until the vault has been quiet for [`DEBOUNCE`], then the changed notes
//! go out as one batch of at most [`MAX_BATCH`] (a sync touching hundreds of files is one wave, not hundreds of
//! sparks). Hidden folders (`.git`, `.obsidian`, `.trash`) and editor scratch files are not the owner's notes and are
//! left out. The workspace root is read once at start; a changed root takes effect with the next start, like every
//! other workspace change.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axiomata_core::AxiomataCore;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Notes of the vault changed.
pub const VAULT_CHANGED: &str = "vault:changed";

/// Quiet time after the last event before a burst is reported.
const DEBOUNCE: Duration = Duration::from_millis(400);

/// The most notes one batch names; the rest of a large burst is counted, not listed.
const MAX_BATCH: usize = 50;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VaultChanged {
    /// Workspace-relative paths, forward slashes, as the graph names its files.
    files: Vec<String>,
    /// How many more changed in the same burst than `files` lists.
    more: usize,
    /// When the burst settled, in ms since the epoch (what the frontend's `Date` takes).
    at: u64,
}

/// Starts the watch on its own thread. It lives as long as the app does; a vault that cannot be watched is logged
/// and left alone.
pub fn start(app: &AppHandle) {
    let root = app
        .state::<AxiomataCore>()
        .config_read()
        .workspace_root
        .clone();
    let app = app.clone();
    let spawned = thread::Builder::new()
        .name("vault-watch".into())
        .spawn(move || run(&app, &root));
    if let Err(err) = spawned {
        tracing::warn!(%err, "could not start the vault watch");
    }
}

fn run(app: &AppHandle, root: &Path) {
    let (tx, rx) = mpsc::channel();
    let mut watcher = match RecommendedWatcher::new(tx, notify::Config::default()) {
        Ok(w) => w,
        Err(err) => {
            tracing::warn!(%err, "no vault watch: the watcher could not be made");
            return;
        }
    };
    if let Err(err) = watcher.watch(root, RecursiveMode::Recursive) {
        tracing::warn!(%err, root = %root.display(), "no vault watch: the workspace cannot be watched");
        return;
    }
    let mut pending: BTreeSet<String> = BTreeSet::new();
    let mut last_event = Instant::now();
    loop {
        match rx.recv_timeout(DEBOUNCE) {
            Ok(Ok(event)) => {
                if !event.kind.is_access() {
                    pending.extend(event.paths.iter().filter_map(|p| note_of(root, p)));
                    last_event = Instant::now();
                }
            }
            Ok(Err(err)) => tracing::debug!(%err, "vault watch event error"),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if !pending.is_empty() && last_event.elapsed() >= DEBOUNCE {
            let all: Vec<String> = std::mem::take(&mut pending).into_iter().collect();
            let more = all.len().saturating_sub(MAX_BATCH);
            let payload = VaultChanged {
                files: all.into_iter().take(MAX_BATCH).collect(),
                more,
                at: now_ms(),
            };
            if let Err(err) = app.emit(VAULT_CHANGED, &payload) {
                tracing::warn!(%err, "could not tell the frontend about changed notes");
            }
        }
    }
}

/// Now, in ms since the epoch; 0 for a clock set before 1970.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The workspace-relative path of a changed file, if it is one of the owner's notes: inside the root, in no hidden
/// folder, and no editor's scratch file.
fn note_of(root: &Path, path: &Path) -> Option<String> {
    let rel: PathBuf = path.strip_prefix(root).ok()?.to_path_buf();
    let mut parts = Vec::new();
    for part in rel.components() {
        let name = part.as_os_str().to_str()?;
        if name.starts_with('.') {
            return None;
        }
        parts.push(name);
    }
    let file = *parts.last()?;
    let scratch = file.ends_with('~') || file.ends_with(".swp") || file.ends_with(".tmp");
    (!scratch).then(|| parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_a_note_relative_to_the_root() {
        let root = Path::new("/vault");
        assert_eq!(
            note_of(root, Path::new("/vault/Work/Plan.md")),
            Some("Work/Plan.md".into())
        );
        assert_eq!(
            note_of(root, Path::new("/vault/README.md")),
            Some("README.md".into())
        );
    }

    #[test]
    fn leaves_out_hidden_folders_scratch_files_and_paths_outside() {
        let root = Path::new("/vault");
        assert_eq!(
            note_of(root, Path::new("/vault/.obsidian/workspace.json")),
            None
        );
        assert_eq!(note_of(root, Path::new("/vault/Work/.git/HEAD")), None);
        assert_eq!(note_of(root, Path::new("/vault/Work/Plan.md~")), None);
        assert_eq!(note_of(root, Path::new("/vault/Work/.Plan.md.swp")), None);
        assert_eq!(note_of(root, Path::new("/elsewhere/Plan.md")), None);
        assert_eq!(note_of(root, Path::new("/vault")), None);
    }
}
