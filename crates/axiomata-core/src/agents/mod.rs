//! Agent backends: `Opencode` (turns on Opencode 2's shared background
//! service — the single agent harness for every model Axiomata runs, cloud or
//! local), `Ollama` (a
//! single raw completion against a local model for simple, tool-free tasks)
//! and `Axh` (a run through Axiomata's own harness, `glut` —
//! step HS6b, `docs/plans/harness-hs6b.md`).
//!
//! Skill and routine execution dispatches through the [`AgentBackend`] enum
//! rather than a plugin registry or trait-object abstraction — a deliberate
//! choice, since only a few backends are needed and a generic multi-CLI
//! abstraction would be premature. See `docs/architecture.md` §6 for the
//! rationale. A further variant can be added later without reworking the runner
//! or scheduler.
//!
//! The interactive assistant-bar chat also runs through opencode (a
//! continued Opencode session), not a separate backend: one harness for
//! everything means a provider/model that works in skills works in chat too.
//! Foreign agents (Claude Code, the M1-era `ollama-agent` tool loop) were
//! removed in Stufe 2 CP5 in favour of this single harness.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use crate::config::{Config, HarnessId, ProviderId, ProviderRole};
use crate::error::AxiomataError;

pub mod axh;
pub mod claude_auth;
pub mod claude_code;
pub(crate) mod json_reply;
pub mod ollama;
pub(crate) mod read_access;

/// Backend identifier stored verbatim in a `SKILL.md` frontmatter `backend`
/// field and in a routine's DB row. Kept as a plain string on disk so the file
/// format stays self-explanatory. A skill or routine that names none runs on the configured
/// `agents.skill_harness` (`docs/plans/harness-choice.md` D3).
pub const BACKEND_OPENCODE: &str = "opencode";
/// See [`BACKEND_OPENCODE`].
pub const BACKEND_OLLAMA: &str = "ollama";
/// See [`BACKEND_OPENCODE`]. Runs the skill through Axiomata's own harness
/// (`docs/plans/harness-hs6b.md`, B1): an OpenAI-compatible endpoint of the
/// skill provider, no opencode involved.
pub const BACKEND_AXH: &str = "axh";
/// See [`BACKEND_OPENCODE`]. Claude Code as `claude -p` on the app's own login folder
/// (`docs/plans/harness-choice.md`, D2/D5).
pub const BACKEND_CLAUDE_CODE: &str = "claude_code";

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

/// Process-environment variables the `opencode` commands Axiomata runs
/// (`debug paths`, `service start`) inherit by exact name. A service started
/// from here keeps this environment for every session it runs. The CLI needs
/// a working process environment — `PATH` to find the tools it shells out to, `HOME` to locate its own config/state,
/// locale for correct text handling — but must **not** inherit an ambient
/// `ANTHROPIC_*` / `CLAUDE_CODE_*` / `OPENAI_API_KEY` export from the shell
/// Axiomata itself was launched from (see
/// `docs/plans/provider-hardening.md` checkpoint 3 — the question "does
/// launching from my configured shell change billing?").
const INHERITED_ENV_ALLOWLIST: &[&str] =
    &["PATH", "HOME", "USER", "SHELL", "TERM", "LANG", "TMPDIR"];

/// Prefixes whose every matching process-environment variable is inherited:
/// `LC_*` (locale categories), `XDG_*` (base-dir spec), `SSL_CERT_*` (OpenSSL
/// trust-store overrides), and `__CF*` (the CoreFoundation vars macOS injects,
/// e.g. `__CF_USER_TEXT_ENCODING`; absent on other platforms).
const INHERITED_ENV_PREFIXES: &[&str] = &["LC_", "XDG_", "SSL_CERT_", "__CF"];

/// The complete environment for an `opencode` command, applied on top of an
/// `env_clear`: the allowlisted subset of this process's environment.
pub(crate) fn agent_child_env() -> Vec<(String, String)> {
    agent_child_env_from(std::env::vars())
}

/// [`agent_child_env`] with the ambient environment injected, so it is
/// unit-testable without touching the real process environment.
pub(crate) fn agent_child_env_from<I>(ambient: I) -> Vec<(String, String)>
where
    I: IntoIterator<Item = (String, String)>,
{
    let inherited = |key: &str| {
        INHERITED_ENV_ALLOWLIST.contains(&key)
            || INHERITED_ENV_PREFIXES.iter().any(|p| key.starts_with(p))
    };
    ambient
        .into_iter()
        .filter(|(key, _)| inherited(key))
        .collect()
}

/// Caps how many Opencode turns may be running at once, across *every*
/// caller — a manual "run now" click, a routine firing, and a chat turn all
/// funnel through the same backend. Without this, several due
/// routines firing in the same tick (see [`crate::routines::scheduler::tick`],
/// which fires them concurrently) plus a stray UI click could start
/// unboundedly many agent loops on the service at once (each with its own
/// model calls and MCP tool use). A modest,
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
    /// A single completion call against a local Ollama model — no tools, no
    /// loop. Intended for simple, deterministic tasks (e.g. appending a to-do
    /// item).
    Ollama { model: String },
    /// A run through Axiomata's own harness (`glut`,
    /// `docs/plans/harness.md`, step HS6b): the same lean tool-calling loop
    /// the `axh` binary runs, driven from the core — guarded file tools, the
    /// skill's own `allowed_tools` as its permissions, the sandbox, the
    /// harness's MCP servers, an OpenAI-compatible endpoint of the skill
    /// provider (so the provider must carry a base URL and key — B1, no
    /// silent fallback to opencode). The prompt arrives fully built
    /// (`skills::runner::build_prompt_at`), as on opencode; the model id
    /// travels bare through [`AgentRequest::model`].
    Axh,
    /// Claude Code as `claude -p` on the app's own login folder, Anthropic only
    /// (`docs/plans/harness-choice.md`, CP2; [`claude_code`]). The model
    /// travels bare (`claude-haiku-5-5`) through [`AgentRequest::model`].
    ClaudeCode,
}

impl AgentBackend {
    /// Resolves a backend identifier string plus an optional model override
    /// onto a concrete backend.
    ///
    /// Args:
    ///     backend: `"opencode"`, `"claude_code"`, `"axh"` or `"ollama"` (or a retired id, which follows
    ///         `agents.skill_harness`).
    ///     model_override: For `"ollama"`, a model name that wins over the
    ///         configured default; ignored for `"opencode"` and `"axh"` (their
    ///         models are resolved separately, see [`opencode::model_id`] and
    ///         `skills::runner::resolve_run_model`).
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
            BACKEND_AXH => Ok(Self::Axh),
            BACKEND_CLAUDE_CODE => Ok(Self::ClaudeCode),
            BACKEND_OLLAMA => {
                let model = model_override
                    .map(str::to_owned)
                    .unwrap_or_else(|| config.agents.ollama_model.clone());
                Ok(Self::Ollama { model })
            }
            // Retired backend ids that still live in `SKILL.md` frontmatter /
            // routine rows seeded before the opencode consolidation. A seed
            // never overwrites an existing copy, so pre-upgrade installs keep
            // the old ids on disk; they name no harness of today, so they run
            // on the configured one, as a skill without the field does.
            // Opencode left too (`docs/plans/opencode-exit.md`, O3): a skill or routine that names it runs on the
            // configured harness as well.
            BACKEND_OPENCODE | "claude-code" | "ollama-agent" => {
                let harness = config.agents.skill_harness.as_str();
                tracing::warn!(
                    backend,
                    harness,
                    "retired backend id — running on the skill harness"
                );
                Self::resolve(harness, model_override, config)
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
            Self::Ollama { .. } => BACKEND_OLLAMA,
            Self::Axh => BACKEND_AXH,
            Self::ClaudeCode => BACKEND_CLAUDE_CODE,
        }
    }

    /// Runs `request` on this backend, capturing output and wall-clock timing.
    ///
    /// A non-zero `exit_code` in the returned [`AgentRunResult`] means the
    /// agent ran but reported failure; an `Err` means it could not be run at
    /// all (spawn failure, timeout, transport error).
    ///
    /// `config` supplies the skill provider's endpoint to [`Self::Axh`]; the
    /// other backends resolve their own model and talk to a service that
    /// carries its auth, so they ignore it.
    pub async fn run(
        &self,
        request: AgentRequest,
        config: &Config,
    ) -> Result<AgentRunResult, AxiomataError> {
        match self {
            Self::Ollama { model } => ollama::run(request, model).await,
            Self::Axh => axh::run(request, config).await,
            Self::ClaudeCode => claude_code::run(request).await,
        }
    }
}

/// How an assistant-bar turn may act. Both modes run with the same
/// unattended permission rules (see `opencode`'s module doc), so a turn never
/// stalls waiting for a permission prompt nobody will answer; the mode still
/// records in the spend log and the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatMode {
    Chat,
    Instruct,
    /// The chat's plan switch (Glut 2, D, G12): the turn reads and plans but changes nothing — Glut in Plan mode
    /// (`exit_plan` asks to leave it), Claude Code and a new Opencode session with read rights only.
    Plan,
}

impl ChatMode {
    /// The token stored in `chat_turns.mode`.
    pub fn as_log_str(self) -> &'static str {
        match self {
            ChatMode::Chat => "chat",
            ChatMode::Instruct => "instruct",
            ChatMode::Plan => "plan",
        }
    }
}

/// The result of a chat turn, read back from the session's messages.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChatReply {
    pub session_id: String,
    pub reply_markdown: String,
    pub is_error: bool,
    pub cost_usd: Option<f64>,
    pub usage: Option<serde_json::Value>,
    /// Input tokens across the turn's answers.
    pub input_tokens: Option<u64>,
    /// The part of the input tokens read from the provider's prompt cache, billed at its lower price (glut-round
    /// CP4). `None` when the backend does not say.
    #[serde(default)]
    pub cached_input_tokens: Option<u64>,
    /// Output tokens across the turn's answers.
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
    /// A workspace file whose contents are prepended to the message (the
    /// dashboard's module manifest when it exists), marked as data.
    pub system_prompt_file: Option<PathBuf>,
    /// The full opencode `provider/model` id this turn runs on.
    pub model: Option<String>,
    /// Whether tool use is pre-approved for the turn; see
    /// [`crate::config::AgentDefaults::auto_approve_tools`]. `false` makes a
    /// tool-using turn fail on unapproved calls instead of executing them —
    /// the prompt-injection guard for prompts that embed untrusted content.
    pub auto_approve_tools: bool,
    /// The turn's tools as `allowed_tools` names them (a connector's instruct
    /// turn: its one MCP tool). Ignored by opencode (its rules are the
    /// session's); Claude Code and Glut take them as the turn's whole rights.
    pub allowed_tools: Option<String>,
    /// A slash command instead of a prompt (#108): one of the harness's own commands, `message` its arguments, or a
    /// compaction of the conversation.
    pub command: Option<ChatCommand>,
    /// Durable runs: the journal of an interrupted Glut chat turn this turn finishes, instead of a new message.
    pub resume: Option<String>,
    /// The conversation's `/effort` (`docs/plans/glut-effort.md`, E1): only Glut reads it, over `[agents.glut]
    /// chat_effort`.
    pub effort: Option<String>,
}

/// A chat turn that is a command rather than a prompt (`docs/plans/slash-commands.md`, S8/S9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatCommand {
    /// One of the chat harness's own commands ([`chat_commands`]), by its name; the turn's message is its arguments.
    Run { name: String },
    /// Summarise the conversation so far to free its context; the turn's message is unused.
    Compact,
}

impl ChatCommand {
    /// What the chat shows for a command that answered nothing — a compaction answers nothing on every harness.
    pub(crate) fn silent_reply(&self) -> String {
        match self {
            ChatCommand::Run { name } => format!("`/{name}` done."),
            ChatCommand::Compact => "Conversation compacted.".to_owned(),
        }
    }
}

/// The error of a compaction asked for before the conversation has a turn.
pub(crate) fn nothing_to_compact(backend: &'static str) -> AxiomataError {
    AxiomataError::AgentApi {
        backend,
        message: "nothing to compact yet: the conversation has no turn".to_owned(),
    }
}

/// What a chat turn carries besides its text (#108): a model for this conversation only, a command.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatExtras {
    /// The chat provider's model id this turn runs on instead of its `chat_model` (S5) — the provider's own
    /// spelling, as the model catalog lists it; each harness prefixes it as it does `chat_model`.
    pub model: Option<String>,
    pub command: Option<ChatCommand>,
    /// Finish the interrupted Glut chat turn of this journal ([`interrupted_chat_turns`]).
    pub resume: Option<String>,
    /// The conversation's `/effort` level; Glut alone reads it.
    pub effort: Option<String>,
}

/// Which harness offers a chat command: the source chip of the bar's list (S10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandSource {
    Glut,
    Oc,
    Cc,
}

/// One command the chat's harness offers (#108, S7) — data the bar shows as text and sends back by name.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EngineCommand {
    pub name: String,
    pub source: CommandSource,
    pub hint: Option<String>,
    pub description: String,
}

/// The commands of the configured chat harness: Glut's from the owner's command files, Opencode's from its service,
/// Claude Code's from the last chat turn (none before the first). Names a turn would refuse are left out.
///
/// # Errors
///
/// A Glut command folder that cannot be used, or an Opencode service that cannot be reached.
pub async fn chat_commands(config: &Config) -> Result<Vec<EngineCommand>, AxiomataError> {
    let commands = match config.agents.chat_harness {
        HarnessId::Axh => axh::chat_commands(&config.workspace_root)?
            .into_iter()
            .map(|command| EngineCommand {
                name: command.name,
                source: CommandSource::Glut,
                hint: command.argument_hint,
                description: command
                    .description
                    .unwrap_or_else(|| "command file".to_owned()),
            })
            .collect(),
        HarnessId::ClaudeCode => claude_code::known_commands()
            .into_iter()
            .map(|command| EngineCommand {
                name: command.name,
                source: CommandSource::Cc,
                hint: None,
                description: if command.skill { "skill" } else { "command" }.to_owned(),
            })
            .collect::<Vec<_>>(),
    };
    Ok(commands
        .into_iter()
        .filter(|command: &EngineCommand| valid_command_name(&command.name))
        .collect())
}

/// One earlier conversation `/resume` lists (#108).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ChatSession {
    pub id: String,
    /// The first words of its first message; empty where the harness keeps none the app can read (Opencode).
    pub title: String,
    /// Its latest turn, as `chat_turns` recorded it (RFC 3339).
    pub updated_at: String,
    pub turns: u32,
}

/// How many earlier conversations `/resume` lists.
const MAX_LISTED_SESSIONS: usize = 20;
/// How many lines, and bytes, of a conversation file a title is looked for in: a file may hold a huge line (an image).
const MAX_TITLE_LINES: usize = 200;
const MAX_TITLE_BYTES: u64 = 256 * 1024;
/// How much of a conversation's first message its title shows, in characters.
const TITLE_CHARS: usize = 80;

/// A message's first line, cut to [`TITLE_CHARS`].
fn title_of(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim();
    match line.char_indices().nth(TITLE_CHARS) {
        Some((at, _)) => format!("{}…", &line[..at]),
        None => line.to_owned(),
    }
}

/// Whether `id` is a conversation of `harness`: each spells its ids its own way.
fn session_of(harness: HarnessId, id: &str) -> bool {
    match harness {
        HarnessId::Axh => id.starts_with("axh-"),
        HarnessId::ClaudeCode => {
            id.len() == 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
        }
    }
}

/// The chat's earlier conversations on its harness, newest first (#108, `/resume`): the ones `chat_turns` recorded —
/// so a skill run in the same folder is never one — whose harness still holds them.
///
/// # Errors
///
/// The database cannot be read.
pub fn chat_sessions(
    config: &Config,
    db: &std::sync::Mutex<rusqlite::Connection>,
) -> Result<Vec<ChatSession>, AxiomataError> {
    let harness = config.agents.chat_harness;
    let recorded: Vec<(String, String, u32)> = {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        let mut statement = conn.prepare(
            "SELECT session_id, MAX(created_at), COUNT(*) FROM chat_turns \
             WHERE mode IN ('chat', 'plan') AND is_error = 0 AND session_id != '' \
             GROUP BY session_id ORDER BY MAX(created_at) DESC LIMIT 200",
        )?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        rows.collect::<Result<_, _>>()?
    };
    Ok(recorded
        .into_iter()
        .filter(|(id, _, _)| session_of(harness, id) && valid_session_id(id))
        .filter_map(|(id, updated_at, turns)| {
            let title = match harness {
                HarnessId::Axh => axh::session_title(&id)?,
                HarnessId::ClaudeCode => claude_code::session_title(&config.workspace_root, &id)?,
            };
            Some(ChatSession {
                id,
                title,
                updated_at,
                turns,
            })
        })
        .take(MAX_LISTED_SESSIONS)
        .collect())
}

/// `/undo` (#108, S9): the file changes of the conversation's latest turn undone, as its harness can — Glut from its
/// snapshots, Opencode by its revert; Claude Code has no rewind in `-p` and says so.
///
/// # Errors
///
/// Nothing to undo, the harness cannot, or the undo itself failed.
pub async fn chat_undo(
    config: &Config,
    db: &std::sync::Mutex<rusqlite::Connection>,
    session_id: &str,
) -> Result<String, AxiomataError> {
    // Only a chat conversation: the services also hold card and agent sessions, which `/undo` must never revert.
    let recorded = valid_session_id(session_id)
        && {
            let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
            conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM chat_turns WHERE session_id = ?1 AND mode IN ('chat', 'plan'))",
            [session_id],
            |row| row.get::<_, bool>(0),
        )?
        };
    if !recorded {
        return Err(AxiomataError::AgentApi {
            backend: "chat",
            message: "nothing to undo: not a conversation of this chat".to_owned(),
        });
    }
    match config.agents.chat_harness {
        HarnessId::Axh => axh::undo_last(session_id, &config.workspace_root).await,
        HarnessId::ClaudeCode => Err(AxiomataError::AgentApi {
            backend: BACKEND_CLAUDE_CODE,
            message: "Claude Code cannot undo in the app's chat: it has no rewind outside its own terminal".to_owned(),
        }),
    }
}

/// The longest model id or command name a chat turn takes.
const MAX_CHAT_NAME: usize = 200;

/// Whether `name` can be a model id or a command name: printable ASCII without spaces, quotes or a leading `-` — it
/// ends up in an argument list or a prompt's first line, where nothing may read it as a flag or a second line.
fn valid_chat_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_CHAT_NAME
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/@+-".contains(&b))
}

/// The config a turn with `extras` runs on: the chat provider's `chat_model` replaced by the turn's own model.
fn with_turn_model(config: &Config, extras: &ChatExtras) -> Result<Option<Config>, AxiomataError> {
    let Some(model) = extras.model.as_deref() else {
        return Ok(None);
    };
    if !valid_chat_name(model) {
        return Err(AxiomataError::InvalidAgentModel {
            reason: format!("{model:?} is not a model id"),
        });
    }
    // On a metered provider a model without a price would be recorded at no cost and slip past the daily cap: the
    // turn takes only a priced model (the catalog writes OpenRouter's prices, #17) or the configured one.
    let provider = config.agents.chat_provider;
    let configured = default_chat_model(config);
    if provider.is_redirected()
        && !config.agents.costs.contains_key(model)
        && configured.as_deref() != Some(model)
    {
        return Err(AxiomataError::InvalidAgentModel {
            reason: format!(
                "{model} has no price in agents.costs; refresh the model list or set its price"
            ),
        });
    }
    let mut config = config.clone();
    let settings = config.agents.providers.get_mut(&provider).ok_or_else(|| {
        AxiomataError::InvalidAgentModel {
            reason: format!("the chat provider {} has no settings", provider.as_str()),
        }
    })?;
    settings.chat_model = model.to_owned();
    Ok(Some(config))
}

/// Whether `name` can be a command name: [`valid_chat_name`] without the characters only a model id needs (`/`, `@`,
/// `+`) and without `..` — a name is never a path.
fn valid_command_name(name: &str) -> bool {
    valid_chat_name(name) && !name.contains("..") && !name.contains(['/', '@', '+'])
}

fn check_command(command: Option<&ChatCommand>) -> Result<(), AxiomataError> {
    match command {
        Some(ChatCommand::Run { name }) if !valid_command_name(name) => {
            Err(AxiomataError::AgentApi {
                backend: "chat",
                message: format!("{name:?} is not a command name"),
            })
        }
        _ => Ok(()),
    }
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
    extras: ChatExtras,
) -> Result<ChatReply, AxiomataError> {
    check_command(extras.command.as_ref())?;
    // Only Glut keeps a chat turn's journal; a resume elsewhere would be an empty, billed turn.
    if extras.resume.is_some()
        && (config.agents.chat_harness != HarnessId::Axh || extras.command.is_some())
    {
        return Err(AxiomataError::AgentApi {
            backend: "chat",
            message: "only a Glut chat turn can be resumed, and not as a command".to_owned(),
        });
    }
    let turn_config = with_turn_model(config, &extras)?;
    let config = turn_config.as_ref().unwrap_or(config);
    let harness = config.agents.chat_harness;
    let request = ChatRequest {
        message,
        session_id,
        mode,
        cwd: config.workspace_root.clone(),
        timeout: Duration::from_secs(config.agents.skill_timeout_secs),
        system_prompt_file: module_context_if_present(),
        model: None,
        auto_approve_tools: config.agents.auto_approve_tools,
        allowed_tools,
        command: extras.command,
        resume: extras.resume,
        effort: extras.effort,
    };
    match harness {
        // Claude Code serves Anthropic only (harness-choice D2); a hand-edited config that pairs it with another
        // provider refuses the turn rather than sending that provider's model to Anthropic.
        HarnessId::ClaudeCode if config.agents.chat_provider != ProviderId::Anthropic => {
            Err(AxiomataError::HarnessUnavailable {
                harness: harness.as_str().to_owned(),
                reason: format!(
                    "Claude Code runs Anthropic only, the chat provider is {}",
                    config.agents.chat_provider.as_str()
                ),
            })
        }
        HarnessId::ClaudeCode => {
            claude_code::chat(ChatRequest {
                model: default_chat_model(config),
                ..request
            })
            .await
        }
        // Glut has no Anthropic client yet (harness-choice D6): a hand-edited config pairing the two refuses the
        // turn rather than reaching for another provider.
        HarnessId::Axh if config.agents.chat_provider == ProviderId::Anthropic => {
            Err(AxiomataError::HarnessUnavailable {
                harness: harness.as_str().to_owned(),
                reason: "Glut has no Anthropic client yet; choose open_router or ollama as the chat provider"
                    .to_owned(),
            })
        }
        HarnessId::Axh => {
            axh::chat(
                ChatRequest {
                    model: default_chat_model(config),
                    ..request
                },
                config,
            )
            .await
        }
    }
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
    chat_and_record_with(
        config,
        db,
        message,
        session_id,
        mode,
        allowed_tools,
        ChatExtras::default(),
    )
    .await
}

/// [`chat_and_record`] with a turn model or a command (#108); the turn is metered at the model it ran on.
pub async fn chat_and_record_with(
    config: &Config,
    db: &std::sync::Mutex<rusqlite::Connection>,
    message: String,
    session_id: Option<String>,
    mode: ChatMode,
    allowed_tools: Option<String>,
    extras: ChatExtras,
) -> Result<ChatReply, AxiomataError> {
    {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        crate::spend::guard_redirected_turn(&conn, config, ProviderRole::Chat)?;
    }
    let model = with_turn_model(config, &extras)?
        .as_ref()
        .map_or_else(|| default_chat_model(config), default_chat_model);
    let reply = chat(config, message, session_id, mode, allowed_tools, extras).await?;
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
                reply.cached_input_tokens,
                reply.output_tokens,
            )
            .or(reply.cost_usd),
            input_tokens: reply.input_tokens,
            cached_input_tokens: reply.cached_input_tokens,
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
    /// The run's name — the skill or routine that triggered it. For `axh` it
    /// names the gate's narrowing, so a refused call says which skill
    /// narrowed the run (`docs/plans/harness-hs6b.md`, B3).
    pub name: Option<String>,
    /// Working directory for the agent: the session's location, so workspace
    /// context and project opencode config load from here.
    pub cwd: PathBuf,
    /// Hard wall-clock limit. On expiry the run fails with
    /// [`AxiomataError::AgentTimeout`] and the turn is interrupted.
    pub timeout: Duration,
    /// A workspace file whose contents are prepended to the prompt (the module
    /// bridge manifest); opencode has no `--append-system-prompt-file`, so the
    /// backend does the prepending itself.
    pub system_prompt_file: Option<PathBuf>,
    /// For opencode: the full `provider/model` id built by
    /// [`opencode::model_id`]. For `axh`: the **bare** model as the API names
    /// it (`deepseek/deepseek-chat`) — see
    /// `skills::runner::resolve_run_model`. Ignored by Ollama (its model lives
    /// in the backend enum).
    pub model: Option<String>,
    /// Whether tool use is pre-approved for the run; see
    /// [`crate::config::AgentDefaults::auto_approve_tools`]. `false` makes a
    /// tool-using run fail on unapproved calls instead of executing them —
    /// the prompt-injection guard for prompts that embed untrusted content.
    pub auto_approve_tools: bool,
    /// Carried over from `SKILL.md` frontmatter for symmetry; the opencode
    /// backend ignores it (MCP servers come from opencode's own config). The
    /// `axh` backend makes it the run's permissions — what it names is
    /// allowed, the rest refused (HS6b, B3). Kept so callers don't lose the
    /// declared tools.
    pub allowed_tools: Option<String>,
    /// The reply must contain one JSON object (`output: json` in the skill's
    /// frontmatter). The opencode backend asks once more, in the same
    /// session, when the first reply has none; Ollama ignores it.
    pub expects_json: bool,
    /// The run only looks (`access: read` in the skill's frontmatter,
    /// `docs/plans/agent-isolation.md` D8): the opencode backend runs it on
    /// the read instance, which refuses every edit and every command but the
    /// read-only ones; Claude Code adds the same read commands and denies every
    /// edit tool. Ollama and `axh` ignore it.
    pub read_only: bool,
    /// The run's journal (durable runs, `docs/plans/durable.md`): only `axh` keeps one, so a run the app lost goes
    /// on from its last step; with `resume` the run goes on from it instead of starting. Other backends ignore it.
    pub journal: Option<RunJournal>,
    /// The skill's `effort:` frontmatter (`docs/plans/glut-effort.md`, E1): only `axh` reads it, over
    /// `[agents.glut] skill_effort`; a level it does not know fails the run before it starts.
    pub effort: Option<String>,
}

/// A run's journal id, and whether the run goes on from it (`resume`) or starts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunJournal {
    pub id: String,
    pub resume: bool,
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
    /// The reply. For opencode the joined `text` parts of the turn's answers —
    /// byte-for-byte what the model replied, so a connector skill's digest
    /// JSON is what lands here.
    pub stdout: String,
    /// Why an opencode turn failed, or empty (success, Ollama).
    pub stderr: String,
    /// `0` for success, `1` for an opencode turn that ran but failed.
    /// Always `0` for Ollama, whose failures surface as `Err` instead.
    pub exit_code: i32,
    /// Wall-clock duration of the run, in milliseconds.
    pub duration_ms: u64,
    /// The cost Opencode computed for the turn, when the provider reported
    /// one. `None` for Ollama and for a free turn.
    pub cost_usd: Option<f64>,
    /// Input tokens across the turn's answers. `None` when none carried counts.
    pub input_tokens: Option<u64>,
    /// The part of the input tokens read from the provider's prompt cache, billed at its lower price (glut-round
    /// CP4). `None` when the backend does not say.
    pub cached_input_tokens: Option<u64>,
    /// Output tokens across the turn's answers. `None` when none carried counts.
    pub output_tokens: Option<u64>,
    /// How many answering messages (model steps) the agent loop took.
    pub num_turns: Option<u32>,
}

impl AgentRunResult {
    /// Whether the run reported success (`exit_code == 0`).
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// A result without cost/token data — the Ollama backend's (it has no
    /// usage envelope).
    pub(crate) fn bare(stdout: String, stderr: String, exit_code: i32, duration_ms: u64) -> Self {
        Self {
            stdout,
            stderr,
            exit_code,
            duration_ms,
            cost_usd: None,
            input_tokens: None,
            cached_input_tokens: None,
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
    fn resolve_maps_opencode_onto_the_skill_harness_now_that_it_left() {
        // `docs/plans/opencode-exit.md`, O3: a skill or routine that still names Opencode runs on today's harness.
        let mut config = Config::default();
        config.agents.skill_harness = HarnessId::Axh;
        assert_eq!(
            AgentBackend::resolve(BACKEND_OPENCODE, None, &config).unwrap(),
            AgentBackend::Axh
        );
    }

    #[test]
    fn resolve_maps_axh_identifier_onto_the_own_harness() {
        // HS6b: `backend: axh` is a first-class id; the model override is not
        // its business (the frontmatter/`skill_model` resolution happens in
        // `resolve_run_model`), so it resolves the same whatever is passed.
        for override_model in [None, Some("ignored")] {
            let backend =
                AgentBackend::resolve(BACKEND_AXH, override_model, &Config::default()).unwrap();
            assert_eq!(backend, AgentBackend::Axh);
            assert_eq!(backend.id(), "axh");
        }
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
    fn resume_lists_the_chat_turns_of_the_chat_harness_only() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE chat_turns (id INTEGER PRIMARY KEY, session_id TEXT, mode TEXT, provider TEXT, model TEXT, \
             is_error INTEGER DEFAULT 0, cost_usd REAL, input_tokens INTEGER, output_tokens INTEGER, \
             num_turns INTEGER, duration_ms INTEGER DEFAULT 0, created_at TEXT NOT NULL, cached_input_tokens INTEGER);
             INSERT INTO chat_turns (session_id, mode, created_at) VALUES
               ('ses_a', 'chat', '2026-10-20T08:00:00Z'), ('ses_a', 'plan', '2026-10-20T09:00:00Z'),
               ('ses_b', 'chat', '2026-10-20T10:00:00Z'), ('ses_c', 'instruct', '2026-10-20T11:00:00Z'),
               ('ses_d', 'commit_message', '2026-10-20T11:30:00Z'), ('axh-gone', 'chat', '2026-10-20T12:00:00Z');
             INSERT INTO chat_turns (session_id, mode, is_error, created_at) VALUES ('ses_e', 'chat', 1, 'z');",
        )
        .unwrap();
        let db = std::sync::Mutex::new(conn);
        let mut config = Config::default();
        // Opencode's `ses_` conversations name no harness of today's.
        config.agents.chat_harness = HarnessId::ClaudeCode;
        assert!(chat_sessions(&config, &db).unwrap().is_empty());
        // Glut's conversation lists only while its file is there.
        config.agents.chat_harness = HarnessId::Axh;
        assert!(chat_sessions(&config, &db).unwrap().is_empty());
    }

    #[tokio::test]
    async fn undo_reaches_only_a_conversation_the_chat_recorded() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE chat_turns (id INTEGER PRIMARY KEY, session_id TEXT, mode TEXT, created_at TEXT NOT NULL);
             INSERT INTO chat_turns (session_id, mode, created_at) VALUES ('ses_card', 'instruct', 'x');",
        )
        .unwrap();
        let db = std::sync::Mutex::new(conn);
        let mut config = Config::default();
        config.agents.chat_harness = HarnessId::Axh;
        for id in ["ses_card", "ses_never", "../x"] {
            let err = chat_undo(&config, &db, id).await.unwrap_err();
            assert!(
                err.to_string().contains("not a conversation of this chat"),
                "{id}: {err}"
            );
        }
    }

    /// Durable runs: a resume is Glut's, and never a command — elsewhere it would be an empty, billed turn.
    #[tokio::test]
    async fn only_a_glut_chat_turn_is_resumed() {
        let mut config = Config::default();
        config.agents.chat_harness = HarnessId::ClaudeCode;
        let resume = |command: Option<ChatCommand>| ChatExtras {
            resume: Some("run-1".into()),
            command,
            ..ChatExtras::default()
        };
        let err = chat(
            &config,
            String::new(),
            None,
            ChatMode::Chat,
            None,
            resume(None),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("only a Glut chat turn"), "{err}");
        config.agents.chat_harness = HarnessId::Axh;
        let err = chat(
            &config,
            String::new(),
            None,
            ChatMode::Chat,
            None,
            resume(Some(ChatCommand::Compact)),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("only a Glut chat turn"), "{err}");
    }

    #[test]
    fn a_title_is_the_first_line_cut_short() {
        assert_eq!(title_of("\n  fix the tests\nand more"), "fix the tests");
        assert_eq!(
            title_of(&"x".repeat(TITLE_CHARS + 5)),
            format!("{}…", "x".repeat(TITLE_CHARS))
        );
        assert!(session_of(
            HarnessId::ClaudeCode,
            "8d6a44a7-1f5f-4938-86bd-51e771359397"
        ));
        assert!(!session_of(HarnessId::ClaudeCode, "ses_abc"));
        assert!(session_of(HarnessId::Axh, "axh-123"));
    }

    #[test]
    fn a_turn_model_or_command_name_is_one_plain_word() {
        for good in [
            "deepseek/deepseek-v4-flash",
            "claude-haiku-5-5",
            "qwen3.8:27b",
            "apple-reminders:weekly",
        ] {
            assert!(valid_chat_name(good), "{good}");
        }
        for bad in [
            "",
            "-p",
            "a b",
            "a\nb",
            "a\"b",
            &"x".repeat(MAX_CHAT_NAME + 1),
        ] {
            assert!(!valid_chat_name(bad), "{bad:?}");
        }
        assert!(check_command(Some(&ChatCommand::Run { name: "x y".into() })).is_err());
        for path_like in ["../x", "a/b", "a@b"] {
            assert!(
                check_command(Some(&ChatCommand::Run {
                    name: path_like.into()
                }))
                .is_err(),
                "{path_like}"
            );
        }
        assert!(
            check_command(Some(&ChatCommand::Run {
                name: "apple-reminders:weekly".into()
            }))
            .is_ok()
        );
        assert!(check_command(Some(&ChatCommand::Compact)).is_ok());
    }

    #[test]
    fn a_turn_model_replaces_the_chat_model_of_the_chat_provider_only() {
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::OpenRouter;
        config.agents.skill_provider = ProviderId::OpenRouter;
        config.agents.providers.insert(
            ProviderId::OpenRouter,
            crate::config::ProviderSettings {
                base_url: None,
                api_key: None,
                chat_model: "configured".into(),
                skill_model: "skill".into(),
            },
        );
        assert!(
            with_turn_model(&config, &ChatExtras::default())
                .unwrap()
                .is_none()
        );
        let extras = ChatExtras {
            model: Some("picked/model".into()),
            ..ChatExtras::default()
        };
        // OpenRouter is metered: an unpriced model is refused, a priced one taken.
        assert!(with_turn_model(&config, &extras).is_err());
        config.agents.costs.insert(
            "picked/model".into(),
            crate::config::ModelCost {
                input_per_m: 0.1,
                cached_input_per_m: None,
                output_per_m: 0.2,
            },
        );
        let turned = with_turn_model(&config, &extras).unwrap().unwrap();
        assert_eq!(default_chat_model(&turned).as_deref(), Some("picked/model"));
        assert_eq!(default_skill_model(&turned).as_deref(), Some("skill"));
        assert_eq!(default_chat_model(&config).as_deref(), Some("configured"));
        let flag = ChatExtras {
            model: Some("--dangerously".into()),
            ..ChatExtras::default()
        };
        assert!(with_turn_model(&config, &flag).is_err());
    }

    /// Glut refuses a chat turn paired with Anthropic, and Claude Code one paired with another provider than
    /// Anthropic — neither hops to Opencode. Claude Code with Anthropic gets as far as the runner, which a unit test
    /// never starts.
    #[tokio::test]
    async fn chat_runs_on_the_chat_harness_and_never_hops_to_opencode() {
        let turn = |config: Config| async move {
            chat(
                &config,
                "hi".to_owned(),
                None,
                ChatMode::Chat,
                None,
                ChatExtras::default(),
            )
            .await
            .unwrap_err()
        };
        // Glut with the default (Anthropic) chat provider: no client for it yet (D6).
        let mut config = Config::default();
        config.agents.chat_harness = HarnessId::Axh;
        let err = turn(config).await;
        assert!(
            matches!(&err, AxiomataError::HarnessUnavailable { harness, .. } if harness == "axh"),
            "{err:?}"
        );

        let mut config = Config::default();
        config.agents.chat_harness = HarnessId::ClaudeCode;
        config.agents.chat_provider = ProviderId::OpenRouter;
        let err = turn(config).await;
        assert!(
            matches!(&err, AxiomataError::HarnessUnavailable { reason, .. } if reason.contains("open_router")),
            "{err:?}"
        );

        let mut config = Config::default();
        config.agents.chat_harness = HarnessId::ClaudeCode;
        let err = turn(config).await;
        assert!(
            matches!(
                &err,
                AxiomataError::AgentApi { backend, message }
                    if *backend == BACKEND_CLAUDE_CODE && message.contains("never reach")
            ),
            "{err:?}"
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
    fn resolve_maps_retired_backend_ids_onto_the_skill_harness() {
        // Skills seeded before the opencode consolidation carry these ids in
        // their frontmatter, and a seed never overwrites an existing copy — so
        // they must keep resolving (onto the configured harness) rather than
        // break every run with `UnknownAgentBackend`.
        let mut config = Config::default();
        for retired in ["claude-code", "ollama-agent"] {
            let backend = AgentBackend::resolve(retired, None, &config).unwrap();
            assert_eq!(
                backend,
                AgentBackend::ClaudeCode,
                "{retired} -> the default harness"
            );
        }
        config.agents.skill_harness = crate::config::HarnessId::Axh;
        assert_eq!(
            AgentBackend::resolve("claude-code", None, &config).unwrap(),
            AgentBackend::Axh
        );
    }

    #[test]
    fn resolve_maps_claude_code_and_a_retired_id_under_it() {
        assert_eq!(
            AgentBackend::resolve(BACKEND_CLAUDE_CODE, None, &Config::default()).unwrap(),
            AgentBackend::ClaudeCode
        );
        let mut config = Config::default();
        config.agents.skill_harness = HarnessId::ClaudeCode;
        // A retired id follows the skill harness.
        let backend = AgentBackend::resolve("claude-code", None, &config).unwrap();
        assert_eq!(backend, AgentBackend::ClaudeCode);
        assert_eq!(backend.id(), "claude_code");
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
