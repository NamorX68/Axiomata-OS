//! Agent backends: `Opencode` (the headless Opencode CLI — the single agent
//! harness for every model Axiomata runs, cloud or local) and `Ollama` (a
//! single raw completion against a local model for simple, tool-free tasks).
//!
//! Skill and routine execution dispatches through the [`AgentBackend`] enum
//! rather than a plugin registry or trait-object abstraction — a deliberate
//! choice, since only a few backends are needed and a generic multi-CLI
//! abstraction would be premature. See `docs/architecture.md` §6 for the
//! rationale. A further variant can be added later without reworking the runner
//! or scheduler.
//!
//! The interactive assistant-bar chat also runs through opencode (a
//! session-continuing `opencode run`), not a separate backend: one harness for
//! everything means a provider/model that works in skills works in chat too.
//! Foreign agents (Claude Code, the M1-era `ollama-agent` tool loop) were
//! removed in Stufe 2 CP5 in favour of this single harness.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use crate::config::{Config, ProviderId, ProviderRole};
use crate::error::AxiomataError;

pub mod ollama;
pub mod opencode;

/// Backend identifier stored verbatim in a `SKILL.md` frontmatter `backend`
/// field and in a routine's DB row. Kept as a plain string on disk so the file
/// format stays self-explanatory. The fallback when a skill names no backend.
pub const BACKEND_OPENCODE: &str = "opencode";
/// See [`BACKEND_OPENCODE`].
pub const BACKEND_OLLAMA: &str = "ollama";

/// Upper bound on how much of a backend's output is kept, shared by the raw
/// completion [`AgentBackend::Ollama`] and the headless
/// [`AgentBackend::Opencode`].
pub(crate) const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

/// Truncates `text` to at most `max_bytes`, respecting UTF-8 char boundaries.
pub(crate) fn truncate_utf8(mut text: String, max_bytes: usize) -> String {
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

/// Converts captured child output to a `String`, reusing the buffer directly
/// when it is already valid UTF-8 (the common case) and only allocating a
/// replacement string on the lossy path.
pub(crate) fn into_string_lossy(bytes: Vec<u8>) -> String {
    match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
    }
}

/// Strips ANSI/VT100 escape sequences (colour, cursor movement, terminal
/// title-setting, …) from captured child output. `opencode run` does not
/// reliably detect a non-terminal target and suppress them, and once output is
/// stored in a run record or shown in a non-terminal UI panel (the Skills Deck
/// tile, `axiomata-cli get-run --json`) a raw escape code is just noise.
/// Recognises CSI sequences (`ESC '[' … <letter>`, e.g. colour codes) and OSC
/// sequences (`ESC ']' … (BEL | ESC '\')`, e.g. a terminal title); any other
/// escape is dropped on its own so one stray `ESC` byte can't swallow the rest
/// of the output.
///
/// Args:
///     input: Text as captured from the child's stdout or stderr.
///
/// Returns:
///     The same text with every recognised escape sequence removed.
pub(crate) fn strip_ansi(input: &str) -> String {
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

/// Process-environment variables inherited by an agent child process by exact
/// name. The CLI needs a working process environment — `PATH` to find `node`
/// and the tools it shells out to, `HOME` to locate its own config/state,
/// locale for correct text handling — but must **not** inherit an ambient
/// `ANTHROPIC_*` / `CLAUDE_CODE_*` / `OPENAI_API_KEY` export from the shell
/// Axiomata itself was launched from (see
/// `docs/plans/provider-hardening.md` checkpoint 3 — the question "does
/// launching from my configured shell change billing?"). Everything the child
/// gets beyond this list comes through `request.env`, applied last.
const INHERITED_ENV_ALLOWLIST: &[&str] =
    &["PATH", "HOME", "USER", "SHELL", "TERM", "LANG", "TMPDIR"];

/// Prefixes whose every matching process-environment variable is inherited:
/// `LC_*` (locale categories), `XDG_*` (base-dir spec), `SSL_CERT_*` (OpenSSL
/// trust-store overrides), and `__CF*` (the CoreFoundation vars macOS injects,
/// e.g. `__CF_USER_TEXT_ENCODING`; absent on other platforms).
const INHERITED_ENV_PREFIXES: &[&str] = &["LC_", "XDG_", "SSL_CERT_", "__CF"];

/// The complete environment for an agent child, applied on top of a
/// [`tokio::process::Command::env_clear`]: the allowlisted subset of this
/// process's environment first, then `request_env` layered over it so a
/// caller's override still wins.
pub(crate) fn agent_child_env(request_env: &[(String, String)]) -> Vec<(String, String)> {
    agent_child_env_from(std::env::vars(), request_env)
}

/// [`agent_child_env`] with the ambient environment injected, so it is
/// unit-testable without touching the real process environment.
pub(crate) fn agent_child_env_from<I>(
    ambient: I,
    request_env: &[(String, String)],
) -> Vec<(String, String)>
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

/// Caps how many `opencode` child processes may be running at once, across
/// *every* caller — a manual "run now" click, a routine firing, and a chat
/// turn all funnel through the shared spawn harness. Without this, several due
/// routines firing in the same tick (see [`crate::routines::scheduler::tick`],
/// which fires them concurrently) plus a stray UI click could start
/// unboundedly many `opencode` processes at once (each a Node CLI). A modest,
/// fixed cap rather than a config knob: this is a resource-safety floor, not a
/// tuning surface.
const MAX_CONCURRENT_AGENT_RUNS: usize = 4;

/// The semaphore [`MAX_CONCURRENT_AGENT_RUNS`] is enforced through, created
/// once and shared for the life of the process.
///
/// A process-wide `static` rather than a field on `AxiomataCore` — it holds
/// contended, mutable-in-effect state, and is sound only because exactly one
/// `AxiomataCore` is ever constructed per process today (the CLI is one-shot;
/// the Tauri app is a single instance). The place this would bite: any test
/// that spawns a *real* agent process shares this same 4-slot budget with every
/// other test in the same binary running concurrently (`cargo test`'s default),
/// and a hypothetical second `AxiomataCore` in one process would silently share
/// it too.
pub(crate) fn agent_slots() -> &'static tokio::sync::Semaphore {
    static AGENT_SLOTS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    AGENT_SLOTS.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT_AGENT_RUNS))
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
            // e.g. `z-ai/glm-5.3-flash`, and opencode's `provider/model`
            // prefix). It's flag-safe: the value is passed as a single
            // `Command::arg` token (no shell), the first byte is already
            // forced to be alphanumeric so it can't start a `-` flag, and
            // `opencode --model` only forwards it as the API `model` string.
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'[' | b']' | b'/')
        })
}

/// A session id is only ever something `opencode` printed earlier; refuse
/// anything that could read as a flag or shell noise.
pub fn valid_session_id(id: &str) -> bool {
    let starts_alnum = id.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric());
    starts_alnum
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Which agent runs a given skill or routine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentBackend {
    /// The headless Opencode CLI (`opencode run`). A model-agnostic agent
    /// harness: talks to whatever provider/model you point `--model` at with
    /// the provider's own native tool-calling protocol, loads the user's
    /// existing opencode config (MCP servers, auth) — so a non-Anthropic model
    /// like deepseek-v4-flash runs its skills the same way it runs in opencode
    /// itself. Model travels via [`AgentRequest::model`] as the full
    /// `provider/model` id the CLI expects, e.g.
    /// `openrouter/deepseek/deepseek-v4-flash-0731`.
    Opencode,
    /// A single completion call against a local Ollama model — no tools, no
    /// loop. Intended for simple, deterministic tasks (e.g. appending a to-do
    /// item).
    Ollama { model: String },
}

impl AgentBackend {
    /// Resolves a backend identifier string plus an optional model override
    /// onto a concrete backend.
    ///
    /// Args:
    ///     backend: `"opencode"` or `"ollama"`.
    ///     model_override: For `"ollama"`, a model name that wins over the
    ///         configured default; ignored for `"opencode"` (its model is
    ///         resolved separately, see [`opencode::model_id`]).
    ///     config: Provides the model fallback (`agents.ollama_model`).
    ///
    /// Returns:
    ///     The resolved [`AgentBackend`].
    ///
    /// Errors:
    ///     [`AxiomataError::UnknownAgentBackend`] if `backend` is neither
    ///     known identifier.
    pub fn resolve(
        backend: &str,
        model_override: Option<&str>,
        config: &Config,
    ) -> Result<Self, AxiomataError> {
        match backend {
            BACKEND_OPENCODE => Ok(Self::Opencode),
            BACKEND_OLLAMA => {
                let model = model_override
                    .map(str::to_owned)
                    .unwrap_or_else(|| config.agents.ollama_model.clone());
                Ok(Self::Ollama { model })
            }
            // Retired backend ids that still live in `SKILL.md` frontmatter /
            // routine rows seeded before the opencode consolidation. A seed
            // never overwrites an existing copy, so pre-upgrade installs keep
            // the old ids on disk; mapping them onto the single harness (with
            // a warning) keeps those skills runnable instead of failing every
            // run with `UnknownAgentBackend`.
            "claude-code" | "ollama-agent" => {
                tracing::warn!(backend, "deprecated backend id — running on opencode");
                Ok(Self::Opencode)
            }
            other => Err(AxiomataError::UnknownAgentBackend {
                backend: other.to_owned(),
            }),
        }
    }

    /// Returns the identifier string for this backend, matching what
    /// [`AgentBackend::resolve`] accepts.
    pub fn id(&self) -> &'static str {
        match self {
            Self::Opencode => BACKEND_OPENCODE,
            Self::Ollama { .. } => BACKEND_OLLAMA,
        }
    }

    /// Runs `request` on this backend, capturing output and wall-clock timing.
    ///
    /// A non-zero `exit_code` in the returned [`AgentRunResult`] means the
    /// agent ran but reported failure; an `Err` means it could not be run at
    /// all (spawn failure, timeout, transport error).
    pub async fn run(&self, request: AgentRequest) -> Result<AgentRunResult, AxiomataError> {
        match self {
            Self::Opencode => opencode::run(request).await,
            Self::Ollama { model } => ollama::run(request, model).await,
        }
    }
}

/// How an assistant-bar turn may act. Opencode has no per-mode permission
/// switch in headless `run` — both modes are spawned with `--auto`
/// (auto-approve tool use) so the turn never stalls waiting for a permission
/// prompt nobody will answer; the mode still records in the spend log and the
/// dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatMode {
    Chat,
    Instruct,
}

impl ChatMode {
    /// The token stored in `chat_turns.mode`.
    pub fn as_log_str(self) -> &'static str {
        match self {
            ChatMode::Chat => "chat",
            ChatMode::Instruct => "instruct",
        }
    }
}

/// The parsed `--format json` result of a chat turn.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChatReply {
    pub session_id: String,
    pub reply_markdown: String,
    pub is_error: bool,
    pub cost_usd: Option<f64>,
    pub usage: Option<serde_json::Value>,
    /// `input_tokens` from the NDJSON `step_finish` events.
    pub input_tokens: Option<u64>,
    /// `output_tokens` from the NDJSON `step_finish` events.
    pub output_tokens: Option<u64>,
    /// How many assistant steps the turn took.
    pub num_turns: Option<u32>,
    pub duration_ms: u64,
}

/// One turn of the dashboard assistant, as the opencode chat path sees it.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub message: String,
    /// A `session_id` returned by an earlier turn; `None` starts a session.
    pub session_id: Option<String>,
    pub mode: ChatMode,
    pub cwd: PathBuf,
    pub timeout: Duration,
    pub env: Vec<(String, String)>,
    /// A workspace file whose contents are prepended to the message (the
    /// dashboard's module manifest when it exists) — opencode has no
    /// `--append-system-prompt-file`, so the context becomes part of the task.
    pub system_prompt_file: Option<PathBuf>,
    /// The full opencode `provider/model` id this turn runs on.
    pub model: Option<String>,
    /// Whether the harness may auto-approve tool use (`--auto`); see
    /// [`crate::config::AgentDefaults::auto_approve_tools`]. `false` makes a
    /// tool-using turn fail on unapproved calls instead of executing them —
    /// the prompt-injection guard for prompts that embed untrusted content.
    pub auto_approve_tools: bool,
    /// Ignored by opencode (it auto-approves via `--auto` and resolves MCP
    /// servers from its own config); kept on the request for API stability.
    pub allowed_tools: Option<String>,
}

/// One dashboard-assistant turn on the opencode backend, built from the
/// config: cwd = workspace root (so opencode loads the workspace context), the
/// skill timeout, and the module manifest (`paths::module_context_path()`)
/// prepended to the message if present.
pub async fn chat(
    config: &Config,
    message: String,
    session_id: Option<String>,
    mode: ChatMode,
    allowed_tools: Option<String>,
) -> Result<ChatReply, AxiomataError> {
    opencode::chat(ChatRequest {
        message,
        session_id,
        mode,
        cwd: config.workspace_root.clone(),
        timeout: Duration::from_secs(config.agents.skill_timeout_secs),
        env: Vec::new(),
        system_prompt_file: module_context_if_present(),
        // The provider-specific reading happens inside `opencode::chat_model_id`;
        // `None` fails the turn with a clear config error rather than silently
        // hitting an opencode default model.
        model: opencode::chat_model_id(config).ok(),
        auto_approve_tools: config.agents.auto_approve_tools,
        allowed_tools,
    })
    .await
}

/// [`chat`] plus the spend guardrail and the `chat_turns` log entry, in one
/// call — the form every real caller (the dashboard command, the CLI) wants.
///
/// Order: the daily-cap check runs *before* the turn (an
/// [`AxiomataError::SpendCapReached`] means nothing was spawned); the turn
/// runs with no lock held; then the result is written to `chat_turns` for the
/// per-provider rollup. A logging failure is warned about, not propagated —
/// the turn already happened and its reply is what the caller needs.
pub async fn chat_and_record(
    config: &Config,
    db: &std::sync::Mutex<rusqlite::Connection>,
    message: String,
    session_id: Option<String>,
    mode: ChatMode,
    allowed_tools: Option<String>,
) -> Result<ChatReply, AxiomataError> {
    {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        crate::spend::guard_redirected_turn(&conn, config, ProviderRole::Chat)?;
    }
    let model = default_chat_model(config);
    let reply = chat(config, message, session_id, mode, allowed_tools).await?;
    {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        let turn = crate::spend::ChatTurnRecord {
            session_id: reply.session_id.clone(),
            mode: mode.as_log_str(),
            provider: Some(config.agents.chat_provider.as_str().to_string()),
            model: model.clone(),
            is_error: reply.is_error,
            // Same metering override as skill runs — see
            // `spend::metered_cost_usd`.
            cost_usd: crate::spend::metered_cost_usd(
                config,
                model.as_deref(),
                reply.input_tokens,
                reply.output_tokens,
            )
            .or(reply.cost_usd),
            input_tokens: reply.input_tokens,
            output_tokens: reply.output_tokens,
            num_turns: reply.num_turns,
            duration_ms: reply.duration_ms,
        };
        if let Err(err) = crate::spend::record_chat_turn(&conn, &turn) {
            tracing::warn!(%err, "failed to record a chat turn to the spend log");
        }
    }
    Ok(reply)
}

/// A single headless agent invocation.
#[derive(Debug, Clone)]
pub struct AgentRequest {
    /// The prompt handed to the agent. For opencode it is written to the CLI's
    /// stdin and becomes the task message; for Ollama it is the raw completion
    /// prompt.
    pub prompt: String,
    /// Working directory for the agent. Opencode treats this as its project
    /// root (`--dir`, so workspace context / opencode config load).
    pub cwd: PathBuf,
    /// Hard wall-clock limit. On expiry the run fails with
    /// [`AxiomataError::AgentTimeout`] and the child process (if any) is
    /// killed.
    pub timeout: Duration,
    /// Extra environment variables for the agent process. Opencode resolves
    /// providers/auth from its own config store and gets none (and nothing
    /// leaks in — see `agent_child_env`'s allowlist); kept on the request for
    /// future backends.
    pub env: Vec<(String, String)>,
    /// A workspace file whose contents are prepended to the prompt (the module
    /// bridge manifest); opencode has no `--append-system-prompt-file`, so the
    /// backend does the prepending itself.
    pub system_prompt_file: Option<PathBuf>,
    /// For opencode: the full `provider/model` id built by
    /// [`opencode::model_id`]. Ignored by Ollama (its model lives in the
    /// backend enum).
    pub model: Option<String>,
    /// Whether the harness may auto-approve tool use (`--auto`); see
    /// [`crate::config::AgentDefaults::auto_approve_tools`]. `false` makes a
    /// tool-using run fail on unapproved calls instead of executing them —
    /// the prompt-injection guard for prompts that embed untrusted content.
    pub auto_approve_tools: bool,
    /// Carried over from `SKILL.md` frontmatter for symmetry; the opencode
    /// backend ignores it (MCP servers come from opencode's own config, tool
    /// use is auto-approved). Kept so callers don't lose the declared tools.
    pub allowed_tools: Option<String>,
}

/// The chat provider's `chat_model`, for interactive dashboard-assistant
/// turns — `None` when unset or the chat provider is missing from
/// `config.agents.providers` (shouldn't happen once [`crate::config::Config::load`]
/// has run its migration, but a config built by hand in a test could still
/// lack it).
pub fn default_chat_model(config: &Config) -> Option<String> {
    provider_model(config, config.agents.chat_provider, |settings| {
        &settings.chat_model
    })
}

/// The skill provider's `skill_model` — the fallback for skill/routine runs
/// that don't pin their own model. A skill's `SKILL.md` frontmatter `model:`
/// takes precedence over this in [`crate::skills::runner::execute_skill`].
pub fn default_skill_model(config: &Config) -> Option<String> {
    provider_model(config, config.agents.skill_provider, |settings| {
        &settings.skill_model
    })
}

/// Shared lookup behind [`default_chat_model`] / [`default_skill_model`]:
/// resolves `provider`'s settings, then reads whichever model field `pick`
/// names off of it, treating a missing/blank value as unset.
fn provider_model(
    config: &Config,
    provider: ProviderId,
    pick: impl FnOnce(&crate::config::ProviderSettings) -> &String,
) -> Option<String> {
    let settings = config.agents.providers.get(&provider)?;
    let m = pick(settings).trim();
    (!m.is_empty()).then(|| m.to_string())
}

/// `paths::module_context_path()` if the dashboard has written it.
pub fn module_context_if_present() -> Option<PathBuf> {
    let path = crate::paths::module_context_path();
    path.is_file().then_some(path)
}

/// The outcome of an [`AgentRequest`] that actually ran.
#[derive(Debug, Clone)]
pub struct AgentRunResult {
    /// Captured standard output. For opencode this is the joined `text` parts
    /// of the `--format json` NDJSON event stream — byte-for-byte what the
    /// model replied, so a connector skill's digest JSON is what lands here.
    pub stdout: String,
    /// Captured standard error (opencode diagnostics) or empty (Ollama).
    pub stderr: String,
    /// Process exit code for opencode. Synthetic for Ollama: always `0` here,
    /// since Ollama failures surface as `Err` rather than a run result.
    pub exit_code: i32,
    /// Wall-clock duration of the run, in milliseconds.
    pub duration_ms: u64,
    /// Estimated cost from the opencode `step_finish` events, when the run went
    /// through a provider that reported it. `None` for Ollama and whenever the
    /// event stream carried no cost.
    pub cost_usd: Option<f64>,
    /// Total `tokens.input` across the `step_finish` events. `None` when the
    /// stream didn't carry them.
    pub input_tokens: Option<u64>,
    /// Total `tokens.output` across the `step_finish` events. `None` when the
    /// stream didn't carry them.
    pub output_tokens: Option<u64>,
    /// How many assistant steps the agent loop took (`step_finish` count).
    pub num_turns: Option<u32>,
}

impl AgentRunResult {
    /// Whether the run reported success (`exit_code == 0`).
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// A result carrying only the process-level fields (no cost/token data
    /// parsed yet). Used by the opencode spawn harness before the NDJSON event
    /// stream is inspected, and by the Ollama backend (which has no envelope).
    pub(crate) fn bare(stdout: String, stderr: String, exit_code: i32, duration_ms: u64) -> Self {
        Self {
            stdout,
            stderr,
            exit_code,
            duration_ms,
            cost_usd: None,
            input_tokens: None,
            output_tokens: None,
            num_turns: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentDefaults;

    /// A config whose Ollama default model is set to `model`.
    fn config_with_ollama_model(model: &str) -> Config {
        Config {
            agents: AgentDefaults {
                ollama_model: model.to_owned(),
                ..AgentDefaults::default()
            },
            ..Config::default()
        }
    }

    #[test]
    fn resolve_maps_opencode_identifier() {
        let backend = AgentBackend::resolve(BACKEND_OPENCODE, None, &Config::default()).unwrap();
        assert_eq!(backend, AgentBackend::Opencode);
        assert_eq!(backend.id(), "opencode");
    }

    #[test]
    fn resolve_ollama_uses_config_default_model_when_no_override() {
        let config = config_with_ollama_model("llama3.2");
        let backend = AgentBackend::resolve(BACKEND_OLLAMA, None, &config).unwrap();
        assert_eq!(
            backend,
            AgentBackend::Ollama {
                model: "llama3.2".to_owned()
            }
        );
        assert_eq!(backend.id(), "ollama");
    }

    #[test]
    fn resolve_ollama_prefers_model_override() {
        let config = config_with_ollama_model("llama3.2");
        let backend = AgentBackend::resolve(BACKEND_OLLAMA, Some("mistral"), &config).unwrap();
        assert_eq!(
            backend,
            AgentBackend::Ollama {
                model: "mistral".to_owned()
            }
        );
    }

    #[test]
    fn resolve_rejects_unknown_identifier() {
        let err = AgentBackend::resolve("gemini-2.5", None, &Config::default()).unwrap_err();
        assert!(matches!(
            err,
            AxiomataError::UnknownAgentBackend { backend } if backend == "gemini-2.5"
        ));
    }

    #[test]
    fn resolve_maps_retired_backend_ids_onto_opencode() {
        // Skills seeded before the opencode consolidation carry these ids in
        // their frontmatter, and a seed never overwrites an existing copy — so
        // they must keep resolving (onto the single harness) rather than break
        // every run with `UnknownAgentBackend`.
        for retired in ["claude-code", "ollama-agent"] {
            let backend = AgentBackend::resolve(retired, None, &Config::default()).unwrap();
            assert_eq!(backend, AgentBackend::Opencode, "{retired} -> Opencode");
            assert_eq!(backend.id(), "opencode");
        }
    }

    #[test]
    fn default_chat_and_skill_models_read_the_active_providers_own_fields_independently() {
        let mut config = Config::default();
        config
            .agents
            .providers
            .get_mut(&ProviderId::Anthropic)
            .unwrap()
            .chat_model = "chat-model".to_owned();
        config
            .agents
            .providers
            .get_mut(&ProviderId::Anthropic)
            .unwrap()
            .skill_model = "skill-model".to_owned();

        assert_eq!(default_chat_model(&config), Some("chat-model".to_owned()));
        assert_eq!(default_skill_model(&config), Some("skill-model".to_owned()));
    }

    #[test]
    fn default_models_follow_their_own_role_provider() {
        let mut config = Config::default();
        // Chat routes through OpenRouter, skills stay on Anthropic.
        config.agents.chat_provider = ProviderId::OpenRouter;
        config
            .agents
            .providers
            .get_mut(&ProviderId::OpenRouter)
            .unwrap()
            .chat_model = "or-chat".to_owned();
        config
            .agents
            .providers
            .get_mut(&ProviderId::Anthropic)
            .unwrap()
            .skill_model = "claude-skill".to_owned();

        assert_eq!(default_chat_model(&config), Some("or-chat".to_owned()));
        assert_eq!(
            default_skill_model(&config),
            Some("claude-skill".to_owned())
        );
    }

    #[test]
    fn default_model_is_none_when_the_active_providers_field_is_blank() {
        let mut config = Config::default();
        config
            .agents
            .providers
            .get_mut(&ProviderId::Anthropic)
            .unwrap()
            .chat_model = "   ".to_owned();

        assert_eq!(default_chat_model(&config), None);
    }

    #[test]
    fn default_models_are_none_when_the_active_provider_is_missing_from_the_map_entirely() {
        let mut config = Config::default();
        config.agents.providers.remove(&ProviderId::Anthropic);

        assert_eq!(default_chat_model(&config), None);
        assert_eq!(default_skill_model(&config), None);
    }

    #[test]
    fn session_ids_are_validated() {
        for ok in ["ses_f69ee4e6fffewcQksbhPtIRCDc", "abc-123_X"] {
            assert!(valid_session_id(ok), "{ok}");
        }
        for bad in ["", "--dangerous", "a b", "x;rm", &"a".repeat(129)] {
            assert!(!valid_session_id(bad), "{bad:?}");
        }
    }

    #[test]
    fn model_names_are_validated() {
        for ok in [
            "claude-sonnet-5",
            "z-ai/glm-5.3-flash",
            "anthropic/claude-sonnet-5",
            "openrouter/deepseek/deepseek-v4-flash-0731",
            "ollama/qwen3.8:27b-mlx",
        ] {
            assert!(valid_model_name(ok), "{ok}");
        }
        for bad in ["", "--model", "a b", "x;rm", &"a".repeat(81), "/etc/passwd"] {
            assert!(!valid_model_name(bad), "{bad:?}");
        }
    }

    #[test]
    fn agent_slots_caps_concurrent_permits() {
        // Kept as one test (rather than split across two) so nothing else
        // running in parallel can touch this same static in between the steps
        // below.
        let sem = agent_slots();
        let starting = sem.available_permits();
        assert!(starting <= MAX_CONCURRENT_AGENT_RUNS);
        let permit = sem.try_acquire().expect("a permit should be available");
        assert_eq!(sem.available_permits(), starting - 1);
        drop(permit);
        assert_eq!(sem.available_permits(), starting);

        let mut held = Vec::new();
        while let Ok(permit) = sem.try_acquire() {
            held.push(permit);
        }
        assert_eq!(sem.available_permits(), 0);
        assert!(matches!(
            sem.try_acquire(),
            Err(tokio::sync::TryAcquireError::NoPermits)
        ));
        held.pop();
        assert!(sem.try_acquire().is_ok());
    }
}
