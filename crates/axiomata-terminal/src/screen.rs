//! The terminal screen model: a cell grid, cursor, and current SGR "pen" —
//! Checkpoint 2's `vte::Perform` implementation that turns parsed
//! ANSI/VT100 events into screen state. This is the "own code" half of the
//! crate's design (see the crate-level doc comment): `vte` only tokenizes
//! escape sequences, everything a parsed event actually *does* to the
//! screen lives here.
//!
//! Deliberately basic, matching Checkpoint 2's own scope: cursor movement,
//! erase-in-line/-display, and the common SGR codes (16/256/truecolor,
//! bold, underline, reset). No scrollback (Checkpoint 4 — this screen keeps
//! exactly `rows` lines and drops the top one when content overflows, so a
//! shell stays usable but nothing scrolled off is recoverable yet), no
//! alternate screen, no saved cursor, no scroll regions, no wide-char
//! spacer cells (a width-2 character occupies one cell and simply advances
//! the cursor by 2 — real terminals also blank the following cell, not
//! implemented here). None of that is silently broken so much as not yet
//! asked for by this checkpoint.

use serde::Serialize;
use unicode_width::UnicodeWidthChar;
use vte::{Params, Perform};

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
}

impl Default for Cell {
    fn default() -> Self {
        Cell {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            bold: false,
            underline: false,
        }
    }
}

/// The screen: a fixed `rows`x`cols` grid of [`Cell`]s, a cursor position,
/// and the "pen" (the SGR attributes any newly printed character gets).
/// Implements [`vte::Perform`] directly — this struct *is* the terminal
/// state machine the crate's own doc comment promises, not a thin
/// passthrough to something else.
pub struct Screen {
    rows: usize,
    cols: usize,
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
        }
    }

    /// Resizes the grid, keeping whatever existing content still fits in
    /// the top-left corner (simplest reflow policy — a smarter one, e.g.
    /// reflowing wrapped lines, isn't asked for until real resize handling
    /// lands in Checkpoint 3). Clamps the cursor back into bounds.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = (rows as usize).max(1);
        let cols = (cols as usize).max(1);
        let mut grid = vec![vec![Cell::default(); cols]; rows];
        for (new_row, old_row) in grid.iter_mut().zip(self.grid.iter()) {
            for (new_cell, &old_cell) in new_row.iter_mut().zip(old_row.iter()) {
                *new_cell = old_cell;
            }
        }
        self.grid = grid;
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

    /// Shifts every row up by one, dropping the top line and appending a
    /// blank one at the bottom — the bare minimum "content keeps flowing"
    /// behaviour a shell needs once output exceeds one screen. Not a real
    /// scrollback (the dropped line is gone, not just off-screen) — that's
    /// Checkpoint 4.
    fn scroll_up(&mut self) {
        self.grid.remove(0);
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
    /// [`Cell::default`].
    fn blank_cell(&self) -> Cell {
        Cell {
            ch: ' ',
            fg: Color::Default,
            bg: self.pen.bg,
            bold: false,
            underline: false,
        }
    }

    fn clear_range(&mut self, row: usize, from_col: usize, to_col_exclusive: usize) {
        let blank = self.blank_cell();
        for cell in &mut self.grid[row][from_col..to_col_exclusive] {
            *cell = blank;
        }
    }

    /// Implements CSI `J` (erase in display); `mode` follows the standard ED
    /// values (0 = cursor to end, 1 = start to cursor inclusive, 2/3 = whole
    /// screen). An unrecognized value is a silent no-op — same convention
    /// `csi_dispatch` uses for any other unimplemented sequence.
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
            // Whole screen. `3` (also clear scrollback) is the same here —
            // there is no scrollback yet to clear.
            2 | 3 => {
                for r in 0..self.rows {
                    self.clear_range(r, 0, self.cols);
                }
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
            _ => {} // other C0/C1 controls (bell, …) have no screen effect here
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        _intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
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
            'J' => self.erase_in_display(Self::first_param_or(params, 0)),
            'K' => self.erase_in_line(Self::first_param_or(params, 0)),
            'm' => self.apply_sgr(params),
            // Scroll regions, insert/delete line/char, mode set/reset
            // (cursor visibility, bracketed paste, alternate screen, …) are
            // all out of Checkpoint 2's scope — silently no-op rather than
            // corrupting the grid on an unrecognized sequence.
            _ => {}
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
}
