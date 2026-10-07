//! The login shell's `PATH` for an app that was not started from a shell.
//!
//! The Finder and the Dock start an app with `/usr/bin:/bin:/usr/sbin:/sbin`, which has neither `opencode`, `claude`
//! nor `git` from Homebrew, `cargo` nor `rust-analyzer`. Everything that starts a program by name (the Opencode
//! service, the agents' commands, the language servers) would then fail only in the installed app. The terminal panes
//! already run a login shell; this gives the app's own process the same `PATH`.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Wraps the printed `PATH`, so whatever a shell's start-up files print (a banner, a warning) cannot be mistaken for it.
const MARK: &str = "__AXIOMATA_PATH__";

/// An interactive shell may run a slow `.zshrc`; the app must not wait on it for ever and falls back to its own `PATH`.
const TIMEOUT: Duration = Duration::from_secs(5);

/// Makes the process's `PATH` the login shell's, followed by what it had besides.
///
/// Must run first thing in `run`, while no other thread exists: it changes the environment.
pub fn adopt() {
    let Some(login) = login_shell_path() else {
        // Tracing is not up yet at this point, so stderr is the only place to say it.
        eprintln!("axiomata: could not read the login shell's PATH; keeping the process's own");
        return;
    };
    let current = std::env::var_os("PATH").unwrap_or_default();
    let merged = merge(&login, &current);
    // SAFETY: called first thing in `run`, before tracing, Tauri or any other thread exists, so nothing can read the
    // environment while it changes — the condition `set_var`'s safety contract asks for.
    unsafe { std::env::set_var("PATH", merged) };
}

fn login_shell_path() -> Option<OsString> {
    let shell = std::env::var_os("SHELL")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into());
    // `-i` as well as `-l`: `PATH` is often extended in `.zshrc` (nvm, pyenv), not only in `.zprofile`.
    let mut child = Command::new(shell)
        .args([
            "-l",
            "-i",
            "-c",
            &format!("printf '%s' \"{MARK}${{PATH}}{MARK}\""),
        ])
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait().ok()? {
            Some(status) if status.success() => break,
            Some(_) => return None,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let output = child.wait_with_output().ok()?;
    extract(&String::from_utf8_lossy(&output.stdout)).map(OsString::from)
}

/// The text between the two marks.
fn extract(output: &str) -> Option<&str> {
    let start = output.find(MARK)? + MARK.len();
    let end = start + output[start..].find(MARK)?;
    let path = &output[start..end];
    (!path.is_empty()).then_some(path)
}

/// The login `PATH`'s directories first, then the ones only `current` has; each directory once.
fn merge(login: &OsString, current: &OsString) -> OsString {
    let mut seen = Vec::<PathBuf>::new();
    for dir in std::env::split_paths(login).chain(std::env::split_paths(current)) {
        if !dir.as_os_str().is_empty() && !seen.contains(&dir) {
            seen.push(dir);
        }
    }
    std::env::join_paths(seen).unwrap_or_else(|_| current.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_ignores_what_the_startup_files_print() {
        let out = format!("Welcome back\n{MARK}/opt/homebrew/bin:/usr/bin{MARK}");
        assert_eq!(extract(&out), Some("/opt/homebrew/bin:/usr/bin"));
    }

    #[test]
    fn extract_needs_both_marks_and_a_value() {
        assert_eq!(extract("no marks"), None);
        assert_eq!(extract(&format!("{MARK}/usr/bin")), None);
        assert_eq!(extract(&format!("{MARK}{MARK}")), None);
    }

    #[test]
    fn merge_puts_the_login_directories_first_and_keeps_the_rest_once() {
        let merged = merge(
            &"/opt/homebrew/bin:/usr/bin".into(),
            &"/usr/bin:/bin:/usr/bin".into(),
        );
        assert_eq!(merged, OsString::from("/opt/homebrew/bin:/usr/bin:/bin"));
    }

    #[test]
    fn merge_drops_empty_entries_which_would_mean_the_current_directory() {
        let merged = merge(&"/a::/b".into(), &"".into());
        assert_eq!(merged, OsString::from("/a:/b"));
    }
}
