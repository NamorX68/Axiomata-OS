//! Agent backends: `ClaudeCode` (headless Claude Code CLI), `Ollama` (local
//! models via the Ollama HTTP API, single completion) and `OllamaAgent` (the
//! same local models, but a bounded tool-call loop over MCP servers — the
//! "lean local agent" of Stufe 2).
//!
//! Skill and routine execution dispatches through the [`AgentBackend`] enum
//! rather than a plugin registry or trait-object abstraction — a deliberate
//! choice, since only a few backends are needed and a generic multi-CLI
//! abstraction would be premature. See `docs/architecture.md` §6 for the
//! rationale. A further variant can be added later without reworking the runner
//! or scheduler.
//!
//! Implemented in M1; `OllamaAgent` in Stufe 2 CP2.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::config::{Config, McpServerConfig, ProviderId, ProviderRole};
use crate::error::AxiomataError;

pub mod claude_code;
pub mod ollama;
pub mod ollama_agent;

/// Backend identifier stored verbatim in a `SKILL.md` frontmatter `backend`
/// field and in a routine's DB row. Kept as a plain string on disk so the file
/// format stays self-explanatory.
pub const BACKEND_CLAUDE_CODE: &str = "claude-code";
/// See [`BACKEND_CLAUDE_CODE`].
pub const BACKEND_OLLAMA: &str = "ollama";
/// See [`BACKEND_CLAUDE_CODE`].
pub const BACKEND_OLLAMA_AGENT: &str = "ollama-agent";

/// Upper bound on how much of a backend's output is kept, shared by the raw
/// completion [`AgentBackend::Ollama`] and the tool-call loop
/// [`AgentBackend::OllamaAgent`] (mirrors the Claude Code backend's output
/// cap).
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

/// Which agent runs a given skill or routine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentBackend {
    /// The headless Claude Code CLI (`claude -p`). Full agent loop: tool use,
    /// MCP, skill resolution, automatic loading of the workspace `CLAUDE.md`.
    ClaudeCode,
    /// A single completion call against a local Ollama model — no tools, no
    /// loop. Intended for simple, deterministic tasks (e.g. appending a to-do
    /// item).
    Ollama { model: String },
    /// A bounded tool-call loop against a local Ollama model (`POST /api/chat`
    /// with native tool calling) over the MCP servers a skill's `allowed_tools`
    /// names — the "lean local agent" of Stufe 2: only the skill's SOP + the
    /// tool schemas it needs, no Claude Code system prompt. See
    /// [`ollama_agent`] and `docs/plans/stufe2-lean-ollama-agent.md`.
    OllamaAgent { model: String },
}

impl AgentBackend {
    /// Resolves a backend identifier string plus an optional model override
    /// onto a concrete backend.
    ///
    /// Args:
    ///     backend: `"claude-code"`, `"ollama"`, or `"ollama-agent"`.
    ///     model_override: For `"ollama"` and `"ollama-agent"`, a model name
    ///         that wins over the configured default(s); ignored for
    ///         `"claude-code"`.
    ///     config: Provides the model fallbacks (`agents.ollama_model` and, for
    ///         `ollama-agent`, the Ollama provider's `skill_model`).
    ///
    /// Returns:
    ///     The resolved [`AgentBackend`].
    ///
    /// Errors:
    ///     [`AxiomataError::UnknownAgentBackend`] if `backend` is neither known
    ///     identifier.
    pub fn resolve(
        backend: &str,
        model_override: Option<&str>,
        config: &Config,
    ) -> Result<Self, AxiomataError> {
        match backend {
            BACKEND_CLAUDE_CODE => Ok(Self::ClaudeCode),
            BACKEND_OLLAMA => {
                let model = model_override
                    .map(str::to_owned)
                    .unwrap_or_else(|| config.agents.ollama_model.clone());
                Ok(Self::Ollama { model })
            }
            BACKEND_OLLAMA_AGENT => {
                // One tier more than the plain `Ollama` arm: the provider's
                // `skill_model` sits between `SKILL.md model:` and the legacy
                // `agents.ollama_model` fallback (see the plan, "Model
                // resolution").
                let provider_default = config
                    .agents
                    .providers
                    .get(&ProviderId::Ollama)
                    .map(|s| s.skill_model.trim())
                    .filter(|m| !m.is_empty())
                    .map(str::to_owned);
                let model = model_override
                    .map(str::to_owned)
                    .or(provider_default)
                    .unwrap_or_else(|| config.agents.ollama_model.clone());
                Ok(Self::OllamaAgent { model })
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
            Self::ClaudeCode => BACKEND_CLAUDE_CODE,
            Self::Ollama { .. } => BACKEND_OLLAMA,
            Self::OllamaAgent { .. } => BACKEND_OLLAMA_AGENT,
        }
    }

    /// Runs `request` on this backend, capturing output and wall-clock timing.
    ///
    /// A non-zero `exit_code` in the returned [`AgentRunResult`] means the
    /// agent ran but reported failure; an `Err` means it could not be run at
    /// all (spawn failure, timeout, transport error).
    pub async fn run(&self, request: AgentRequest) -> Result<AgentRunResult, AxiomataError> {
        match self {
            Self::ClaudeCode => claude_code::run(request).await,
            Self::Ollama { model } => ollama::run(request, model).await,
            Self::OllamaAgent { model } => ollama_agent::run(request, model).await,
        }
    }
}

pub use claude_code::{ChatMode, ChatReply};

/// One dashboard-assistant turn on the Claude Code backend, built from the
/// config: cwd = workspace root (so its `CLAUDE.md` loads), the skill timeout,
/// the filtered provider env, and the module manifest
/// (`paths::module_context_path()`) as an appended system prompt if present.
///
/// `allowed_tools` is `None` for a plain assistant-bar turn; a module that
/// needs an instruct turn to reach an MCP tool (e.g. a connector module's
/// write actions) sets it to exactly the tool it needs — see
/// [`AgentRequest::allowed_tools`] for why that's required at all.
pub async fn chat(
    config: &Config,
    message: String,
    session_id: Option<String>,
    mode: ChatMode,
    allowed_tools: Option<String>,
) -> Result<ChatReply, AxiomataError> {
    claude_code::chat(claude_code::ChatRequest {
        message,
        session_id,
        mode,
        cwd: config.workspace_root.clone(),
        timeout: Duration::from_secs(config.agents.skill_timeout_secs),
        env: crate::skills::runner::claude_env(
            config,
            &AgentBackend::ClaudeCode,
            ProviderRole::Chat,
        ),
        system_prompt_file: module_context_if_present(),
        allowed_tools,
        model: default_chat_model(config),
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
            model,
            is_error: reply.is_error,
            cost_usd: reply.cost_usd,
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
    /// The prompt handed to the agent. For Claude Code it is passed to
    /// `claude -p`; for Ollama it is the raw completion prompt; for
    /// `ollama-agent` it is the user turn of the tool-call loop (the
    /// `SKILL.md` body).
    pub prompt: String,
    /// Working directory for the agent. Claude Code treats this as its project
    /// root (loads `CLAUDE.md`, resolves project skills); ignored by Ollama.
    pub cwd: PathBuf,
    /// Hard wall-clock limit. On expiry the run fails with
    /// [`AxiomataError::AgentTimeout`] and the child process (if any) is
    /// killed.
    pub timeout: Duration,
    /// Extra environment variables for the agent process — used for Claude Code
    /// provider routing (`ANTHROPIC_BASE_URL`, `CLAUDE_CODE_USE_BEDROCK`, …).
    /// Ignored by Ollama.
    pub env: Vec<(String, String)>,
    /// Appended to Claude Code's system prompt (`--append-system-prompt-file`);
    /// the dashboard's module manifest when it exists. Ignored by Ollama.
    pub system_prompt_file: Option<PathBuf>,
    /// `claude --model`; `None` lets the CLI choose. Ignored by Ollama (its
    /// model lives in the backend enum).
    pub model: Option<String>,
    /// `claude --allowedTools` — a space/comma-separated tool allow-list
    /// (Claude Code's own syntax, passed through verbatim), e.g.
    /// `"mcp__apple-reminders__calendar_events"`. Needed because MCP tool
    /// calls are not covered by `--permission-mode`: a `-p` run with no
    /// interactive approver denies them outright otherwise, silently, no
    /// matter the permission mode (found live while building the calendar
    /// skill — the run "succeeds" but the tool call is refused). `None`
    /// passes no flag, i.e. no MCP tools beyond whatever the permission mode
    /// already allows. For `ollama-agent` it is instead the **derivation
    /// source** for which MCP servers to spawn and which of their tools to
    /// advertise to the model (the `mcp__<server>__<tool>` prefixes), not a
    /// verbatim allow-list. Ignored by plain Ollama (no tool use at all).
    pub allowed_tools: Option<String>,
    /// MCP stdio servers this run may spawn — a copy of `config.mcp_servers`.
    /// Read only by the `ollama-agent` backend, which spawns just the servers
    /// its `allowed_tools` name (see [`ollama_agent`]); every other backend
    /// ignores this field.
    pub mcp_servers: BTreeMap<String, McpServerConfig>,
    /// Local Ollama daemon URL for `ollama-agent`
    /// (`config.agents.providers[Ollama].base_url`); `None` → the library
    /// default (`http://127.0.0.1:11434`). Ignored by other backends.
    pub ollama_base_url: Option<String>,
}

/// The chat provider's `chat_model`, for interactive dashboard-assistant
/// turns — `None` (let the CLI pick its own default) if unset or the chat
/// provider is missing from `config.agents.providers` (shouldn't happen once
/// [`crate::config::Config::load`] has run its migration, but a config built
/// by hand in a test could still lack it).
pub fn default_chat_model(config: &Config) -> Option<String> {
    provider_model(config, config.agents.chat_provider, |settings| {
        &settings.chat_model
    })
}

/// The skill provider's `skill_model` — the fallback for skill/routine runs
/// that don't pin their own model. A skill's `SKILL.md` frontmatter `model:`
/// takes precedence over this in [`crate::skills::runner::execute_skill`];
/// this is only the fallback source, unchanged in that respect from the old
/// single global `claude_model`.
pub fn default_skill_model(config: &Config) -> Option<String> {
    provider_model(config, config.agents.skill_provider, |settings| {
        &settings.skill_model
    })
}

/// Shared lookup behind [`default_chat_model`] / [`default_skill_model`]:
/// resolves `provider`'s settings, then reads whichever model field `pick`
/// names off of it, treating a missing/blank value as "let the CLI choose".
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
    /// Captured standard output (Claude Code) or completion text (Ollama).
    /// For a Claude Code skill run this is the **unwrapped** `result` string
    /// from the `--output-format json` envelope, so it is byte-for-byte what
    /// the old plain-text mode produced — the JSON wrapper only exists to
    /// carry the cost/token fields below.
    pub stdout: String,
    /// Captured standard error (Claude Code) or empty (Ollama).
    pub stderr: String,
    /// Process exit code for Claude Code. Synthetic for Ollama: always `0`
    /// here, since Ollama failures surface as `Err` rather than a run result.
    pub exit_code: i32,
    /// Wall-clock duration of the run, in milliseconds.
    pub duration_ms: u64,
    /// `total_cost_usd` from the Claude Code JSON envelope, when the run went
    /// through a paid provider and the CLI reported it. `None` for Ollama, for
    /// the subscription-billed Anthropic path (the CLI reports `0.0` or omits
    /// it), and whenever the envelope could not be parsed.
    pub cost_usd: Option<f64>,
    /// `usage.input_tokens` from the JSON envelope (the non-cached input
    /// count, as the CLI reports it). `None` when unavailable.
    pub input_tokens: Option<u64>,
    /// `usage.output_tokens` from the JSON envelope. `None` when unavailable.
    pub output_tokens: Option<u64>,
    /// `num_turns` from the JSON envelope — how many assistant turns the agent
    /// loop took. `None` when unavailable.
    pub num_turns: Option<u32>,
}

impl AgentRunResult {
    /// Whether the run reported success (`exit_code == 0`).
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// A result carrying only the process-level fields (no cost/token data
    /// parsed yet). Used by [`claude_code::spawn_and_collect`] before the
    /// JSON envelope is inspected and by backends that have no such envelope.
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
    use crate::config::{AgentDefaults, Config, ProviderId};

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
    fn resolve_maps_claude_code_identifier() {
        let backend = AgentBackend::resolve(BACKEND_CLAUDE_CODE, None, &Config::default()).unwrap();
        assert_eq!(backend, AgentBackend::ClaudeCode);
        assert_eq!(backend.id(), "claude-code");
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

    /// `ollama-agent` has one more precedence tier than `ollama`: the
    /// Ollama provider's `skill_model` sits between an explicit override and
    /// the legacy `agents.ollama_model` fallback.
    #[test]
    fn resolve_ollama_agent_uses_config_default_model_with_no_override_and_blank_provider_model() {
        let mut config = config_with_ollama_model("llama3.2");
        config
            .agents
            .providers
            .get_mut(&ProviderId::Ollama)
            .unwrap()
            .skill_model = "".to_owned();

        let backend = AgentBackend::resolve(BACKEND_OLLAMA_AGENT, None, &config).unwrap();
        assert_eq!(
            backend,
            AgentBackend::OllamaAgent {
                model: "llama3.2".to_owned()
            }
        );
        assert_eq!(backend.id(), "ollama-agent");
    }

    #[test]
    fn resolve_ollama_agent_prefers_a_provider_skill_model_over_the_default_model() {
        let mut config = config_with_ollama_model("llama3.2");
        config
            .agents
            .providers
            .get_mut(&ProviderId::Ollama)
            .unwrap()
            .skill_model = "qwen3:30b".to_owned();

        let backend = AgentBackend::resolve(BACKEND_OLLAMA_AGENT, None, &config).unwrap();
        assert_eq!(
            backend,
            AgentBackend::OllamaAgent {
                model: "qwen3:30b".to_owned()
            }
        );
    }

    #[test]
    fn resolve_ollama_agent_treats_a_blank_provider_model_as_unset() {
        let mut config = config_with_ollama_model("llama3.2");
        config
            .agents
            .providers
            .get_mut(&ProviderId::Ollama)
            .unwrap()
            .skill_model = "   ".to_owned();

        let backend = AgentBackend::resolve(BACKEND_OLLAMA_AGENT, None, &config).unwrap();
        assert_eq!(
            backend,
            AgentBackend::OllamaAgent {
                model: "llama3.2".to_owned()
            }
        );
    }

    #[test]
    fn resolve_ollama_agent_prefers_the_model_override_over_the_provider_model() {
        let mut config = config_with_ollama_model("llama3.2");
        config
            .agents
            .providers
            .get_mut(&ProviderId::Ollama)
            .unwrap()
            .skill_model = "qwen3:30b".to_owned();

        let backend =
            AgentBackend::resolve(BACKEND_OLLAMA_AGENT, Some("mistral"), &config).unwrap();
        assert_eq!(
            backend,
            AgentBackend::OllamaAgent {
                model: "mistral".to_owned()
            }
        );
    }

    #[test]
    fn resolve_rejects_unknown_identifier() {
        let err = AgentBackend::resolve("opencode", None, &Config::default()).unwrap_err();
        assert!(matches!(
            err,
            AxiomataError::UnknownAgentBackend { backend } if backend == "opencode"
        ));
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
        // Untouched provider entries stay independent — pointing the chat role
        // at OpenRouter must not leak Anthropic's own chat_model through.
        assert_ne!(
            default_chat_model(&config),
            Some("claude-sonnet-5".to_owned())
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
        // Distinct from the blank-field case above: here the active
        // provider has no entry in `providers` at all (a hand-built test
        // config, or — per the doc comment on `default_chat_model` — a
        // config that somehow skipped `Config::load`'s migration), so the
        // lookup itself must short-circuit to `None` rather than panic.
        let mut config = Config::default();
        config.agents.providers.remove(&ProviderId::Anthropic);

        assert_eq!(default_chat_model(&config), None);
        assert_eq!(default_skill_model(&config), None);
    }
}
