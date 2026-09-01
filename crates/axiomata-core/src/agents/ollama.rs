//! Runs a prompt against a local Ollama model over its HTTP API.
//!
//! A single completion call (`POST /api/generate`, non-streaming) — no tools,
//! no agent loop. Talks to the default local daemon at `http://127.0.0.1:11434`
//! (making the host configurable is deferred to a later milestone, when
//! routines need it).
//!
//! Implemented in M1.

use std::sync::LazyLock;
use std::time::Instant;

use ollama_rs::Ollama;
use ollama_rs::generation::completion::request::GenerationRequest;
use tokio::time::timeout;

use super::{AgentRequest, AgentRunResult, BACKEND_OLLAMA};
use crate::error::AxiomataError;

/// Shared client for the local daemon, so successive runs reuse the connection
/// pool instead of opening a fresh TCP connection each time. Fine to be
/// process-global while the host is fixed; revisit if it becomes configurable.
static OLLAMA: LazyLock<Ollama> = LazyLock::new(Ollama::default);

/// Upper bound on how much of a completion is kept, mirroring the Claude Code
/// backend's output cap.
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// Truncates `text` to at most `max_bytes`, respecting UTF-8 char boundaries.
fn truncate_utf8(mut text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

/// Sends `request.prompt` to the local Ollama daemon as a single completion
/// call against `model`.
///
/// `request.cwd` and `request.env` are ignored — Ollama has no working
/// directory or child process. Unlike the Claude Code backend, an Ollama
/// failure (daemon down, unknown model, HTTP error) surfaces as `Err`, not as a
/// run result with a non-zero exit code; the caller records it as a failed run.
///
/// Args:
///     request: Supplies `prompt` and `timeout`; other fields are unused.
///     model: The Ollama model tag to generate with (e.g. `"llama3.2"`).
///
/// Returns:
///     The completion text as `stdout`, an `exit_code` of `0`, and the
///     wall-clock duration.
///
/// Errors:
///     [`AxiomataError::AgentTimeout`] if generation exceeds `request.timeout`;
///     [`AxiomataError::AgentApi`] if the Ollama API call itself fails.
pub async fn run(request: AgentRequest, model: &str) -> Result<AgentRunResult, AxiomataError> {
    let started = Instant::now();
    let generation = GenerationRequest::new(model.to_owned(), request.prompt);

    let response = match timeout(request.timeout, OLLAMA.generate(generation)).await {
        Ok(Ok(response)) => response,
        Ok(Err(source)) => {
            return Err(AxiomataError::AgentApi {
                backend: BACKEND_OLLAMA,
                message: source.to_string(),
            });
        }
        Err(_elapsed) => {
            return Err(AxiomataError::AgentTimeout {
                backend: BACKEND_OLLAMA,
                timeout: request.timeout,
            });
        }
    };

    Ok(AgentRunResult {
        stdout: truncate_utf8(response.response, MAX_RESPONSE_BYTES),
        stderr: String::new(),
        exit_code: 0,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}
