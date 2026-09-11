//! Resolves a skill's agent backend and executes it, recording the result.
//!
//! Entry points:
//!
//! - [`execute_skill`] — resolves a skill by name and runs it, returning an
//!   **unpersisted** [`RunRecord`] (`id: None`). Touches no database.
//! - [`execute_prompt`] — runs a raw prompt string on a named backend, with no
//!   `SKILL.md` involved. The routine scheduler's `prompt` target uses this.
//! - [`execute_and_record_skill`] — `execute_skill`, then take the database
//!   `Mutex` just long enough to write the row via [`runlog::record_run`]. The
//!   agent call happens before any lock is taken, so this is safe to call from
//!   an async task. Both the CLI and the Tauri command use it.
//!
//! It ties together the registry ([`crate::skills::registry`]), the agent
//! backends ([`crate::agents`]), and the run log ([`crate::skills::runlog`]).
//!
//! Implemented in M1.

use std::collections::{BTreeMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use chrono::Utc;
use rusqlite::Connection;

use crate::agents::{AgentBackend, AgentRequest, AgentRunResult};
use crate::config::{Config, ProviderRole};
use crate::error::AxiomataError;
use crate::skills::model::{RunRecord, RunSource, RunStatus};
use crate::skills::registry;
use crate::skills::runlog;

/// Runs the skill named `name`, returning an unpersisted [`RunRecord`].
///
/// Resolution failures (no such skill, malformed `SKILL.md`) are returned as
/// `Err` — there is no run to attribute. Once the skill is found, every other
/// outcome (success, non-zero exit, unknown backend, spawn failure, timeout,
/// API error, and a `prepend_files` path error) yields `Ok(record)` with
/// `record.id == None`; the caller inspects [`RunRecord::status`] and persists
/// via [`runlog::record_run`].
///
/// Args:
///     name: Skill name to run.
///     config: Supplies the working directory (`workspace_root`), the Ollama
///         default model, the run timeout, and the `claude` provider
///         environment.
///
/// Errors:
///     [`AxiomataError::SkillNotFound`] / [`AxiomataError::InvalidSkill`] if the
///     skill cannot be resolved; [`AxiomataError::AlreadyRunning`] if `name` is
///     mid-run elsewhere (a real `Err`, deliberately *not* recorded — see
///     [`run_on_backend`]'s doc comment).
pub async fn execute_skill(name: &str, config: &Config) -> Result<RunRecord, AxiomataError> {
    let skill = registry::find_skill(name)?;

    // `effective_backend` resolves the skill's `local_backend` when the
    // configured skill provider is Ollama — so one `skill_provider` switch
    // moves every opted-in skill (e.g. the connector digests) onto
    // `ollama-agent`, and flipping back to a cloud provider reverts them.
    let backend_id = skill.effective_backend(config);

    // An unknown backend string in the frontmatter is a recordable failure: the
    // skill exists and someone tried to run it.
    let backend = match AgentBackend::resolve(backend_id, skill.model.as_deref(), config) {
        Ok(backend) => backend,
        Err(err) => {
            return Ok(misconfigured_skill_record(
                &skill, backend_id,
                // The backend string didn't resolve, so there is no provider
                // to attribute this to.
                None, &err,
            ));
        }
    };

    let model = match backend {
        AgentBackend::ClaudeCode => resolve_skill_model(skill.model.as_deref(), config),
        AgentBackend::Ollama { .. } | AgentBackend::OllamaAgent { .. } => None,
    };
    // The skill's own instruction body is the prompt on every backend. Claude
    // Code used to be sent `/<name>` instead, on the assumption that its own
    // slash-command skill machinery would resolve it — it doesn't: that
    // machinery looks under `.claude/skills/` (project) or `~/.claude/skills/`
    // (global), never Axiomata's own `~/.axiomata/skills/` registry, so every
    // run got back a plain "Unknown command: /<name>" chat reply and — since
    // `claude -p` still exits 0 for an ordinary reply — was logged as a
    // success despite the SOP never reaching the model.
    let prompt = match build_prompt(config, &skill) {
        Ok(prompt) => prompt,
        // A prompt-build failure (today: a `prepend_files` entry breaking the
        // workspace-relative guard — `build_prompt` is the only fallible step)
        // means the skill exists but is misconfigured, which is not a
        // resolution failure: record a `Failed` run the dashboard can show
        // (matching the unknown-backend arm above), rather than returning
        // `Err`, which `execute_and_record_skill` would drop without
        // persisting anything.
        Err(err) => {
            // Provider is known here: the backend already resolved, only the
            // prompt build failed.
            return Ok(misconfigured_skill_record(
                &skill,
                backend_id,
                provider_label(&backend, config),
                &err,
            ));
        }
    };
    run_on_backend(
        &skill.name,
        prompt,
        &backend,
        config,
        model,
        skill.allowed_tools.clone(),
        skill.timeout_secs,
    )
    .await
}

/// Assembles the prompt for a skill run: the skill's declared
/// `prepend_files`, read from the workspace and prefixed as
/// `## Context file: <rel>` blocks (missing/blank files contribute nothing),
/// then the `SKILL.md` body itself.
///
/// This bridge exists for a skill that references a workspace file (e.g.
/// `mail-digest`'s `Mail/.topics.md`) but runs on a backend with no file tool
/// — the local `ollama-agent` only has its MCP tools. Applies to every
/// backend: a `claude-code` run gets the topics inline instead of reading
/// them, which is strictly fine.
///
/// Errors:
///     [`AxiomataError::InvalidSkill`] if a `prepend_files` entry is an
///     absolute path or contains `..` (workspace-relative only, no escape).
fn build_prompt(config: &Config, skill: &registry::Skill) -> Result<String, AxiomataError> {
    let mut prompt = String::new();
    for rel in &skill.prepend_files {
        if rel.contains("..") || std::path::Path::new(rel).is_absolute() {
            return Err(AxiomataError::InvalidSkill {
                path: skill.path.clone(),
                reason: format!("prepend_files entry {rel:?} must be a workspace-relative path"),
            });
        }
        match std::fs::read_to_string(config.workspace_root.join(rel)) {
            // Missing or blank file → contribute nothing. Traced at debug
            // level so a typo'd path is diagnosable instead of silently
            // yielding no context.
            Ok(content) if !content.trim().is_empty() => {
                prompt.push_str(&format!(
                    "## Context file: {rel}\n\n{}\n\n---\n\n",
                    content.trim()
                ));
            }
            _ => {
                tracing::debug!(
                    rel,
                    "prepend_files: skipping a missing/unreadable context file"
                );
            }
        }
    }
    prompt.push_str(&skill.body);
    Ok(prompt)
}

/// A `Failed` [`RunRecord`] for a skill that *resolved* but couldn't be run —
/// the unknown-backend and prompt-build misconfiguration outcomes of
/// [`execute_skill`].
///
/// `provider` is `None` when the failure happened before a backend existed to
/// attribute a provider to it (an unresolvable backend string has no provider);
/// `Some(label)` when the backend resolved (`provider_label`) and only the
/// prompt build failed. Both cases deliberately persist a record rather than
/// return `Err` — `execute_and_record_skill` keeps runs visible for them.
fn misconfigured_skill_record(
    skill: &registry::Skill,
    backend_id: &str,
    provider: Option<String>,
    err: &AxiomataError,
) -> RunRecord {
    failure_record(
        &skill.name,
        backend_id,
        provider,
        Utc::now(),
        0,
        err.to_string(),
    )
}

/// Runs the raw string `prompt` on `backend_id` (`"claude-code"` / `"ollama"`
/// — the routine store validates those two; `"ollama-agent"` is also accepted
/// by `AgentBackend::resolve`),
/// attributing the resulting [`RunRecord`] to `name`. The counterpart to
/// [`execute_skill`] for callers that have a prompt but no `SKILL.md` — the
/// routine scheduler's `prompt` target. An unresolvable `backend_id` yields a
/// `Failed` record, not an `Err`; [`AxiomataError::AlreadyRunning`] (`name`
/// already mid-run elsewhere) is the one case that *is* an `Err` — see
/// [`run_on_backend`]'s doc comment for why that one must not become a
/// persisted `Failed` record.
pub async fn execute_prompt(
    name: &str,
    prompt: String,
    backend_id: &str,
    config: &Config,
) -> Result<RunRecord, AxiomataError> {
    let backend = match AgentBackend::resolve(backend_id, None, config) {
        Ok(backend) => backend,
        Err(err) => {
            return Ok(failure_record(
                name,
                backend_id,
                None,
                Utc::now(),
                0,
                err.to_string(),
            ));
        }
    };
    run_on_backend(
        name,
        prompt,
        &backend,
        config,
        // A `prompt`-target routine is unattended, same as a skill run — use
        // the skill-model fallback, not the interactive chat model.
        crate::agents::default_skill_model(config),
        // Raw prompt targets (no `SKILL.md`) have nowhere to declare a tool
        // allow-list yet — same limitation `execute_prompt`'s doc already
        // implies by only taking a bare string.
        None,
        // ...nor a per-target timeout override; the global default applies.
        None,
    )
    .await
}

/// Names currently mid-run, so a second trigger for the same skill/prompt
/// name (a mount-time auto-refresh racing a manual ↻, a Routine firing while
/// the tile is also refreshing, a dev-mode hot-reload remounting a component
/// whose previous run hadn't finished yet — the exact sequence that produced
/// three concurrent real `mail-digest` runs live, all deadlocking each other
/// against the same Mail.app automation and timing out at 300s with no
/// output at all) fails fast instead of launching a competing agent process.
/// This is a *name* lock, not a slot count like [`crate::agents::claude_code`]'s
/// `MAX_CONCURRENT_AGENT_RUNS` semaphore (which caps total concurrency but
/// happily hands out separate slots to N runs of the *same* skill) — the two
/// guards are complementary, not redundant.
static RUNNING_NAMES: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn running_names() -> &'static Mutex<HashSet<String>> {
    RUNNING_NAMES.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Removes `name` from [`RUNNING_NAMES`] on drop — including on an early
/// return or a panic unwinding through `run_on_backend` — so a run can never
/// wedge the name claimed forever.
struct RunningGuard(String);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        running_names()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(&self.0);
    }
}

/// Shared tail of [`execute_skill`] / [`execute_prompt`]: claims `name` for
/// the duration of the run, builds the request, runs the backend, and maps
/// the outcome onto an unpersisted [`RunRecord`] attributed to `name`.
///
/// Returns [`AxiomataError::AlreadyRunning`] — a real `Err`, not
/// `Ok(Failed record)` — if `name` is already claimed. That distinction
/// matters: both `execute_and_record_skill` and the routine scheduler only
/// call `runlog::record_run` / persist a `routine_runs` history entry when
/// they get `Ok(record)` back, so an `Err` here never becomes a `runs` row.
/// It was one, briefly, live: a dedup rejection recorded as an ordinary
/// `Failed` run sorted *after* the real (successful, or still in-flight) run
/// it was rejected in favour of, so `list_runs`-based "find the latest run"
/// lookups (every connector module's own refresh) picked up the rejection
/// instead of the actual result — the Skills Deck showed red and the tile
/// showed nothing even though a good run had just completed.
async fn run_on_backend(
    name: &str,
    prompt: String,
    backend: &AgentBackend,
    config: &Config,
    model: Option<String>,
    allowed_tools: Option<String>,
    timeout_override_secs: Option<u64>,
) -> Result<RunRecord, AxiomataError> {
    let started_at = Utc::now();
    let already_running = {
        let mut running = running_names()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if running.contains(name) {
            true
        } else {
            running.insert(name.to_string());
            false
        }
    };
    if already_running {
        return Err(AxiomataError::AlreadyRunning {
            name: name.to_string(),
        });
    }
    let _guard = RunningGuard(name.to_string());

    let provider = provider_label(backend, config);
    let request = agent_request(
        prompt,
        backend,
        config,
        model,
        allowed_tools,
        timeout_override_secs,
    );
    Ok(match backend.run(request).await {
        Ok(result) => record_from_result(name, backend.id(), provider, started_at, result),
        Err(err) => {
            let elapsed = (Utc::now() - started_at).num_milliseconds().max(0) as u64;
            failure_record(
                name,
                backend.id(),
                provider,
                started_at,
                elapsed,
                err.to_string(),
            )
        }
    })
}

/// The `provider` string to stamp on a run's record: the **skill** provider
/// for a Claude Code run (every runner path here is an unattended skill /
/// routine run — the interactive chat path lives in [`crate::agents`]),
/// `None` for the local Ollama backend (which has no provider and never bills).
fn provider_label(backend: &AgentBackend, config: &Config) -> Option<String> {
    match backend {
        AgentBackend::ClaudeCode => Some(
            config
                .agents
                .provider_for(ProviderRole::Skill)
                .as_str()
                .to_string(),
        ),
        AgentBackend::Ollama { .. } | AgentBackend::OllamaAgent { .. } => None,
    }
}

/// Builds the [`AgentRequest`] for a prompt on `backend`: the caller-supplied
/// prompt, the workspace as the working directory, the run timeout (the
/// skill's own `timeout_secs` frontmatter if set, else
/// `config.agents.skill_timeout_secs`), and (for Claude Code only) the
/// filtered provider environment.
fn agent_request(
    prompt: String,
    backend: &AgentBackend,
    config: &Config,
    model: Option<String>,
    allowed_tools: Option<String>,
    timeout_override_secs: Option<u64>,
) -> AgentRequest {
    AgentRequest {
        prompt,
        cwd: config.workspace_root.clone(),
        timeout: Duration::from_secs(
            timeout_override_secs.unwrap_or(config.agents.skill_timeout_secs),
        ),
        env: claude_env(config, backend, ProviderRole::Skill),
        // Skill and routine runs do NOT get the dashboard module manifest
        // (`module-context.md`). No current skill calls a module action — the
        // digests only read via MCP and emit JSON, `cleanup` edits files — so
        // it was ~1.6K tokens of dead weight resent on every step of the agent
        // loop, on every provider. Only interactive chat keeps it
        // (`agents::chat`). A future skill that genuinely needs module access
        // should reintroduce this behind a `SKILL.md` frontmatter flag.
        system_prompt_file: None,
        model,
        allowed_tools,
        // The `ollama-agent` backend's inputs: which MCP servers to spawn and
        // where the local daemon lives. Every other backend ignores them.
        mcp_servers: config.mcp_servers.clone(),
        ollama_base_url: config
            .agents
            .providers
            .get(&crate::config::ProviderId::Ollama)
            .and_then(|settings| settings.base_url.clone()),
    }
}

/// Maps a successful backend invocation onto an unpersisted [`RunRecord`]
/// attributed to `name` (a skill or routine name).
///
/// Pure and synchronous — separated out so the status/exit-code/duration
/// mapping can be unit-tested without a real agent.
fn record_from_result(
    skill_name: &str,
    backend_id: &str,
    provider: Option<String>,
    started_at: chrono::DateTime<Utc>,
    result: AgentRunResult,
) -> RunRecord {
    RunRecord {
        id: None,
        skill_name: skill_name.to_owned(),
        backend: backend_id.to_owned(),
        status: if result.is_success() {
            RunStatus::Success
        } else {
            RunStatus::Failed
        },
        exit_code: Some(result.exit_code),
        duration_ms: result.duration_ms,
        stdout: result.stdout,
        stderr: result.stderr,
        error: None,
        started_at,
        finished_at: Utc::now(),
        provider,
        cost_usd: result.cost_usd,
        input_tokens: result.input_tokens,
        output_tokens: result.output_tokens,
        num_turns: result.num_turns,
        // Neither this function nor its caller here knows whether a
        // routine fired it — routines::scheduler::fire_one overrides this
        // to `Routine` itself, right before recording it.
        source: RunSource::default(),
    }
}

/// [`execute_skill`] followed by persisting the result.
///
/// The agent call in `execute_skill` runs with no lock held; the database
/// `Mutex` is taken only afterwards, just long enough to write the row. This
/// is the one place the "run, then record" sequence lives — the CLI and the
/// Tauri command both call it.
///
/// Errors:
///     Everything [`execute_skill`] can return, plus [`AxiomataError::Database`]
///     / [`AxiomataError::Io`] if recording the result fails.
pub async fn execute_and_record_skill(
    name: &str,
    config: &Config,
    db: &Mutex<Connection>,
) -> Result<RunRecord, AxiomataError> {
    // Refuse before doing any work if today's spend through a paid provider
    // is already over the cap (provider-hardening checkpoint 5). No-op on the
    // subscription-billed Anthropic path.
    {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        crate::spend::guard_redirected_turn(&conn, config, ProviderRole::Skill)?;
    }
    let record = execute_skill(name, config).await?;
    let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
    runlog::record_run(&db, record)
}

/// Model precedence for a Claude Code skill run: the skill's own `SKILL.md`
/// frontmatter `model:` wins when present; otherwise falls back to the
/// active provider's `skill_model` (unset/blank there means "let the CLI
/// choose its own default"). Pulled out of [`execute_skill`] as its own
/// function so this precedence is unit-testable without a real agent spawn.
fn resolve_skill_model(frontmatter_model: Option<&str>, config: &Config) -> Option<String> {
    // Trim + treat blank as "not pinned", same convention `provider_model`
    // already uses for a blank provider-config field — a `model:` line left
    // empty or whitespace-only should fall through to the provider default,
    // not literally reach the child process as `claude --model ""`.
    let pinned = frontmatter_model.map(str::trim).filter(|m| !m.is_empty());
    pinned
        .map(str::to_owned)
        .or_else(|| crate::agents::default_skill_model(config))
}

/// Environment-variable name prefixes that `config.agents.claude_env` is
/// allowed to set on the `claude` child process. Anything else — in particular
/// `PATH`, `IFS`, `BASH_ENV`, and any `LD_*` / `DYLD_*` loader variable — is
/// dropped, so a poisoned config cannot redirect the binary or inject a
/// preloaded library into it.
const CLAUDE_ENV_ALLOWED_PREFIXES: &[&str] = &[
    "ANTHROPIC_",
    "CLAUDE_CODE_",
    "AWS_",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
];

/// The provider environment for the `claude` process; empty for Ollama.
///
/// Two layers, in precedence order (later wins): first the `role` provider's
/// `ProviderSettings` (`agents.providers[agents.provider_for(role)]`) —
/// `base_url` into `ANTHROPIC_BASE_URL`, `api_key` into whichever env var
/// [`crate::config::ProviderId::auth_env_var`] names for that provider — then
/// `config.agents.claude_env`, filtered through
/// [`CLAUDE_ENV_ALLOWED_PREFIXES`], layered on top as a power-user escape
/// hatch that can still add to or override what the provider derived (e.g.
/// Bedrock, which doesn't fit the provider model at all). For the Anthropic
/// provider (`base_url`/`api_key` both `None` by default) the first layer
/// contributes nothing, so behavior is unchanged from before providers
/// existed: empty env unless `claude_env` sets something, i.e. the real
/// Anthropic API via the CLI's own subscription login.
pub(crate) fn claude_env(
    config: &Config,
    backend: &AgentBackend,
    role: ProviderRole,
) -> Vec<(String, String)> {
    match backend {
        AgentBackend::ClaudeCode => {
            let mut env = provider_env(config, role);
            // The `claude_env` power-user escape hatch, filtered and layered
            // on top so it can add to or override what the provider derived.
            for (key, value) in &config.agents.claude_env {
                if CLAUDE_ENV_ALLOWED_PREFIXES
                    .iter()
                    .any(|prefix| key.starts_with(prefix))
                {
                    env.insert(key.clone(), value.clone());
                }
            }
            env.into_iter().collect()
        }
        AgentBackend::Ollama { .. } | AgentBackend::OllamaAgent { .. } => Vec::new(),
    }
}

/// The `ANTHROPIC_*` environment derived from the `role` provider's
/// `ProviderSettings` alone (`agents.providers[agents.provider_for(role)]`):
/// `base_url` → `ANTHROPIC_BASE_URL`, `api_key` → the provider's
/// [`auth_env_var`](crate::config::ProviderId::auth_env_var), and — for a
/// redirected bearer-token provider — an explicit empty `ANTHROPIC_API_KEY`.
/// Empty for the Anthropic provider's own defaults (no base URL / key), so
/// behaviour there is unchanged from before providers existed.
fn provider_env(config: &Config, role: ProviderRole) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let provider = config.agents.provider_for(role);
    let Some(settings) = config.agents.providers.get(&provider) else {
        return env;
    };

    let base_url = non_blank(settings.base_url.as_deref());
    if let Some(base_url) = base_url {
        env.insert("ANTHROPIC_BASE_URL".to_string(), base_url.to_string());
    }
    if let Some(api_key) = non_blank(settings.api_key.as_deref()) {
        env.insert(provider.auth_env_var().to_string(), api_key.to_string());
    }
    // A non-Anthropic provider fronts the Messages API through a bearer token
    // (`ANTHROPIC_AUTH_TOKEN`). The `claude` CLI otherwise prefers a direct
    // `ANTHROPIC_API_KEY` — inherited or implied by a subscription login — as
    // an `x-api-key` header, which conflicts with the bearer token and makes
    // the CLI silently keep talking to Anthropic. `spawn_and_collect` clears
    // the env now (checkpoint 3), but a `claude_env` override or a stray
    // allowlisted var could still reintroduce one, so neutralise it with an
    // explicit empty value — only when a `base_url` redirect is in effect, so
    // a half-configured provider can't break Anthropic-via-login too. Phase 0
    // spike: `docs/plans/settings-provider-overhaul.md`.
    if base_url.is_some() && provider.auth_env_var() == "ANTHROPIC_AUTH_TOKEN" {
        env.insert("ANTHROPIC_API_KEY".to_string(), String::new());
    }
    env
}

/// `Some(s)` unless `s` is `None`, empty, or all whitespace.
fn non_blank(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// Builds an unpersisted `Failed` [`RunRecord`] for a run that produced no
/// agent result (unknown backend, spawn failure, timeout, API error).
fn failure_record(
    skill_name: &str,
    backend_id: &str,
    provider: Option<String>,
    started_at: chrono::DateTime<Utc>,
    duration_ms: u64,
    message: String,
) -> RunRecord {
    RunRecord {
        id: None,
        skill_name: skill_name.to_owned(),
        backend: backend_id.to_owned(),
        status: RunStatus::Failed,
        exit_code: None,
        duration_ms,
        stdout: String::new(),
        stderr: String::new(),
        error: Some(message),
        started_at,
        finished_at: Utc::now(),
        provider,
        cost_usd: None,
        input_tokens: None,
        output_tokens: None,
        num_turns: None,
        // See the matching comment in `record_from_result`.
        source: RunSource::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, ProviderId, ProviderSettings};
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Isolated `AXIOMATA_HOME` + a scratch working directory + migrated database.
    struct Fixture {
        _guard: std::sync::MutexGuard<'static, ()>,
        home: PathBuf,
        cwd: PathBuf,
        config: Config,
        db: Mutex<Connection>,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let guard = ENV_MUTEX.lock().unwrap();
            let home = unique_temp_dir(&format!("axiomata-test-runner-{tag}-home"));
            let cwd = unique_temp_dir(&format!("axiomata-test-runner-{tag}-cwd"));
            fs::create_dir_all(home.join("logs")).unwrap();
            fs::create_dir_all(&cwd).unwrap();
            // SAFETY: serialized by `guard`, see `paths::tests`.
            unsafe {
                env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
            }
            let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();
            let mut config = Config {
                workspace_root: cwd.clone(),
                ..Config::default()
            };
            // Keep tests fast even if a real agent happens to be reachable.
            config.agents.skill_timeout_secs = 5;
            Self {
                _guard: guard,
                home,
                cwd,
                config,
                db: Mutex::new(db),
            }
        }

        fn write_skill(&self, name: &str, contents: &str) {
            let dir = crate::paths::global_skills_dir().join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("SKILL.md"), contents).unwrap();
        }

        /// A locked handle to the test database, for direct `runlog` calls.
        fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
            self.db.lock().unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // SAFETY: still holding `_guard`.
            unsafe {
                env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
            }
            let _ = fs::remove_dir_all(&self.home);
            let _ = fs::remove_dir_all(&self.cwd);
        }
    }

    fn on_path(program: &str) -> bool {
        env::var_os("PATH")
            .map(|paths| {
                env::split_paths(&paths).any(|dir| Path::new(&dir).join(program).is_file())
            })
            .unwrap_or(false)
    }

    #[test]
    fn record_from_result_maps_status_exit_code_and_duration() {
        let now = Utc::now();

        let ok = record_from_result(
            "s",
            "ollama",
            None,
            now,
            AgentRunResult::bare("hi".to_owned(), String::new(), 0, 12),
        );
        assert_eq!(ok.status, RunStatus::Success);
        assert_eq!(ok.exit_code, Some(0));
        assert_eq!(ok.duration_ms, 12);
        assert_eq!(ok.backend, "ollama");
        assert!(ok.error.is_none());
        // Defaults to Manual — routines::scheduler::fire_one is the one
        // caller that overrides this, after the fact.
        assert_eq!(ok.source, RunSource::Manual);

        let bad = record_from_result(
            "s",
            "ollama",
            Some("open_router".to_owned()),
            now,
            AgentRunResult {
                cost_usd: Some(0.012),
                ..AgentRunResult::bare(String::new(), "boom".to_owned(), 3, 5)
            },
        );
        assert_eq!(bad.status, RunStatus::Failed);
        assert_eq!(bad.exit_code, Some(3));
        assert_eq!(bad.provider.as_deref(), Some("open_router"));
        assert_eq!(bad.cost_usd, Some(0.012));
    }

    /// Skill / routine runs never attach the dashboard module manifest, even
    /// when `module-context.md` exists — it is dead weight in the agent loop
    /// and no skill uses a module action. (Interactive chat still gets it, via
    /// `agents::chat`; that path is exercised in `agents::mod`'s tests.)
    #[test]
    fn agent_request_never_attaches_the_module_manifest() {
        let _guard = crate::test_support::ENV_MUTEX.lock().unwrap();
        let home = crate::test_support::unique_temp_dir("axiomata-test-runner-manifest");
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            std::env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }
        let config = Config::default();
        std::fs::write(home.join("module-context.md"), "# modules").unwrap();
        let req = agent_request(
            "p".to_string(),
            &AgentBackend::ClaudeCode,
            &config,
            None,
            None,
            None,
        );
        assert_eq!(req.system_prompt_file, None);
        unsafe {
            std::env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn agent_request_passes_allowed_tools_through_unchanged() {
        let _guard = crate::test_support::ENV_MUTEX.lock().unwrap();
        let home = crate::test_support::unique_temp_dir("axiomata-test-runner-allowed-tools");
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            std::env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }
        let config = Config::default();

        let req = agent_request(
            "p".to_string(),
            &AgentBackend::ClaudeCode,
            &config,
            None,
            Some("mcp__apple-reminders__calendar_events".to_string()),
            None,
        );
        assert_eq!(
            req.allowed_tools,
            Some("mcp__apple-reminders__calendar_events".to_string())
        );

        let req = agent_request(
            "p".to_string(),
            &AgentBackend::ClaudeCode,
            &config,
            None,
            None,
            None,
        );
        assert_eq!(req.allowed_tools, None);

        unsafe {
            std::env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn agent_request_timeout_uses_the_override_when_set_else_the_config_default() {
        let _guard = crate::test_support::ENV_MUTEX.lock().unwrap();
        let home = crate::test_support::unique_temp_dir("axiomata-test-runner-timeout");
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            std::env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }
        let mut config = Config::default();
        config.agents.skill_timeout_secs = 300;

        let default_req = agent_request(
            "p".into(),
            &AgentBackend::ClaudeCode,
            &config,
            None,
            None,
            None,
        );
        assert_eq!(default_req.timeout, Duration::from_secs(300));

        let overridden = agent_request(
            "p".into(),
            &AgentBackend::ClaudeCode,
            &config,
            None,
            None,
            Some(600),
        );
        assert_eq!(overridden.timeout, Duration::from_secs(600));

        unsafe {
            std::env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A minimal `Skill` whose prompt-relevant fields are set.
    fn skill(backend: &str, prepend: &[&str]) -> registry::Skill {
        registry::Skill {
            name: "s".to_string(),
            description: "d".to_string(),
            model: None,
            effort: None,
            trigger: None,
            backend: backend.to_string(),
            local_backend: None,
            prepend_files: prepend.iter().map(|s| s.to_string()).collect(),
            allowed_tools: None,
            timeout_secs: None,
            path: PathBuf::from("/tmp/s/SKILL.md"),
            body: "BODY-BODY".to_string(),
        }
    }

    #[test]
    fn build_prompt_prepends_a_present_file_and_skips_a_missing_one() {
        let cwd = unique_temp_dir("axiomata-test-runner-prepend");
        fs::create_dir_all(cwd.join("Mail")).unwrap();
        fs::write(cwd.join("Mail/.topics.md"), "Fotografie\nDevelopment\n").unwrap();
        let config = Config {
            workspace_root: cwd.clone(),
            ..Config::default()
        };

        let prompt = build_prompt(
            &config,
            &skill("claude-code", &["Mail/.topics.md", "Mail/ghost.md"]),
        )
        .unwrap();
        assert!(
            prompt.starts_with(
                "## Context file: Mail/.topics.md\n\nFotografie\nDevelopment\n\n---\n\n"
            ),
            "{prompt}"
        );
        assert!(
            !prompt.contains("ghost.md"),
            "a missing file contributes nothing"
        );
        assert!(prompt.ends_with("BODY-BODY"), "{prompt}");
        fs::remove_dir_all(&cwd).ok();
    }

    #[test]
    fn build_prompt_skips_a_missing_or_blank_context_file() {
        let cwd = unique_temp_dir("axiomata-test-runner-blank");
        fs::create_dir_all(cwd.join("Mail")).unwrap();
        fs::write(cwd.join("Mail/.topics.md"), "   \n").unwrap();
        let config = Config {
            workspace_root: cwd.clone(),
            ..Config::default()
        };

        let prompt = build_prompt(&config, &skill("claude-code", &["Mail/.topics.md"])).unwrap();
        assert_eq!(
            prompt, "BODY-BODY",
            "a blank topic file adds no context block"
        );
        fs::remove_dir_all(&cwd).ok();
    }

    #[test]
    fn build_prompt_rejects_dotdot_and_absolute_prepend_paths() {
        let config = Config {
            workspace_root: unique_temp_dir("axiomata-test-runner-escape"),
            ..Config::default()
        };
        for bad in ["../evil", "/etc/passwd"] {
            let err = build_prompt(&config, &skill("claude-code", &[bad])).unwrap_err();
            assert!(
                matches!(&err, AxiomataError::InvalidSkill { reason, .. }
                    if reason.contains("workspace-relative")),
                "{bad}: {err}"
            );
        }
    }

    #[test]
    fn claude_env_drops_loader_and_path_keys() {
        let mut config = Config::default();
        for key in [
            "ANTHROPIC_BASE_URL",
            "CLAUDE_CODE_USE_BEDROCK",
            "AWS_REGION",
            "HTTPS_PROXY",
            "PATH",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
            "IFS",
        ] {
            config
                .agents
                .claude_env
                .insert(key.to_owned(), "x".to_owned());
        }

        let env = claude_env(&config, &AgentBackend::ClaudeCode, ProviderRole::Skill);
        let kept: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(kept.contains(&"ANTHROPIC_BASE_URL"));
        assert!(kept.contains(&"CLAUDE_CODE_USE_BEDROCK"));
        assert!(kept.contains(&"AWS_REGION"));
        assert!(kept.contains(&"HTTPS_PROXY"));
        assert!(!kept.contains(&"PATH"));
        assert!(!kept.contains(&"LD_PRELOAD"));
        assert!(!kept.contains(&"DYLD_INSERT_LIBRARIES"));
        assert!(!kept.contains(&"IFS"));

        // Ollama never gets provider env.
        assert!(
            claude_env(
                &config,
                &AgentBackend::Ollama {
                    model: "m".to_owned()
                },
                ProviderRole::Skill,
            )
            .is_empty()
        );
    }

    #[test]
    fn claude_env_is_empty_for_anthropic_with_no_claude_env_overrides() {
        let config = Config::default();
        assert_eq!(config.agents.chat_provider, ProviderId::Anthropic);
        assert_eq!(config.agents.skill_provider, ProviderId::Anthropic);
        // Anthropic's `ProviderSettings` carry no base_url/api_key by design
        // (billed via the CLI's own subscription login) — this must stay
        // byte-identical to pre-provider behavior.
        assert!(claude_env(&config, &AgentBackend::ClaudeCode, ProviderRole::Skill).is_empty());
    }

    #[test]
    fn claude_env_derives_base_url_and_bearer_token_for_openrouter() {
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::OpenRouter;
        config.agents.skill_provider = ProviderId::OpenRouter;
        config.agents.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some("https://openrouter.ai/api".to_string()),
                api_key: Some("sk-or-test".to_string()),
                chat_model: String::new(),
                skill_model: String::new(),
            },
        );

        let env: HashMap<String, String> =
            claude_env(&config, &AgentBackend::ClaudeCode, ProviderRole::Skill)
                .into_iter()
                .collect();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://openrouter.ai/api")
        );
        // OpenRouter's key goes into the bearer-token var, not
        // `ANTHROPIC_API_KEY` (see `ProviderId::auth_env_var`) — setting both
        // makes the CLI prefer the wrong/absent direct key.
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").map(String::as_str),
            Some("sk-or-test")
        );
        // `ANTHROPIC_API_KEY` must be present *and empty* — an explicit
        // neutraliser for any inherited/subscription key, since the child is
        // spawned without `env_clear()`.
        assert_eq!(env.get("ANTHROPIC_API_KEY").map(String::as_str), Some(""));
    }

    #[test]
    fn claude_env_derives_base_url_only_when_ollama_has_no_key_configured() {
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::Ollama;
        config.agents.skill_provider = ProviderId::Ollama;
        config.agents.providers.insert(
            ProviderId::Ollama,
            ProviderSettings {
                base_url: Some("http://localhost:11434".to_string()),
                api_key: None,
                chat_model: String::new(),
                skill_model: String::new(),
            },
        );

        let env = claude_env(&config, &AgentBackend::ClaudeCode, ProviderRole::Skill);
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"ANTHROPIC_BASE_URL"));
        assert!(!keys.contains(&"ANTHROPIC_AUTH_TOKEN"));
    }

    #[test]
    fn claude_env_treats_an_empty_or_whitespace_only_base_url_and_key_as_absent() {
        // `Some("")` / `Some("   ")` are distinct from `None` at the type
        // level -- this exercises `non_blank`'s job of collapsing both down
        // to "not configured" rather than emitting a blank env var value.
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::OpenRouter;
        config.agents.skill_provider = ProviderId::OpenRouter;
        config.agents.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some("   ".to_string()),
                api_key: Some(String::new()),
                chat_model: String::new(),
                skill_model: String::new(),
            },
        );

        assert!(claude_env(&config, &AgentBackend::ClaudeCode, ProviderRole::Skill).is_empty());
    }

    #[test]
    fn claude_env_lets_claude_env_entries_override_the_provider_derived_base_url() {
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::OpenRouter;
        config.agents.skill_provider = ProviderId::OpenRouter;
        config.agents.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some("https://openrouter.ai/api".to_string()),
                api_key: Some("sk-or-test".to_string()),
                chat_model: String::new(),
                skill_model: String::new(),
            },
        );
        // The power-user escape hatch (e.g. for Bedrock) wins over the
        // provider-derived value, per the documented precedence.
        config.agents.claude_env.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://my-proxy.example".to_string(),
        );

        let env: HashMap<String, String> =
            claude_env(&config, &AgentBackend::ClaudeCode, ProviderRole::Skill)
                .into_iter()
                .collect();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("https://my-proxy.example")
        );
    }

    /// The chat and skill roles resolve their `ANTHROPIC_*` env from their own
    /// provider: with chat on Anthropic (no redirect) and skills on Ollama,
    /// `ProviderRole::Skill` derives the loopback base URL while
    /// `ProviderRole::Chat` derives nothing.
    #[test]
    fn claude_env_is_role_aware_across_a_split_provider_config() {
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::Anthropic;
        config.agents.skill_provider = ProviderId::Ollama;
        config.agents.providers.insert(
            ProviderId::Ollama,
            ProviderSettings {
                base_url: Some("http://localhost:11434".to_string()),
                api_key: None,
                chat_model: String::new(),
                skill_model: "qwen3:30b".to_string(),
            },
        );

        let skill_env: HashMap<String, String> =
            claude_env(&config, &AgentBackend::ClaudeCode, ProviderRole::Skill)
                .into_iter()
                .collect();
        assert_eq!(
            skill_env.get("ANTHROPIC_BASE_URL").map(String::as_str),
            Some("http://localhost:11434")
        );

        assert!(
            claude_env(&config, &AgentBackend::ClaudeCode, ProviderRole::Chat).is_empty(),
            "the chat role stays on Anthropic-via-login — no provider env"
        );
    }

    #[test]
    fn resolve_skill_model_prefers_frontmatter_over_the_provider_skill_model() {
        let mut config = Config::default();
        config
            .agents
            .providers
            .get_mut(&ProviderId::Anthropic)
            .unwrap()
            .skill_model = "provider-default".to_string();

        assert_eq!(
            resolve_skill_model(Some("pinned-model"), &config),
            Some("pinned-model".to_string())
        );
    }

    #[test]
    fn resolve_skill_model_falls_back_to_the_provider_skill_model_when_unpinned() {
        let mut config = Config::default();
        config
            .agents
            .providers
            .get_mut(&ProviderId::Anthropic)
            .unwrap()
            .skill_model = "provider-default".to_string();

        assert_eq!(
            resolve_skill_model(None, &config),
            Some("provider-default".to_string())
        );
    }

    #[test]
    fn resolve_skill_model_treats_a_blank_frontmatter_model_as_unset_not_pinned() {
        // A `model:` line left empty or whitespace-only in `SKILL.md`
        // frontmatter must fall through to the provider default, the same
        // convention `provider_model` uses for a blank provider-config
        // field — not literally reach the child process as `claude --model
        // ""`. Found as a real inconsistency during Phase 2 test review, then
        // fixed in `resolve_skill_model` itself (trim + treat blank as
        // "not pinned").
        let mut config = Config::default();
        config
            .agents
            .providers
            .get_mut(&ProviderId::Anthropic)
            .unwrap()
            .skill_model = "provider-default".to_string();

        assert_eq!(
            resolve_skill_model(Some(""), &config),
            Some("provider-default".to_string())
        );
        assert_eq!(
            resolve_skill_model(Some("   "), &config),
            Some("provider-default".to_string())
        );
    }

    #[tokio::test]
    async fn unknown_skill_is_an_error_and_records_nothing() {
        let fx = Fixture::new("unknown");
        let err = execute_and_record_skill("ghost", &fx.config, &fx.db)
            .await
            .unwrap_err();
        assert!(matches!(err, AxiomataError::SkillNotFound { .. }));
        assert!(runlog::list_runs(&fx.conn(), 10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_backend_in_frontmatter_records_a_failed_run() {
        let fx = Fixture::new("bad-backend");
        fx.write_skill(
            "weird",
            "---\nname: weird\ndescription: bad backend\nbackend: opencode\n---\nbody\n",
        );

        let record = execute_and_record_skill("weird", &fx.config, &fx.db)
            .await
            .unwrap();
        assert_eq!(record.status, RunStatus::Failed);
        assert_eq!(record.exit_code, None);
        assert!(record.error.unwrap().contains("opencode"));

        let recent = runlog::list_runs(&fx.conn(), 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].skill_name, "weird");
    }

    #[tokio::test]
    async fn ollama_backend_records_failed_run_when_daemon_unreachable() {
        let fx = Fixture::new("ollama-down");
        fx.write_skill(
            "note",
            "---\nname: note\ndescription: append a note\nbackend: ollama\n---\nWrite a short note.\n",
        );

        let record = execute_and_record_skill("note", &fx.config, &fx.db)
            .await
            .unwrap();
        // Either the daemon is absent (Failed with an error message) or, on a
        // dev machine that happens to run Ollama, it succeeds — both are valid,
        // but the run must always be persisted.
        let recent = runlog::list_runs(&fx.conn(), 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].skill_name, "note");
        assert_eq!(recent[0].backend, "ollama");
        if record.status == RunStatus::Failed {
            assert!(record.error.is_some());
            assert_eq!(record.exit_code, None);
        }
    }

    /// Reproduces live: three concurrent triggers for the same skill (a
    /// mount-time auto-refresh racing a manual click, a dev hot-reload
    /// remounting before a previous run finished, …) used to spawn three
    /// competing agent processes instead of one running and the rest
    /// failing fast. `tokio::test`'s single-threaded runtime makes this
    /// deterministic, not just probable: `join!` runs the first future
    /// synchronously up to its first real await (inside `backend.run`,
    /// past the dedup check), so by the time the second future's own dedup
    /// check runs, the first has already claimed the name.
    #[tokio::test]
    async fn a_second_concurrent_run_of_the_same_name_fails_fast_instead_of_racing() {
        let fx = Fixture::new("dedupe-race");
        fx.write_skill(
            "racer",
            "---\nname: racer\ndescription: d\nbackend: ollama\n---\nDo a thing.\n",
        );

        let (a, b) = tokio::join!(
            execute_skill("racer", &fx.config),
            execute_skill("racer", &fx.config)
        );
        let dedup_hits = [&a, &b]
            .iter()
            .filter(|r| matches!(r, Err(AxiomataError::AlreadyRunning { .. })))
            .count();
        assert_eq!(
            dedup_hits, 1,
            "exactly one of two concurrent same-name runs should be rejected: a={a:?} b={b:?}"
        );

        // The name is released once its run finishes, so a third, later
        // call isn't permanently locked out by an earlier one.
        let c = execute_skill("racer", &fx.config).await;
        assert!(!matches!(c, Err(AxiomataError::AlreadyRunning { .. })));
    }

    /// The dedup keyspace is a bare `name` string shared by both entry
    /// points — [`execute_skill`] keys on the skill's own frontmatter
    /// `name`, [`execute_prompt`] keys on whatever the caller (the routine
    /// scheduler, passing `routine.name`) hands it. A raw-prompt routine
    /// that happens to share a name with a skill therefore serializes
    /// against that skill's runs too, not just against other routines. This
    /// pins that down as intentional-by-construction (the run log already
    /// conflates the two under one `skill_name` column) rather than an
    /// accident that a future refactor could silently change.
    #[tokio::test]
    async fn a_skill_and_a_same_named_prompt_run_serialize_against_each_other() {
        let fx = Fixture::new("dedupe-shared-namespace");
        fx.write_skill(
            "racer",
            "---\nname: racer\ndescription: d\nbackend: ollama\n---\nDo a thing.\n",
        );

        let (skill_result, prompt_result) = tokio::join!(
            execute_skill("racer", &fx.config),
            execute_prompt("racer", "say hi".to_owned(), "ollama", &fx.config)
        );
        let dedup_hits = [&skill_result, &prompt_result]
            .iter()
            .filter(|r| matches!(r, Err(AxiomataError::AlreadyRunning { .. })))
            .count();
        assert_eq!(
            dedup_hits, 1,
            "a skill run and a same-named prompt run should serialize against \
             each other: skill={skill_result:?} prompt={prompt_result:?}"
        );
    }

    /// The dedup guard is keyed by name, not a single global lock — two
    /// concurrent runs of two *different* names must not interfere with
    /// each other.
    #[tokio::test]
    async fn concurrent_runs_of_different_names_do_not_dedup_against_each_other() {
        let fx = Fixture::new("dedupe-different-names");
        fx.write_skill(
            "racer-a",
            "---\nname: racer-a\ndescription: d\nbackend: ollama\n---\nDo a thing.\n",
        );
        fx.write_skill(
            "racer-b",
            "---\nname: racer-b\ndescription: d\nbackend: ollama\n---\nDo a thing.\n",
        );

        let (a, b) = tokio::join!(
            execute_skill("racer-a", &fx.config),
            execute_skill("racer-b", &fx.config)
        );
        let a = a.unwrap();
        let b = b.unwrap();
        for record in [&a, &b] {
            assert!(
                record
                    .error
                    .as_deref()
                    .is_none_or(|e| !e.contains("already running")),
                "different-named runs must not dedup against each other: {record:?}"
            );
        }
    }

    /// The name is released even when the run itself fails (not just on
    /// success, which is all the ollama-backed dedup test above can prove
    /// on a machine where the daemon happens to be reachable) — a spawn
    /// failure must not wedge the name claimed forever.
    #[tokio::test]
    async fn the_name_lock_is_released_even_when_the_run_fails() {
        if on_path("claude") {
            // The binary exists here; this test only covers the guaranteed
            // spawn-failure case (binary absent).
            return;
        }
        let fx = Fixture::new("dedupe-release-on-failure");
        fx.write_skill(
            "summarize",
            "---\nname: summarize\ndescription: summary\nbackend: claude-code\n---\nSummarize.\n",
        );

        let first = execute_skill("summarize", &fx.config).await.unwrap();
        assert_eq!(first.status, RunStatus::Failed);
        assert!(
            first
                .error
                .as_deref()
                .is_some_and(|e| !e.contains("already running")),
            "the first run should fail on the absent spawn, not on dedup: {first:?}"
        );

        let second = execute_skill("summarize", &fx.config).await.unwrap();
        assert!(
            second
                .error
                .as_deref()
                .is_none_or(|e| !e.contains("already running")),
            "the name must be released after a failed run, not just after a \
             successful one: {second:?}"
        );
    }

    #[tokio::test]
    async fn a_bad_prepend_files_path_is_recorded_as_a_failed_run_not_an_error() {
        let fx = Fixture::new("bad-prepend");
        fx.write_skill(
            "prep",
            "---\nname: prep\ndescription: d\nbackend: claude-code\n\
             prepend_files: [\"../evil\"]\n---\nDo a thing.\n",
        );

        // §D decision: a `prepend_files` entry that breaks the workspace-
        // relative guard is a misconfigured skill, not a resolution failure —
        // it persists a `Failed` run the dashboard can show instead of
        // returning `Err` (which records nothing).
        let record = execute_and_record_skill("prep", &fx.config, &fx.db)
            .await
            .unwrap();
        assert_eq!(record.status, RunStatus::Failed);
        assert!(record.error.unwrap().contains("workspace-relative"));

        let recent = runlog::list_runs(&fx.conn(), 10).unwrap();
        assert_eq!(recent.len(), 1, "the failed run must be persisted");
        assert_eq!(recent[0].skill_name, "prep");
        assert_eq!(recent[0].backend, "claude-code");
        assert_eq!(recent[0].status, RunStatus::Failed);
    }

    #[tokio::test]
    async fn claude_code_backend_spawn_failure_is_recorded_when_binary_absent() {
        if on_path("claude") {
            // The binary exists here; this test only covers the absent case.
            return;
        }
        let fx = Fixture::new("no-claude");
        fx.write_skill(
            "summarize",
            "---\nname: summarize\ndescription: summary\nbackend: claude-code\n---\nSummarize.\n",
        );

        let record = execute_and_record_skill("summarize", &fx.config, &fx.db)
            .await
            .unwrap();
        assert_eq!(record.status, RunStatus::Failed);
        assert_eq!(record.exit_code, None);
        assert!(record.error.is_some());

        let recent = runlog::list_runs(&fx.conn(), 10).unwrap();
        assert_eq!(recent.len(), 1);
    }
}
