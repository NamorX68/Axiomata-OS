//! Running `git` — the one place that does (`docs/plans/editor-projekt-werkzeuge.md`, #48).
//!
//! **Driven by the `git` command line, not `git2`/libgit2**: the user's own configuration
//! applies for free (credential helpers, `includeIf`, hooks, `core.hooksPath`), `git worktree`
//! is the reference implementation, and there is no C dependency. The price is a runtime
//! dependency on `git` being installed.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::{GitError, Result};

/// Runs `git` in `repo`, returning stdout on success.
///
/// Every git call in the crate goes through here (or [`git_with`]) so that a
/// failure always carries the command that failed and git's own stderr — the
/// two things that make a git error readable.
pub fn git(repo: &Path, args: &[&str]) -> Result<String> {
    git_with(repo, args, &[0]).map(|(_, stdout)| stdout)
}

/// Like [`git`], but any exit code in `ok` counts as success, and the code is
/// returned with stdout. Needed where git reports an answer through its exit
/// code: `diff --no-index` exits 1 when the files differ, `diff --quiet`
/// exits 1 when there is something to report.
///
/// Always runs with two variables set:
///
/// * `GIT_OPTIONAL_LOCKS=0` — the IDE reads a worktree while the agent in it
///   runs git itself, and a read that briefly takes the index lock (as
///   `git status` does by default) can make the agent's own commit fail with
///   `index.lock: File exists`. It only drops *opportunistic* locks; the
///   index and `HEAD` locks that `commit`, `merge` and `restore` take for
///   correctness stay, so it is safe on the project folder's writes too.
/// * `GIT_LITERAL_PATHSPECS=1` — every path this crate hands git is a file
///   name, never a pattern. Without it a file the agent called `:(glob)**`
///   turns "discard this file" into "discard everything that matches", and a
///   `--` in front does not prevent that (security review, M7.3 CP7).
pub fn git_with(repo: &Path, args: &[&str], ok: &[i32]) -> Result<(i32, String)> {
    git_raw(repo, args, ok, None)
        .map(|(code, stdout)| (code, String::from_utf8_lossy(&stdout).into_owned()))
}

/// Like [`git`], but stdout comes back as it is — for a blob, which may be
/// binary (an image the agent changed, M7.3 H8).
pub fn git_bytes(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    git_raw(repo, args, &[0], None).map(|(_, stdout)| stdout)
}

/// Like [`git`], with `input` written to git's stdin — a patch for `git apply`
/// (M7.3 H6), which this crate never writes into a worktree as a file.
pub fn git_with_input(repo: &Path, args: &[&str], input: &[u8]) -> Result<String> {
    git_raw(repo, args, &[0], Some(input))
        .map(|(_, stdout)| String::from_utf8_lossy(&stdout).into_owned())
}

/// The one place that runs git; see [`git_with`] for the environment it sets.
fn git_raw(repo: &Path, args: &[&str], ok: &[i32], input: Option<&[u8]>) -> Result<(i32, Vec<u8>)> {
    let command_line = || format!("git {}", args.join(" "));
    let could_not_run = |err: std::io::Error| GitError::Git {
        command: command_line(),
        reason: format!("could not run git: {err}"),
    };
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    let mut child = command.spawn().map_err(could_not_run)?;
    let output = match (input, child.stdin.take()) {
        // Written on its own thread while this one drains stdout and stderr:
        // a hunk can be up to 2 MiB, far past a pipe's buffer, and a git that
        // writes before it has read everything would otherwise wait on us
        // while we wait on it (security review, CP9).
        (Some(input), Some(mut stdin)) => std::thread::scope(|scope| {
            let writer = scope.spawn(move || stdin.write_all(input));
            let output = child.wait_with_output();
            // A write error is git having stopped reading; its exit status
            // and stderr below say why.
            let _ = writer.join();
            output
        }),
        _ => child.wait_with_output(),
    }
    .map_err(could_not_run)?;

    let code = output.status.code().unwrap_or(-1);
    if !ok.contains(&code) {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitError::Git {
            command: command_line(),
            reason: if stderr.is_empty() {
                format!("exited with {}", output.status)
            } else {
                stderr
            },
        });
    }
    Ok((code, output.stdout))
}

/// Checks that `git` can be run at all, so a missing tool is reported once and clearly.
pub fn ensure_available() -> Result<String> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .map_err(|err| GitError::Git {
            command: "git --version".into(),
            reason: format!("git does not appear to be installed: {err}"),
        })?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
