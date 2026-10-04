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
//!
//! Checkpoint 6 changed *when* a snapshot goes out, not its shape: through
//! CP5, every session's reader thread sent one full `screen_event` per raw
//! PTY `read()` (up to 4096 bytes), completely unthrottled — a full-screen
//! repaint from `bpytop`/`nvim`/`opencode` routinely spans many `read()`s in
//! a couple of milliseconds, each one its own IPC round trip, and a
//! genuinely mid-repaint, incomplete grid state could reach and get painted
//! by the frontend before the burst's final chunk arrived (owner-reported:
//! visible flicker on full-screen redraws, and non-smooth scrolling inside
//! `nvim`, whose alt-screen redraws go over this exact channel). CP6 splits
//! "feed the parser" (still the reader thread, on every `read()`) from
//! "build and send a snapshot" (now [`spawn_flush_ticker`], one shared
//! thread per app process, running at `FLUSH_INTERVAL`) via a per-session
//! `dirty` flag on [`Session`] — see both items' own doc comments.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once};
use std::thread;
use std::time::Duration;

use axiomata_terminal::{Cell, PtySession, Terminal};
use serde::{Deserialize, Serialize};
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
        /// Whether a BEL (`0x07`) arrived since the *previous* snapshot
        /// (Checkpoint 5b's "Visueller Bell" setting) — `Screen::take_bell()`'s
        /// value at snapshot time, already consumed, so it never repeats
        /// across two consecutive events for the same bell. The frontend
        /// briefly flashes the tile when this is `true`; no audio.
        bell: bool,
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
///
/// `on_output`/`dirty` (CP6) are the coalescing mechanism between the reader
/// thread and [`spawn_flush_ticker`]'s periodic flush: the reader thread
/// only ever sets `dirty = true` after feeding bytes into `terminal`, never
/// building or sending a snapshot itself — see that function's own doc
/// comment for why a single shared ticker replaces the old "one full-grid
/// event per PTY `read()`" behaviour. `on_output` is a `Clone` of the same
/// channel `terminal_spawn`'s caller already holds, so the ticker can send
/// on it without going through the reader thread at all.
struct Session {
    pty: PtySession,
    terminal: Terminal,
    on_output: Channel<TerminalEvent>,
    dirty: bool,
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

/// A session's current screen as a `TerminalEvent::Screen`. Takes `&mut
/// Terminal` (not `&Terminal`, unlike every other read-only accessor in this
/// file) because `bell` reads and clears `Screen::take_bell()` as a side
/// effect — see that method's own doc comment for why it's consume-once
/// rather than a plain getter.
fn screen_event(terminal: &mut Terminal) -> TerminalEvent {
    let bell = terminal.take_bell();
    let screen = terminal.screen();
    let (cursor_row, cursor_col) = screen.cursor();
    TerminalEvent::Screen {
        rows: screen.rows().to_vec(),
        cursor_row,
        cursor_col,
        bracketed_paste: screen.bracketed_paste(),
        bell,
    }
}

/// How often [`spawn_flush_ticker`] checks sessions for unsent output. 8ms
/// is ~125Hz — comfortably above both 60Hz and 120Hz display refresh, so no
/// display ever waits on this being the limiting factor, while still
/// collapsing a burst of PTY `read()`s (a full-screen `bpytop`/`nvim`/
/// `opencode` repaint routinely spans 5-15 of them within a millisecond or
/// two) into a single snapshot instead of one per `read()` (CP6; see
/// `docs/plans/terminal.md`'s CP6 entry for the flicker this fixes: a
/// mid-repaint, genuinely incomplete grid state reaching the frontend and
/// getting painted before the burst's final chunk arrived).
const FLUSH_INTERVAL: Duration = Duration::from_millis(8);

/// Runs forever on its own thread, one per app process (not per session —
/// [`terminal_spawn`] starts this exactly once via `TICKER_STARTED`). Every
/// `FLUSH_INTERVAL`, flushes every session whose `dirty` flag the reader
/// thread has set: builds one `screen_event` snapshot and sends it, instead
/// of the reader thread doing that on every single `read()`. A session whose
/// `on_output.send` fails (its webview/channel is already gone) is removed
/// here rather than left permanently `dirty` — the still-running shell's own
/// reader thread then either sees the removal directly (`guard.get_mut`
/// returns `None` on its next `read()`) or, more commonly, has its blocked
/// `read()` unblocked first by `PtySession`'s `Drop` killing the child
/// process — either way it ends the same way it always did when the
/// frontend disappeared mid-stream.
///
/// Two accepted trade-offs, not addressed here (architecture review, CP6;
/// fine for this app's actual scale — a personal desktop app with a handful
/// of tiles — revisit only if that changes): this thread runs for the rest
/// of the process's life once started, even after every session closes and
/// the registry is empty again; and one sweep holds the registry lock for
/// as long as it takes to clone+serialize *every* dirty session's grid, so
/// several simultaneously busy terminals serialize their snapshot-building
/// against each other and briefly block reader threads/`terminal_write`/
/// `terminal_resize`, where pre-CP6 contention was naturally per-session.
fn spawn_flush_ticker(app: AppHandle) {
    thread::spawn(move || {
        loop {
            thread::sleep(FLUSH_INTERVAL);
            let sessions = app.state::<TerminalSessions>();
            let mut guard = sessions.lock();
            let mut dead = Vec::new();
            for (id, session) in guard.iter_mut() {
                if !session.dirty {
                    continue;
                }
                session.dirty = false;
                let event = screen_event(&mut session.terminal);
                if session.on_output.send(event).is_err() {
                    dead.push(id.clone());
                }
            }
            for id in dead {
                guard.remove(&id);
            }
        }
    });
}

/// Guards [`spawn_flush_ticker`] so it starts exactly once regardless of how
/// many terminal tiles get spawned — one shared ticker serves every session
/// in the registry, not one per session.
static TICKER_STARTED: Once = Once::new();

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
///
/// The per-instance settings `terminal_spawn` accepts, bundled into one
/// struct (Checkpoint 5b, architecture review) rather than four more
/// individual parameters — `shell` (Checkpoint 5) plus `cwd`/`env`/
/// `scrollback_limit` (Checkpoint 5b) already pushed the command past a
/// comfortable flat argument list, and each *is* a genuine per-instance
/// setting (unlike `rows`/`cols`/`on_output`, which are spawn mechanics, not
/// settings — those stay their own parameters). Bundling here means the
/// next settings addition that reaches this far down the stack only grows
/// this struct, not `terminal_spawn`'s own signature.
///
/// All four fields are forwarded to `PtySession::spawn`/`Terminal` without
/// any validation of their own — same "thin glue, translation only" role
/// this whole file plays; the actual behaviour (including `cwd`'s "bad path
/// is a loud `Err`" contract) lives in `axiomata-terminal` itself. `env`
/// arrives as a flat `Vec<(String, String)>` rather than a `HashMap` since
/// the setting's own UI (a `KEY=value`-per-line textarea) parses into an
/// ordered list, and `PtySession::spawn` applies entries in that order.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSpawnOptions {
    /// `None` (the frontend sends this whenever its own `config.shell` is
    /// unset) falls back to `PtySession::spawn`'s own `$SHELL`/`/bin/zsh`
    /// default; a bad path surfaces as this call's own `Err`, same as any
    /// other spawn failure.
    pub shell: Option<String>,
    /// `None` leaves `PtySession::spawn`'s `cwd_override` unset (whatever
    /// `CommandBuilder`'s own default resolves to); `Some` of a path that
    /// isn't an existing directory surfaces as this call's own `Err` rather
    /// than a silent fallback — see `PtySession::spawn`'s own doc comment.
    pub cwd: Option<String>,
    /// `None` is the same as an empty list (no extra environment variables
    /// beyond `TERM=xterm-256color`); forwarded to `PtySession::spawn`'s
    /// `extra_env` in order, applied after `TERM` — see that parameter's own
    /// doc comment for the "later entry wins" ordering.
    pub env: Option<Vec<(String, String)>>,
    /// `None` leaves `Terminal::new`'s own default scrollback cap
    /// (`SCROLLBACK_LIMIT` in `screen.rs`) in place; `Some(n)` overrides it
    /// via `Terminal::with_scrollback_limit`, including `Some(0)` for no
    /// scrollback at all — see that method's own doc comment.
    pub scrollback_limit: Option<usize>,
}

#[tauri::command]
pub fn terminal_spawn(
    app: AppHandle,
    sessions: State<'_, TerminalSessions>,
    rows: u16,
    cols: u16,
    options: TerminalSpawnOptions,
    on_output: Channel<TerminalEvent>,
) -> Result<String, String> {
    let cwd_path = options.cwd.as_ref().map(|c| Path::new(c.as_str()));
    let extra_env = options.env.unwrap_or_default();
    let pty = PtySession::spawn(rows, cols, options.shell.as_deref(), cwd_path, &extra_env)
        .map_err(|err| err.to_string())?;
    let mut reader = pty.try_clone_reader().map_err(|err| err.to_string())?;
    let terminal = match options.scrollback_limit {
        Some(limit) => Terminal::new(rows, cols).with_scrollback_limit(limit),
        None => Terminal::new(rows, cols),
    };

    TICKER_STARTED.call_once(|| spawn_flush_ticker(app.clone()));

    let id = next_session_id();
    sessions.lock().insert(
        id.clone(),
        Session {
            pty,
            terminal,
            on_output: on_output.clone(),
            dirty: false,
        },
    );

    let closing_id = id.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            let n = match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            // CP6: this thread only feeds the parser and marks the session
            // dirty now — it no longer builds or sends a snapshot itself.
            // `spawn_flush_ticker` does that, at most every `FLUSH_INTERVAL`,
            // so a burst of `read()`s from one full-screen repaint collapses
            // into one snapshot instead of one per `read()`.
            let sessions = app.state::<TerminalSessions>();
            let mut guard = sessions.lock();
            let Some(session) = guard.get_mut(&closing_id) else {
                break;
            };
            session.terminal.feed(&buf[..n]);
            session.dirty = true;
        }
        // One forced flush of whatever the ticker hasn't sent yet, before
        // the session disappears — otherwise the frontend's last-seen screen
        // could be a stale, pre-flush state rather than the shell's true
        // final one (e.g. a program's exit-time cleanup redraw arriving in
        // the same `read()` that returns `Ok(0)` next). Holding the lock
        // across this flush and the removal below closes the same race
        // `spawn_flush_ticker` itself is exposed to: it cannot flush a
        // session that's already gone by the time it gets to it.
        let sessions = app.state::<TerminalSessions>();
        let mut guard = sessions.lock();
        if let Some(session) = guard.get_mut(&closing_id)
            && session.dirty
        {
            let event = screen_event(&mut session.terminal);
            let _ = session.on_output.send(event);
        }
        // Removing the whole `Session` here — not just the `PtySession` —
        // also discards its `Terminal`'s scrollback the instant the shell
        // ends, so `terminal_scrollback` has nothing left to serve once
        // `Exited` fires below. Raised in architecture review and confirmed
        // as the intended behavior (owner decision, 2026-09-14): scrollback
        // dying with the shell is consistent with Checkpoint 0's original
        // "no docking, closed/ended means gone" call, not an oversight.
        guard.remove(&closing_id);
        drop(guard);
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
    use tauri::ipc::InvokeResponseBody;

    /// Builds a `Channel<TerminalEvent>` that records every event it's sent
    /// (as parsed JSON, so assertions can read `TerminalEvent`'s fields
    /// without needing `Deserialize` on it — it currently only derives
    /// `Serialize`, and adding `Deserialize` purely for tests isn't worth
    /// the production-code change) instead of forwarding to a real webview.
    /// Used by the CP6 tests below, which have no `AppHandle`/running Tauri
    /// app to drive a real `Channel::send` through.
    fn recording_channel() -> (Channel<TerminalEvent>, Arc<Mutex<Vec<serde_json::Value>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let channel = Channel::new(move |body| {
            let InvokeResponseBody::Json(json) = body else {
                panic!("TerminalEvent always serializes as JSON, not raw bytes");
            };
            let value: serde_json::Value =
                serde_json::from_str(&json).expect("TerminalEvent must serialize to valid JSON");
            sink.lock().unwrap().push(value);
            Ok(())
        });
        (channel, events)
    }

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
        let pty = PtySession::spawn(24, 80, None, None, &[])
            .expect("failed to spawn pty session for test");
        let terminal = Terminal::new(24, 80);

        sessions.lock().insert(
            "term-test".to_string(),
            Session {
                pty,
                terminal,
                on_output: Channel::new(|_| Ok(())),
                dirty: false,
            },
        );
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
        } = screen_event(&mut terminal)
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

        let TerminalEvent::Screen { rows, .. } = screen_event(&mut terminal) else {
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
        } = screen_event(&mut terminal)
        else {
            panic!("expected a Screen event");
        };
        assert!(!bracketed_paste, "no program asked for it yet");

        terminal.feed(b"\x1b[?2004h");
        let TerminalEvent::Screen {
            bracketed_paste, ..
        } = screen_event(&mut terminal)
        else {
            panic!("expected a Screen event");
        };
        assert!(bracketed_paste);
    }

    /// `screen_event`'s `bell` field (Checkpoint 5b) has to actually read
    /// and consume `Terminal::take_bell()` — a stale/stuck `true` would
    /// flash the frontend's overlay on every subsequent snapshot instead of
    /// just once for the one BEL that actually arrived.
    #[test]
    fn screen_event_bell_is_true_once_then_false_again() {
        let mut terminal = Terminal::new(1, 5);

        let TerminalEvent::Screen { bell, .. } = screen_event(&mut terminal) else {
            panic!("expected a Screen event");
        };
        assert!(!bell, "no BEL has arrived yet");

        terminal.feed(b"\x07");
        let TerminalEvent::Screen { bell, .. } = screen_event(&mut terminal) else {
            panic!("expected a Screen event");
        };
        assert!(bell);

        // The same BEL must not show up again in the *next* snapshot.
        let TerminalEvent::Screen { bell, .. } = screen_event(&mut terminal) else {
            panic!("expected a Screen event");
        };
        assert!(!bell);
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

    /// CP6's whole point: several `feed()` calls between two flushes must
    /// collapse into a single sent snapshot, carrying *all* of them, not one
    /// snapshot per `feed()`. `spawn_flush_ticker` and the reader thread's
    /// loop in `terminal_spawn` both need a live `AppHandle`/webview `State`
    /// to run for real (same constraint the file-level review note on
    /// `insert_find_then_remove_round_trips_a_session` already documents for
    /// every command body here), so this drives a real `Session`'s
    /// `dirty`/`terminal`/`on_output` fields directly, replaying the reader
    /// thread's `feed` + `dirty = true` two-liner and the ticker's
    /// `if dirty { ... }` flush verbatim — the exact statements those two
    /// call sites run, just not through the thread/timer machinery around
    /// them.
    #[test]
    fn dirty_flag_coalesces_two_feeds_into_one_flush_and_then_goes_quiet() {
        let pty =
            PtySession::spawn(1, 5, None, None, &[]).expect("failed to spawn pty session for test");
        let (channel, events) = recording_channel();
        let mut session = Session {
            pty,
            terminal: Terminal::new(1, 5),
            on_output: channel,
            dirty: false,
        };
        assert!(!session.dirty, "a fresh session has nothing pending");

        // Two PTY reads' worth of bytes, exactly as the reader thread would
        // feed them one at a time before the ticker ever gets a chance to
        // run in between.
        session.terminal.feed(b"a");
        session.dirty = true;
        session.terminal.feed(b"b");
        session.dirty = true;
        assert!(
            session.dirty,
            "still dirty after the second feed, same flag as after the first"
        );

        // One ticker pass: `spawn_flush_ticker`'s own `if !session.dirty { continue; }`
        // / `session.dirty = false` / `screen_event` / `send` sequence.
        if session.dirty {
            session.dirty = false;
            let event = screen_event(&mut session.terminal);
            session
                .on_output
                .send(event)
                .expect("recording channel never fails");
        }
        assert!(!session.dirty, "the flush must clear the flag");

        let sent = events.lock().unwrap();
        assert_eq!(
            sent.len(),
            1,
            "two feeds between two flushes must produce exactly one Screen event, not two"
        );
        assert_eq!(sent[0]["type"], "screen");
        assert_eq!(sent[0]["rows"][0][0]["ch"], "a");
        assert_eq!(sent[0]["rows"][0][1]["ch"], "b");
        drop(sent);

        // A second ticker pass with no feed in between must stay quiet —
        // nothing changed, so there's nothing to coalesce or send.
        if session.dirty {
            session.dirty = false;
            let event = screen_event(&mut session.terminal);
            session
                .on_output
                .send(event)
                .expect("recording channel never fails");
        }
        assert_eq!(
            events.lock().unwrap().len(),
            1,
            "a flush of a clean session must not send another event"
        );
    }

    /// The reader thread's exit path (in `terminal_spawn`) forces one last
    /// flush of a still-`dirty` session before removing it from the registry
    /// and sending `Exited` — otherwise the frontend's last-seen screen could
    /// be stale by up to `FLUSH_INTERVAL`. This replays that exact sequence
    /// (same reasoning as the coalescing test above for why it's replayed
    /// rather than called: no `AppHandle` to run the real reader thread with)
    /// against a session with pending, unflushed output, and checks both the
    /// event *and* its ordering ahead of `Exited`.
    #[test]
    fn forced_flush_on_exit_sends_the_pending_screen_before_exited_when_dirty() {
        let sessions = TerminalSessions::default();
        let pty =
            PtySession::spawn(1, 5, None, None, &[]).expect("failed to spawn pty session for test");
        let (channel, events) = recording_channel();
        let mut terminal = Terminal::new(1, 5);
        terminal.feed(b"z");
        sessions.lock().insert(
            "term-exit-dirty".to_string(),
            Session {
                pty,
                terminal,
                on_output: channel.clone(),
                dirty: true,
            },
        );

        // `terminal_spawn`'s reader thread, right before it removes the
        // session and sends `Exited`.
        let mut guard = sessions.lock();
        if let Some(session) = guard.get_mut("term-exit-dirty")
            && session.dirty
        {
            let event = screen_event(&mut session.terminal);
            let _ = session.on_output.send(event);
        }
        guard.remove("term-exit-dirty");
        drop(guard);
        let _ = channel.send(TerminalEvent::Exited);

        assert!(
            sessions.lock().is_empty(),
            "the session must be gone once the exit path has run"
        );
        let sent = events.lock().unwrap();
        assert_eq!(
            sent.len(),
            2,
            "a dirty session must get its pending screen flushed, then Exited"
        );
        assert_eq!(
            sent[0]["type"], "screen",
            "the forced flush must go out before Exited"
        );
        assert_eq!(sent[0]["rows"][0][0]["ch"], "z");
        assert_eq!(sent[1]["type"], "exited");
    }

    /// Mirrors the test above, but for a session that has nothing pending
    /// (already flushed by the ticker, or never fed anything) at the moment
    /// the shell exits — the forced flush must be skipped entirely rather
    /// than sending an extra, redundant Screen event ahead of `Exited`.
    #[test]
    fn forced_flush_on_exit_skips_the_extra_screen_event_when_not_dirty() {
        let sessions = TerminalSessions::default();
        let pty =
            PtySession::spawn(1, 5, None, None, &[]).expect("failed to spawn pty session for test");
        let (channel, events) = recording_channel();
        sessions.lock().insert(
            "term-exit-clean".to_string(),
            Session {
                pty,
                terminal: Terminal::new(1, 5),
                on_output: channel.clone(),
                dirty: false,
            },
        );

        let mut guard = sessions.lock();
        if let Some(session) = guard.get_mut("term-exit-clean")
            && session.dirty
        {
            let event = screen_event(&mut session.terminal);
            let _ = session.on_output.send(event);
        }
        guard.remove("term-exit-clean");
        drop(guard);
        let _ = channel.send(TerminalEvent::Exited);

        let sent = events.lock().unwrap();
        assert_eq!(
            sent.len(),
            1,
            "a clean session must only produce the Exited event, no extra Screen event"
        );
        assert_eq!(sent[0]["type"], "exited");
    }
}
