//! Runs a prompt headlessly through the Claude Code CLI (`claude -p`).
//!
//! Two entry points share one process harness ([`spawn_and_collect`]):
//! [`run`] (M1 — skills and routines, plain text out) and [`chat`] (M5 — the
//! dashboard's assistant bar, `--output-format json`, multi-turn via
//! `--resume <session_id>`).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

use super::{AgentRequest, AgentRunResult, BACKEND_CLAUDE_CODE};
use crate::error::AxiomataError;

/// Name of the Claude Code binary; expected on `PATH`.
const CLAUDE_BIN: &str = "claude";

/// [`resolve_claude_binary`]'s result, cached for the life of the process —
/// resolved once, reused by every skill run, routine fire, and chat turn.
static CLAUDE_PATH: OnceLock<Result<PathBuf, String>> = OnceLock::new();

/// Resolves [`CLAUDE_BIN`] to an absolute path by walking `PATH` ourselves,
/// rather than handing the bare name to [`Command::new`] and letting the OS
/// loader search `PATH` implicitly on every single spawn. Two benefits: the
/// resolved path is pinned for the process's lifetime (no ambiguity from a
/// `PATH` that some other process mutates between calls), and a missing
/// binary becomes one clear error the moment it's first needed instead of a
/// generic spawn failure surfacing on whatever skill or routine happens to
/// fire first.
///
/// Errors:
///     [`AxiomataError::AgentSpawn`] if no executable named [`CLAUDE_BIN`]
///     exists on any `PATH` entry.
fn resolve_claude_binary() -> Result<&'static Path, AxiomataError> {
    let cached = CLAUDE_PATH.get_or_init(|| {
        std::env::var_os("PATH")
            .and_then(|path_var| {
                std::env::split_paths(&path_var).find_map(|dir| {
                    let candidate = dir.join(CLAUDE_BIN);
                    is_executable_file(&candidate).then_some(candidate)
                })
            })
            .ok_or_else(|| format!("{CLAUDE_BIN} not found on PATH"))
    });
    cached
        .as_deref()
        .map_err(|message| AxiomataError::AgentSpawn {
            backend: BACKEND_CLAUDE_CODE,
            program: CLAUDE_BIN,
            source: std::io::Error::new(std::io::ErrorKind::NotFound, message.clone()),
        })
}

/// Whether `path` is a regular file with at least one executable bit set
/// (Unix) — on other platforms, existence as a file is all we can cheaply
/// check, so any regular file counts.
#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// Upper bound on how many bytes of the child's stdout (and, separately,
/// stderr) are captured. A runaway or hostile child cannot balloon memory past
/// this within the timeout window; output beyond it is discarded.
const MAX_CAPTURE_BYTES: u64 = 1024 * 1024;

/// Caps how many `claude` child processes may be running at once, across
/// *every* caller — a manual "run now" click, a routine firing, and a chat
/// turn all funnel through [`spawn_and_collect`]. Without this, several due
/// routines firing in the same tick (see [`crate::routines::scheduler::tick`],
/// which fires them concurrently) plus a stray UI click could start
/// unboundedly many `claude` processes at once. A modest, fixed cap rather
/// than a config knob: this is a resource-safety floor, not a tuning surface.
const MAX_CONCURRENT_AGENT_RUNS: usize = 4;

/// The semaphore [`MAX_CONCURRENT_AGENT_RUNS`] is enforced through, created
/// once and shared for the life of the process.
///
/// A process-wide `static` rather than a field on `AxiomataCore` — unlike
/// `CLAUDE_PATH` above (also process-wide, but caching an immutable fact:
/// where the binary lives), this one holds contended, mutable-in-effect
/// state. It's sound only because exactly one `AxiomataCore` is ever
/// constructed per process today (the CLI is one-shot; the Tauri app is a
/// single instance) — every real `spawn_and_collect` caller in the running
/// app shares this one instance either way. The place this would bite: any
/// test that spawns a *real* `claude` process shares this same 4-slot budget
/// with every other test in the same binary running concurrently (`cargo
/// test`'s default), and a hypothetical second `AxiomataCore` in one process
/// (headless multi-tenant, say) would silently share it too. No current test
/// does the former (see `agent_slots_caps_concurrent_permits`'s comment) and
/// nothing in this codebase does the latter, but if either changes, this is
/// the thing to revisit.
static AGENT_SLOTS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();

fn agent_slots() -> &'static tokio::sync::Semaphore {
    AGENT_SLOTS.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_AGENT_RUNS))
}

/// Spawns `claude -p` in `request.cwd`, enforcing `request.timeout`. The
/// child's environment is *not* inherited wholesale: it starts empty and is
/// repopulated from a fixed allowlist plus `request.env` (see [`child_env`]),
/// so an ambient `ANTHROPIC_*` export can never leak in.
///
/// The prompt is written to the child's **stdin**, not passed as a command-line
/// argument: `claude` reads its prompt from stdin in print mode when given no
/// positional prompt, and this keeps a prompt that begins with `-` from being
/// parsed as a `claude` flag (the routine scheduler fires this path unattended).
/// stdin is closed right after the prompt so the CLI never blocks on further
/// input; stdout and stderr are drained concurrently with the process wait and
/// the stdin write, so nothing can deadlock on a full pipe buffer. On timeout
/// the child is signalled and reaped before [`AxiomataError::AgentTimeout`] is
/// returned, so the process is gone (not merely detached) by the time the
/// caller sees the error. A process that runs to completion — even with a
/// non-zero exit code — yields `Ok`, with the exit code recorded in the result.
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
    let raw = spawn_and_collect(
        request,
        &["--output-format".to_string(), "json".to_string()],
    )
    .await?;
    Ok(parse_run_output(raw))
}

/// What `claude -p --output-format json` prints for a one-shot (non-chat) run.
/// Every field is optional/defaulted: an older CLI, a crash, or a provider
/// that doesn't fill the envelope must degrade to "no cost data", never a
/// hard error on a run that actually produced output.
#[derive(Debug, Deserialize)]
struct RawRunResult {
    #[serde(default)]
    result: String,
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    total_cost_usd: Option<f64>,
    #[serde(default)]
    usage: Option<serde_json::Value>,
    #[serde(default)]
    num_turns: Option<u32>,
}

/// Unwraps the `--output-format json` envelope produced by [`run`] into an
/// [`AgentRunResult`]: `stdout` becomes the inner `result` string (identical
/// to what the old plain-text mode returned), and the cost/token fields are
/// lifted out of the envelope.
///
/// If the captured stdout is not the expected JSON — an older CLI, a spawn
/// that printed a diagnostic instead, a truncated capture — the raw output is
/// passed straight through unchanged, with no cost data. A well-formed
/// envelope whose `is_error` is set is forced to a non-zero `exit_code` so
/// the run is recorded as `Failed` (plain-text mode couldn't see this: an
/// error *reply* still exits 0).
fn parse_run_output(raw: AgentRunResult) -> AgentRunResult {
    let Ok(parsed) = serde_json::from_str::<RawRunResult>(raw.stdout.trim()) else {
        return raw;
    };
    let (input_tokens, output_tokens) = usage_tokens(parsed.usage.as_ref());
    let exit_code = if parsed.is_error && raw.exit_code == 0 {
        1
    } else {
        raw.exit_code
    };
    AgentRunResult {
        stdout: parsed.result,
        stderr: raw.stderr,
        exit_code,
        duration_ms: raw.duration_ms,
        // The subscription-billed Anthropic path reports 0.0; treat that as
        // "nothing to meter" so a $0 row doesn't imply a paid call happened.
        cost_usd: parsed.total_cost_usd.filter(|c| *c > 0.0),
        input_tokens,
        output_tokens,
        num_turns: parsed.num_turns,
    }
}

/// Pulls `input_tokens` / `output_tokens` out of the CLI's `usage` object,
/// tolerating its absence or an unexpected shape.
fn usage_tokens(usage: Option<&serde_json::Value>) -> (Option<u64>, Option<u64>) {
    let get = |key: &str| {
        usage
            .and_then(|u| u.get(key))
            .and_then(serde_json::Value::as_u64)
    };
    (get("input_tokens"), get("output_tokens"))
}

/// How a chat turn may act: `Chat` never asks (read-mostly, `dontAsk`);
/// `Instruct` may edit workspace files unattended (`acceptEdits`) — used for
/// one-shot `/` instructions like "add X to my todo list".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatMode {
    Chat,
    Instruct,
}

impl ChatMode {
    fn permission_mode(self) -> &'static str {
        match self {
            ChatMode::Chat => "dontAsk",
            ChatMode::Instruct => "acceptEdits",
        }
    }

    /// The token stored in `chat_turns.mode`.
    pub fn as_log_str(self) -> &'static str {
        match self {
            ChatMode::Chat => "chat",
            ChatMode::Instruct => "instruct",
        }
    }
}

/// One turn of the dashboard assistant.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub message: String,
    /// A `session_id` returned by an earlier turn; `None` starts a session.
    pub session_id: Option<String>,
    pub mode: ChatMode,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub env: Vec<(String, String)>,
    /// Appended to the system prompt (`--append-system-prompt-file`) — the
    /// module manifest written by the dashboard, when it exists.
    pub system_prompt_file: Option<PathBuf>,
    /// `claude --model`; `None` lets the CLI choose.
    pub model: Option<String>,
    /// `claude --allowedTools` — see [`super::AgentRequest::allowed_tools`].
    /// A module that needs an instruct turn to reach an MCP tool (e.g. a
    /// connector module's "create"/"delete" write action) sets this to
    /// exactly the tool it needs; plain chat/instruct turns from the
    /// assistant bar leave it `None`.
    pub allowed_tools: Option<String>,
}

/// The parsed `--output-format json` result of a chat turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatReply {
    pub session_id: String,
    pub reply_markdown: String,
    pub is_error: bool,
    pub cost_usd: Option<f64>,
    pub usage: Option<serde_json::Value>,
    /// `usage.input_tokens` / `usage.output_tokens`, lifted out for the
    /// `chat_turns` spend log; `None` when the envelope didn't carry them.
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    /// `num_turns` from the envelope.
    pub num_turns: Option<u32>,
    pub duration_ms: u64,
}

/// What `claude -p --output-format json` prints (the fields we read).
#[derive(Debug, Deserialize)]
struct RawChatResult {
    #[serde(default)]
    result: String,
    session_id: String,
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    total_cost_usd: Option<f64>,
    #[serde(default)]
    usage: Option<serde_json::Value>,
    #[serde(default)]
    num_turns: Option<u32>,
}

/// Runs one assistant turn: `claude -p --output-format json` with the
/// message on stdin, `--resume` for a follow-up, and the permission mode
/// implied by [`ChatMode`].
///
/// Errors:
///     [`AxiomataError::AgentApi`] if the session id is malformed, the CLI
///     exits non-zero, or its output is not the expected JSON; spawn/timeout
///     errors as for [`run`].
pub async fn chat(request: ChatRequest) -> Result<ChatReply, AxiomataError> {
    let args = chat_args(&request)?;
    let result = spawn_and_collect(
        AgentRequest {
            prompt: request.message,
            cwd: request.cwd,
            timeout: request.timeout,
            env: request.env,
            system_prompt_file: request.system_prompt_file,
            model: request.model,
            allowed_tools: request.allowed_tools,
        },
        &args,
    )
    .await?;
    parse_chat_output(&result)
}

/// Model names come from config / skill frontmatter; keep them to the alias
/// and id alphabet so they can never read as another flag.
pub fn valid_model_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && name.bytes().all(|b| {
            // `/` is needed for `vendor/model` IDs (every OpenRouter model,
            // e.g. `z-ai/glm-5.3-flash`). It's flag-safe: the value is passed
            // as a single `Command::arg` token (no shell), the first byte is
            // already forced to be alphanumeric so it can't start a `-` flag,
            // and `claude --model` only forwards it as the API `model` string
            // — never touches the filesystem with it.
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'[' | b']' | b'/')
        })
}

/// Decides what to pass to `claude --model`, and — critically — when to
/// *refuse to spawn at all* rather than let the CLI fall back to its own
/// built-in default model.
///
/// `redirected` is true when `ANTHROPIC_BASE_URL` is set for this run, i.e. a
/// non-Anthropic provider is pointing the CLI at a paid third-party endpoint.
/// In that state the CLI's default model would be billed through the proxy,
/// so a missing or malformed model is a hard configuration error, not a
/// silently-dropped flag (that gap turned a one-character config typo into a
/// real OpenRouter bill once — see `docs/plans/provider-hardening.md`).
///
/// When the run is *not* redirected (Anthropic via the CLI's own
/// subscription login), a missing model is fine: the CLI's default is the
/// intended behaviour and costs nothing extra.
fn resolve_model_arg(model: Option<&str>, redirected: bool) -> Result<Option<&str>, AxiomataError> {
    match model.map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) if valid_model_name(m) => Ok(Some(m)),
        Some(m) => Err(AxiomataError::InvalidAgentModel {
            reason: format!(
                "the configured model {m:?} is not a valid model id \
                 (allowed characters: letters, digits, and - _ . : [ ] /)"
            ),
        }),
        None if redirected => Err(AxiomataError::InvalidAgentModel {
            reason: "the active model provider redirects the Claude CLI \
                     (ANTHROPIC_BASE_URL is set) but no model is configured for \
                     it — set that provider's chat/skill model so the CLI's \
                     built-in default model isn't billed through the proxy"
                .to_string(),
        }),
        None => Ok(None),
    }
}

/// `--allowedTools` values come from `SKILL.md` frontmatter, same trust
/// boundary as `valid_model_name` — not shell-parsed (no injection risk
/// either way), but a value that could read as another `claude` flag would
/// still misbehave silently, so it gets the same narrow-alphabet treatment.
/// Wide enough for real tool specs: MCP's `mcp__server__tool` naming, plain
/// tool names, and scoped specs like `Bash(git *)`, space/comma-separated.
pub fn valid_allowed_tools(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4000
        && value
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'-' | b'_' | b',' | b' ' | b'(' | b')' | b'*' | b'/' | b':' | b'.'
                )
        })
}

/// A session id is only ever something `claude` printed earlier; refuse
/// anything that could read as a flag or shell noise.
fn valid_session_id(id: &str) -> bool {
    let starts_alnum = id.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric());
    starts_alnum
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The argument list after `-p` for a chat turn.
fn chat_args(request: &ChatRequest) -> Result<Vec<String>, AxiomataError> {
    let mut args = vec![
        "--output-format".to_string(),
        "json".to_string(),
        "--permission-mode".to_string(),
        request.mode.permission_mode().to_string(),
    ];
    if let Some(id) = &request.session_id {
        if !valid_session_id(id) {
            return Err(AxiomataError::AgentApi {
                backend: BACKEND_CLAUDE_CODE,
                message: "malformed session id".to_string(),
            });
        }
        args.push("--resume".to_string());
        args.push(id.clone());
    }
    Ok(args)
}

/// Turns the captured process output into a [`ChatReply`].
fn parse_chat_output(result: &AgentRunResult) -> Result<ChatReply, AxiomataError> {
    let api_err = |message: String| AxiomataError::AgentApi {
        backend: BACKEND_CLAUDE_CODE,
        message,
    };
    if !result.is_success() {
        let detail = if result.stderr.trim().is_empty() {
            result.stdout.trim()
        } else {
            result.stderr.trim()
        };
        return Err(api_err(format!(
            "claude exited with code {}: {}",
            result.exit_code,
            tail(detail, 600)
        )));
    }
    let raw: RawChatResult = serde_json::from_str(result.stdout.trim()).map_err(|err| {
        api_err(format!(
            "could not parse claude's JSON output ({err}): {}",
            tail(result.stdout.trim(), 300)
        ))
    })?;
    if !valid_session_id(&raw.session_id) {
        return Err(api_err(
            "claude returned a malformed session id".to_string(),
        ));
    }
    let (input_tokens, output_tokens) = usage_tokens(raw.usage.as_ref());
    Ok(ChatReply {
        session_id: raw.session_id,
        reply_markdown: raw.result,
        is_error: raw.is_error,
        cost_usd: raw.total_cost_usd.filter(|c| *c > 0.0),
        usage: raw.usage,
        input_tokens,
        output_tokens,
        num_turns: raw.num_turns,
        duration_ms: result.duration_ms,
    })
}

/// The last `max` bytes of `text` on a char boundary.
fn tail(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut start = text.len() - max;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

/// Process-environment variables inherited by a `claude` child by exact
/// name. The CLI needs a working process environment — `PATH` to find `node`
/// and the tools it shells out to, `HOME` to locate its own `~/.claude`,
/// locale for correct text handling — but must **not** inherit an ambient
/// `ANTHROPIC_*` / `CLAUDE_CODE_*` export from the shell Axiomata itself was
/// launched from. That leakage is exactly what made "does launching from my
/// GLM shell change billing?" a real question (`docs/plans/provider-hardening.md`
/// checkpoint 3). Everything the child gets beyond this list comes through
/// `request.env` (provider routing, the Bedrock escape hatch), applied last.
const INHERITED_ENV_ALLOWLIST: &[&str] =
    &["PATH", "HOME", "USER", "SHELL", "TERM", "LANG", "TMPDIR"];

/// Prefixes whose every matching process-environment variable is inherited:
/// `LC_*` (locale categories), `XDG_*` (base-dir spec), `SSL_CERT_*` (OpenSSL
/// trust-store overrides), and `__CF*` (the CoreFoundation vars macOS injects,
/// e.g. `__CF_USER_TEXT_ENCODING`; absent on other platforms).
const INHERITED_ENV_PREFIXES: &[&str] = &["LC_", "XDG_", "SSL_CERT_", "__CF"];

/// Assembles the complete environment for a `claude` child, to be applied on
/// top of a [`Command::env_clear`]: the allowlisted subset of this process's
/// environment first, then `request_env` layered over it so a provider's
/// `ANTHROPIC_BASE_URL` / token always wins and a caller can still override an
/// inherited key (`PATH`, say) on purpose.
fn child_env(request_env: &[(String, String)]) -> Vec<(String, String)> {
    child_env_from(std::env::vars(), request_env)
}

/// [`child_env`] with the ambient environment injected, so it is unit-testable
/// without touching the real process environment.
fn child_env_from<I>(ambient: I, request_env: &[(String, String)]) -> Vec<(String, String)>
where
    I: IntoIterator<Item = (String, String)>,
{
    let inherited = |key: &str| {
        INHERITED_ENV_ALLOWLIST.contains(&key)
            || INHERITED_ENV_PREFIXES.iter().any(|p| key.starts_with(p))
    };
    let mut env: Vec<(String, String)> = ambient
        .into_iter()
        .filter(|(key, _)| inherited(key))
        .collect();
    for (key, value) in request_env {
        // `request.env` wins over anything inherited under the same key.
        env.retain(|(k, _)| k != key);
        env.push((key.clone(), value.clone()));
    }
    env
}

/// The shared harness: spawns `claude -p <extra_args…>` in `request.cwd` and
/// feeds `request.prompt` on stdin. See [`run`] for the I/O and timeout
/// guarantees.
async fn spawn_and_collect(
    request: AgentRequest,
    extra_args: &[String],
) -> Result<AgentRunResult, AxiomataError> {
    // Validate the model *before* taking a concurrency slot or starting the
    // clock: a config error here must fail loudly and cheaply, never spawn.
    let redirected = request
        .env
        .iter()
        .any(|(key, value)| key == "ANTHROPIC_BASE_URL" && !value.trim().is_empty());
    let model_arg = resolve_model_arg(request.model.as_deref(), redirected)?;

    // Held until this call returns, so at most `MAX_CONCURRENT_AGENT_RUNS`
    // `claude` processes are ever running at once; queued callers simply wait
    // here rather than piling up spawned processes. Acquired before the
    // timer starts, so queueing time isn't counted as this run's duration.
    let _permit = agent_slots()
        .acquire()
        .await
        .expect("agent_slots semaphore is never closed");

    let started = Instant::now();

    let mut command = Command::new(resolve_claude_binary()?);
    command.arg("-p").args(extra_args);
    if let Some(model) = model_arg {
        command.arg("--model").arg(model);
    }
    if let Some(file) = &request.system_prompt_file {
        command.arg("--append-system-prompt-file").arg(file);
    }
    if let Some(tools) = request
        .allowed_tools
        .as_deref()
        .filter(|t| valid_allowed_tools(t))
    {
        command.arg("--allowedTools").arg(tools);
    }
    command
        .current_dir(&request.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        // Start from nothing, not the launching shell's environment: an
        // ambient `ANTHROPIC_*` / `CLAUDE_CODE_*` export must never reach the
        // child except through `request.env`. See `child_env`.
        .env_clear();
    for (key, value) in child_env(&request.env) {
        command.env(key, value);
    }

    let spawn_err = |source: std::io::Error| AxiomataError::AgentSpawn {
        backend: BACKEND_CLAUDE_CODE,
        program: CLAUDE_BIN,
        source,
    };

    let mut child = command.spawn().map_err(spawn_err)?;
    let mut stdin_pipe = child.stdin.take().expect("stdin was configured as a pipe");
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

    let prompt_bytes = request.prompt.clone().into_bytes();
    let feed_stdin = async move {
        stdin_pipe.write_all(&prompt_bytes).await?;
        // Close stdin so `claude` sees EOF and stops waiting for more input.
        stdin_pipe.shutdown().await?;
        Ok::<(), std::io::Error>(())
    };

    let collect = async {
        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let (status, _, _, _) = tokio::try_join!(
            child.wait(),
            feed_stdin,
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

    Ok(AgentRunResult::bare(
        strip_ansi(&into_string_lossy(stdout_buf)),
        strip_ansi(&into_string_lossy(stderr_buf)),
        status.code().unwrap_or(-1),
        started.elapsed().as_millis() as u64,
    ))
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

/// Strips ANSI/VT100 escape sequences (colour, cursor movement, terminal
/// title-setting, …) from captured child output. `claude -p`'s plain-text
/// mode is not guaranteed to detect a non-terminal target and suppress them,
/// and once output is stored in a run record or shown in a non-terminal UI
/// panel (the Skills Deck tile, `axiomata-cli get-run --json`) a raw escape
/// code is just noise. Recognises CSI sequences (`ESC '[' … <letter>`, e.g.
/// colour codes) and OSC sequences (`ESC ']' … (BEL | ESC '\')`, e.g. a
/// terminal title); any other escape is dropped on its own so one stray
/// `ESC` byte can't swallow the rest of the output.
///
/// Args:
///     input: Text as captured from the child's stdout or stderr.
///
/// Returns:
///     The same text with every recognised escape sequence removed.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('[') => {
                chars.next(); // consume '['
                // CSI: parameter/intermediate bytes, terminated by a byte
                // in the 0x40..=0x7E range (here, any ASCII letter or one
                // of the less common terminator symbols).
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() || "@{|}~".contains(c) {
                        break;
                    }
                }
            }
            Some(']') => {
                chars.next(); // consume ']'
                // OSC: runs until BEL, or ESC '\' (String Terminator).
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {
                // Unrecognised or truncated escape — drop just the ESC byte.
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_ansi_removes_csi_colour_codes() {
        assert_eq!(strip_ansi("\u{1b}[31mred\u{1b}[0m"), "red");
        assert_eq!(
            strip_ansi("\u{1b}[1;32mbold green\u{1b}[0m normal"),
            "bold green normal"
        );
    }

    #[test]
    fn strip_ansi_removes_osc_sequences_terminated_by_bel_or_st() {
        assert_eq!(strip_ansi("\u{1b}]0;title\u{7}rest"), "rest");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{1b}\\rest"), "rest");
    }

    #[test]
    fn strip_ansi_drops_a_trailing_truncated_escape() {
        assert_eq!(strip_ansi("trailing\u{1b}"), "trailing");
    }

    #[test]
    fn strip_ansi_leaves_plain_text_untouched() {
        assert_eq!(
            strip_ansi("no escapes here\nsecond line"),
            "no escapes here\nsecond line"
        );
    }

    #[test]
    fn agent_slots_caps_concurrent_permits_and_blocks_once_exhausted() {
        // No test in this module spawns a real `claude` process (they all
        // exercise arg-building/parsing/path-resolution), so the shared
        // process-wide semaphore is otherwise untouched here. Kept as one
        // test (rather than split across two `#[test]` functions) so nothing
        // else running in parallel can touch this same static in between the
        // steps below.
        let sem = agent_slots();
        let starting = sem.available_permits();
        assert!(starting <= MAX_CONCURRENT_AGENT_RUNS);
        let permit = sem.try_acquire().expect("a permit should be available");
        assert_eq!(sem.available_permits(), starting - 1);
        drop(permit);
        assert_eq!(sem.available_permits(), starting);

        // Drain every permit currently available (not necessarily `starting`
        // — a concurrently-running test elsewhere in this binary could have
        // one checked out right now) so this part is self-contained: once
        // exhausted, a further acquire must be refused outright, not queued
        // and silently granted anyway.
        let mut held = Vec::new();
        while let Ok(permit) = sem.try_acquire() {
            held.push(permit);
        }
        assert_eq!(sem.available_permits(), 0);
        assert!(matches!(
            sem.try_acquire(),
            Err(tokio::sync::TryAcquireError::NoPermits)
        ));

        // Releasing exactly one permit is what actually unblocks the next
        // caller — not merely a bookkeeping decrement.
        held.pop();
        assert!(sem.try_acquire().is_ok());
    }

    #[test]
    fn resolve_claude_binary_finds_something_on_this_machines_path() {
        // Not a mock: this environment does have a real `claude` on PATH
        // (every other integration test in this crate that hits the real
        // CLI depends on it too). Asserts the resolved path is absolute and
        // actually named `claude`, and that the result is cached (same path
        // on a second call).
        let first = resolve_claude_binary().expect("claude should be on PATH in this environment");
        assert!(first.is_absolute());
        assert_eq!(first.file_name().and_then(|n| n.to_str()), Some(CLAUDE_BIN));
        let second = resolve_claude_binary().expect("cached result should still resolve");
        assert_eq!(first, second);
    }

    fn request(session: Option<&str>, mode: ChatMode) -> ChatRequest {
        ChatRequest {
            message: "hi".to_string(),
            session_id: session.map(str::to_string),
            mode,
            cwd: PathBuf::from("."),
            timeout: Duration::from_secs(1),
            env: Vec::new(),
            system_prompt_file: None,
            model: None,
            allowed_tools: None,
        }
    }

    fn output(stdout: &str, exit_code: i32) -> AgentRunResult {
        AgentRunResult::bare(stdout.to_string(), String::new(), exit_code, 7)
    }

    #[test]
    fn first_turn_args_use_json_output_and_dont_ask() {
        let args = chat_args(&request(None, ChatMode::Chat)).unwrap();
        assert_eq!(
            args,
            ["--output-format", "json", "--permission-mode", "dontAsk"]
        );
    }

    #[test]
    fn follow_up_resumes_and_instruct_accepts_edits() {
        let req = request(Some("abc-123_X"), ChatMode::Instruct);
        let args = chat_args(&req).unwrap();
        assert_eq!(
            args,
            [
                "--output-format",
                "json",
                "--permission-mode",
                "acceptEdits",
                "--resume",
                "abc-123_X",
            ]
        );
    }

    #[test]
    fn malformed_session_ids_are_refused() {
        for bad in ["", "--dangerous", "a b", "x;rm", &"a".repeat(129)] {
            let err = chat_args(&request(Some(bad), ChatMode::Chat)).unwrap_err();
            assert!(matches!(err, AxiomataError::AgentApi { .. }), "{bad:?}");
        }
    }

    #[test]
    fn parse_run_output_unwraps_the_envelope_and_lifts_cost_and_tokens() {
        let raw = output(
            r#"{"type":"result","is_error":false,"result":"digest text","session_id":"s-9",
                "total_cost_usd":0.0123,"num_turns":4,
                "usage":{"input_tokens":1200,"output_tokens":340,"cache_read_input_tokens":50}}"#,
            0,
        );
        let parsed = parse_run_output(raw);
        assert_eq!(parsed.stdout, "digest text");
        assert_eq!(parsed.exit_code, 0);
        assert_eq!(parsed.cost_usd, Some(0.0123));
        assert_eq!(parsed.input_tokens, Some(1200));
        assert_eq!(parsed.output_tokens, Some(340));
        assert_eq!(parsed.num_turns, Some(4));
    }

    #[test]
    fn parse_run_output_forces_failure_on_is_error_and_drops_zero_cost() {
        let raw = output(
            r#"{"is_error":true,"result":"the tool call was denied","total_cost_usd":0.0}"#,
            0,
        );
        let parsed = parse_run_output(raw);
        assert_eq!(parsed.stdout, "the tool call was denied");
        assert_ne!(parsed.exit_code, 0, "is_error must map to a non-zero exit");
        // A subscription-path 0.0 is "nothing to meter", not a recorded $0.
        assert_eq!(parsed.cost_usd, None);
    }

    #[test]
    fn parse_run_output_passes_non_json_straight_through() {
        // An older CLI, or a diagnostic printed instead of the envelope.
        let raw = output("plain text, not an envelope", 0);
        let parsed = parse_run_output(raw.clone());
        assert_eq!(parsed.stdout, raw.stdout);
        assert_eq!(parsed.cost_usd, None);
        assert_eq!(parsed.num_turns, None);
    }

    #[test]
    fn parses_a_json_result() {
        let out = output(
            r#"{"type":"result","subtype":"success","is_error":false,"result":"**hi**","session_id":"s-1","total_cost_usd":0.01,"usage":{"input_tokens":3}}"#,
            0,
        );
        let reply = parse_chat_output(&out).unwrap();
        assert_eq!(reply.session_id, "s-1");
        assert_eq!(reply.reply_markdown, "**hi**");
        assert!(!reply.is_error);
        assert_eq!(reply.cost_usd, Some(0.01));
        assert_eq!(reply.usage.unwrap()["input_tokens"], 3);
        assert_eq!(reply.duration_ms, 7);
    }

    #[test]
    fn non_zero_exit_and_bad_json_are_api_errors() {
        let err = parse_chat_output(&output("boom", 1)).unwrap_err();
        assert!(err.to_string().contains("code 1"), "{err}");
        let err = parse_chat_output(&output("not json", 0)).unwrap_err();
        assert!(err.to_string().contains("parse"), "{err}");
        let err =
            parse_chat_output(&output(r#"{"result":"x","session_id":"bad id"}"#, 0)).unwrap_err();
        assert!(err.to_string().contains("session id"), "{err}");
    }

    #[test]
    fn model_names_are_validated() {
        for ok in [
            "claude-sonnet-5",
            "sonnet",
            "claude-opus-4-1[1m]",
            "us.anthropic.claude-x:0",
            // `vendor/model` OpenRouter IDs — the whole point of allowing `/`.
            "z-ai/glm-5.3-flash",
            "anthropic/claude-sonnet-5",
            "deepseek/deepseek-v4-flash-0731",
        ] {
            assert!(valid_model_name(ok), "{ok}");
        }
        for bad in [
            "",
            "--model",
            "a b",
            "x;rm",
            &"a".repeat(81),
            // `/` is allowed inside, but the first byte must still be
            // alphanumeric — a leading slash (path-shaped) stays rejected.
            "/etc/passwd",
        ] {
            assert!(!valid_model_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn resolve_model_arg_passes_a_valid_model_through_unchanged() {
        assert_eq!(
            resolve_model_arg(Some("z-ai/glm-5.3-flash[1m]"), true).unwrap(),
            Some("z-ai/glm-5.3-flash[1m]")
        );
        assert_eq!(
            resolve_model_arg(Some("  claude-sonnet-5  "), false).unwrap(),
            Some("claude-sonnet-5"),
            "surrounding whitespace is trimmed, not treated as invalid"
        );
    }

    #[test]
    fn resolve_model_arg_allows_no_model_only_when_not_redirected() {
        // Anthropic via the CLI's own login: no model is fine, the CLI default
        // is intended and free.
        assert_eq!(resolve_model_arg(None, false).unwrap(), None);
        assert_eq!(resolve_model_arg(Some("   "), false).unwrap(), None);

        // ANTHROPIC_BASE_URL in effect: a missing model must NOT fall through
        // to the CLI default (that default would be billed via the proxy).
        assert!(matches!(
            resolve_model_arg(None, true),
            Err(AxiomataError::InvalidAgentModel { .. })
        ));
    }

    #[test]
    fn resolve_model_arg_rejects_a_malformed_model_rather_than_dropping_it() {
        // The exact typo that caused the real OpenRouter bill: `)` for `]`.
        for redirected in [true, false] {
            assert!(
                matches!(
                    resolve_model_arg(Some("z-ai/glm-5.3-flash[1m)"), redirected),
                    Err(AxiomataError::InvalidAgentModel { .. })
                ),
                "redirected={redirected}"
            );
        }
    }

    #[test]
    fn allowed_tools_values_are_validated() {
        for ok in [
            "mcp__apple-reminders__calendar_events",
            "mcp__apple-reminders__calendar_calendars mcp__apple-reminders__calendar_events",
            "Bash(git *) Edit",
            "a,b,c",
        ] {
            assert!(valid_allowed_tools(ok), "{ok}");
        }
        for bad in ["", "--allowedTools", ";rm -rf", &"a".repeat(4001)] {
            assert!(!valid_allowed_tools(bad), "{bad:?}");
        }
    }

    #[test]
    fn tail_respects_char_boundaries() {
        assert_eq!(tail("héllo", 3), "llo");
        assert_eq!(tail("héllo", 4), "llo");
        assert_eq!(tail("abc", 10), "abc");
    }

    fn ambient(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn lookup<'a>(env: &'a [(String, String)], key: &str) -> Option<&'a str> {
        env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    #[test]
    fn child_env_drops_an_ambient_anthropic_base_url_unless_request_env_sets_it() {
        // Leaked from the launching shell, no request override: gone.
        let assembled = child_env_from(
            ambient(&[("ANTHROPIC_BASE_URL", "https://leaked.example")]),
            &[],
        );
        assert_eq!(lookup(&assembled, "ANTHROPIC_BASE_URL"), None);

        // Same leak, but the active provider sets its own: the request value
        // wins and appears exactly once.
        let assembled = child_env_from(
            ambient(&[("ANTHROPIC_BASE_URL", "https://leaked.example")]),
            &[(
                "ANTHROPIC_BASE_URL".to_string(),
                "https://openrouter.ai/api".to_string(),
            )],
        );
        assert_eq!(
            lookup(&assembled, "ANTHROPIC_BASE_URL"),
            Some("https://openrouter.ai/api")
        );
        assert_eq!(
            assembled
                .iter()
                .filter(|(k, _)| k == "ANTHROPIC_BASE_URL")
                .count(),
            1
        );
    }

    #[test]
    fn child_env_keeps_the_allowlist_and_prefixes_but_nothing_else() {
        let assembled = child_env_from(
            ambient(&[
                ("PATH", "/usr/bin"),
                ("HOME", "/home/ada"),
                ("LC_ALL", "en_US.UTF-8"),
                ("XDG_CONFIG_HOME", "/home/ada/.config"),
                ("SSL_CERT_FILE", "/etc/ssl/cert.pem"),
                ("__CF_USER_TEXT_ENCODING", "0x1F5:0x0:0x0"),
                ("CLAUDE_CODE_USE_BEDROCK", "1"),
                ("ANTHROPIC_AUTH_TOKEN", "sk-leak"),
                ("AWS_SECRET_ACCESS_KEY", "leak"),
                ("RANDOM_SHELL_VAR", "x"),
            ]),
            &[],
        );

        for kept in [
            "PATH",
            "HOME",
            "LC_ALL",
            "XDG_CONFIG_HOME",
            "SSL_CERT_FILE",
            "__CF_USER_TEXT_ENCODING",
        ] {
            assert!(lookup(&assembled, kept).is_some(), "{kept} should be kept");
        }
        for dropped in [
            "CLAUDE_CODE_USE_BEDROCK",
            "ANTHROPIC_AUTH_TOKEN",
            "AWS_SECRET_ACCESS_KEY",
            "RANDOM_SHELL_VAR",
        ] {
            assert_eq!(
                lookup(&assembled, dropped),
                None,
                "{dropped} should be dropped"
            );
        }
    }

    #[test]
    fn child_env_request_env_overrides_an_inherited_key() {
        let assembled = child_env_from(
            ambient(&[("PATH", "/usr/bin")]),
            &[("PATH".to_string(), "/opt/custom/bin:/usr/bin".to_string())],
        );
        assert_eq!(lookup(&assembled, "PATH"), Some("/opt/custom/bin:/usr/bin"));
        assert_eq!(assembled.iter().filter(|(k, _)| k == "PATH").count(), 1);
    }
}
