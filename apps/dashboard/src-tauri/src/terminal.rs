//! Tauri glue for the Terminal module: a small session registry plus the
//! four IPC commands the frontend calls (`terminal_spawn`/`_write`/
//! `_resize`/`_close`). All PTY/ANSI/screen logic lives in the standalone
//! `axiomata-terminal` crate (`PtySession` for the shell process,
//! `Terminal` for interpreting its output) — this file only translates
//! between Tauri IPC and those two types, the same translation-only role
//! `commands.rs` plays for `axiomata-core` (see this crate's own doc
//! comment).
//!
//! Since Checkpoint 2, each session owns a `Terminal` alongside its
//! `PtySession`, so what reaches the frontend is an interpreted screen
//! snapshot, not raw bytes — see `TerminalEvent`. Checkpoint 3 upgraded that
//! snapshot from plain text to full `Cell` data (colour/attributes), now
//! that the frontend has a canvas renderer able to draw it. Checkpoint 4
//! added `bracketed_paste` to the same snapshot (so the frontend knows
//! whether to wrap pasted text) and a `terminal_scrollback` command for
//! fetching history on demand — the live `on_output` channel only ever
//! streams the *current* screen, scrollback is a separate, deliberately
//! pull-based fetch (see that command's own doc comment for why).

use std::collections::HashMap;
use std::io::Read;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use axiomata_terminal::{Cell, PtySession, Terminal};
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
/// forwarding bytes untouched. Checkpoint 2 sent plain text per row;
/// Checkpoint 3's canvas renderer needs to actually draw colour/attributes,
/// so `rows` is now full `Cell` data (`axiomata_terminal::Cell` derives
/// `Serialize` itself — no parallel DTO here) and `cursor_row`/`cursor_col`
/// are drawn as a blinking cursor instead of sitting unused. `bracketed_paste`
/// (Checkpoint 4) is `Screen::bracketed_paste()`'s value at snapshot time —
/// the frontend's own paste handler reads it to decide whether to wrap
/// pasted text in `\x1b[200~...\x1b[201~` before sending it.
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalEvent {
    Screen {
        rows: Vec<Vec<Cell>>,
        cursor_row: u16,
        cursor_col: u16,
        bracketed_paste: bool,
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
        rows: screen.rows().to_vec(),
        cursor_row,
        cursor_col,
        bracketed_paste: screen.bracketed_paste(),
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
        // Removing the whole `Session` here — not just the `PtySession` —
        // also discards its `Terminal`'s scrollback the instant the shell
        // ends, so `terminal_scrollback` has nothing left to serve once
        // `Exited` fires below. Raised in architecture review and confirmed
        // as the intended behavior (owner decision, 2026-09-14): scrollback
        // dying with the shell is consistent with Checkpoint 0's original
        // "no docking, closed/ended means gone" call, not an oversight.
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

/// Fetches a scrollback viewport on demand: `offset` lines up from the live
/// bottom (see `Screen::visible_rows`'s own doc comment for the exact
/// semantics). Deliberately a separate, pull-based command rather than
/// something `on_output`'s live channel keeps pushing — the owner only
/// needs this while actively scrolled up (a mouse-wheel gesture in the
/// frontend), and streaming a scrollback-aware snapshot on every single PTY
/// read regardless of whether anyone is looking at history would be pure
/// waste. Same "unknown id is an error" convention as `terminal_write`.
#[tauri::command]
pub fn terminal_scrollback(
    sessions: State<'_, TerminalSessions>,
    id: String,
    offset: u16,
) -> Result<Vec<Vec<Cell>>, String> {
    let guard = sessions.lock();
    let session = guard
        .get(&id)
        .ok_or_else(|| format!("no terminal session {id:?}"))?;
    Ok(session.terminal.screen().visible_rows(offset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiomata_terminal::Color;
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
    /// PTY needed, so this is where the cell/cursor snapshot logic itself is
    /// checked, rather than only exercising it indirectly through a spawned
    /// shell.
    #[test]
    fn screen_event_reflects_fed_bytes_and_cursor_position() {
        let mut terminal = Terminal::new(2, 5);
        terminal.feed(b"hi");

        let TerminalEvent::Screen {
            rows,
            cursor_row,
            cursor_col,
            ..
        } = screen_event(&terminal)
        else {
            panic!("expected a Screen event");
        };
        let line_of = |row: &[Cell]| row.iter().map(|c| c.ch).collect::<String>();
        assert_eq!(line_of(&rows[0]), "hi   ");
        assert_eq!(line_of(&rows[1]), "     ");
        assert_eq!((cursor_row, cursor_col), (0, 2));
    }

    /// The predecessor of this test (`screen_event_reflects_fed_bytes_and_cursor_position`,
    /// above) only ever checked `Cell::ch` — it would have passed unchanged
    /// even if `screen_event` still flattened `TerminalEvent::Screen` down to
    /// plain text the way the pre-Checkpoint-3 `lines: Vec<String>` field
    /// did. This checks the actual reason `rows` is now `Vec<Vec<Cell>>`:
    /// colour and attributes reach the frontend's `Cell` data unflattened.
    #[test]
    fn screen_event_rows_carry_cell_color_and_attributes_not_just_the_character() {
        let mut terminal = Terminal::new(1, 2);
        terminal.feed(b"\x1b[1;31mA");

        let TerminalEvent::Screen { rows, .. } = screen_event(&terminal) else {
            panic!("expected a Screen event");
        };

        let hot = rows[0][0];
        assert_eq!(hot.ch, 'A');
        assert_eq!(hot.fg, Color::Indexed { index: 1 });
        assert!(hot.bold);

        // The second cell was never printed to, so it must keep the
        // renderer's default rather than also picking up the bold-red pen.
        let untouched = rows[0][1];
        assert_eq!(untouched.fg, Color::Default);
        assert!(!untouched.bold);
    }

    /// `screen_event`'s `bracketed_paste` field (Checkpoint 4) has to
    /// actually read `Screen::bracketed_paste()`, not just default to
    /// `false` regardless of what the program running in the shell asked
    /// for — the frontend's paste handler trusts this field completely.
    #[test]
    fn screen_event_bracketed_paste_reflects_the_screen_flag() {
        let mut terminal = Terminal::new(1, 5);

        let TerminalEvent::Screen {
            bracketed_paste, ..
        } = screen_event(&terminal)
        else {
            panic!("expected a Screen event");
        };
        assert!(!bracketed_paste, "no program asked for it yet");

        terminal.feed(b"\x1b[?2004h");
        let TerminalEvent::Screen {
            bracketed_paste, ..
        } = screen_event(&terminal)
        else {
            panic!("expected a Screen event");
        };
        assert!(bracketed_paste);
    }

    /// `terminal_scrollback`'s whole body is `session.terminal.screen()
    /// .visible_rows(offset)` behind a lookup that (like every other command
    /// body in this file) needs a real Tauri `State` to exercise directly —
    /// see the review note on `insert_find_then_remove_round_trips_a_session`.
    /// `Screen::visible_rows` itself is already covered end-to-end in
    /// `screen.rs`, but only ever through a bare `Screen`; this mirrors the
    /// command's actual call path — through a real `Terminal`, the type the
    /// command's `Session` actually stores — so a future change to how
    /// `Terminal` exposes its screen wouldn't go unnoticed here.
    #[test]
    fn terminal_scrollback_logic_reads_scrolled_off_lines_through_a_real_terminal() {
        let mut terminal = Terminal::new(1, 4);
        terminal.feed(b"one\r\ntwo\r\nthr");

        let line_of = |row: &[Cell]| row.iter().map(|c| c.ch).collect::<String>();
        // offset 0: exactly what the live on_output snapshot already shows.
        let live = terminal.screen().visible_rows(0);
        assert_eq!(line_of(&live[0]), "thr ");

        // offset 1: one line further back into scrollback than the live view.
        let scrolled = terminal.screen().visible_rows(1);
        assert_eq!(line_of(&scrolled[0]), "two ");
    }
}
