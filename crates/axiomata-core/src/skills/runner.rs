//! Resolves a skill's agent backend and executes it, recording the result.
//!
//! Two entry points:
//!
//! - [`execute_skill`] — runs the skill and returns an **unpersisted**
//!   [`RunRecord`] (`id: None`). Touches no database, so a caller that only
//!   holds a `std::sync::Mutex` around its state can drop the lock before the
//!   `await` and re-take it just to persist.
//! - [`run_skill`] — the convenience wrapper: `execute_skill` followed by
//!   [`runlog::record_run`]. Used by the CLI, which owns its `Connection`.
//!
//! It ties together the registry ([`crate::skills::registry`]), the agent
//! backends ([`crate::agents`]), and the run log ([`crate::skills::runlog`]).
//!
//! Implemented in M1.

use std::time::Duration;

use chrono::Utc;
use rusqlite::Connection;

use crate::agents::{AgentBackend, AgentRequest, AgentRunResult};
use crate::config::Config;
use crate::error::AxiomataError;
use crate::skills::registry::{self, Skill};
use crate::skills::runlog::{self, RunRecord, RunStatus};

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
///     config: Supplies `workspace_root`, the Ollama default model, the run
///         timeout, and the `claude` provider environment.
///
/// Errors:
///     [`AxiomataError::SkillNotFound`] / [`AxiomataError::InvalidSkill`] if the
///     skill cannot be resolved.
pub async fn execute_skill(name: &str, config: &Config) -> Result<RunRecord, AxiomataError> {
    let skill = registry::find_skill(name, config)?;
    let timeout = Duration::from_secs(config.agents.skill_timeout_secs);
    let started_at = Utc::now();

    // An unknown backend string in the frontmatter is a recordable failure: the
    // skill exists and someone tried to run it.
    let backend = match AgentBackend::resolve(&skill.backend, skill.model.as_deref(), config) {
        Ok(backend) => backend,
        Err(err) => return Ok(failure_record(&skill, started_at, 0, err.to_string())),
    };

    let request = AgentRequest {
        prompt: prompt_for(&skill, &backend),
        cwd: config.workspace_root.clone(),
        timeout,
        env: claude_env(config, &backend),
    };

    let record = match backend.run(request).await {
        Ok(result) => record_from_result(&skill, &backend, started_at, result),
        Err(err) => {
            let elapsed = (Utc::now() - started_at).num_milliseconds().max(0) as u64;
            failure_record(&skill, started_at, elapsed, err.to_string())
        }
    };
    Ok(record)
}

/// Maps a successful backend invocation onto an unpersisted [`RunRecord`].
///
/// Pure and synchronous — separated from [`execute_skill`] so the
/// status/exit-code/duration mapping can be unit-tested without a real agent.
fn record_from_result(
    skill: &Skill,
    backend: &AgentBackend,
    started_at: chrono::DateTime<Utc>,
    result: AgentRunResult,
) -> RunRecord {
    RunRecord {
        id: None,
        skill_name: skill.name.clone(),
        skill_source: skill.source.as_str().to_owned(),
        backend: backend.id().to_owned(),
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

/// [`execute_skill`] followed by persisting the result to `db`.
///
/// Errors:
///     Everything [`execute_skill`] can return, plus [`AxiomataError::Database`]
///     / [`AxiomataError::Io`] if recording the result fails.
pub async fn run_skill(
    name: &str,
    config: &Config,
    db: &Connection,
) -> Result<RunRecord, AxiomataError> {
    let record = execute_skill(name, config).await?;
    runlog::record_run(db, record)
}

/// Builds the prompt handed to the agent for `skill` on `backend`.
///
/// Claude Code is invoked with the skill as a slash command (`/<name>`) so its
/// own skill machinery runs; Ollama, which has no skill system, is fed the
/// skill's instruction body directly.
fn prompt_for(skill: &Skill, backend: &AgentBackend) -> String {
    match backend {
        AgentBackend::ClaudeCode => format!("/{}", skill.name),
        AgentBackend::Ollama { .. } => skill.body.clone(),
    }
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
fn claude_env(config: &Config, backend: &AgentBackend) -> Vec<(String, String)> {
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
    skill: &Skill,
    started_at: chrono::DateTime<Utc>,
    duration_ms: u64,
    message: String,
) -> RunRecord {
    RunRecord {
        id: None,
        skill_name: skill.name.clone(),
        skill_source: skill.source.as_str().to_owned(),
        backend: skill.backend.clone(),
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

    /// Isolated `AXIOMATA_HOME` + scratch workspace + migrated database.
    struct Fixture {
        _guard: std::sync::MutexGuard<'static, ()>,
        home: PathBuf,
        config: Config,
        db: Connection,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let guard = ENV_MUTEX.lock().unwrap();
            let home = unique_temp_dir(&format!("axiomata-test-runner-{tag}-home"));
            let workspace = unique_temp_dir(&format!("axiomata-test-runner-{tag}-ws"));
            fs::create_dir_all(home.join("logs")).unwrap();
            fs::create_dir_all(&workspace).unwrap();
            // SAFETY: serialized by `guard`, see `paths::tests`.
            unsafe {
                env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
            }
            let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();
            let mut config = Config {
                workspace_root: workspace,
                ..Config::default()
            };
            // Keep tests fast even if a real agent happens to be reachable.
            config.agents.skill_timeout_secs = 5;
            Self {
                _guard: guard,
                home,
                config,
                db,
            }
        }

        fn write_workspace_skill(&self, name: &str, contents: &str) {
            let dir = registry::workspace_skills_dir(&self.config).join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("SKILL.md"), contents).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            // SAFETY: still holding `_guard`.
            unsafe {
                env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
            }
            let _ = fs::remove_dir_all(&self.home);
            let _ = fs::remove_dir_all(&self.config.workspace_root);
        }
    }

    fn on_path(program: &str) -> bool {
        env::var_os("PATH")
            .map(|paths| {
                env::split_paths(&paths).any(|dir| Path::new(&dir).join(program).is_file())
            })
            .unwrap_or(false)
    }

    fn skill_for(name: &str, backend: &str) -> Skill {
        Skill {
            name: name.to_owned(),
            description: String::new(),
            model: None,
            effort: None,
            trigger: None,
            backend: backend.to_owned(),
            source: crate::skills::registry::SkillSource::Workspace,
            path: PathBuf::from("/tmp/none/SKILL.md"),
            body: String::new(),
        }
    }

    #[test]
    fn record_from_result_maps_status_exit_code_and_duration() {
        let skill = skill_for("s", "ollama");
        let backend = AgentBackend::Ollama {
            model: "m".to_owned(),
        };
        let now = Utc::now();

        let ok = record_from_result(
            &skill,
            &backend,
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
        assert_eq!(ok.skill_source, "workspace");
        assert_eq!(ok.backend, "ollama");
        assert!(ok.error.is_none());

        let bad = record_from_result(
            &skill,
            &backend,
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
        let err = run_skill("ghost", &fx.config, &fx.db).await.unwrap_err();
        assert!(matches!(err, AxiomataError::SkillNotFound { .. }));
        assert!(runlog::list_runs(&fx.db, 10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_backend_in_frontmatter_records_a_failed_run() {
        let fx = Fixture::new("bad-backend");
        fx.write_workspace_skill(
            "weird",
            "---\nname: weird\ndescription: bad backend\nbackend: opencode\n---\nbody\n",
        );

        let record = run_skill("weird", &fx.config, &fx.db).await.unwrap();
        assert_eq!(record.status, RunStatus::Failed);
        assert_eq!(record.exit_code, None);
        assert!(record.error.unwrap().contains("opencode"));

        let recent = runlog::list_runs(&fx.db, 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].skill_name, "weird");
    }

    #[tokio::test]
    async fn ollama_backend_records_failed_run_when_daemon_unreachable() {
        let fx = Fixture::new("ollama-down");
        // Point the (shared) client at a port nothing listens on by relying on
        // the daemon simply not running in CI; the call fails fast with a
        // connection error and must be recorded, not panic.
        fx.write_workspace_skill(
            "note",
            "---\nname: note\ndescription: append a note\nbackend: ollama\n---\nWrite a short note.\n",
        );

        let record = run_skill("note", &fx.config, &fx.db).await.unwrap();
        // Either the daemon is absent (Failed with an error message) or, on a
        // dev machine that happens to run Ollama, it succeeds — both are valid,
        // but the run must always be persisted.
        let recent = runlog::list_runs(&fx.db, 10).unwrap();
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
        fx.write_workspace_skill(
            "summarize",
            "---\nname: summarize\ndescription: summary\nbackend: claude-code\n---\nSummarize.\n",
        );

        let record = run_skill("summarize", &fx.config, &fx.db).await.unwrap();
        assert_eq!(record.status, RunStatus::Failed);
        assert_eq!(record.exit_code, None);
        assert!(record.error.is_some());

        let recent = runlog::list_runs(&fx.db, 10).unwrap();
        assert_eq!(recent.len(), 1);
    }
}
