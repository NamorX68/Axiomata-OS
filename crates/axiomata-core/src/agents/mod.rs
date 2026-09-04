//! Agent backends: `ClaudeCode` (headless Claude Code CLI) and `Ollama` (local
//! models via the Ollama HTTP API).
//!
//! Skill and routine execution dispatches through the [`AgentBackend`] enum
//! rather than a plugin registry or trait-object abstraction — a deliberate
//! choice, since only two backends are needed and a generic multi-CLI
//! abstraction would be premature. See `docs/architecture.md` §6 for the
//! rationale. A further variant can be added later without reworking the runner
//! or scheduler.
//!
//! Implemented in M1.

use std::path::PathBuf;
use std::time::Duration;

use crate::config::Config;
use crate::error::AxiomataError;

pub mod claude_code;
pub mod ollama;

/// Backend identifier stored verbatim in a `SKILL.md` frontmatter `backend`
/// field and in a routine's DB row. Kept as a plain string on disk so the file
/// format stays self-explanatory.
pub const BACKEND_CLAUDE_CODE: &str = "claude-code";
/// See [`BACKEND_CLAUDE_CODE`].
pub const BACKEND_OLLAMA: &str = "ollama";

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
}

impl AgentBackend {
    /// Resolves a backend identifier string plus an optional model override
    /// onto a concrete backend.
    ///
    /// Args:
    ///     backend: `"claude-code"` or `"ollama"`.
    ///     model_override: For `"ollama"`, a model name that wins over the
    ///         configured default; ignored for `"claude-code"`.
    ///     config: Provides `agents.ollama_model` as the fallback model.
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
        env: crate::skills::runner::claude_env(config, &AgentBackend::ClaudeCode),
        system_prompt_file: module_context_if_present(),
        allowed_tools,
        model: default_claude_model(config),
    })
    .await
}

/// A single headless agent invocation.
#[derive(Debug, Clone)]
pub struct AgentRequest {
    /// The prompt handed to the agent. For Claude Code it is passed to
    /// `claude -p`; for Ollama it is the raw completion prompt.
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
    /// already allows. Ignored by Ollama (no tool use at all).
    pub allowed_tools: Option<String>,
}

/// `config.agents.claude_model` unless empty.
pub fn default_claude_model(config: &Config) -> Option<String> {
    let m = config.agents.claude_model.trim();
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
    pub stdout: String,
    /// Captured standard error (Claude Code) or empty (Ollama).
    pub stderr: String,
    /// Process exit code for Claude Code. Synthetic for Ollama: always `0`
    /// here, since Ollama failures surface as `Err` rather than a run result.
    pub exit_code: i32,
    /// Wall-clock duration of the run, in milliseconds.
    pub duration_ms: u64,
}

impl AgentRunResult {
    /// Whether the run reported success (`exit_code == 0`).
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AgentDefaults, Config};

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

    #[test]
    fn resolve_rejects_unknown_identifier() {
        let err = AgentBackend::resolve("opencode", None, &Config::default()).unwrap_err();
        assert!(matches!(
            err,
            AxiomataError::UnknownAgentBackend { backend } if backend == "opencode"
        ));
    }
}
