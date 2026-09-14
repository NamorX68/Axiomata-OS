//! Checkpoint-0 proof of concept: spawns `$SHELL` in a PTY and relays raw
//! bytes between it and this process's own stdin/stdout — a minimal
//! "terminal in a terminal", with no ANSI/VT100 interpretation of its own.
//! Escape sequences (colors, cursor movement, …) pass through unchanged and
//! are interpreted by whichever real terminal this binary itself runs in;
//! that's expected at this checkpoint, not a bug.
//!
//! Run with `cargo run -p axiomata-terminal --bin term-poc`, type shell
//! commands, and `exit` when done.

use std::io::{self, Read, Write};
use std::thread;

use axiomata_terminal::PtySession;

/// Fixed grid size for this checkpoint's PoC. Real character-metrics-based
/// sizing (from the placed tile's pixel dimensions) is a Checkpoint-3
/// concern, not something this standalone CLI binary needs.
const ROWS: u16 = 24;
const COLS: u16 = 80;

fn main() -> io::Result<()> {
    let mut session = PtySession::spawn(ROWS, COLS, None)?;
    let mut reader = session.try_clone_reader()?;

    // Relay the shell's output to our own stdout. When the shell exits, the
    // PTY closes and this read hits EOF — our cue that there's nothing left
    // to relay, so this process exits too (Checkpoint 0 has no session
    // persistence: closing the shell closes the whole program).
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let mut stdout = io::stdout().lock();
                    if stdout.write_all(&buf[..n]).is_err() || stdout.flush().is_err() {
                        break;
                    }
                }
            }
        }
        std::process::exit(0);
    });

    // Relay our own stdin to the shell, on the main thread. This process's
    // stdin is never put into raw mode, so the *outer* real terminal stays
    // line-buffered/cooked — closer to `script`'s non-interactive mode than
    // a fully interactive terminal (arrow keys, Ctrl+C, etc. won't reach the
    // shell as expected). That's fine for proving the PTY pipe works; a real
    // typing feel is the UI's job from Checkpoint 3 on, not this CLI PoC's.
    let stdin = io::stdin();
    let mut buf = [0u8; 4096];
    loop {
        let n = match stdin.lock().read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        if session.write(&buf[..n]).is_err() {
            break;
        }
    }

    Ok(())
}
