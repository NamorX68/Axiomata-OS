//! Flags the workspace as stale when tracked files change after the last sync.
//!
//! This is a **reactivity optimization only**. [`super::status`] already
//! computes staleness authoritatively by comparing file mtimes; the watcher
//! just lets the GUI flip its indicator the instant a file changes, instead of
//! waiting for the next status poll to re-walk the tree.
//!
//! The watcher never rewrites anything — it only sets an in-memory flag.
//!
//! Implemented in M2.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Watches a workspace directory and raises a flag on any relevant change.
///
/// Keep the value alive for as long as watching is wanted — dropping it stops
/// the watch. If the underlying OS watcher could not be set up, the type still
/// constructs and simply never raises the flag (callers fall back to
/// [`super::status`]); check [`MemoryWatcher::is_active`].
pub struct MemoryWatcher {
    /// `Some` while watching; `None` if `notify` setup failed. Held only to
    /// keep the watch alive.
    _watcher: Option<RecommendedWatcher>,
    changed: Arc<AtomicBool>,
}

impl MemoryWatcher {
    /// Starts watching `workspace_root` recursively. Infallible — see the type
    /// docs for the degraded (`!is_active`) case.
    pub fn start(workspace_root: &Path) -> Self {
        let changed = Arc::new(AtomicBool::new(false));
        let watcher = build_watcher(workspace_root, changed.clone());
        Self {
            _watcher: watcher,
            changed,
        }
    }

    /// Whether the OS-level watch is actually running.
    pub fn is_active(&self) -> bool {
        self._watcher.is_some()
    }

    /// Whether a relevant filesystem change has been observed since the last
    /// [`MemoryWatcher::mark_synced`].
    pub fn observed_change(&self) -> bool {
        self.changed.load(Ordering::Relaxed)
    }

    /// Clears the flag — call right after a successful [`super::sync`].
    pub fn mark_synced(&self) {
        self.changed.store(false, Ordering::Relaxed);
    }
}

/// Builds the `notify` watcher, or `None` if it cannot be created / attached.
fn build_watcher(workspace_root: &Path, changed: Arc<AtomicBool>) -> Option<RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        if matches!(&result, Ok(event) if is_relevant(event)) {
            changed.store(true, Ordering::Relaxed);
        }
    })
    .ok()?;
    watcher
        .watch(workspace_root, RecursiveMode::Recursive)
        .ok()?;
    Some(watcher)
}

/// Whether an event should mark the router stale: a create/modify/remove that
/// touches at least one path which is neither a generated `CLAUDE.md` nor
/// inside a hidden directory (`.git/`, `.claude/`, …).
fn is_relevant(event: &Event) -> bool {
    let interesting_kind = matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    );
    interesting_kind && event.paths.iter().any(|path| !is_ignored_path(path))
}

/// A path that changes to which must not mark the router stale.
fn is_ignored_path(path: &Path) -> bool {
    let is_claude_md = path.file_name().and_then(|n| n.to_str()) == Some("CLAUDE.md");
    let in_hidden_dir = path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|s| s.starts_with('.') && s.len() > 1 && s != "..")
    });
    is_claude_md || in_hidden_dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;
    use std::fs;
    use std::time::{Duration, Instant};

    /// Polls `f` until it is `true` or `timeout` elapses.
    fn wait_until(timeout: Duration, mut f: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if f() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        f()
    }

    #[test]
    fn raises_the_flag_on_a_content_file_change() {
        let root = unique_temp_dir("axiomata-test-watcher");
        fs::create_dir_all(&root).unwrap();
        let watcher = MemoryWatcher::start(&root);
        assert!(watcher.is_active(), "OS watcher should have started");
        assert!(!watcher.observed_change());

        // Give the backend a moment to arm before the first write.
        std::thread::sleep(Duration::from_millis(150));
        fs::write(root.join("note.md"), "hello\n").unwrap();

        assert!(
            wait_until(Duration::from_secs(5), || watcher.observed_change()),
            "a content-file write should raise the flag"
        );

        watcher.mark_synced();
        assert!(!watcher.observed_change());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ignores_generated_claude_md_and_hidden_dirs() {
        let root = unique_temp_dir("axiomata-test-watcher-ignored");
        fs::create_dir_all(root.join(".git")).unwrap();
        let watcher = MemoryWatcher::start(&root);
        std::thread::sleep(Duration::from_millis(150));

        fs::write(root.join("CLAUDE.md"), "generated\n").unwrap();
        fs::write(root.join(".git/HEAD"), "ref\n").unwrap();

        // Should stay down; a full second is plenty for an event to arrive.
        std::thread::sleep(Duration::from_secs(1));
        assert!(
            !watcher.observed_change(),
            "writes to CLAUDE.md / hidden dirs must not raise the flag"
        );

        let _ = fs::remove_dir_all(&root);
    }
}
