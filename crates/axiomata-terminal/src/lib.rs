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
//! `apps/axiomata/src-tauri/src/terminal.rs`). `Screen` itself has since
//! grown scrollback, an alternate screen, and bracketed-paste tracking
//! (Checkpoint 4) — see its own module doc comment for the current state.

mod pty;
mod screen;

pub use pty::{PtySession, PtyWriter};
pub use screen::{Cell, Color, MouseReporting, MouseTracking, Screen, UnderlineStyle};

/// Turns a shell's raw output bytes into screen state — a thin pairing of a
/// [`vte::Parser`] (which tokenizes escape sequences) and a [`Screen`]
/// (which is both the state those tokens update and the `vte::Perform`
/// implementation that applies them, see its own doc comment). The parser
/// must persist across calls to [`Terminal::feed`] since an escape sequence
/// can arrive split across two PTY reads.
pub struct Terminal {
    parser: vte::Parser,
    screen: Screen,
    /// The OSC sequence being read (TUI0b): vte buffers one without a bound, so its length is counted here.
    osc: OscGuard,
}

/// The longest OSC sequence passed to the parser: a clipboard write of 1 MB is about 1.4 MB of base64. A longer one —
/// output that opens an OSC and never ends it — is cancelled and the rest of it dropped, so it cannot fill memory.
const MAX_OSC_BYTES: usize = 2 * 1024 * 1024;

/// Where the byte stream stands for [`MAX_OSC_BYTES`].
#[derive(Default)]
struct OscGuard {
    /// The previous byte was ESC (the start of `ESC ]` or of the `ESC \` terminator).
    after_esc: bool,
    /// Bytes of the open OSC so far.
    open: Option<usize>,
    /// An OSC was too long: its bytes are dropped until it ends.
    dropping: bool,
}

impl Terminal {
    /// A terminal with a blank `rows`x`cols` screen.
    pub fn new(rows: u16, cols: u16) -> Self {
        Terminal {
            parser: vte::Parser::new(),
            screen: Screen::new(rows, cols),
            osc: OscGuard::default(),
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
        let mut start = 0;
        for (at, &byte) in bytes.iter().enumerate() {
            let after_esc = std::mem::replace(&mut self.osc.after_esc, byte == 0x1b);
            if self.osc.dropping {
                // BEL or ST ends the dropped OSC; an ESC starting anything else ends it too, and that sequence is read
                // as usual — another `ESC ]` included, so a second endless OSC is counted again.
                let ends = byte == 0x07 || (after_esc && byte == b'\\');
                if !ends && !(after_esc && byte != 0x1b) {
                    start = at + 1;
                    continue;
                }
                self.osc = OscGuard {
                    after_esc: byte == 0x1b,
                    ..OscGuard::default()
                };
                if ends {
                    start = at + 1;
                    continue;
                }
                self.parser.advance(&mut self.screen, &[0x1b]);
                start = at;
            }
            match self.osc.open {
                Some(_) if matches!(byte, 0x07 | 0x18 | 0x1a | 0x1b) => self.osc.open = None,
                Some(length) if length >= MAX_OSC_BYTES => {
                    // Feed what came before, then cancel the sequence without acting on it.
                    self.parser.advance(&mut self.screen, &bytes[start..at]);
                    self.screen.drop_next_osc();
                    self.parser.advance(&mut self.screen, &[0x18]);
                    self.osc = OscGuard {
                        dropping: true,
                        ..OscGuard::default()
                    };
                    start = at + 1;
                }
                Some(length) => self.osc.open = Some(length + 1),
                None if after_esc && byte == b']' => self.osc.open = Some(0),
                None => {}
            }
        }
        if start < bytes.len() {
            self.parser.advance(&mut self.screen, &bytes[start..]);
        }
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

    /// What the terminal answers the program (device attributes, mode reports, the cursor position, colours) since
    /// the last call — the caller writes it to the PTY. See [`Screen::take_replies`].
    pub fn take_replies(&mut self) -> Vec<u8> {
        self.screen.take_replies()
    }

    /// Text a program put on the clipboard (OSC 52) since the last call.
    pub fn take_clipboard(&mut self) -> Option<String> {
        self.screen.take_clipboard()
    }

    /// The pane's colours, which OSC 10/11 queries are answered with.
    pub fn set_colors(&mut self, foreground: [u8; 3], background: [u8; 3]) {
        self.screen.set_colors(foreground, background);
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

    /// TUI0b: an OSC that never ends is cut off at `MAX_OSC_BYTES`, never acted on, and what follows its end is read
    /// as usual.
    #[test]
    fn an_endless_osc_is_dropped_and_never_acted_on() {
        let mut terminal = Terminal::new(1, 8);
        terminal.feed(b"\x1b]52;c;");
        let chunk = vec![b'A'; 64 * 1024];
        for _ in 0..(MAX_OSC_BYTES / chunk.len() + 2) {
            terminal.feed(&chunk);
        }
        terminal.feed(b"AAAA\x07ok");
        assert_eq!(
            terminal.take_clipboard(),
            None,
            "the cut-off write is not done"
        );
        assert_eq!(terminal.screen().line_text(0), "ok      ");
        terminal.feed(b"\x1b]52;c;aGk=\x1b\\");
        assert_eq!(
            terminal.take_clipboard().as_deref(),
            Some("hi"),
            "the next one is read again"
        );
        // A second endless OSC right after a dropped one is counted too: the guard is never off.
        terminal.feed(b"\x1b]52;c;");
        for _ in 0..(MAX_OSC_BYTES / chunk.len() + 2) {
            terminal.feed(&chunk);
        }
        terminal.feed(b"\x1b]0;");
        assert!(
            terminal.osc.open.is_some() || !terminal.osc.dropping,
            "the next OSC is tracked again"
        );
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
