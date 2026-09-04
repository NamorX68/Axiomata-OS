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

use std::sync::Mutex;
use std::time::Duration;

use chrono::Utc;
use rusqlite::Connection;

use crate::agents::{AgentBackend, AgentRequest, AgentRunResult};
use crate::config::Config;
use crate::error::AxiomataError;
use crate::skills::model::{RunRecord, RunStatus};
use crate::skills::registry;
use crate::skills::runlog;

/// Runs the skill named `name`, returning an unpersisted [`RunRecord`].
///
/// Resolution failures (no such skill, malformed `SKILL.md`) are returned as
/// `Err` — there is no run to attribute. Once the skill is found, every other
/// outcome (success, non-zero exit, unknown backend, spawn failure, timeout,
/// API error) yields `Ok(record)` with `record.id == None`; the caller
/// inspects [`RunRecord::status`] and persists via [`runlog::record_run`].
///
/// Args:
///     name: Skill name to run.
///     config: Supplies the working directory (`workspace_root`), the Ollama
///         default model, the run timeout, and the `claude` provider
///         environment.
///
/// Errors:
///     [`AxiomataError::SkillNotFound`] / [`AxiomataError::InvalidSkill`] if the
///     skill cannot be resolved.
pub async fn execute_skill(name: &str, config: &Config) -> Result<RunRecord, AxiomataError> {
    let skill = registry::find_skill(name)?;

    // An unknown backend string in the frontmatter is a recordable failure: the
    // skill exists and someone tried to run it.
    let backend = match AgentBackend::resolve(&skill.backend, skill.model.as_deref(), config) {
        Ok(backend) => backend,
        Err(err) => {
            return Ok(failure_record(
                &skill.name,
                &skill.backend,
                Utc::now(),
                0,
                err.to_string(),
            ));
        }
    };

    let model = match backend {
        AgentBackend::ClaudeCode => skill
            .model
            .clone()
            .or_else(|| crate::agents::default_claude_model(config)),
        AgentBackend::Ollama { .. } => None,
    };
    // The skill's own instruction body is the prompt on every backend. Claude
    // Code used to be sent `/<name>` instead, on the assumption that its own
    // slash-command skill machinery would resolve it — it doesn't: that
    // machinery looks under `.claude/skills/` (project) or `~/.claude/skills/`
    // (global), never Axiomata's own `~/.axiomata/skills/` registry, so every
    // run got back a plain "Unknown command: /<name>" chat reply and — since
    // `claude -p` still exits 0 for an ordinary reply — was logged as a
    // success despite the SOP never reaching the model.
    Ok(run_on_backend(
        &skill.name,
        skill.body.clone(),
        &backend,
        config,
        model,
        skill.allowed_tools.clone(),
    )
    .await)
}

/// Runs the raw string `prompt` on `backend_id` (`"claude-code"` / `"ollama"`),
/// attributing the resulting [`RunRecord`] to `name`. The counterpart to
/// [`execute_skill`] for callers that have a prompt but no `SKILL.md` — the
/// routine scheduler's `prompt` target. An unresolvable `backend_id` yields a
/// `Failed` record, not an `Err`.
pub async fn execute_prompt(
    name: &str,
    prompt: String,
    backend_id: &str,
    config: &Config,
) -> RunRecord {
    let backend = match AgentBackend::resolve(backend_id, None, config) {
        Ok(backend) => backend,
        Err(err) => return failure_record(name, backend_id, Utc::now(), 0, err.to_string()),
    };
    run_on_backend(
        name,
        prompt,
        &backend,
        config,
        crate::agents::default_claude_model(config),
        // Raw prompt targets (no `SKILL.md`) have nowhere to declare a tool
        // allow-list yet — same limitation `execute_prompt`'s doc already
        // implies by only taking a bare string.
        None,
    )
    .await
}

/// Shared tail of [`execute_skill`] / [`execute_prompt`]: build the request,
/// run the backend, and map the outcome onto an unpersisted [`RunRecord`]
/// attributed to `name`.
async fn run_on_backend(
    name: &str,
    prompt: String,
    backend: &AgentBackend,
    config: &Config,
    model: Option<String>,
    allowed_tools: Option<String>,
) -> RunRecord {
    let started_at = Utc::now();
    let request = agent_request(prompt, backend, config, model, allowed_tools);
    match backend.run(request).await {
        Ok(result) => record_from_result(name, backend.id(), started_at, result),
        Err(err) => {
            let elapsed = (Utc::now() - started_at).num_milliseconds().max(0) as u64;
            failure_record(name, backend.id(), started_at, elapsed, err.to_string())
        }
    }
}

/// Builds the [`AgentRequest`] for a prompt on `backend`: the caller-supplied
/// prompt, the workspace as the working directory, the configured run timeout,
/// and (for Claude Code only) the filtered provider environment.
fn agent_request(
    prompt: String,
    backend: &AgentBackend,
    config: &Config,
    model: Option<String>,
    allowed_tools: Option<String>,
) -> AgentRequest {
    AgentRequest {
        prompt,
        cwd: config.workspace_root.clone(),
        timeout: Duration::from_secs(config.agents.skill_timeout_secs),
        env: claude_env(config, backend),
        // Skills and routines get the dashboard's module manifest too, so an
        // unattended run can call mounted modules; None when the GUI never ran.
        system_prompt_file: crate::agents::module_context_if_present(),
        model,
        allowed_tools,
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
    let record = execute_skill(name, config).await?;
    let db = db.lock().expect("run-log database mutex is poisoned");
    runlog::record_run(&db, record)
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

/// The provider environment for the `claude` process, filtered through
/// [`CLAUDE_ENV_ALLOWED_PREFIXES`]; empty for Ollama.
pub(crate) fn claude_env(config: &Config, backend: &AgentBackend) -> Vec<(String, String)> {
    match backend {
        AgentBackend::ClaudeCode => config
            .agents
            .claude_env
            .iter()
            .filter(|(key, _)| {
                CLAUDE_ENV_ALLOWED_PREFIXES
                    .iter()
                    .any(|prefix| key.starts_with(prefix))
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        AgentBackend::Ollama { .. } => Vec::new(),
    }
}

/// Builds an unpersisted `Failed` [`RunRecord`] for a run that produced no
/// agent result (unknown backend, spawn failure, timeout, API error).
fn failure_record(
    skill_name: &str,
    backend_id: &str,
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
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
            now,
            AgentRunResult {
                stdout: "hi".to_owned(),
                stderr: String::new(),
                exit_code: 0,
                duration_ms: 12,
            },
        );
        assert_eq!(ok.status, RunStatus::Success);
        assert_eq!(ok.exit_code, Some(0));
        assert_eq!(ok.duration_ms, 12);
        assert_eq!(ok.backend, "ollama");
        assert!(ok.error.is_none());

        let bad = record_from_result(
            "s",
            "ollama",
            now,
            AgentRunResult {
                stdout: String::new(),
                stderr: "boom".to_owned(),
                exit_code: 3,
                duration_ms: 5,
            },
        );
        assert_eq!(bad.status, RunStatus::Failed);
        assert_eq!(bad.exit_code, Some(3));
    }

    #[test]
    fn agent_request_picks_up_the_module_manifest_only_when_present() {
        let _guard = crate::test_support::ENV_MUTEX.lock().unwrap();
        let home = crate::test_support::unique_temp_dir("axiomata-test-runner-manifest");
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
            None,
        );
        assert_eq!(req.system_prompt_file, None);
        std::fs::write(home.join("module-context.md"), "# modules").unwrap();
        let req = agent_request(
            "p".to_string(),
            &AgentBackend::ClaudeCode,
            &config,
            None,
            None,
        );
        assert_eq!(req.system_prompt_file, Some(home.join("module-context.md")));
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
        );
        assert_eq!(req.allowed_tools, None);

        unsafe {
            std::env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
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

        let env = claude_env(&config, &AgentBackend::ClaudeCode);
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
                }
            )
            .is_empty()
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
