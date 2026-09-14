//! Standalone PTY + terminal-emulation engine.
//!
//! No dependency on Tauri or `axiomata-core` — this crate only knows about
//! PTYs, raw bytes, and its own screen model, so it stays unit-testable and
//! runnable on its own (see the `term-poc` binary). See
//! `docs/plans/terminal.md` for the full phased plan.
//!
//! Since Checkpoint 2: [`Terminal`] feeds a shell's raw output through a
//! [`vte`] parser into a [`Screen`] — a cell grid with cursor, colours, and
//! basic attributes. `PtySession` (Checkpoint 0/1) and `Terminal` are
//! deliberately separate: the former only knows about PTYs and bytes, the
//! latter only knows about interpreting those bytes, so either is
//! independently testable (`Terminal` needs no real PTY at all — see
//! `screen.rs`'s tests) and a caller wires them together (see
//! `apps/dashboard/src-tauri/src/terminal.rs`). `Screen` itself has since
//! grown scrollback, an alternate screen, and bracketed-paste tracking
//! (Checkpoint 4) — see its own module doc comment for the current state.

mod pty;
mod screen;

pub use pty::PtySession;
pub use screen::{Cell, Color, Screen};

/// Turns a shell's raw output bytes into screen state — a thin pairing of a
/// [`vte::Parser`] (which tokenizes escape sequences) and a [`Screen`]
/// (which is both the state those tokens update and the `vte::Perform`
/// implementation that applies them, see its own doc comment). The parser
/// must persist across calls to [`Terminal::feed`] since an escape sequence
/// can arrive split across two PTY reads.
pub struct Terminal {
    parser: vte::Parser,
    screen: Screen,
}

impl Terminal {
    /// A terminal with a blank `rows`x`cols` screen.
    pub fn new(rows: u16, cols: u16) -> Self {
        Terminal {
            parser: vte::Parser::new(),
            screen: Screen::new(rows, cols),
        }
    }

    /// Overrides the underlying [`Screen`]'s scrollback cap (Checkpoint 5b's
    /// per-instance scrollback-size setting) — see
    /// [`Screen::with_scrollback_limit`]. A builder, like its `Screen`
    /// counterpart, so `Terminal::new`'s signature and every existing call
    /// site stay unchanged.
    pub fn with_scrollback_limit(mut self, limit: usize) -> Self {
        self.screen = self.screen.with_scrollback_limit(limit);
        self
    }

    /// Feeds newly read output bytes through the parser, updating the
    /// screen in place.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.screen, bytes);
    }

    /// Resizes the underlying screen (see [`Screen::resize`]).
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.screen.resize(rows, cols);
    }

    /// The current screen state, for a caller that wants to render it (e.g.
    /// via [`Screen::to_lines`] or [`Screen::cell`]) rather than only
    /// feeding it more bytes.
    pub fn screen(&self) -> &Screen {
        &self.screen
    }

    /// Delegates to [`Screen::take_bell`] — see its own doc comment. Exposed
    /// here (not just via `screen()`, which only hands out `&Screen`)
    /// because it needs `&mut self`.
    pub fn take_bell(&mut self) -> bool {
        self.screen.take_bell()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real PTY read can land on any byte boundary, including mid-escape-
    /// sequence. `Terminal::feed` must keep re-using the same `vte::Parser`
    /// across calls (not, say, recreate one per call) so a sequence split
    /// this way still parses as one unit rather than leaking the first
    /// half's raw bytes as printable text.
    #[test]
    fn feed_persists_parser_state_across_chunks_split_mid_escape_sequence() {
        let mut terminal = Terminal::new(1, 3);
        // A bold-red SGR sequence (`\x1b[1;31m`), split right in the middle
        // of its numeric parameters.
        terminal.feed(b"\x1b[1;3");
        terminal.feed(b"1mA");

        let cell = terminal.screen().cell(0, 0);
        assert_eq!(cell.ch, 'A');
        assert_eq!(cell.fg, Color::Indexed { index: 1 });
        assert!(cell.bold);
        // Only "A" is visible -- none of the split escape bytes leaked
        // through as printed characters.
        assert_eq!(terminal.screen().line_text(0), "A  ");
    }

    #[test]
    fn resize_delegates_to_the_underlying_screen() {
        let mut terminal = Terminal::new(2, 2);
        terminal.feed(b"ab");

        terminal.resize(3, 4);

        assert_eq!(terminal.screen().size(), (3, 4));
        // Existing content survives the resize, same as `Screen::resize`
        // itself guarantees.
        assert_eq!(terminal.screen().line_text(0), "ab  ");
    }

    /// `Terminal::take_bell` is a thin delegation to `Screen::take_bell` (see
    /// its own doc comment) — `screen.rs`'s tests cover the underlying
    /// "read once, then clears itself" semantics in depth; this only checks
    /// the delegation itself actually reaches through `Terminal::feed`,
    /// mirroring `resize_delegates_to_the_underlying_screen` above.
    #[test]
    fn take_bell_delegates_to_the_underlying_screen() {
        let mut terminal = Terminal::new(1, 5);
        assert!(!terminal.take_bell(), "no bell has arrived yet");

        terminal.feed(b"\x07");
        assert!(terminal.take_bell());
        assert!(
            !terminal.take_bell(),
            "must be cleared after being read once"
        );
    }
}
