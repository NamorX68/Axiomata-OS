//! The terminal screen model: a cell grid, cursor, current SGR "pen",
//! scrollback, and an alternate screen — the `vte::Perform` implementation
//! that turns parsed ANSI/VT100 events into screen state. This is the "own
//! code" half of the crate's design (see the crate-level doc comment):
//! `vte` only tokenizes escape sequences, everything a parsed event
//! actually *does* to the screen lives here.
//!
//! Checkpoint 4 added scrollback (a capped ring buffer, `SCROLLBACK_LIMIT`
//! lines — the primary screen's own overflow only; the alternate screen
//! never feeds it, matching real terminals), the alternate screen itself
//! (`CSI ?1049h`/`l` — what `vim`/`less`/`htop` use so they can "give the
//! screen back" on exit), and bracketed-paste tracking (`CSI ?2004h`/`l`).
//! Still no saved-cursor (`DECSC`/`DECRC`), scroll regions, or wide-char
//! spacer cells (a width-2 character occupies one cell and simply advances
//! the cursor by 2 — real terminals also blank the following cell, not
//! implemented here). None of that is silently broken so much as not yet
//! asked for by this checkpoint.

use std::collections::VecDeque;

use serde::Serialize;
use unicode_width::UnicodeWidthChar;
use vte::{Params, Perform};

/// Scrollback is capped, not unbounded — a long-running shell (`yes`, a
/// build log tailed for hours) would otherwise grow memory forever. 2000
/// lines is a plain, generous default (a few hundred KB of `Cell`s at
/// typical widths); Checkpoint 5's "config" pass is the place to make this
/// owner-adjustable if it ever needs to be, not this checkpoint's job.
const SCROLLBACK_LIMIT: usize = 2000;

/// A cursor column advances by 8 to the next multiple — the traditional
/// terminal tab stop, and the only one Checkpoint 2 supports (no
/// custom/cleared tab stops).
const TAB_STOP: usize = 8;

/// A cell's foreground/background colour. Palette resolution (which actual
/// RGB an `Indexed` value maps to, e.g. against the active theme) is a
/// rendering concern — Checkpoint 3's canvas renderer (`TerminalScreen.ts`),
/// not this model. `Serialize` (tagged, snake_case — matching the Tauri
/// glue layer's own `TerminalEvent` convention) lets that renderer receive
/// this enum over IPC as-is, with no parallel DTO to keep in sync.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Color {
    /// No SGR colour set (or explicitly reset via 39/49) — the renderer's
    /// own default foreground/background, whatever that ends up being.
    #[default]
    Default,
    /// A 16- or 256-colour palette index (SGR 30-37/90-97/40-47/100-107, or
    /// the extended `38;5;n` / `48;5;n` forms). A struct variant (not a
    /// tuple one) purely so `#[serde(tag = "type")]` — internally-tagged
    /// representation, matching `TerminalEvent`'s own convention — is even
    /// allowed here; serde requires struct or unit variants for that.
    Indexed { index: u8 },
    /// A 24-bit truecolor value (`38;2;r;g;b` / `48;2;r;g;b`).
    Rgb { r: u8, g: u8, b: u8 },
}

/// One character cell: what to draw, and how. `Serialize` for the same
/// reason as [`Color`] — sent to the frontend renderer as-is.
#[derive(Clone, Copy, PartialEq, Debug, Serialize)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub bold: bool,
    pub underline: bool,
    /// Whether this cell is part of an active OSC 8 terminal hyperlink —
    /// Checkpoint 5r, owner-reported (Ghostty screenshot comparison):
    /// Claude Code's own TUI wraps its clickable terms (its own name,
    /// model/plan info, the working-directory path, action hints like
    /// "auto mode on") in real OSC 8 hyperlinks, commonly also wrapped in a
    /// literal SGR 4 (underline) as a plain-text fallback for terminals
    /// with no hyperlink support at all. Ghostty (like most hyperlink-aware
    /// terminals) *suppresses* that fallback underline for its own
    /// hyperlink-hover styling instead — this crate had no OSC 8 handling
    /// at all, so every one of those underline bytes rendered permanently,
    /// looking visibly different from Ghostty. Deliberately just a `bool`
    /// (not the actual URL/id): the only consumer so far is the renderer's
    /// "was this underlined via a hyperlink, and if so don't draw the
    /// underline" decision — no click-to-open support yet, which is the
    /// only reason an actual URL would matter. See [`Screen::active_hyperlink`]
    /// for how this gets set, and why it's tracked independently of `pen`'s
    /// own SGR-reset lifecycle rather than as a `pen`-only attribute.
    pub hyperlink: bool,
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            underline: false,
            hyperlink: false,
        }
    }
}

/// The currently-inactive primary screen's state, kept aside while the
/// alternate screen is showing (`Screen::saved_primary`) so `CSI ?1049l`
/// can hand it back exactly as it was.
struct SavedPrimary {
    grid: Vec<Vec<Cell>>,
    cursor_row: usize,
    cursor_col: usize,
    wrap_pending: bool,
    pen: Cell,
    /// Checkpoint 5r architecture-review finding (CRITICAL): `pen` already
    /// crossed this exact boundary correctly, but `active_hyperlink` didn't
    /// — without this field, a hyperlink open on the primary screen at the
    /// moment `vim`/`htop`/etc. took over would leak onto the alternate
    /// screen's first characters, and a hyperlink the alternate-screen
    /// program itself left open would leak back onto the primary screen on
    /// exit. Same class of "boundary state survives past where it should"
    /// bug `Screen::active_hyperlink`'s own doc comment already warns about
    /// for `CSI 0 m` — just at this boundary instead of that one.
    active_hyperlink: bool,
}

/// The screen: a fixed `rows`x`cols` grid of [`Cell`]s, a cursor position,
/// and the "pen" (the SGR attributes any newly printed character gets).
/// Implements [`vte::Perform`] directly — this struct *is* the terminal
/// state machine the crate's own doc comment promises, not a thin
/// passthrough to something else.
pub struct Screen {
    rows: usize,
    cols: usize,
    /// The *currently active* grid — the primary screen normally, or the
    /// alternate screen while [`Self::in_alt_screen`] is set. Both share
    /// this one field (rather than two permanently-live grids) because only
    /// one is ever drawn from or written to at a time; the other sits idle
    /// in [`Self::saved_primary`] until swapped back.
    grid: Vec<Vec<Cell>>,
    cursor_row: usize,
    cursor_col: usize,
    /// Set when the cursor has just printed the last column and is
    /// "hanging" there rather than having already wrapped — real terminals
    /// defer the actual line feed until the *next* character actually
    /// arrives, rather than wrapping eagerly the instant the last column is
    /// filled. Without this, a line exactly `cols` characters long would
    /// scroll itself away immediately after being printed (found by a
    /// failing test, not by inspection: `erase_in_line_from_cursor_...`
    /// erased a line that eager-wrap had already scrolled off-screen).
    /// Cleared by anything other than `print` — a cursor move, `\r`, `\n`,
    /// etc. all cancel the pending wrap rather than letting it fire later.
    wrap_pending: bool,
    /// Current SGR state for the next printed character. Only the
    /// colour/bold/underline fields are meaningful here — `pen.ch` is never
    /// read; `print` always overwrites it before storing the cell.
    pen: Cell,
    /// Lines the primary screen has scrolled off the top, oldest first,
    /// capped at [`Self::scrollback_limit`]. Never touched while
    /// [`Self::in_alt_screen`] — `vim`/`less`/`htop` redraw their own
    /// screen constantly, and none of that transient churn belongs in a
    /// history a user might later scroll back into.
    scrollback: VecDeque<Vec<Cell>>,
    /// `true` while showing the alternate screen (`CSI ?1049h` was seen and
    /// not yet followed by `?1049l`).
    in_alt_screen: bool,
    /// The primary screen's state, set aside for the duration of
    /// [`Self::in_alt_screen`] — `None` whenever it isn't (the common case).
    saved_primary: Option<SavedPrimary>,
    /// Whether the running program has asked for bracketed paste (`CSI
    /// ?2004h`) — read by the Tauri glue layer so pasted text can be
    /// wrapped in `\x1b[200~...\x1b[201~` only when the program actually
    /// asked for it (see [`Self::bracketed_paste`]'s own doc comment).
    bracketed_paste: bool,
    /// The cap [`Self::scroll_up`] enforces on [`Self::scrollback`].
    /// Defaults to [`SCROLLBACK_LIMIT`] in [`Self::new`]; overridable via
    /// [`Self::with_scrollback_limit`] (Checkpoint 5b's per-instance
    /// scrollback-size setting) without changing `new`'s own signature —
    /// keeps every existing `Screen::new(rows, cols)` call site (most of
    /// this file's own tests included) working unchanged.
    scrollback_limit: usize,
    /// Set by [`Self::execute`] on a BEL byte (`0x07`), read and cleared by
    /// [`Self::take_bell`] — see that method's own doc comment for why this
    /// is a plain flag rather than a counter/queue.
    bell_pending: bool,
    /// Whether an OSC 8 hyperlink is currently open (`ESC ] 8 ; params ;
    /// URI ST` seen with a non-empty `URI`, not yet closed by a matching
    /// empty-URI form) — Checkpoint 5r, applied to every [`Cell`] printed
    /// while it's set (see [`Cell::hyperlink`]'s own doc comment for why).
    /// Deliberately a field on `Screen` itself, *not* on `pen` alongside
    /// `bold`/`underline`: `apply_sgr`'s `CSI 0 m` full reset replaces
    /// `pen` wholesale (`self.pen = Cell::default()`), and a real terminal's
    /// hyperlink boundary is *not* an SGR attribute — an app is free to
    /// change colours (even reset them entirely) partway through a single
    /// hyperlink's link text without ending the link; only a matching
    /// empty-URI OSC 8 does that. Had this lived on `pen` instead, any
    /// mid-hyperlink `CSI 0 m` would have silently (and wrongly) turned the
    /// rest of that same link's text back into a non-hyperlink cell.
    active_hyperlink: bool,
}

impl Screen {
    /// A blank screen of the given size (each dimension floored to 1 — a
    /// 0-sized grid has nowhere for the cursor to live).
    pub fn new(rows: u16, cols: u16) -> Self {
        let rows = (rows as usize).max(1);
        let cols = (cols as usize).max(1);
        Screen {
            rows,
            cols,
            grid: vec![vec![Cell::default(); cols]; rows],
            cursor_row: 0,
            cursor_col: 0,
            wrap_pending: false,
            pen: Cell::default(),
            scrollback: VecDeque::new(),
            in_alt_screen: false,
            saved_primary: None,
            bracketed_paste: false,
            scrollback_limit: SCROLLBACK_LIMIT,
            bell_pending: false,
            active_hyperlink: false,
        }
    }

    /// Overrides the scrollback cap set by [`Self::new`] (Checkpoint 5b's
    /// per-instance scrollback-size setting). A builder rather than a `new`
    /// parameter so every existing `Screen::new(rows, cols)` call site —
    /// this file's own tests included — keeps compiling unchanged.
    ///
    /// `0` is accepted as-is (no scrollback at all, `scroll_up` evicts
    /// immediately) rather than silently substituting the default — an
    /// owner who explicitly sets `0` gets exactly that, not a surprise.
    pub fn with_scrollback_limit(mut self, limit: usize) -> Self {
        self.scrollback_limit = limit;
        self
    }

    /// Builds a `new_rows`x`new_cols` grid from `old`, keeping whatever
    /// existing content still fits in the top-left corner (simplest reflow
    /// policy — a smarter one, e.g. reflowing wrapped lines, isn't asked
    /// for by any checkpoint yet). A plain function of its inputs (no
    /// `&self`) so `exit_alt_screen` can reconcile a screen that was
    /// saved at one size against whatever size the terminal is *now* — the
    /// tile may well have been resized while `vim`/`less`/etc. had the
    /// alternate screen up.
    fn resized_grid(old: &[Vec<Cell>], new_rows: usize, new_cols: usize) -> Vec<Vec<Cell>> {
        let mut grid = vec![vec![Cell::default(); new_cols]; new_rows];
        for (new_row, old_row) in grid.iter_mut().zip(old.iter()) {
            for (new_cell, &old_cell) in new_row.iter_mut().zip(old_row.iter()) {
                *new_cell = old_cell;
            }
        }
        grid
    }

    /// Resizes the active grid (see `resized_grid`) and clamps the
    /// cursor back into bounds. Deliberately does *not* touch
    /// `saved_primary` while the alternate screen is up — resizing an
    /// idle, invisible grid just to resize it again (or not) whenever it's
    /// eventually restored would be wasted work; `exit_alt_screen`
    /// reconciles the size at that point instead.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = (rows as usize).max(1);
        let cols = (cols as usize).max(1);
        self.grid = Self::resized_grid(&self.grid, rows, cols);
        self.rows = rows;
        self.cols = cols;
        self.cursor_row = self.cursor_row.min(rows - 1);
        self.cursor_col = self.cursor_col.min(cols - 1);
        self.wrap_pending = false;
    }

    /// `(rows, cols)`.
    pub fn size(&self) -> (u16, u16) {
        (self.rows as u16, self.cols as u16)
    }

    /// `(row, col)`, both 0-indexed.
    pub fn cursor(&self) -> (u16, u16) {
        (self.cursor_row as u16, self.cursor_col as u16)
    }

    /// The cell at `(row, col)` (0-indexed). Panics on an out-of-bounds
    /// index, like a direct slice index would — callers (e.g. Checkpoint 3's
    /// canvas renderer) are expected to iterate within [`Self::size`]'s
    /// bounds, not probe it.
    pub fn cell(&self, row: u16, col: u16) -> Cell {
        self.grid[row as usize][col as usize]
    }

    /// One row's characters as plain text (no colour/attribute info —
    /// that's for a caller iterating `cell()` directly, e.g. Checkpoint 3's
    /// canvas renderer). Trailing blank cells are kept as spaces rather
    /// than trimmed, matching what's actually in the grid.
    pub fn line_text(&self, row: u16) -> String {
        self.grid[row as usize].iter().map(|c| c.ch).collect()
    }

    /// Every row's plain text, top to bottom — the whole screen as a
    /// caller-ready snapshot (used as-is for Checkpoint 2's interim
    /// `<pre>`-text rendering; Checkpoint 3's canvas renderer will read
    /// `cell()` directly instead once it can draw colour).
    pub fn to_lines(&self) -> Vec<String> {
        (0..self.rows as u16).map(|r| self.line_text(r)).collect()
    }

    /// Every row's full cell data (character, colours, attributes) — what
    /// Checkpoint 3's canvas renderer actually draws from, unlike
    /// [`Self::to_lines`]'s plain-text view (kept for whatever still wants
    /// it, e.g. a future copy/paste feature, but no longer what the Tauri
    /// glue layer sends over IPC once there's colour to show).
    pub fn rows(&self) -> &[Vec<Cell>] {
        &self.grid
    }

    /// How many lines are in scrollback right now (0..=`scrollback_limit`,
    /// `SCROLLBACK_LIMIT` by default — see [`Self::with_scrollback_limit`]).
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// `true` while `vim`/`less`/`htop`/etc.'s alternate screen is up
    /// (`CSI ?1049h` seen, no matching `?1049l` yet).
    pub fn in_alt_screen(&self) -> bool {
        self.in_alt_screen
    }

    /// Whether the running program has asked for bracketed paste (`CSI
    /// ?2004h`, not yet followed by `?2004l`) — checked by the Tauri glue
    /// layer before wrapping pasted text in `\x1b[200~...\x1b[201~`.
    /// Un-bracketed paste into a program that *did* ask for it can trigger
    /// unwanted side effects (auto-indent in an editor mistaking pasted
    /// text for individually typed keystrokes); wrapping it for a program
    /// that *didn't* ask makes the literal marker bytes show up as text —
    /// this flag is what lets the caller pick correctly either way.
    pub fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    /// Reads and clears whether a BEL (`0x07`) has arrived since the last
    /// call — Checkpoint 5b's "Visueller Bell" setting. A plain flag, not a
    /// counter or queue: the Tauri glue layer calls this once per `feed()`
    /// (see `terminal.rs`'s `screen_event`) to build each `TerminalEvent::Screen`
    /// snapshot, and the frontend only ever needs "did at least one bell
    /// happen since the last snapshot" to flash its own overlay once — a
    /// program ringing the bell five times between two snapshots doesn't
    /// need the tile to flash five times, it needs to flash once, same as
    /// the debounced-by-nature "redraw the whole grid from the model every
    /// update" approach the rest of this crate already uses. `take_bell`
    /// (not `bell`/`bell_pending()`) is named as a verb specifically to
    /// signal the side effect — an ordinary getter wouldn't be expected to
    /// also reset state, and a caller polling this in a loop without
    /// intending to consume it would silently miss every bell after the
    /// first read otherwise.
    pub fn take_bell(&mut self) -> bool {
        std::mem::take(&mut self.bell_pending)
    }

    /// A `rows`-tall window of content, `scroll_offset` lines up from the
    /// very bottom: `0` is exactly [`Self::rows`]'s own live view;
    /// increasing it looks further back into scrollback. `scroll_offset` is
    /// clamped to [`Self::scrollback_len`] internally, so a caller can pass
    /// an arbitrarily large "scroll all the way up" value without checking
    /// that first. Builds a fresh `Vec` each call (unlike the zero-copy
    /// [`Self::rows`]) since scrollback and the live grid aren't stored
    /// contiguously — fine for how this is actually used (an on-demand
    /// fetch when the owner scrolls, not part of the per-keystroke
    /// snapshot; see `terminal_scrollback` in the Tauri glue layer).
    pub fn visible_rows(&self, scroll_offset: u16) -> Vec<Vec<Cell>> {
        let sb_len = self.scrollback.len();
        let offset = (scroll_offset as usize).min(sb_len);
        let start = sb_len - offset;
        (start..start + self.rows)
            .map(|i| {
                let row = if i < sb_len {
                    &self.scrollback[i]
                } else {
                    &self.grid[i - sb_len]
                };
                Self::normalized_width(row, self.cols)
            })
            .collect()
    }

    /// Truncates or pads `row` to exactly `cols` cells. Scrollback rows keep
    /// whatever width they had when [`Self::scroll_up`] captured them — a
    /// resize only ever touches the *live* grid, not lines already tucked
    /// away in history — so a [`Self::visible_rows`] window straddling the
    /// scrollback/live boundary after a resize could otherwise hand back a
    /// `Vec` mixing old- and new-width rows in the same snapshot (found in
    /// architecture review, not by a failing test this time: nothing
    /// panics, since every consumer already iterates `row.len()` rather
    /// than assuming `self.cols`, but the rendered result would be visibly
    /// ragged for that one fetch). This doesn't reflow wrapped *text* to
    /// the new width — that's a real, separate feature this checkpoint
    /// still doesn't attempt — it only restores the row-length invariant
    /// every other accessor here already guarantees.
    fn normalized_width(row: &[Cell], cols: usize) -> Vec<Cell> {
        let mut out = row.to_vec();
        out.truncate(cols);
        out.resize(cols, Cell::default());
        out
    }

    /// Switches to the alternate screen (`CSI ?1049h`): the primary
    /// screen's grid/cursor/pen are set aside in [`Self::saved_primary`]
    /// and the active grid becomes a fresh blank one. A no-op if already on
    /// the alternate screen — entering twice must not clobber the one real
    /// saved primary with a blank alternate screen's own state.
    fn enter_alt_screen(&mut self) {
        if self.in_alt_screen {
            return;
        }
        let blank = vec![vec![Cell::default(); self.cols]; self.rows];
        self.saved_primary = Some(SavedPrimary {
            grid: std::mem::replace(&mut self.grid, blank),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            wrap_pending: self.wrap_pending,
            pen: self.pen,
            active_hyperlink: self.active_hyperlink,
        });
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.wrap_pending = false;
        // The fresh alternate-screen grid starts blank, same as `pen`
        // conceptually resetting to whatever the alt-screen program sets up
        // itself — a hyperlink open on the primary screen has nothing to do
        // with the alternate screen's own, entirely separate content.
        self.active_hyperlink = false;
        self.in_alt_screen = true;
    }

    /// Switches back to the primary screen (`CSI ?1049l`), restoring
    /// exactly what [`Self::enter_alt_screen`] set aside — reconciled
    /// against the *current* `rows`/`cols` via `resized_grid` in
    /// case the tile was resized while the alternate screen was up (the
    /// saved copy is whatever size it was when saved, not necessarily
    /// today's). A no-op if the primary screen is already showing.
    fn exit_alt_screen(&mut self) {
        let Some(saved) = self.saved_primary.take() else {
            return;
        };
        self.grid = Self::resized_grid(&saved.grid, self.rows, self.cols);
        self.cursor_row = saved.cursor_row.min(self.rows - 1);
        self.cursor_col = saved.cursor_col.min(self.cols - 1);
        self.pen = saved.pen;
        self.active_hyperlink = saved.active_hyperlink;
        self.wrap_pending = saved.wrap_pending;
        self.in_alt_screen = false;
    }

    /// Applies a DEC private mode (`CSI ?<code>h`/`l`) — the two this
    /// checkpoint actually acts on are 1049 (alternate screen) and 2004
    /// (bracketed paste); anything else (cursor visibility `?25`, etc.) is
    /// a silent no-op, same convention as an unrecognized plain CSI action.
    fn set_private_mode(&mut self, code: u16, enabled: bool) {
        match code {
            1049 => {
                if enabled {
                    self.enter_alt_screen();
                } else {
                    self.exit_alt_screen();
                }
            }
            2004 => self.bracketed_paste = enabled,
            _ => {}
        }
    }

    /// Shifts every row up by one, dropping the top line and appending a
    /// blank one at the bottom — the bare minimum "content keeps flowing"
    /// behaviour a shell needs once output exceeds one screen. Not a real
    /// scrollback (the dropped line is gone, not just off-screen) — that's
    /// Checkpoint 4.
    fn scroll_up(&mut self) {
        let dropped = self.grid.remove(0);
        // The alternate screen never feeds scrollback (see `scrollback`'s
        // own doc comment) — its dropped line is just discarded.
        if !self.in_alt_screen {
            self.scrollback.push_back(dropped);
            // A `while` (not `if`) so this stays correct even if a future
            // caller shrinks `scrollback_limit` on a `Screen` that already
            // holds more lines than the new cap allows — not reachable
            // today (the limit is only ever set once, at construction via
            // `with_scrollback_limit`), but cheap to keep robust.
            while self.scrollback.len() > self.scrollback_limit {
                self.scrollback.pop_front();
            }
        }
        self.grid.push(vec![self.blank_cell(); self.cols]);
    }

    /// Moves the cursor down one row, scrolling the screen if it was
    /// already on the last row.
    fn line_feed(&mut self) {
        if self.cursor_row + 1 >= self.rows {
            self.scroll_up();
        } else {
            self.cursor_row += 1;
        }
    }

    /// An erased cell takes the *current* background colour, like a real
    /// terminal (`\x1b[42m\x1b[2J` clears to green) — not always
    /// [`Cell::default`]. Built from `..Cell::default()` (architecture
    /// review, Checkpoint 5r) rather than a full field-by-field literal —
    /// the previous form had already silently drifted once (this method
    /// existed before `hyperlink` did, and needed a manual edit to add
    /// `hyperlink: false` when that field was introduced); every field
    /// *not* explicitly named here now automatically inherits its default,
    /// so a future field addition can't be forgotten at this call site the
    /// way a fully-spelled-out literal would let it be.
    fn blank_cell(&self) -> Cell {
        Cell {
            bg: self.pen.bg,
            ..Cell::default()
        }
    }

    fn clear_range(&mut self, row: usize, from_col: usize, to_col_exclusive: usize) {
        let blank = self.blank_cell();
        for cell in &mut self.grid[row][from_col..to_col_exclusive] {
            *cell = blank;
        }
    }

    /// Implements CSI `J` (erase in display); `mode` follows the standard ED
    /// values (0 = cursor to end, 1 = start to cursor inclusive, 2 = whole
    /// screen, 3 = whole screen *and* scrollback — xterm's extension). An
    /// unrecognized value is a silent no-op — same convention `csi_dispatch`
    /// uses for any other unimplemented sequence.
    fn erase_in_display(&mut self, mode: u16) {
        match mode {
            // Cursor to end of screen.
            0 => {
                self.clear_range(self.cursor_row, self.cursor_col, self.cols);
                for r in (self.cursor_row + 1)..self.rows {
                    self.clear_range(r, 0, self.cols);
                }
            }
            // Start of screen to cursor (inclusive).
            1 => {
                for r in 0..self.cursor_row {
                    self.clear_range(r, 0, self.cols);
                }
                self.clear_range(self.cursor_row, 0, self.cursor_col + 1);
            }
            // Whole screen.
            2 => {
                for r in 0..self.rows {
                    self.clear_range(r, 0, self.cols);
                }
            }
            // Whole screen *and* scrollback (xterm's `CSI 3 J` extension —
            // what `clear` and `tput reset` actually send). Distinguished
            // from `2` above since Checkpoint 4 added a real `scrollback`
            // buffer for this to mean something; before that, `3` behaved
            // identically to `2` because there was nothing else to clear.
            3 => {
                for r in 0..self.rows {
                    self.clear_range(r, 0, self.cols);
                }
                self.scrollback.clear();
            }
            _ => {}
        }
    }

    /// Implements CSI `K` (erase in line); `mode` follows the standard EL
    /// values (0 = cursor to end, 1 = start to cursor inclusive, 2 = whole
    /// line). Same "unrecognized value is a no-op" convention as
    /// [`Self::erase_in_display`].
    fn erase_in_line(&mut self, mode: u16) {
        match mode {
            0 => self.clear_range(self.cursor_row, self.cursor_col, self.cols),
            1 => self.clear_range(self.cursor_row, 0, self.cursor_col + 1),
            2 => self.clear_range(self.cursor_row, 0, self.cols),
            _ => {}
        }
    }

    /// Applies one `m` (SGR) sequence's parameters to the pen. Parameters
    /// are flattened (colon-grouped subparams and semicolon-separated ones
    /// alike) into one sequence and walked with an explicit index so the
    /// extended-colour forms (`38;5;n`, `38;2;r;g;b`) can consume the right
    /// number of trailing values.
    fn apply_sgr(&mut self, params: &Params) {
        let flat: Vec<u16> = params
            .iter()
            .flat_map(|group| group.iter().copied())
            .collect();
        if flat.is_empty() {
            // A bare `CSI m` means `CSI 0 m` — full reset.
            self.pen = Cell::default();
            return;
        }
        let mut i = 0;
        while i < flat.len() {
            match flat[i] {
                0 => self.pen = Cell::default(),
                1 => self.pen.bold = true,
                4 => self.pen.underline = true,
                22 => self.pen.bold = false,
                24 => self.pen.underline = false,
                code @ 30..=37 => {
                    self.pen.fg = Color::Indexed {
                        index: (code - 30) as u8,
                    }
                }
                39 => self.pen.fg = Color::Default,
                code @ 40..=47 => {
                    self.pen.bg = Color::Indexed {
                        index: (code - 40) as u8,
                    }
                }
                49 => self.pen.bg = Color::Default,
                code @ 90..=97 => {
                    self.pen.fg = Color::Indexed {
                        index: (code - 90 + 8) as u8,
                    }
                }
                code @ 100..=107 => {
                    self.pen.bg = Color::Indexed {
                        index: (code - 100 + 8) as u8,
                    }
                }
                code @ (38 | 48) => {
                    let is_fg = code == 38;
                    match flat.get(i + 1) {
                        Some(5) => {
                            if let Some(&idx) = flat.get(i + 2) {
                                let color = Color::Indexed { index: idx as u8 };
                                if is_fg {
                                    self.pen.fg = color;
                                } else {
                                    self.pen.bg = color;
                                }
                            }
                            i += 2;
                        }
                        Some(2) => {
                            if let (Some(&r), Some(&g), Some(&b)) =
                                (flat.get(i + 2), flat.get(i + 3), flat.get(i + 4))
                            {
                                let color = Color::Rgb {
                                    r: r as u8,
                                    g: g as u8,
                                    b: b as u8,
                                };
                                if is_fg {
                                    self.pen.fg = color;
                                } else {
                                    self.pen.bg = color;
                                }
                            }
                            i += 4;
                        }
                        _ => {}
                    }
                }
                // Everything else (italic, blink, reverse, strikethrough,
                // …) is out of Checkpoint 2's "grundlegend" scope.
                _ => {}
            }
            i += 1;
        }
    }

    /// The first parameter's value, with CSI's "0 or omitted means the
    /// default" convention applied — correct both for movement commands
    /// (default 1) and erase commands (default 0, where an explicit `0`
    /// and an omitted parameter are the same value anyway).
    fn first_param_or(params: &Params, default: u16) -> u16 {
        let value = params
            .iter()
            .next()
            .and_then(|group| group.first().copied())
            .unwrap_or(0);
        if value == 0 { default } else { value }
    }

    /// Same convention as [`Self::first_param_or`], for CUP/HVP's row/col pair.
    fn param_pair_or(params: &Params, default: u16) -> (u16, u16) {
        let mut values = params.iter();
        let a = values
            .next()
            .and_then(|group| group.first().copied())
            .unwrap_or(0);
        let b = values
            .next()
            .and_then(|group| group.first().copied())
            .unwrap_or(0);
        (
            if a == 0 { default } else { a },
            if b == 0 { default } else { b },
        )
    }
}

impl Perform for Screen {
    fn print(&mut self, c: char) {
        // Zero-width characters (combining marks, …) aren't modeled in
        // Checkpoint 2 — skipped rather than occupying a cell of their own.
        let width = match c.width() {
            Some(0) | None => return,
            Some(w) => w,
        };
        // A pending wrap from the *previous* character finally happens now,
        // right before this one is placed — see `wrap_pending`'s own doc
        // comment for why this can't just happen eagerly instead.
        if self.wrap_pending {
            self.wrap_pending = false;
            self.cursor_col = 0;
            self.line_feed();
        }
        let mut cell = self.pen;
        cell.ch = c;
        // Not part of `pen` — see `Self::active_hyperlink`'s own doc
        // comment for why it's applied here instead, on every printed
        // cell, rather than living on `pen` and following its SGR-reset
        // lifecycle.
        cell.hyperlink = self.active_hyperlink;
        self.grid[self.cursor_row][self.cursor_col] = cell;
        self.cursor_col += width;
        if self.cursor_col >= self.cols {
            self.cursor_col = self.cols - 1;
            self.wrap_pending = true;
        }
    }

    fn execute(&mut self, byte: u8) {
        self.wrap_pending = false;
        match byte {
            b'\r' => self.cursor_col = 0,
            b'\n' => self.line_feed(),
            0x08 => self.cursor_col = self.cursor_col.saturating_sub(1),
            b'\t' => {
                let next_stop = (self.cursor_col / TAB_STOP + 1) * TAB_STOP;
                self.cursor_col = next_stop.min(self.cols - 1);
            }
            0x07 => self.bell_pending = true, // BEL — see `take_bell`'s own doc comment
            _ => {}                           // other C0/C1 controls have no screen effect here
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        // A leading `?` intermediate byte marks a DEC private-mode sequence
        // (`CSI ?1049h`, `CSI ?2004l`, …) — `h`/`l` mean something entirely
        // different without it (SM/RM, ANSI modes this crate doesn't model)
        // so the marker has to be checked before treating `h`/`l` as one of
        // `set_private_mode`'s codes. Checked *before* the blanket
        // `wrap_pending = false` below (not after it, as an earlier version
        // of this method had it — found by a failing test): `?1049h` has to
        // see the primary screen's *actual* pending-wrap state to save it
        // correctly in `enter_alt_screen`, not one this function already
        // clobbered to `false` on its way there.
        //
        // Any *other* non-empty intermediate byte (`>`, `<`, `=`, …) marks a
        // different xterm/vendor private-sequence family this crate doesn't
        // model at all — e.g. `CSI > 4 m` (xterm's `modifyOtherKeys` set,
        // final byte `m`) or `CSI < u` (Kitty keyboard protocol query).
        // These are NOT SGR just because they happen to share a final byte
        // with one (real SGR, ECMA-48, never carries an intermediate byte).
        // A real live-app bug (owner-reported, terminal text permanently
        // underlined session-wide from the very first character): before
        // this check existed, `CSI > 4 m` fell through to the generic
        // `match action` below, which matched `'m' => apply_sgr(params)`
        // on the final byte alone and misread it as bare SGR code 4 —
        // underline — applied to `pen` at session start, before any content
        // even printed, with nothing after it to ever turn underline back
        // off again. Silently no-op'd here instead, same convention as the
        // unrecognized-sequence fallback at the bottom of this method.
        if let Some(&marker) = intermediates.first() {
            if marker == b'?' {
                match action {
                    'h' => self.set_private_mode(Self::first_param_or(params, 0), true),
                    'l' => self.set_private_mode(Self::first_param_or(params, 0), false),
                    _ => {}
                }
            }
            return;
        }
        self.wrap_pending = false;
        match action {
            'A' => {
                self.cursor_row = self
                    .cursor_row
                    .saturating_sub(Self::first_param_or(params, 1) as usize)
            }
            'B' => {
                self.cursor_row =
                    (self.cursor_row + Self::first_param_or(params, 1) as usize).min(self.rows - 1)
            }
            'C' => {
                self.cursor_col =
                    (self.cursor_col + Self::first_param_or(params, 1) as usize).min(self.cols - 1)
            }
            'D' => {
                self.cursor_col = self
                    .cursor_col
                    .saturating_sub(Self::first_param_or(params, 1) as usize)
            }
            'H' | 'f' => {
                let (row, col) = Self::param_pair_or(params, 1);
                self.cursor_row = (row as usize - 1).min(self.rows - 1);
                self.cursor_col = (col as usize - 1).min(self.cols - 1);
            }
            // CHA/HPA (`CSI n G` / `CSI n` + backtick) — absolute column on
            // the current row, 1-indexed. Owner-reported, live-tested: the
            // Claude Code CLI's own TUI streams a line word-by-word as
            // `<word>CSI colG<next word>` — jumping straight to each word's
            // start column instead of printing literal space characters in
            // between (presumably relying on those cells already being
            // blank) — so every gap this crate silently dropped (the
            // catch-all below) rendered as no space at all: consecutive
            // words glued together with zero whitespace, reproduced by
            // capturing this CLI's raw PTY output and replaying it against
            // this exact `match`. `nvim`/`bpytop`/`opencode` never happened
            // to exercise this particular sequence, which is why only
            // Claude Code's own output looked broken.
            'G' | '`' => {
                self.cursor_col = (Self::first_param_or(params, 1) as usize)
                    .saturating_sub(1)
                    .min(self.cols - 1)
            }
            // VPA (`CSI n d`) — absolute row, same column. Not yet observed
            // from a live app the way `G` above was, but it's `G`'s direct
            // vertical counterpart in the same ECMA-48 family and just as
            // cheap/safe to support properly rather than leave silently
            // dropped for whatever the next app to use it turns out to be.
            'd' => {
                self.cursor_row = (Self::first_param_or(params, 1) as usize)
                    .saturating_sub(1)
                    .min(self.rows - 1)
            }
            'J' => self.erase_in_display(Self::first_param_or(params, 0)),
            'K' => self.erase_in_line(Self::first_param_or(params, 0)),
            'm' => self.apply_sgr(params),
            // Scroll regions and insert/delete line/char are still out of
            // scope (private-mode `h`/`l` — cursor visibility, bracketed
            // paste, alternate screen — is handled above, before this
            // match) — silently no-op rather than corrupting the grid on
            // an unrecognized sequence.
            _ => {}
        }
    }

    /// OSC 8 (`ESC ] 8 ; params ; URI ST`) — terminal hyperlinks, Checkpoint
    /// 5r (see [`Screen::active_hyperlink`]'s own doc comment for the full
    /// story). `vte` splits the OSC payload on `;` into `params`: `params[0]`
    /// is the OSC number itself (`b"8"`), `params[1]` is OSC 8's own
    /// `id=...`/`key=value` parameter list, `params[2]` is the URI —
    /// present and non-empty opens a hyperlink, empty or altogether missing
    /// closes one (the standard `ESC]8;;ST` closing form has an empty, not
    /// missing, third segment, but a missing one is treated the same way
    /// defensively rather than leaving a hyperlink stuck open on a
    /// malformed sequence). Every other OSC number (window title, clipboard,
    /// …) is silently ignored — same "unrecognized is a no-op" convention
    /// `csi_dispatch` already uses, not a gap specific to this method.
    ///
    /// `params[1]` (the `id=...` parameter) and `params[2]` itself (the
    /// actual URI) are both read here only far enough to decide open/closed
    /// — neither is stored on [`Cell`]. Intentional, scoped-down debt, not
    /// an oversight: [`Cell::hyperlink`] is a plain `bool` because the only
    /// consumer today is the renderer's "don't draw the fallback underline"
    /// decision, which needs no more than that. Click-to-open, or grouping
    /// same-link cells for a Ghostty-style hover highlight across the whole
    /// span, would need the URI (and possibly this `id=`) threaded through
    /// [`Cell`]/the wire format for real — a future implementer picking
    /// that up doesn't need to rediscover that both are available here and
    /// simply not kept yet.
    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        if params.first() == Some(&b"8".as_slice()) {
            self.active_hyperlink = params.get(2).is_some_and(|uri| !uri.is_empty());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_plain_text_left_to_right() {
        let mut screen = Screen::new(3, 10);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"hi");
        assert_eq!(screen.line_text(0), "hi        ");
        assert_eq!(screen.cursor(), (0, 2));
    }

    #[test]
    fn carriage_return_and_line_feed_move_the_cursor_independently() {
        let mut screen = Screen::new(3, 10);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"ab\r\ncd");
        assert_eq!(screen.line_text(0), "ab        ");
        assert_eq!(screen.line_text(1), "cd        ");
        assert_eq!(screen.cursor(), (1, 2));
    }

    #[test]
    fn wrapping_a_full_line_defers_until_the_next_character() {
        let mut screen = Screen::new(2, 4);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"abcd");
        // The wrap is pending, not yet visible: the cursor stays "hanging"
        // at the still-full row's last column (see `wrap_pending`'s doc
        // comment) until another character actually arrives — it does not
        // jump to the next row on its own.
        assert_eq!(screen.line_text(0), "abcd");
        assert_eq!(screen.cursor(), (0, 3));

        parser.advance(&mut screen, b"e");
        assert_eq!(screen.line_text(1), "e   ");
        assert_eq!(screen.cursor(), (1, 1));
    }

    #[test]
    fn output_past_the_last_row_scrolls_instead_of_panicking() {
        let mut screen = Screen::new(2, 4);
        let mut parser = vte::Parser::new();
        // `\r\n`, not a bare `\n`: a real PTY's line discipline expands an
        // application's plain `\n` to `\r\n` before this parser ever sees
        // it (`ONLCR`), so that — not "LF also does a CR" — is the
        // realistic byte stream to test against. A bare `\n` genuinely
        // doesn't reset the column, matching real VT100 semantics.
        parser.advance(&mut screen, b"one\r\ntwo\r\nthr");
        // "one" scrolled off the top; only "two" and "thr" remain.
        assert_eq!(screen.line_text(0), "two ");
        assert_eq!(screen.line_text(1), "thr ");
    }

    #[test]
    fn backspace_moves_left_without_erasing() {
        let mut screen = Screen::new(1, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"ab\x08\x08");
        assert_eq!(screen.cursor(), (0, 0));
        assert_eq!(screen.line_text(0), "ab   ");
    }

    #[test]
    fn tab_advances_to_the_next_stop() {
        let mut screen = Screen::new(1, 20);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"a\t");
        assert_eq!(screen.cursor(), (0, 8));
    }

    #[test]
    fn cursor_movement_sequences_reposition_within_bounds() {
        let mut screen = Screen::new(5, 5);
        let mut parser = vte::Parser::new();
        // Down 3, forward 2, then up 1, back 1.
        parser.advance(&mut screen, b"\x1b[3B\x1b[2C\x1b[1A\x1b[1D");
        assert_eq!(screen.cursor(), (2, 1));
        // Clamped, not panicking, when asked to overshoot the grid.
        parser.advance(&mut screen, b"\x1b[100B\x1b[100C");
        assert_eq!(screen.cursor(), (4, 4));
    }

    /// `CSI n G` (CHA/HPA — absolute column, 1-indexed) was silently
    /// dropped by the catch-all in `csi_dispatch` until this test's fix —
    /// reproduced from a real captured PTY transcript of the Claude Code
    /// CLI's own TUI, which streams a line as `<word>` + `CSI colG` +
    /// `<next word>` instead of printing literal space characters between
    /// words (relying on those cells already being blank). Without `G`
    /// moving the cursor, the next word printed immediately after the
    /// previous one with no gap at all — owner-reported as "words run
    /// together with no spaces", specific to this one CLI's output and not
    /// `nvim`/`bpytop`/`opencode`, which never happened to exercise this
    /// sequence.
    #[test]
    fn cursor_horizontal_absolute_g_jumps_to_the_1_indexed_column_leaving_a_real_gap() {
        let mut screen = Screen::new(1, 20);
        let mut parser = vte::Parser::new();
        // Mirrors the captured transcript exactly: "Die" then CHA to column
        // 7 (1-indexed, i.e. index 6) before "Meldung" — a real gap of
        // blank cells must separate the two words, not printed adjacently.
        parser.advance(&mut screen, b"Die\x1b[7GMeldung");
        assert_eq!(screen.line_text(0), "Die   Meldung       ");
        assert_eq!(screen.cursor(), (0, 13));

        // Clamped, not panicking, when asked to overshoot the grid — same
        // convention as every other cursor-movement sequence above.
        parser.advance(&mut screen, b"\x1b[100G");
        assert_eq!(screen.cursor(), (0, 19));
    }

    /// HPA (`` CSI n ` ``) shares `G`'s exact code path in `csi_dispatch` —
    /// this is here so that path has its own direct test under its other
    /// final byte too, not just via `G` above, even though no live app has
    /// been observed sending it yet (`csi_dispatch`'s own doc comment on the
    /// `'G' | '`'` arm explains why they're handled together).
    #[test]
    fn horizontal_position_absolute_backtick_behaves_exactly_like_cha() {
        let mut screen = Screen::new(1, 20);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"Die\x1b[7`Meldung");
        assert_eq!(screen.line_text(0), "Die   Meldung       ");
        assert_eq!(screen.cursor(), (0, 13));
    }

    /// VPA (`CSI n d`) — `G`'s vertical counterpart, same absolute-position-
    /// on-the-current-line-or-column family, added proactively alongside
    /// `G` rather than left silently dropped for whatever the next app to
    /// use it turns out to be.
    #[test]
    fn vertical_position_absolute_d_jumps_to_the_1_indexed_row_leaving_the_column_put() {
        let mut screen = Screen::new(5, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b[3C\x1b[4d");
        assert_eq!(screen.cursor(), (3, 3));

        // Clamped, not panicking, when asked to overshoot the grid.
        parser.advance(&mut screen, b"\x1b[100d");
        assert_eq!(screen.cursor(), (4, 3));
    }

    #[test]
    fn cursor_position_is_1_indexed_and_defaults_to_home() {
        let mut screen = Screen::new(5, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b[3;2H");
        assert_eq!(screen.cursor(), (2, 1));
        parser.advance(&mut screen, b"\x1b[H");
        assert_eq!(screen.cursor(), (0, 0));
    }

    #[test]
    fn erase_in_line_from_cursor_clears_to_end_only() {
        let mut screen = Screen::new(1, 6);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"abcdef\r\x1b[3C\x1b[K");
        assert_eq!(screen.line_text(0), "abc   ");
    }

    #[test]
    fn erase_in_display_whole_screen_clears_every_row() {
        let mut screen = Screen::new(2, 4);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"abcd\r\nefgh\x1b[2J");
        assert_eq!(screen.line_text(0), "    ");
        assert_eq!(screen.line_text(1), "    ");
    }

    #[test]
    fn sgr_16_color_and_bold_are_tracked_on_the_pen_and_reset_by_0() {
        let mut screen = Screen::new(1, 3);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b[1;31mA\x1b[0mB");
        let hot = screen.cell(0, 0);
        assert_eq!(hot.fg, Color::Indexed { index: 1 });
        assert!(hot.bold);
        let plain = screen.cell(0, 1);
        assert_eq!(plain.fg, Color::Default);
        assert!(!plain.bold);
    }

    #[test]
    fn sgr_256_and_truecolor_extended_forms_are_parsed() {
        let mut screen = Screen::new(1, 2);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b[38;5;200mA");
        assert_eq!(screen.cell(0, 0).fg, Color::Indexed { index: 200 });
        parser.advance(&mut screen, b"\x1b[48;2;10;20;30mB");
        assert_eq!(
            screen.cell(0, 1).bg,
            Color::Rgb {
                r: 10,
                g: 20,
                b: 30
            }
        );
    }

    #[test]
    fn erased_cells_take_the_current_background_color() {
        let mut screen = Screen::new(1, 4);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b[42m\x1b[2J");
        assert_eq!(screen.cell(0, 0).bg, Color::Indexed { index: 2 });
    }

    #[test]
    fn resize_grows_the_grid_preserving_existing_content_and_leaving_the_cursor_put() {
        let mut screen = Screen::new(2, 3);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"ab\r\ncd");
        screen.resize(4, 5);
        assert_eq!(screen.size(), (4, 5));
        assert_eq!(screen.line_text(0), "ab   ");
        assert_eq!(screen.line_text(1), "cd   ");
        assert_eq!(screen.line_text(2), "     ");
        assert_eq!(screen.line_text(3), "     ");
        // The cursor was already within the old bounds, so growing the grid
        // doesn't need to move it.
        assert_eq!(screen.cursor(), (1, 2));
    }

    #[test]
    fn resize_shrinking_drops_content_outside_the_new_bounds_and_clamps_the_cursor() {
        let mut screen = Screen::new(3, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"ab\r\ncdef\r\nxy");
        screen.resize(2, 3);
        assert_eq!(screen.size(), (2, 3));
        assert_eq!(screen.line_text(0), "ab ");
        assert_eq!(screen.line_text(1), "cde");
        // The cursor was at (2, 2) in the old 3x5 grid; the new grid only
        // spans rows 0..2 and cols 0..3, so both coordinates clamp down.
        assert_eq!(screen.cursor(), (1, 2));
    }

    #[test]
    fn resize_cancels_a_pending_wrap_so_the_next_character_does_not_jump_a_row() {
        let mut screen = Screen::new(2, 4);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"abcd"); // fills the last column, wrap pending
        assert_eq!(screen.cursor(), (0, 3));

        screen.resize(2, 5);
        parser.advance(&mut screen, b"e");
        // Without `resize` clearing `wrap_pending`, this would land on row 1
        // instead, per the deferred-wrap behavior `wrap_pending`'s own doc
        // comment describes.
        assert_eq!(screen.line_text(0), "abce ");
        assert_eq!(screen.cursor(), (0, 4));
    }

    #[test]
    fn a_1x1_screen_still_accepts_text_without_panicking() {
        let mut screen = Screen::new(1, 1);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"x");
        assert_eq!(screen.line_text(0), "x");
        assert_eq!(screen.cursor(), (0, 0));

        // The single column was already full, so this second character hits
        // the pending-wrap path immediately (line-feed on a 1-row screen
        // scrolls, i.e. rewrites the only row) rather than panicking on an
        // out-of-bounds index.
        parser.advance(&mut screen, b"y");
        assert_eq!(screen.line_text(0), "y");
        assert_eq!(screen.cursor(), (0, 0));
    }

    #[test]
    fn a_zero_sized_request_is_floored_to_one_by_one() {
        let screen = Screen::new(0, 0);
        assert_eq!(screen.size(), (1, 1));
    }

    #[test]
    fn resizing_to_zero_is_also_floored_to_one_by_one() {
        let mut screen = Screen::new(3, 3);
        screen.resize(0, 0);
        assert_eq!(screen.size(), (1, 1));
        assert_eq!(screen.cursor(), (0, 0));
    }

    #[test]
    fn rows_exposes_the_full_grid_with_matching_dimensions_and_cell_data() {
        let mut screen = Screen::new(2, 3);
        let mut parser = vte::Parser::new();
        // Bold red on "AB", left untouched for the rest of row 0, so this
        // also checks that `rows()` isn't just a plain-text view like
        // `line_text`/`to_lines` — colour/attribute data has to survive too.
        parser.advance(&mut screen, b"\x1b[1;31mAB\r\ncd");

        let rows = screen.rows();
        assert_eq!(rows.len(), 2, "rows() must report every row, not a subset");
        assert_eq!(rows[0].len(), 3, "each row must report every column");
        assert_eq!(rows[1].len(), 3);

        // `rows()` reads from the same grid `cell()` does, not a separate
        // snapshot that could drift out of sync with it.
        for row in 0..2u16 {
            for col in 0..3u16 {
                assert_eq!(
                    rows[row as usize][col as usize],
                    screen.cell(row, col),
                    "rows()[{row}][{col}] should match cell({row}, {col})"
                );
            }
        }

        let hot = rows[0][0];
        assert_eq!(hot.ch, 'A');
        assert_eq!(hot.fg, Color::Indexed { index: 1 });
        assert!(hot.bold);
        // The third cell of row 0 was never printed to, so it keeps the
        // grid's original default rather than picking up the still-active
        // bold-red pen.
        assert_eq!(rows[0][2], Cell::default());
    }

    #[test]
    fn unrecognized_escape_sequences_are_ignored_not_printed() {
        // A window-title OSC and a private mode set — neither has a
        // dedicated `csi_dispatch`/`osc_dispatch` case, but `vte` still
        // consumes them as their own tokens, so none of their bytes should
        // leak into the visible text (the whole point of Checkpoint 2).
        let mut screen = Screen::new(1, 20);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b]0;window title\x07hi\x1b[?25l");
        assert_eq!(screen.line_text(0).trim_end(), "hi");
    }

    #[test]
    fn visible_rows_normalizes_scrollback_rows_captured_at_a_different_width() {
        let mut screen = Screen::new(2, 8);
        let mut parser = vte::Parser::new();
        // "one" scrolls off into scrollback at the *original* 8-column
        // width; the grid is then narrowed to 4 columns and a new line
        // printed at that width. A `visible_rows` window spanning both
        // must not return a Vec mixing an 8-wide row with 4-wide ones.
        parser.advance(&mut screen, b"one\r\ntwo\r\n");
        assert_eq!(
            screen.scrollback_len(),
            1,
            "\"one\" should have scrolled off already"
        );
        screen.resize(2, 4);
        parser.advance(&mut screen, b"thr");

        let view = screen.visible_rows(1);
        assert_eq!(view.len(), 2);
        assert_eq!(
            view[0].len(),
            4,
            "the scrollback row must be truncated to the current width"
        );
        assert_eq!(view[0].iter().map(|c| c.ch).collect::<String>(), "one ");
        // Live grid row 0 ("two", printed before the resize) - the window's
        // second row at this offset, not row 1 ("thr", printed after).
        assert_eq!(view[1].len(), 4);
        assert_eq!(view[1].iter().map(|c| c.ch).collect::<String>(), "two ");
    }

    #[test]
    fn visible_rows_pads_a_narrower_scrollback_row_to_the_wider_current_width() {
        let mut screen = Screen::new(1, 3);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"hi\r\n");
        screen.resize(1, 6);

        let view = screen.visible_rows(1);
        assert_eq!(
            view[0].len(),
            6,
            "a narrower scrollback row must be padded to the current width"
        );
        assert_eq!(view[0].iter().map(|c| c.ch).collect::<String>(), "hi    ");
    }

    #[test]
    fn scrolled_off_lines_land_in_scrollback_oldest_first() {
        let mut screen = Screen::new(2, 4);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"one\r\ntwo\r\nthr");
        // "one" scrolled off; it should now be in scrollback, not gone.
        assert_eq!(screen.scrollback_len(), 1);
        assert_eq!(
            screen.visible_rows(1)[0]
                .iter()
                .map(|c| c.ch)
                .collect::<String>(),
            "one "
        );
    }

    #[test]
    fn scrollback_is_capped_at_the_limit_dropping_the_oldest_first() {
        let mut screen = Screen::new(1, 2);
        let mut parser = vte::Parser::new();
        // One line feed per line beyond the first pushes exactly one line
        // into scrollback (a 1-row screen), so this produces
        // SCROLLBACK_LIMIT + 5 scrollback pushes.
        for n in 0..(SCROLLBACK_LIMIT + 5) {
            parser.advance(&mut screen, format!("{:02}\r\n", n % 100).as_bytes());
        }
        assert_eq!(screen.scrollback_len(), SCROLLBACK_LIMIT);
        // The oldest 5 pushes ("00".."04") must have been evicted; the
        // farthest-back line still in scrollback is "05".
        let oldest = screen.visible_rows(SCROLLBACK_LIMIT as u16)[0]
            .iter()
            .map(|c| c.ch)
            .collect::<String>();
        assert_eq!(oldest, "05");
    }

    #[test]
    fn with_scrollback_limit_overrides_the_default_cap() {
        let mut screen = Screen::new(1, 2).with_scrollback_limit(3);
        let mut parser = vte::Parser::new();
        // Same shape as `scrollback_is_capped_at_the_limit_dropping_the_oldest_first`,
        // but against the overridden limit (3) rather than the module
        // default, proving the constructor value actually took effect.
        for n in 0..8 {
            parser.advance(&mut screen, format!("{:02}\r\n", n).as_bytes());
        }
        assert_eq!(screen.scrollback_len(), 3);
        let oldest = screen.visible_rows(3)[0]
            .iter()
            .map(|c| c.ch)
            .collect::<String>();
        assert_eq!(oldest, "05");
    }

    #[test]
    fn visible_rows_at_offset_zero_matches_the_live_grid() {
        let mut screen = Screen::new(2, 4);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"one\r\ntwo\r\nthr");
        assert_eq!(screen.visible_rows(0), screen.rows());
    }

    #[test]
    fn visible_rows_offset_beyond_scrollback_clamps_instead_of_panicking() {
        let mut screen = Screen::new(2, 4);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"one\r\ntwo\r\nthr");
        // Only one line of scrollback exists ("one"); asking for far more
        // must clamp, not panic or return something shorter than `rows`.
        let view = screen.visible_rows(9999);
        assert_eq!(view.len(), 2);
        assert_eq!(view[0].iter().map(|c| c.ch).collect::<String>(), "one ");
    }

    #[test]
    fn alt_screen_hides_and_restores_the_primary_screen_exactly() {
        let mut screen = Screen::new(2, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b[1;31mhello");
        assert!(!screen.in_alt_screen());

        parser.advance(&mut screen, b"\x1b[?1049h");
        assert!(screen.in_alt_screen());
        // The alternate screen starts blank, not a copy of the primary one.
        assert_eq!(screen.line_text(0), "     ");
        assert_eq!(screen.cursor(), (0, 0));

        parser.advance(&mut screen, b"vim stuff");
        parser.advance(&mut screen, b"\x1b[?1049l");
        assert!(!screen.in_alt_screen());
        // The primary screen (text, colour, and cursor position) is back
        // exactly as it was before entering the alternate screen.
        assert_eq!(screen.line_text(0), "hello");
        assert_eq!(screen.cell(0, 0).fg, Color::Indexed { index: 1 });
        // "hello" exactly filled the 5-column row: the cursor is "hanging"
        // at the last column with a wrap pending (see `wrap_pending`'s own
        // doc comment), not already on a next row.
        assert_eq!(screen.cursor(), (0, 4));
    }

    #[test]
    fn alt_screen_boundary_does_not_leak_hyperlink_state_either_direction() {
        // Architecture-review finding (CRITICAL, Checkpoint 5r): `pen`
        // already crossed the alt-screen boundary correctly (see the test
        // above); `active_hyperlink` originally didn't, which would leak a
        // hyperlink open on one side of the switch onto unrelated content
        // on the other side.
        let mut screen = Screen::new(1, 10);
        let mut parser = vte::Parser::new();

        // A hyperlink open on the primary screen must not leak onto the
        // alternate screen's own, entirely unrelated first characters.
        parser.advance(&mut screen, b"\x1b]8;;http://x\x1b\\");
        parser.advance(&mut screen, b"\x1b[?1049h");
        parser.advance(&mut screen, b"alt");
        assert!(
            !screen.cell(0, 0).hyperlink,
            "alt-screen content must not inherit the primary screen's open hyperlink"
        );

        // A hyperlink the alternate-screen program left open (no closing
        // OSC 8 before `?1049l`) must not leak back onto the primary
        // screen's next printed characters either. A fresh `Screen` here,
        // not a continuation of the one above: that one still has its own
        // *primary*-screen hyperlink (`http://x`) open from the first half
        // of this test, and restoring that on exit is the correct, wanted
        // behaviour (see the positive round-trip test below) — reusing it
        // here would conflate "primary's own still-open link correctly
        // survives" with the actual thing under test, whether the
        // alt-screen program's *own* unclosed link leaks out.
        let mut screen = Screen::new(1, 10);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b[?1049h");
        parser.advance(&mut screen, b"\x1b]8;;http://y\x1b\\link");
        parser.advance(&mut screen, b"\x1b[?1049l");
        parser.advance(&mut screen, b"prim");
        assert!(
            !screen.cell(0, 0).hyperlink,
            "primary-screen content must not inherit a hyperlink left open by the alt-screen program"
        );
    }

    #[test]
    fn a_hyperlink_still_open_on_the_primary_screen_survives_an_alt_screen_round_trip() {
        // The positive counterpart of the leak-prevention test above: not
        // just "no leak", but an actually-open hyperlink genuinely
        // surviving the round trip, the same way `pen`'s colour/bold state
        // already does (`alt_screen_hides_and_restores_the_primary_screen_exactly`).
        let mut screen = Screen::new(1, 10);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b]8;;http://x\x1b\\a"); // hyperlink still open, not yet closed
        parser.advance(&mut screen, b"\x1b[?1049h\x1b[?1049l"); // vim opens and immediately quits
        parser.advance(&mut screen, b"b");
        assert!(
            screen.cell(0, 1).hyperlink,
            "the still-open hyperlink from before the round trip must still be open after it"
        );
    }

    #[test]
    fn alt_screen_overflow_never_reaches_scrollback() {
        let mut screen = Screen::new(1, 4);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b[?1049h");
        // Enough line feeds on a 1-row alternate screen to scroll several
        // times over — none of it should end up in scrollback.
        parser.advance(&mut screen, b"a\r\nb\r\nc\r\nd\r\n");
        assert_eq!(screen.scrollback_len(), 0);
    }

    #[test]
    fn re_entering_alt_screen_does_not_clobber_the_saved_primary() {
        let mut screen = Screen::new(1, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"prim1");
        parser.advance(&mut screen, b"\x1b[?1049h");
        parser.advance(&mut screen, b"\x1b[?1049h"); // redundant enter - must be a no-op
        parser.advance(&mut screen, b"\x1b[?1049l");
        assert_eq!(screen.line_text(0), "prim1");
    }

    #[test]
    fn exiting_alt_screen_reconciles_size_if_the_terminal_was_resized_meanwhile() {
        let mut screen = Screen::new(2, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"hello");
        parser.advance(&mut screen, b"\x1b[?1049h");
        // The tile got bigger while the alternate screen (e.g. vim) was up.
        screen.resize(3, 8);
        parser.advance(&mut screen, b"\x1b[?1049l");
        // The restored primary screen must match the *current* size, not
        // whatever size it was saved at.
        assert_eq!(screen.size(), (3, 8));
        assert_eq!(screen.line_text(0), "hello   ");
    }

    #[test]
    fn bracketed_paste_mode_tracks_the_private_mode_sequence() {
        let mut screen = Screen::new(1, 5);
        let mut parser = vte::Parser::new();
        assert!(!screen.bracketed_paste());
        parser.advance(&mut screen, b"\x1b[?2004h");
        assert!(screen.bracketed_paste());
        parser.advance(&mut screen, b"\x1b[?2004l");
        assert!(!screen.bracketed_paste());
    }

    #[test]
    fn take_bell_reports_a_bel_once_and_then_clears_it() {
        let mut screen = Screen::new(1, 5);
        let mut parser = vte::Parser::new();
        assert!(!screen.take_bell(), "no bell has arrived yet");
        parser.advance(&mut screen, b"\x07");
        assert!(screen.take_bell());
        assert!(!screen.take_bell(), "must be cleared after being read once");
    }

    #[test]
    fn several_bells_before_a_read_still_only_report_once() {
        let mut screen = Screen::new(1, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x07\x07\x07");
        assert!(screen.take_bell());
        assert!(!screen.take_bell());
    }

    /// `bell_pending` is deliberately *not* part of `SavedPrimary` (see
    /// `Screen`'s own field doc comment) — `execute()` sets it regardless of
    /// which grid (`vim`/`htop`'s alternate screen, or the primary one) is
    /// currently active, since a `Perform` implementation has no notion of
    /// "the alternate screen suppresses this control code". A bell rung
    /// while the alternate screen is up must still reach `take_bell`, and
    /// switching back to the primary screen afterwards must not resurrect or
    /// swallow it either way.
    #[test]
    fn a_bell_rung_while_the_alternate_screen_is_up_is_still_reported() {
        let mut screen = Screen::new(1, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b[?1049h"); // vim/htop takes over
        assert!(!screen.take_bell(), "no bell yet, just entered alt screen");

        parser.advance(&mut screen, b"\x07"); // the program in the alt screen rings the bell
        assert!(
            screen.take_bell(),
            "a bell rung during the alternate screen must still be reported"
        );
        assert!(!screen.take_bell(), "already consumed");

        // Leaving the alternate screen afterwards must not, on its own,
        // resurrect an already-consumed bell.
        parser.advance(&mut screen, b"\x1b[?1049l");
        assert!(!screen.take_bell());
    }

    /// Same interaction, the other order: a bell rung on the primary screen
    /// that hasn't been read yet must survive entering *and* leaving the
    /// alternate screen untouched, since `enter_alt_screen`/`exit_alt_screen`
    /// only ever move grid/cursor/pen state, never `bell_pending`.
    #[test]
    fn a_pending_bell_survives_a_round_trip_through_the_alternate_screen() {
        let mut screen = Screen::new(1, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x07"); // ring the bell on the primary screen
        parser.advance(&mut screen, b"\x1b[?1049h\x1b[?1049l"); // enter and immediately leave

        assert!(
            screen.take_bell(),
            "a bell pending before the alternate screen must not be lost by the round trip"
        );
    }

    #[test]
    fn resizing_repeatedly_during_the_alternate_screen_only_reconciles_the_saved_primary_once_at_exit()
     {
        let mut screen = Screen::new(2, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"hello");
        parser.advance(&mut screen, b"\x1b[?1049h");
        // Two resizes in a row while the alternate screen is up (e.g. the
        // tile gets dragged bigger, then bigger again) — `saved_primary` is
        // deliberately untouched by either (see `Screen::resize`'s own doc
        // comment); only the *final* size should matter once `?1049l`
        // reconciles it.
        screen.resize(3, 8);
        screen.resize(4, 10);
        parser.advance(&mut screen, b"\x1b[?1049l");
        assert_eq!(screen.size(), (4, 10));
        assert_eq!(screen.line_text(0), "hello     ");
        assert_eq!(screen.cursor(), (0, 4));
    }

    // Found this as a genuine RED test (not flaky): `csi_dispatch` used to
    // unconditionally do `self.wrap_pending = false;` as its very first
    // line, *before* dispatching into `set_private_mode` -> `enter_alt_screen`
    // for the very same `CSI ?1049h` sequence that's supposed to save the
    // pending-wrap flag into `SavedPrimary` — so `enter_alt_screen` always
    // observed `wrap_pending == false`, regardless of what it actually was
    // the instant before. Fixed by moving the private-mode branch (and its
    // early `return`) ahead of that reset in `csi_dispatch`, so entering/
    // exiting the alternate screen never clobbers the flag it's itself
    // trying to preserve.
    #[test]
    fn wrap_pending_survives_a_round_trip_through_the_alternate_screen() {
        let mut screen = Screen::new(2, 5);
        let mut parser = vte::Parser::new();
        // Exactly fills the last column: a wrap is pending but not yet
        // visible (see `wrap_pending`'s own doc comment).
        parser.advance(&mut screen, b"hello");
        assert_eq!(screen.cursor(), (0, 4));

        parser.advance(&mut screen, b"\x1b[?1049h");
        parser.advance(&mut screen, b"vim");
        parser.advance(&mut screen, b"\x1b[?1049l");

        // If `wrap_pending` had NOT been saved/restored across the switch,
        // this next character would overwrite the still-hanging last
        // column in place (turning "hello" into "hellZ", cursor staying on
        // row 0) instead of finally performing the deferred wrap onto row 1.
        parser.advance(&mut screen, b"Z");
        assert_eq!(screen.line_text(0), "hello");
        assert_eq!(screen.line_text(1), "Z    ");
        assert_eq!(screen.cursor(), (1, 1));
    }

    #[test]
    fn erase_in_display_mode_3_also_clears_scrollback_unlike_mode_2() {
        // Found stale during review: the `2 | 3` arm's own comment used to
        // claim mode 3 "also clear[s] scrollback" but excused not actually
        // doing it with "there is no scrollback yet to clear" — true when
        // written (Checkpoint 2), no longer true once Checkpoint 4 added a
        // real `scrollback` buffer. Fixed to match both the comment and
        // real ED-3 semantics (xterm's `CSI 3 J` extension, what `clear`/
        // `tput reset` actually send) rather than leaving code and comment
        // mismatched.
        let mut screen = Screen::new(1, 4);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"one\r\ntwo\r\n"); // scrolls "one" and "two" into scrollback
        assert_eq!(screen.scrollback_len(), 2);

        parser.advance(&mut screen, b"thr\x1b[2J");
        assert_eq!(
            screen.scrollback_len(),
            2,
            "mode 2 clears the visible screen only, not scrollback"
        );

        parser.advance(&mut screen, b"\x1b[3J");
        assert_eq!(screen.scrollback_len(), 0, "mode 3 clears scrollback too");
        // The visible screen is still cleared, same as mode 2.
        assert_eq!(screen.line_text(0), "    ");
    }

    // Checkpoint 5r (owner-reported, Ghostty screenshot comparison): OSC 8
    // terminal hyperlinks weren't tracked at all, so a hyperlink's own SGR-4
    // fallback underline rendered permanently instead of being suppressed
    // the way a hyperlink-aware terminal like Ghostty suppresses it.

    #[test]
    fn osc_8_marks_cells_printed_while_a_hyperlink_is_open() {
        let mut screen = Screen::new(1, 20);
        let mut parser = vte::Parser::new();
        // ESC ] 8 ; <params> ; <uri> ST — an empty `id=` param list is fine,
        // only the URI (third segment) matters to `osc_dispatch`.
        parser.advance(
            &mut screen,
            b"before \x1b]8;;http://example.com\x1b\\link\x1b]8;;\x1b\\ after",
        );
        assert!(!screen.cell(0, 0).hyperlink, "'before' is plain text");
        // "before " is 7 characters (indices 0-6); "link" starts at index 7.
        for col in 7..11 {
            assert!(
                screen.cell(0, col).hyperlink,
                "'link' should be marked, col {col}"
            );
        }
        assert!(
            !screen.cell(0, 11).hyperlink,
            "' after' is plain text again"
        );
    }

    #[test]
    fn osc_8_with_bel_string_terminator_works_the_same_as_esc_backslash() {
        // `ST` (String Terminator) is either `ESC \` or a bare BEL (`\x07`)
        // — real-world emitters use both; this crate's own `execute` already
        // treats BEL specially for `bell_pending`, so worth its own explicit
        // coverage that OSC termination via BEL still reaches `osc_dispatch`
        // correctly rather than being swallowed by that other handling.
        let mut screen = Screen::new(1, 10);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b]8;;http://x\x07hi\x1b]8;;\x07");
        assert!(screen.cell(0, 0).hyperlink);
        assert!(screen.cell(0, 1).hyperlink);
    }

    #[test]
    fn a_mid_hyperlink_sgr_reset_does_not_close_the_hyperlink() {
        // The exact bug class `Screen::active_hyperlink`'s own doc comment
        // warns against: had hyperlink state lived on `pen` instead of its
        // own field, this `CSI 0 m` (full SGR reset) partway through the
        // link text would have wrongly ended it early — a real terminal's
        // hyperlink boundary is only ever another OSC 8, never an SGR reset.
        let mut screen = Screen::new(1, 10);
        let mut parser = vte::Parser::new();
        parser.advance(
            &mut screen,
            b"\x1b]8;;http://x\x1b\\a\x1b[0mb\x1b]8;;\x1b\\c",
        );
        assert!(screen.cell(0, 0).hyperlink, "'a', before the reset");
        assert!(
            screen.cell(0, 1).hyperlink,
            "'b', after a mid-hyperlink CSI 0 m — must still be open"
        );
        assert!(
            !screen.cell(0, 2).hyperlink,
            "'c', after the actual closing OSC 8"
        );
    }

    #[test]
    fn erasing_never_produces_a_hyperlink_cell() {
        // `blank_cell` (erase/clear) must always yield `hyperlink: false`,
        // independent of whatever hyperlink happened to be open when the
        // erase happened — an erased cell has no character to be "part of a
        // link" at all.
        let mut screen = Screen::new(1, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b]8;;http://x\x1b\\ab\x1b[2K");
        for col in 0..5 {
            assert!(
                !screen.cell(0, col).hyperlink,
                "col {col} after erase-in-line"
            );
        }
    }

    #[test]
    fn an_unrelated_osc_number_is_ignored_and_does_not_touch_hyperlink_state() {
        // OSC 0 (window title) is the most common other OSC a real shell
        // sends constantly (every prompt, in many configs) — must not be
        // mistaken for OSC 8 or otherwise disturb `active_hyperlink`.
        let mut screen = Screen::new(1, 5);
        let mut parser = vte::Parser::new();
        parser.advance(
            &mut screen,
            b"\x1b]8;;http://x\x1b\\a\x1b]0;some title\x1b\\b",
        );
        assert!(screen.cell(0, 0).hyperlink);
        assert!(
            screen.cell(0, 1).hyperlink,
            "OSC 0 must not close the still-open hyperlink"
        );
    }

    #[test]
    fn a_non_dec_private_marker_csi_m_sequence_is_not_read_as_sgr() {
        // Owner-reported live bug: real Claude Code CLI sessions sent
        // `CSI > 4 m` (xterm's `modifyOtherKeys` set) once, right at session
        // start, before any content printed. `csi_dispatch` only checked
        // for the `?` DEC-private-mode marker before falling through to the
        // generic `match action` block, so `> 4 m` fell through and matched
        // `'m' => apply_sgr(params)` on the final byte alone — misreading
        // it as bare SGR code 4 (underline), turned on at the very start of
        // the session with nothing after it to ever turn it back off, so
        // every subsequently printed character inherited a permanent
        // underline. `>` (and `<`, `=`, …) must be treated the same as `?`:
        // a vendor/private sequence family this crate doesn't model, not
        // SGR, regardless of shared final byte.
        let mut screen = Screen::new(1, 5);
        let mut parser = vte::Parser::new();
        parser.advance(&mut screen, b"\x1b[>4ma");
        assert!(
            !screen.cell(0, 0).underline,
            "CSI > 4 m must not be misread as SGR 4 (underline)"
        );
    }
}
