//! The general pasteboard as text, through `pbpaste` and `pbcopy`.
//!
//! Both tools convert text by the locale's encoding; `LC_CTYPE=UTF-8` makes
//! that UTF-8 whatever the app was started with (a GUI app's environment has
//! no `LANG`), so umlauts and emoji survive the round trip.

use std::io::{self, Read, Write};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// The most text read from, or written to, the pasteboard. A larger clipboard
/// (someone copied a video's worth of text) is cut here rather than moved into
/// the webview; a larger write is refused.
pub const MAX_CLIPBOARD_BYTES: usize = 16 * 1024 * 1024;

/// Absolute paths, so no `pbcopy` earlier on an inherited `PATH` (a `cargo tauri
/// dev` shell's) stands in for Apple's; both are SIP-protected.
const PBPASTE: &str = "/usr/bin/pbpaste";
const PBCOPY: &str = "/usr/bin/pbcopy";

/// How long a tool may take before it is killed: a wedged pasteboard server
/// must not hold a blocking thread (and Vi's `p`) forever.
const DEADLINE: Duration = Duration::from_secs(5);
/// How often the deadline loop looks whether the tool has exited.
const POLL: Duration = Duration::from_millis(5);

/// What went wrong talking to the pasteboard.
#[derive(Debug, thiserror::Error)]
pub enum ClipboardError {
    /// `pbcopy`/`pbpaste` could not be started, or their pipes failed.
    #[error("clipboard: {0}")]
    Io(#[from] io::Error),
    /// The tool ran but failed.
    #[error("clipboard: {tool} exited with {status}")]
    Failed { tool: &'static str, status: String },
    /// The tool did not finish within [`DEADLINE`] and was killed.
    #[error("clipboard: {tool} did not finish in time")]
    TimedOut { tool: &'static str },
    /// The text to copy is larger than [`MAX_CLIPBOARD_BYTES`].
    #[error("clipboard: {bytes} bytes is more than the clipboard takes")]
    TooLarge { bytes: usize },
}

fn command(tool: &'static str) -> Command {
    let mut cmd = Command::new(tool);
    cmd.env("LC_CTYPE", "UTF-8")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    cmd
}

/// Runs `io` against the child's pipes on a helper thread while this one waits
/// for the child, killing it at [`DEADLINE`]. The child is reaped on every path;
/// killing it closes its pipe ends, which unblocks `io`.
fn finish<T: Send>(
    tool: &'static str,
    mut child: Child,
    io: impl FnOnce() -> io::Result<T> + Send,
) -> Result<(T, ExitStatus), ClipboardError> {
    thread::scope(|scope| {
        let worker = scope.spawn(io);
        let deadline = Instant::now() + DEADLINE;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(ClipboardError::TimedOut { tool });
                }
                Err(err) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(err.into());
                }
            }
        };
        let io = worker
            .join()
            .expect("the clipboard pipe thread does not panic");
        let status = status?;
        Ok((io?, status))
    })
}

/// The pasteboard's text (empty when it holds none), cut at [`MAX_CLIPBOARD_BYTES`].
pub fn read_text() -> Result<String, ClipboardError> {
    let mut child = command(PBPASTE).stdout(Stdio::piped()).spawn()?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let (bytes, status) = finish("pbpaste", child, move || {
        let mut bytes = Vec::new();
        stdout
            .take(MAX_CLIPBOARD_BYTES as u64)
            .read_to_end(&mut bytes)?;
        // Dropping the pipe here ends a tool still writing past the cut (SIGPIPE).
        Ok(bytes)
    })?;
    // Killed by a signal is the cut above, not a failure.
    if !status.success() && bytes.is_empty() && status.code().is_some() {
        return Err(ClipboardError::Failed {
            tool: "pbpaste",
            status: status.to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Puts `text` on the pasteboard, replacing what was there.
pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    if text.len() > MAX_CLIPBOARD_BYTES {
        return Err(ClipboardError::TooLarge { bytes: text.len() });
    }
    let mut child = command(PBCOPY)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    // `stdin` is dropped at the end of the closure: the tool sees EOF and exits.
    let ((), status) = finish("pbcopy", child, move || stdin.write_all(text.as_bytes()))?;
    if !status.success() {
        return Err(ClipboardError::Failed {
            tool: "pbcopy",
            status: status.to_string(),
        });
    }
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// One test for the round trip: the pasteboard is global, so parallel
    /// tests touching it would race. Puts back what was there before.
    #[test]
    fn text_goes_through_the_pasteboard_and_back_with_umlauts_intact() {
        let before = read_text().unwrap_or_default();

        write_text("Grüße 👋\nzweite Zeile\n").unwrap();
        assert_eq!(read_text().unwrap(), "Grüße 👋\nzweite Zeile\n");

        // An empty pasteboard is a valid state (not an error), and reads back as "".
        write_text("").unwrap();
        assert_eq!(read_text().unwrap(), "");

        // Well under MAX_CLIPBOARD_BYTES, but large enough to exercise the piped read/write
        // rather than a single syscall's worth of bytes.
        let large: String = "Grüße 👋 ".repeat((1024 * 1024) / "Grüße 👋 ".len() + 1);
        write_text(&large).unwrap();
        assert_eq!(read_text().unwrap(), large);

        // More than the clipboard takes is refused before anything runs.
        let huge = "x".repeat(MAX_CLIPBOARD_BYTES + 1);
        assert!(matches!(
            write_text(&huge),
            Err(ClipboardError::TooLarge { .. })
        ));
        assert_eq!(read_text().unwrap(), large);

        write_text(&before).unwrap();
    }
}
