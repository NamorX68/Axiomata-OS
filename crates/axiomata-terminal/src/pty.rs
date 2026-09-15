//! PTY spawning and I/O, backed by the `portable-pty` crate.

use std::io::{self, Read, Write};
use std::path::Path;

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
    ///
    /// `cwd_override` (Checkpoint 5b) sets the shell's starting working
    /// directory, analogous to `shell_override`: a bad path surfaces as this
    /// call's own `Err` rather than silently falling back to some other
    /// directory — an owner who typed the wrong path should find out
    /// immediately, not end up somewhere they didn't ask for. This *is*
    /// validated here (unlike `shell_override`, which only fails once
    /// `spawn_command` itself tries and fails to exec it): `portable_pty`'s
    /// own `CommandBuilder` silently falls back to the home directory for a
    /// `cwd` that isn't an existing directory rather than erroring, so
    /// honouring "bad path = loud error" requires checking it ourselves
    /// before handing it off. `None` leaves `CommandBuilder`'s own default
    /// in place (unchanged from every earlier checkpoint).
    ///
    /// `extra_env` (Checkpoint 5b) applies additional environment variables
    /// after `TERM=xterm-256color` and `COLORTERM=truecolor` (Checkpoint
    /// 5l — see below) — so an entry named `TERM`/`COLORTERM` in the list
    /// deliberately wins over the lines above it, should that ever be
    /// wanted, rather than being silently dropped. Applied in the given
    /// order; a repeated key follows `CommandBuilder`/`std::process::Command`'s
    /// own "last call wins" behaviour, not enforced separately here.
    ///
    /// `COLORTERM=truecolor` (Checkpoint 5l, owner-reported: a shell prompt
    /// framework's colours/glyphs looked noticeably different than in
    /// Ghostty or Kitty, even after the terminal-theme background/foreground
    /// fix in Checkpoint 5k) — there's no formal terminfo capability for
    /// 24-bit colour support, so most modern colour-aware CLI tools
    /// (Powerlevel10k/Starship prompts, `chalk`-based Node tools, `git
    /// diff --color`, …) check this de-facto-standard variable instead to
    /// decide whether to emit real RGB escape sequences or fall back to a
    /// 256-colour approximation. This engine has supported real 24-bit
    /// `38;2;r;g;b`/`48;2;r;g;b` SGR sequences since Checkpoint 1 (see
    /// `screen.rs`'s `Color::Rgb`) — this was simply never advertised, so a
    /// capability-detecting tool had no way to know and likely picked
    /// duller, approximated colours by default. Real terminal emulators
    /// with truecolor support (Ghostty, Kitty, iTerm2, Alacritty, …) all set
    /// this themselves, unprompted.
    pub fn spawn(
        rows: u16,
        cols: u16,
        shell_override: Option<&str>,
        cwd_override: Option<&Path>,
        extra_env: &[(String, String)],
    ) -> io::Result<Self> {
        if let Some(cwd) = cwd_override
            && !cwd.is_dir()
        {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("cwd_override {} is not a directory", cwd.display()),
            ));
        }

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
        cmd.env("COLORTERM", "truecolor");
        for (key, value) in extra_env {
            cmd.env(key, value);
        }
        if let Some(cwd) = cwd_override {
            cmd.cwd(cwd);
        }

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
    use std::panic::{self, AssertUnwindSafe};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use super::*;

    /// How long [`with_watchdog`] waits before declaring a test body hung.
    /// Generous relative to how fast these tests normally run (tens of
    /// milliseconds) so it never fires on a merely slow CI box, but short
    /// enough that a real hang fails a test run in well under a minute
    /// rather than needing a manual kill.
    const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(15);

    /// Runs `body` on its own thread and waits up to [`WATCHDOG_TIMEOUT`]
    /// for it to finish, turning a hang into a clearly diagnosed test
    /// failure instead of letting it freeze the entire `cargo test` process.
    ///
    /// Exists because of a confirmed, environment-level hang risk in real
    /// PTY spawning that isn't this crate's own bug: a spawned shell's child
    /// process can occasionally get stuck mid-exit at the kernel level
    /// (observable in `ps` as a zombie-like entry sitting in macOS's `E`
    /// "exiting" state that a `wait()` on it never returns from — this was
    /// reproduced with a plain Python `pty.fork()` script that has no
    /// dependency on this crate, `portable_pty`, or any shell dotfile at
    /// all, so the cause sits below this codebase entirely). Before this
    /// guard, hitting that condition during a test run hung the *whole*
    /// process indefinitely (every other test included), and the only way
    /// out was an external `kill -9` — which itself only made the underlying
    /// resource exhaustion worse, since the killed process's own
    /// `PtySession::drop` (which would otherwise reap its child) never got
    /// to run either.
    ///
    /// A timeout here does *not* claim the hang is fixed — it can't be, from
    /// user-space Rust code, since the kernel-level `wait()` genuinely never
    /// returns — only that one test now fails cleanly and quickly instead of
    /// the whole suite freezing. The stuck background thread itself is not
    /// reclaimed (there's no way to force it to stop mid-syscall); it's
    /// simply abandoned and cleaned up whenever this test process eventually
    /// exits, same as it already would be without this helper.
    fn with_watchdog<F>(body: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = panic::catch_unwind(AssertUnwindSafe(body));
            // Ignore send errors: the receiver may have already timed out
            // and been dropped, in which case there's no one left to tell.
            let _ = tx.send(result);
        });
        match rx.recv_timeout(WATCHDOG_TIMEOUT) {
            Ok(Ok(())) => {}
            Ok(Err(panic)) => panic::resume_unwind(panic),
            Err(_) => panic!(
                "test body did not finish within {WATCHDOG_TIMEOUT:?} — likely the \
                 known environment-level PTY-teardown hang (see with_watchdog's own \
                 doc comment), not a bug in this test or in PtySession itself"
            ),
        }
    }

    /// Spawns a real shell in a PTY, writes a non-interactive command, and
    /// checks its echoed output shows up — deterministic (we only assert a
    /// substring appears, not the exact byte stream, since prompts/rc-file
    /// output vary by shell/config) and independent of any real, externally
    /// attached TTY: the PTY here is one this test opens for itself.
    #[test]
    fn spawn_write_read_roundtrip() {
        with_watchdog(|| {
            let mut session =
                PtySession::spawn(24, 80, None, None, &[]).expect("failed to spawn pty session");
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
        });
    }

    /// `resize` on a freshly spawned session should just succeed — deterministic,
    /// no dependency on any real/attached TTY (this test's PTY is a fresh
    /// `openpty()` of its own), and no interaction with the shell's own I/O.
    #[test]
    fn resize_after_spawn_succeeds() {
        with_watchdog(|| {
            let mut session =
                PtySession::spawn(24, 80, None, None, &[]).expect("failed to spawn pty session");

            session
                .resize(40, 120)
                .expect("resize on a freshly spawned session should succeed");
        });
    }

    /// `shell_override` (Checkpoint 5's per-instance "Shell-Wahl" setting)
    /// actually reaches the spawned process, not just `$SHELL`/the fallback
    /// — deterministic and TTY-independent the same way as
    /// `spawn_write_read_roundtrip`: `/bin/sh` (not `$SHELL`, whatever that
    /// happens to be on the machine running this test) writing `echo hallo`
    /// only produces "hallo" if `/bin/sh` is really what got exec'd.
    #[test]
    fn shell_override_actually_selects_that_shell() {
        with_watchdog(|| {
            let mut session = PtySession::spawn(24, 80, Some("/bin/sh"), None, &[])
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
        });
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
        with_watchdog(|| {
            let result = PtySession::spawn(
                24,
                80,
                Some("/definitely/not/a/real/shell/anywhere"),
                None,
                &[],
            );
            assert!(result.is_err());
        });
    }

    /// `cwd_override` (Checkpoint 5b's "Start-Verzeichnis" setting) actually
    /// reaches the spawned shell — deterministic and TTY-independent the
    /// same way as the other roundtrip tests: `pwd` only echoes the temp
    /// directory's own path if the shell really started there.
    ///
    /// Uses `/bin/sh` (not `None`/`$SHELL`) for the same reason
    /// `shell_override_actually_selects_that_shell` already does: a real
    /// `$SHELL` login shell sources the owner's personal dotfiles, and this
    /// test hung indefinitely the first time it ran with `None` here.
    ///
    /// Update (rust-test-engineer review pass, 2026-09-14): `/bin/sh` alone
    /// does *not* fully eliminate the hang risk here — re-running this exact
    /// test repeatedly on one development machine still reproduced an
    /// indefinite hang roughly every other run, and it was tracked down to
    /// something below this crate entirely: a plain Python `pty.fork()` +
    /// `os.execv("/bin/sh", ...)` script, with no dependency on this crate,
    /// `portable_pty`, or any shell dotfile, reproduced the identical hang
    /// (a spawned child stuck in macOS's `E` "exiting" kernel state,
    /// `waitpid()` on it never returning). So the original "dotfile hook"
    /// theory was likely never the real cause — this looks like an
    /// environment-level PTY-teardown issue on that machine's OS build, not
    /// anything `shell_override`/`cwd_override`/this test's own logic can
    /// fix. [`with_watchdog`] (this module's own tests-only helper) is the
    /// mitigation: it can't make the hang not happen, but it stops one
    /// occurrence of it from freezing the entire test process.
    #[test]
    fn cwd_override_actually_starts_the_shell_there() {
        with_watchdog(|| {
            let dir = std::env::temp_dir();
            // Resolve symlinks (macOS's `/tmp` is a symlink to `/private/tmp`)
            // so the shell's own `pwd` output — which reports the resolved
            // path — can be compared directly against what we asked for.
            let dir = std::fs::canonicalize(&dir).expect("failed to canonicalize temp dir");

            let mut session = PtySession::spawn(24, 80, Some("/bin/sh"), Some(&dir), &[])
                .expect("failed to spawn pty session with a cwd override");
            let mut reader = session.try_clone_reader().expect("failed to clone reader");

            session.write(b"pwd\n").expect("failed to write to pty");

            let expected = dir.display().to_string();
            let (tx, rx) = mpsc::channel();
            thread::spawn(move || {
                let mut buf = [0u8; 4096];
                let mut collected = Vec::new();
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            collected.extend_from_slice(&buf[..n]);
                            if String::from_utf8_lossy(&collected).contains(&expected) {
                                break;
                            }
                        }
                    }
                }
                let _ = tx.send(collected);
            });

            let output = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("timed out waiting for the shell's pwd output");
            assert!(String::from_utf8_lossy(&output).contains(&dir.display().to_string()));
        });
    }

    /// A `cwd_override` that isn't a real directory is a spawn error, not a
    /// silent fallback — `portable_pty`'s own `CommandBuilder` would
    /// otherwise silently fall back to the home directory (see `spawn`'s own
    /// doc comment), which is exactly the surprising behaviour this
    /// validation exists to avoid.
    #[test]
    fn cwd_override_pointing_at_nothing_is_a_spawn_error() {
        with_watchdog(|| {
            let bad_path = std::path::Path::new("/definitely/not/a/real/directory/anywhere");
            let result = PtySession::spawn(24, 80, None, Some(bad_path), &[]);
            assert!(result.is_err());
        });
    }

    /// `extra_env` (Checkpoint 5b's "eigene Umgebungsvariablen" setting)
    /// actually reaches the spawned shell's environment.
    ///
    /// Uses `/bin/sh` for the same reason `cwd_override_actually_starts_the_shell_there`
    /// does — see that test's own doc comment.
    #[test]
    fn extra_env_actually_reaches_the_shell() {
        with_watchdog(|| {
            let extra = vec![("AXIOMATA_TEST_VAR".to_string(), "hallo_welt".to_string())];
            let mut session = PtySession::spawn(24, 80, Some("/bin/sh"), None, &extra)
                .expect("failed to spawn pty session with extra env vars");
            let mut reader = session.try_clone_reader().expect("failed to clone reader");

            session
                .write(b"echo $AXIOMATA_TEST_VAR\n")
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
                            if String::from_utf8_lossy(&collected).contains("hallo_welt") {
                                break;
                            }
                        }
                    }
                }
                let _ = tx.send(collected);
            });

            let output = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("timed out waiting for the shell to echo the extra env var");
            assert!(String::from_utf8_lossy(&output).contains("hallo_welt"));
        });
    }

    /// `TERM=xterm-256color` and `COLORTERM=truecolor` (Checkpoint 5l) both
    /// actually reach the spawned shell's environment — `COLORTERM` is the
    /// signal capability-detecting tools (Powerlevel10k/Starship prompts,
    /// `chalk`-based Node tools, …) use to decide whether to emit real
    /// 24-bit colour instead of a 256-colour approximation.
    ///
    /// Checks *both* variables together, not `COLORTERM` alone (architecture
    /// review, Checkpoint 5l): `portable_pty::CommandBuilder` seeds its
    /// environment from this test process's own *inherited* environment
    /// before `PtySession::spawn`'s own `cmd.env(...)` calls run — and
    /// `COLORTERM=truecolor` alone is a near-universal value in any modern
    /// interactive dev shell (this repo's own `cargo test` runs from one),
    /// so a test checking only that could pass by coincidental inheritance
    /// even if `spawn`'s own line were deleted, proving nothing. `TERM` is
    /// far less likely to coincidentally already equal the literal string
    /// `"xterm-256color"` — most real terminals set their own
    /// self-identifying value instead (`xterm-ghostty`, `xterm-kitty`,
    /// `screen-256color` under `tmux`, …) — so asserting on *both* together
    /// gives much stronger (though, being environment-dependent, still not
    /// perfectly airtight) evidence that `spawn`'s own `cmd.env` calls are
    /// what put these values there, not inheritance alone. A fully airtight
    /// version would need the test to control its own process environment
    /// before spawning, which needs `unsafe` `std::env::set_var` in this
    /// crate's 2024 edition and its own real soundness caveats under
    /// `cargo test`'s parallel execution — judged not worth it here.
    #[test]
    fn term_and_colorterm_reach_the_shell() {
        with_watchdog(|| {
            let mut session = PtySession::spawn(24, 80, Some("/bin/sh"), None, &[])
                .expect("failed to spawn pty session");
            let mut reader = session.try_clone_reader().expect("failed to clone reader");

            session
                .write(b"echo $TERM:$COLORTERM\n")
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
                            if String::from_utf8_lossy(&collected)
                                .contains("xterm-256color:truecolor")
                            {
                                break;
                            }
                        }
                    }
                }
                let _ = tx.send(collected);
            });

            let output = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("timed out waiting for the shell to echo TERM/COLORTERM");
            assert!(String::from_utf8_lossy(&output).contains("xterm-256color:truecolor"));
        });
    }

    /// An `extra_env` entry named `COLORTERM` wins over the built-in
    /// `truecolor` default — same "later entry with this name wins" contract
    /// `spawn`'s own doc comment already establishes for `TERM`, now
    /// actually exercised for `COLORTERM` too rather than just claimed.
    #[test]
    fn extra_env_can_override_colorterm() {
        with_watchdog(|| {
            let extra = vec![("COLORTERM".to_string(), "overridden".to_string())];
            let mut session = PtySession::spawn(24, 80, Some("/bin/sh"), None, &extra)
                .expect("failed to spawn pty session with extra env vars");
            let mut reader = session.try_clone_reader().expect("failed to clone reader");

            session
                .write(b"echo $COLORTERM\n")
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
                            if String::from_utf8_lossy(&collected).contains("overridden") {
                                break;
                            }
                        }
                    }
                }
                let _ = tx.send(collected);
            });

            let output = rx
                .recv_timeout(Duration::from_secs(5))
                .expect("timed out waiting for the shell to echo the overridden COLORTERM");
            assert!(String::from_utf8_lossy(&output).contains("overridden"));
        });
    }
}
