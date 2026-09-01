//! Runs a prompt headlessly through the Claude Code CLI (`claude -p`).
//!
//! Implemented in M1.

use std::process::Stdio;
use std::time::Instant;

use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use super::{AgentRequest, AgentRunResult, BACKEND_CLAUDE_CODE};
use crate::error::AxiomataError;

/// Name of the Claude Code binary; expected on `PATH`.
const CLAUDE_BIN: &str = "claude";

/// Upper bound on how many bytes of the child's stdout (and, separately,
/// stderr) are captured. A runaway or hostile child cannot balloon memory past
/// this within the timeout window; output beyond it is discarded.
const MAX_CAPTURE_BYTES: u64 = 1024 * 1024;

/// Spawns `claude -p "<prompt>"` in `request.cwd`, applying `request.env` and
/// enforcing `request.timeout`.
///
/// stdin is closed so the CLI never blocks on interactive input; stdout and
/// stderr are drained concurrently with the process wait, so a chatty child
/// cannot deadlock on a full pipe buffer. On timeout the child is signalled and
/// reaped before [`AxiomataError::AgentTimeout`] is returned, so the process is
/// gone (not merely detached) by the time the caller sees the error. A process
/// that runs to completion — even with a non-zero exit code — yields `Ok`, with
/// the exit code recorded in the result.
///
/// Args:
///     request: Prompt, working directory, timeout, and extra environment.
///
/// Returns:
///     The captured stdout, stderr, exit code, and wall-clock duration.
///
/// Errors:
///     [`AxiomataError::AgentSpawn`] if the binary cannot be spawned or waited
///     on; [`AxiomataError::AgentTimeout`] if it exceeds `request.timeout`.
pub async fn run(request: AgentRequest) -> Result<AgentRunResult, AxiomataError> {
    let started = Instant::now();

    let mut command = Command::new(CLAUDE_BIN);
    command
        .arg("-p")
        .arg(&request.prompt)
        .current_dir(&request.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (key, value) in &request.env {
        command.env(key, value);
    }

    let spawn_err = |source: std::io::Error| AxiomataError::AgentSpawn {
        backend: BACKEND_CLAUDE_CODE,
        program: CLAUDE_BIN,
        source,
    };

    let mut child = command.spawn().map_err(spawn_err)?;
    let mut stdout_pipe = child
        .stdout
        .take()
        .expect("stdout was configured as a pipe")
        .take(MAX_CAPTURE_BYTES);
    let mut stderr_pipe = child
        .stderr
        .take()
        .expect("stderr was configured as a pipe")
        .take(MAX_CAPTURE_BYTES);

    let collect = async {
        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let (status, _, _) = tokio::try_join!(
            child.wait(),
            stdout_pipe.read_to_end(&mut stdout_buf),
            stderr_pipe.read_to_end(&mut stderr_buf),
        )?;
        Ok::<_, std::io::Error>((status, stdout_buf, stderr_buf))
    };

    // Bind the timeout outcome before matching so the `collect` future (which
    // holds `&mut child`) is dropped and the borrow released before the timeout
    // arm touches `child` again.
    let outcome = timeout(request.timeout, collect).await;
    let (status, stdout_buf, stderr_buf) = match outcome {
        Ok(result) => result.map_err(spawn_err)?,
        Err(_elapsed) => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            return Err(AxiomataError::AgentTimeout {
                backend: BACKEND_CLAUDE_CODE,
                timeout: request.timeout,
            });
        }
    };

    Ok(AgentRunResult {
        stdout: into_string_lossy(stdout_buf),
        stderr: into_string_lossy(stderr_buf),
        exit_code: status.code().unwrap_or(-1),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

/// Converts captured child output to a `String`, reusing the buffer directly
/// when it is already valid UTF-8 (the common case) and only allocating a
/// replacement string on the lossy path.
fn into_string_lossy(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
    }
}
