//! Tauri glue for the Terminal module: a small session registry plus the
//! four IPC commands the frontend calls (`terminal_spawn`/`_write`/
//! `_resize`/`_close`). All PTY/ANSI/screen logic lives in the standalone
//! `axiomata-terminal` crate (`PtySession` for the shell process,
//! `Terminal` for interpreting its output) — this file only translates
//! between Tauri IPC and those two types, the same translation-only role
//! `commands.rs` plays for `axiomata-core` (see this crate's own doc
//! comment).
//!
//! Checkpoint 2 of `docs/plans/terminal.md`: each session now owns a
//! `Terminal` alongside its `PtySession`, so what reaches the frontend is
//! an interpreted screen snapshot (plain text per row, cursor position),
//! not raw bytes — see `TerminalEvent`.

use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use axiomata_terminal::{PtySession, Terminal};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

/// One message on a session's `on_output` channel. A plain `Channel<Vec<u8>>`
/// (Checkpoint 1's first cut) couldn't tell the frontend "the shell exited"
/// apart from "nothing has arrived yet" — the reader thread just stopped
/// forwarding bytes, so the tile looked frozen rather than closed, and the
/// *next* keystroke's `terminal_write` against the now-dead session turned
/// an entirely ordinary `exit` into a hard error (architecture review,
/// Checkpoint 1). `Exited` is that missing signal.
///
/// `Screen` replaces Checkpoint 1's raw `Data { bytes }` now that there's an
/// actual screen model (`axiomata_terminal::Terminal`) to read instead of
/// forwarding bytes untouched: `lines` is the interpreted plain text per
/// row (no colour/attributes yet — the frontend just replaces its `<pre>`
/// content with it), `cursor_row`/`cursor_col` the current cursor position
/// (unused by the frontend until Checkpoint 3 draws one).
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalEvent {
    Screen {
        lines: Vec<String>,
        cursor_row: u16,
        cursor_col: u16,
    },
    /// The shell process ended (on its own, or the PTY tore down) — not an
    /// error, and not something `terminal_write`/`_resize` should still be
    /// called against. Sent exactly once, after the registry entry for this
    /// session has already been removed.
    Exited,
}

/// One running session: the PTY/shell process, and the screen model that
/// interprets its output. Kept together so a single registry lookup (and a
/// single `Mutex` lock) reaches both — `terminal_resize` in particular needs
/// to resize each in step.
struct Session {
    pty: PtySession,
    terminal: Terminal,
}

/// Tauri-managed registry of running terminal sessions, keyed by the id
/// `terminal_spawn` mints. One entry per placed Terminal tile — Checkpoint
/// 0's lifecycle decision (`PtySession`'s own doc comment) carries over
/// unchanged: no docking, `terminal_close` (or this registry simply being
/// dropped at app exit) kills the shell immediately.
#[derive(Default)]
pub struct TerminalSessions {
    sessions: Mutex<HashMap<String, Session>>,
}

impl TerminalSessions {
    /// Locks the registry, recovering the guard if the mutex was poisoned by
    /// a panic in another thread rather than propagating the panic — same
    /// reasoning as `AxiomataCore::db_lock`: a `HashMap` panic mid-mutation
    /// can't leave it memory-unsafe, only possibly short one insert, and for
    /// a personal desktop app degrading one terminal beats taking the whole
    /// process down.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Session>> {
        self.sessions
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

/// Monotonic counter for session ids ("term-1", "term-2", …). Plain and
/// dependency-free (no `uuid` crate) since these ids are process-local IPC
/// handles the frontend just echoes back on `_write`/`_resize`/`_close` —
/// never persisted, never shown to the user.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next_session_id() -> String {
    format!("term-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

/// A session's current screen as a `TerminalEvent::Screen`.
fn screen_event(terminal: &Terminal) -> TerminalEvent {
    let screen = terminal.screen();
    let (cursor_row, cursor_col) = screen.cursor();
    TerminalEvent::Screen {
        lines: screen.to_lines(),
        cursor_row,
        cursor_col,
    }
}

/// Spawns a new shell session in a `rows`x`cols` PTY and streams interpreted
/// screen snapshots to `on_output` (no raw bytes — see `TerminalEvent`).
/// Returns the new session's id, passed back into every other command
/// below.
///
/// The read loop runs on its own thread — `PtySession::try_clone_reader`'s
/// reader blocks, so this is the same threading choice `term-poc` already
/// made in Checkpoint 0, just streaming to a Tauri `Channel` instead of this
/// process's own stdout. It needs no reference to `terminal_close`: it ends
/// itself once the shell exits (`read` returns `Ok(0)`) or errors (the PTY
/// tore down) — either way it removes this session from the registry and
/// sends one `TerminalEvent::Exited` before returning, so a session that
/// died on its own can't linger as a zombie entry (nothing else would ever
/// prune it) or silently swallow the frontend's next write. If
/// `on_output.send` itself fails (the frontend/webview is already gone),
/// the loop still ends the same way, just with no one left to hear it.
///
/// Takes `app: AppHandle` (not just `State`) so the read-loop thread —
/// which outlives this call — can re-fetch the registry via
/// `app.state::<TerminalSessions>()` when it's done, rather than needing a
/// `'static` borrow this function has no way to hand it.
#[tauri::command]
pub fn terminal_spawn(
    app: AppHandle,
    sessions: State<'_, TerminalSessions>,
    rows: u16,
    cols: u16,
    on_output: Channel<TerminalEvent>,
) -> Result<String, String> {
    let pty = PtySession::spawn(rows, cols).map_err(|err| err.to_string())?;
    let mut reader = pty.try_clone_reader().map_err(|err| err.to_string())?;
    let terminal = Terminal::new(rows, cols);

    let id = next_session_id();
    sessions
        .lock()
        .insert(id.clone(), Session { pty, terminal });

    let closing_id = id.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            // Feeding the bytes and reading the resulting snapshot both
            // need the same session, so this holds the registry lock for
            // the whole step — brief (in-memory grid updates only, no I/O
            // under the lock) and not contended by anything long-running.
            let sessions = app.state::<TerminalSessions>();
            let mut guard = sessions.lock();
            let Some(session) = guard.get_mut(&closing_id) else {
                break;
            };
            session.terminal.feed(&buf[..n]);
            let event = screen_event(&session.terminal);
            drop(guard);
            if on_output.send(event).is_err() {
                break;
            }
        }
        app.state::<TerminalSessions>().lock().remove(&closing_id);
        let _ = on_output.send(TerminalEvent::Exited);
    });

    Ok(id)
}

/// Writes input bytes (keystrokes, already encoded) to a session's shell.
/// Unlike `terminal_close`, an unknown `id` here *is* an error: there's no
/// live shell to write to, so silently no-op-ing would just hide a stuck
/// spawn or a session the frontend forgot it already closed.
#[tauri::command]
pub fn terminal_write(
    sessions: State<'_, TerminalSessions>,
    id: String,
    data: Vec<u8>,
) -> Result<(), String> {
    let mut guard = sessions.lock();
    let session = guard
        .get_mut(&id)
        .ok_or_else(|| format!("no terminal session {id:?}"))?;
    session.pty.write(&data).map_err(|err| err.to_string())
}

/// Resizes a session's PTY (tile resize -> `SIGWINCH` for the shell) and its
/// screen model together, so the two stay in agreement about how big the
/// terminal is. Same error convention as `terminal_write`: an unknown `id`
/// is a real error, not a silent no-op.
#[tauri::command]
pub fn terminal_resize(
    sessions: State<'_, TerminalSessions>,
    id: String,
    rows: u16,
    cols: u16,
) -> Result<(), String> {
    let mut guard = sessions.lock();
    let session = guard
        .get_mut(&id)
        .ok_or_else(|| format!("no terminal session {id:?}"))?;
    session
        .pty
        .resize(rows, cols)
        .map_err(|err| err.to_string())?;
    session.terminal.resize(rows, cols);
    Ok(())
}

/// Ends a session: drops its `PtySession` (whose own `Drop` kills and reaps
/// the shell process) and removes it from the registry. Closing an id that
/// isn't there (already closed, or spawn never succeeded) is not an error —
/// the frontend calls this unconditionally from `onDestroy`.
#[tauri::command]
pub fn terminal_close(sessions: State<'_, TerminalSessions>, id: String) -> Result<(), String> {
    sessions.lock().remove(&id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    /// `next_session_id` is the only pure logic this file owns outside the
    /// `#[tauri::command]` bodies themselves — a plain, deterministic
    /// counter, so it's tested directly rather than through a spawned
    /// session.
    #[test]
    fn next_session_id_yields_distinct_increasing_term_ids() {
        let first = next_session_id();
        let second = next_session_id();

        assert_ne!(first, second, "two calls must never mint the same id");
        assert!(first.starts_with("term-"), "got {first:?}");
        assert!(second.starts_with("term-"), "got {second:?}");

        let first_n: u64 = first.strip_prefix("term-").unwrap().parse().unwrap();
        let second_n: u64 = second.strip_prefix("term-").unwrap().parse().unwrap();
        assert!(
            second_n > first_n,
            "ids should be monotonically increasing, got {first_n} then {second_n}"
        );
    }

    /// A freshly constructed registry has no sessions — the state
    /// `terminal_write`/`terminal_resize`/`terminal_close` would see for any
    /// id before `terminal_spawn` has ever run.
    #[test]
    fn fresh_registry_is_empty() {
        let sessions = TerminalSessions::default();
        assert!(sessions.lock().is_empty());
    }

    /// Inserting a session under an id makes it visible to a lookup, and
    /// removing it (what `terminal_close` does) leaves the registry empty
    /// again. This exercises the two `HashMap` operations the command
    /// bodies are built on directly, since the commands themselves take a
    /// `tauri::State<'_, TerminalSessions>` that has no public constructor
    /// outside a real, running Tauri app (see the file-level review note
    /// reported alongside these tests).
    #[test]
    fn insert_find_then_remove_round_trips_a_session() {
        let sessions = TerminalSessions::default();
        let pty = PtySession::spawn(24, 80).expect("failed to spawn pty session for test");
        let terminal = Terminal::new(24, 80);

        sessions
            .lock()
            .insert("term-test".to_string(), Session { pty, terminal });
        assert!(sessions.lock().contains_key("term-test"));

        let removed = sessions.lock().remove("term-test");
        assert!(
            removed.is_some(),
            "the just-inserted session should be found and removed"
        );
        assert!(sessions.lock().is_empty());
    }

    /// Removing an id that was never inserted (already closed, or a spawn
    /// that never succeeded) is a no-op, not a panic — the behavior
    /// `terminal_close`'s doc comment promises for exactly this case.
    #[test]
    fn removing_an_unknown_id_is_a_harmless_no_op() {
        let sessions = TerminalSessions::default();
        assert!(sessions.lock().remove("does-not-exist").is_none());
    }

    /// `lock()` must hand back a usable guard even after the mutex was
    /// poisoned by a panic in another thread — the whole reason it exists
    /// instead of a bare `.lock().unwrap()` at each call site (see its own
    /// doc comment).
    #[test]
    fn lock_recovers_after_the_mutex_is_poisoned() {
        let sessions = Arc::new(TerminalSessions::default());
        let poisoning = Arc::clone(&sessions);

        // Panic while holding the lock, on another thread, to poison the
        // mutex the same way an unrelated bug in a command body could.
        let result = thread::spawn(move || {
            let _guard = poisoning.lock();
            panic!("simulated panic while holding the terminal-session lock");
        })
        .join();
        assert!(result.is_err(), "the spawned thread should have panicked");

        // `lock()` must recover the poisoned guard rather than propagating
        // the panic to this (unrelated) caller.
        assert!(sessions.lock().is_empty());
    }

    /// `screen_event` reads straight off a `Terminal`'s current screen — no
    /// PTY needed, so this is where the plain-text/cursor snapshot logic
    /// itself is checked, rather than only exercising it indirectly through
    /// a spawned shell.
    #[test]
    fn screen_event_reflects_fed_bytes_and_cursor_position() {
        let mut terminal = Terminal::new(2, 5);
        terminal.feed(b"hi");

        let TerminalEvent::Screen {
            lines,
            cursor_row,
            cursor_col,
        } = screen_event(&terminal)
        else {
            panic!("expected a Screen event");
        };
        assert_eq!(lines, vec!["hi   ".to_string(), "     ".to_string()]);
        assert_eq!((cursor_row, cursor_col), (0, 2));
    }
}
