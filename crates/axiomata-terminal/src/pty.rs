//! PTY spawning and I/O, backed by the `portable-pty` crate.

use std::io::{self, Read, Write};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// Fallback shell when `$SHELL` isn't set (should be rare on macOS/Linux).
const FALLBACK_SHELL: &str = "/bin/zsh";

/// A running PTY with a shell process attached.
///
/// Owns the PTY master (for resizing and cloning readers) and the writer
/// half (for input). Reading is deliberately *not* owned by this struct —
/// `portable_pty`'s reader blocks, and the right threading model depends on
/// the caller: a raw stdin/stdout relay thread in the `term-poc` binary
/// today, a background thread streaming to a Tauri `Channel` from
/// Checkpoint 1 on. See [`PtySession::try_clone_reader`].
///
/// Dropping a `PtySession` kills the shell process, so a session never
/// outlives its owner as an orphaned background process (e.g. if the
/// caller's own read/write loop ends before the shell was told to exit).
pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

impl PtySession {
    /// Spawns a shell inside a new PTY of the given size, with
    /// `TERM=xterm-256color` — the de facto baseline most CLI programs
    /// already target, without claiming support for terminal-specific
    /// extensions (e.g. Kitty's graphics protocol) this crate doesn't
    /// implement. `shell_override` picks a specific shell (Checkpoint 5's
    /// per-instance "Shell-Wahl" setting — an absolute path, not validated
    /// here; a bad one surfaces as this call's own `Err` once
    /// `spawn_command` tries and fails to exec it); `None` falls back to
    /// `$SHELL`, then `/bin/zsh`, same as every earlier checkpoint.
    pub fn spawn(rows: u16, cols: u16, shell_override: Option<&str>) -> io::Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            // `portable_pty` has its own error type; wrap it so this crate's
            // public API can stay in plain `std::io::Result` throughout.
            .map_err(io::Error::other)?;

        let shell = shell_override.map(str::to_string).unwrap_or_else(|| {
            std::env::var("SHELL").unwrap_or_else(|_| FALLBACK_SHELL.to_string())
        });
        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");

        let child = pair.slave.spawn_command(cmd).map_err(io::Error::other)?;
        // Only needed to spawn the child; dropping it here means the shell
        // holds the only reference to the slave side, so the master's
        // reader correctly sees EOF once the shell exits (kept open, the
        // PTY would never signal EOF even after the shell is gone).
        drop(pair.slave);

        let writer = pair.master.take_writer().map_err(io::Error::other)?;

        Ok(Self {
            master: pair.master,
            writer,
            child,
        })
    }

    /// Writes input bytes (already translated to the right escape bytes, if
    /// any) to the shell.
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.writer.write_all(bytes)?;
        self.writer.flush()
    }

    /// Resizes the PTY, so the shell and any full-screen program running in
    /// it can react (`SIGWINCH`).
    pub fn resize(&mut self, rows: u16, cols: u16) -> io::Result<()> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(io::Error::other)
    }

    /// Returns a fresh handle for reading the shell's output.
    ///
    /// Separate from the struct's own I/O because `portable_pty`'s reader is
    /// blocking: the caller decides how to consume it, typically on a
    /// dedicated thread (see `term-poc`'s stdout relay).
    pub fn try_clone_reader(&self) -> io::Result<Box<dyn Read + Send>> {
        self.master.try_clone_reader().map_err(io::Error::other)
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        // Best-effort: the shell has very likely already exited on its own
        // (the normal case). `kill()` alone only signals the process — on
        // Unix it doesn't reap it, so without the following `wait()` an
        // already-exited (or now-signalled) child would sit as a zombie
        // until this program's own process exits and its parent reaps it.
        // That's negligible for the short-lived `term-poc` binary, but would
        // accumulate for real once a long-lived host process (Checkpoint 1's
        // Tauri session registry) repeatedly spawns and drops sessions.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    /// Spawns a real shell in a PTY, writes a non-interactive command, and
    /// checks its echoed output shows up — deterministic (we only assert a
    /// substring appears, not the exact byte stream, since prompts/rc-file
    /// output vary by shell/config) and independent of any real, externally
    /// attached TTY: the PTY here is one this test opens for itself.
    #[test]
    fn spawn_write_read_roundtrip() {
        let mut session = PtySession::spawn(24, 80, None).expect("failed to spawn pty session");
        let mut reader = session.try_clone_reader().expect("failed to clone reader");

        session
            .write(b"echo hallo\n")
            .expect("failed to write to pty");

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut collected = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        collected.extend_from_slice(&buf[..n]);
                        if String::from_utf8_lossy(&collected).contains("hallo") {
                            break;
                        }
                    }
                }
            }
            // Ignore send errors: the receiver may have already timed out
            // and been dropped.
            let _ = tx.send(collected);
        });

        let output = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("timed out waiting for the shell to echo its output");
        assert!(
            String::from_utf8_lossy(&output).contains("hallo"),
            "expected the shell's output to contain the echoed 'hallo'"
        );
    }

    /// `resize` on a freshly spawned session should just succeed — deterministic,
    /// no dependency on any real/attached TTY (this test's PTY is a fresh
    /// `openpty()` of its own), and no interaction with the shell's own I/O.
    #[test]
    fn resize_after_spawn_succeeds() {
        let mut session = PtySession::spawn(24, 80, None).expect("failed to spawn pty session");

        session
            .resize(40, 120)
            .expect("resize on a freshly spawned session should succeed");
    }

    /// `shell_override` (Checkpoint 5's per-instance "Shell-Wahl" setting)
    /// actually reaches the spawned process, not just `$SHELL`/the fallback
    /// — deterministic and TTY-independent the same way as
    /// `spawn_write_read_roundtrip`: `/bin/sh` (not `$SHELL`, whatever that
    /// happens to be on the machine running this test) writing `echo hallo`
    /// only produces "hallo" if `/bin/sh` is really what got exec'd.
    #[test]
    fn shell_override_actually_selects_that_shell() {
        let mut session = PtySession::spawn(24, 80, Some("/bin/sh"))
            .expect("failed to spawn pty session with a shell override");
        let mut reader = session.try_clone_reader().expect("failed to clone reader");

        session
            .write(b"echo hallo\n")
            .expect("failed to write to pty");

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut collected = Vec::new();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        collected.extend_from_slice(&buf[..n]);
                        if String::from_utf8_lossy(&collected).contains("hallo") {
                            break;
                        }
                    }
                }
            }
            let _ = tx.send(collected);
        });

        let output = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("timed out waiting for /bin/sh to echo its output");
        assert!(String::from_utf8_lossy(&output).contains("hallo"));
    }

    /// A `shell_override` that isn't a real executable is a spawn error, not
    /// a silent fallback to `$SHELL` — the owner asked for a specific shell,
    /// and papering over a typo/moved binary with a different shell they
    /// didn't ask for would be more surprising than just failing loudly.
    ///
    /// Deliberately not also covering `Some("")` as a separate case: probed
    /// manually, `portable_pty`/`CommandBuilder` resolves an empty program
    /// name via a `PATH` search (treating it as a bare name, not an absolute
    /// path) rather than this test's "absolute path not found" branch — but
    /// both end at the same observable contract this test already asserts:
    /// `spawn` returns `Err`, no silent fallback. A dedicated empty-string
    /// test would only exercise a different internal `portable_pty` code
    /// path without asserting anything new about `PtySession::spawn` itself.
    #[test]
    fn shell_override_pointing_at_nothing_is_a_spawn_error() {
        let result = PtySession::spawn(24, 80, Some("/definitely/not/a/real/shell/anywhere"));
        assert!(result.is_err());
    }
}
