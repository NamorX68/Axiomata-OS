//! The `axh` backend: a skill or prompt run through Axiomata's own harness
//! (`axiomata-harness`, `docs/plans/harness.md`) instead of an opencode
//! session — step HS6b, `docs/plans/harness-hs6b.md`.
//!
//! Division of labour (D1): the core keeps building the run's prompt
//! (`skills::runner::build_prompt_at` — the skill body with its date line and
//! its inlined `prepend_files`) and resolves the model, the timeout and the
//! frontmatter; the harness supplies the loop, the tools, the gate, the
//! sandbox and MCP. What arrives here is an [`AgentRequest`]; its
//! `allowed_tools` become the run's whole permission set (B3): what the skill
//! declares is allowed, what it does not name is refused before anything is
//! asked, nobody answers a question, and the settings files of `~/.axh` never
//! widen the gate. HS8 (B3): the `task` tool is offered only for a skill that
//! names it in its `allowed_tools` — the same declaration feeds the gate and
//! the tool list. HS7 (B4): the owner's hooks from `~/.axh/settings.json` run
//! here too — they may only block, never permit (B2), fail open (B3), and a
//! project's hooks are parsed and left out (B1), said so in the run's
//! `stderr`.
//!
//! B1 — no silent fallback: the endpoint is the skill provider's as it is
//! (`config.agents.providers`). A provider without a base URL — Anthropic,
//! the current default, for which the harness has no client yet (H4) — fails
//! the run with a message naming it, recorded as a `Failed` run under
//! `backend: axh`. A run says `axh` in `runs.backend` or it did not run.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use axiomata_harness::mcp;
use axiomata_harness::permissions::settings;
use axiomata_harness::permissions::{Gate, Mode, Nobody, Rule, RuleSet};
use axiomata_harness::skills::{self, SkillTool};
use axiomata_harness::tools::files::{SkillRoots, file_tools};
use axiomata_harness::tools::shell::{ShellOptions, shell_tools};
use axiomata_harness::tools::{Tool, Toolbox};
use axiomata_harness::{
    Cancel, Event, EventSink, FinishReason, McpStatus, ModelEndpoint, OpenAiCompat, RunConfig,
    SharedSink,
};

use crate::agents::{AgentRequest, AgentRunResult, BACKEND_AXH, agent_slots};
use crate::config::{Config, ProviderId};
use crate::error::AxiomataError;
use crate::paths::global_skills_dir;

/// Runs `request` on this backend — see the module doc for its shape. A turn
/// that ran but was refused its tools, hit the turn limit or was cancelled is
/// an `Ok` with `exit_code == 1` and the reason in `stderr`; an `Err` means
/// there was no answer at all (no endpoint, no usable home folder, a timeout).
pub async fn run(request: AgentRequest, config: &Config) -> Result<AgentRunResult, AxiomataError> {
    // B1: the endpoint as the skill provider has it — before a single token
    // is spent, a provider without a base URL fails here.
    let endpoint = endpoint(config, request.model.clone())?;
    // The harness's home (`~/.axh`, overridable as `AXH_HOME`): its settings,
    // approvals, MCP config and the owner's own `skills/` folder live there.
    let home = settings::home_dir()?;
    // D5: one of the four global agent slots, taken before the tools are
    // built and any MCP server — which spawns a process — is started.
    let _permit = agent_slots()
        .acquire()
        .await
        .expect("agent_slots semaphore is never closed");
    let cwd = request.cwd.clone();
    let declared = declared_rules(request.name.as_deref(), request.allowed_tools.as_deref())?;
    let gate = gate_for(request.name.as_deref(), &declared);
    // B3: a project's settings are never trusted in an unattended run; from
    // them only the sandbox's read secrets count. HS7 B4: the owner's hooks
    // run here as they do in the CLI; a project's hooks are parsed and left
    // out (B1), the run's `stderr` says so below.
    let loaded = settings::load(&home, &cwd, false)?;
    let sandbox = loaded.sandbox.clone();
    let project_hooks_skipped = loaded.project_hooks_skipped;
    // K2/K3: the app's `~/.axiomata/skills` beside the harness's own folders;
    // a skill the catalog cannot use is a note, never an error.
    let skill_dirs = [global_skills_dir()];
    let (catalog, notes) = skills::Catalog::load(&home, &cwd, false, &skill_dirs);
    for note in notes {
        tracing::warn!(note, "skill left out of the harness catalog");
    }
    let catalog = Arc::new(catalog);
    let model = OpenAiCompat::new(endpoint)?;
    // D1: the prompt the core built is the run's user message; the harness's
    // own short system prompt stays as it is.
    let mut run_config = RunConfig::new(&cwd);
    run_config.timeout = request.timeout;
    run_config.skill_dirs = skill_dirs.to_vec();
    // HS8 (B3): the `task` tool exists only for a skill that names it in its
    // `allowed_tools` — the same declaration that is the gate's whole
    // permission set above, so what is offered and what is allowed come from
    // one place; a skill that declares nothing never even sees the tool.
    run_config.subagents = declared.iter().any(|rule| rule.names_tool("task"));
    // HS7 (B4): the owner's hooks, and the sandbox their commands run in (D5).
    run_config.hooks = loaded.hooks;
    run_config.sandbox = Some(sandbox.clone());
    // D7: what the record needs, kept as the run reports it.
    let notes = Notes::default();
    if project_hooks_skipped {
        notes.push(
            "the project's own .axh/ hooks are not run: only the owner's \
             ~/.axh/settings.json hooks run"
                .to_owned(),
        );
    }
    let mut sink = SharedSink::new(Box::new(notes.clone()));
    let mut tools = Toolbox::new();
    tools.extend(file_tools(&cwd, SkillRoots::new(&catalog.folders()))?);
    // The `skill` tool while there is at least one skill to load (K1.2).
    if !catalog.is_empty() {
        tools.extend([
            Box::new(SkillTool::new(Arc::clone(&catalog), sink.clone())) as Box<dyn Tool>
        ]);
    }
    tools.set_gate(Arc::new(gate));
    // B2: the harness's own MCP config, never opencode's. A project's server
    // is never asked about here (headless, M2): left out with a note.
    let entries = mcp::config::load(&home, &cwd)?;
    let entries = mcp::approval::permitted(entries, &home, &cwd, false)?;
    tools.extend(mcp::start(entries, &mut sink).await);
    // The shell after the gate — the sandbox the settings word on rides along
    // (HS3b), so commands run guarded even though nobody asks.
    tools.extend(shell_tools(
        &cwd,
        ShellOptions {
            sandbox: Some(sandbox),
            ..ShellOptions::from_environment()
        },
    )?);
    // No signal handlers: this process is the app, not `axh`, and nothing
    // cancels a recorded run from here — the timeout ends it.
    let cancel = Cancel::new();
    let started = Instant::now();
    let outcome = axiomata_harness::run(
        &run_config,
        &model,
        &tools,
        &request.prompt,
        &mut sink,
        &cancel,
    )
    .await;
    let duration_ms = started.elapsed().as_millis() as u64;
    let input_tokens = (outcome.usage.input_tokens > 0).then_some(outcome.usage.input_tokens);
    let output_tokens = (outcome.usage.output_tokens > 0).then_some(outcome.usage.output_tokens);
    // D6: the harness's own deadline is the run's; an expiry is the same
    // `AgentTimeout` an opencode run records, not a half-done answer.
    let (exit_code, ended) = match outcome.reason {
        FinishReason::Answered => (0, None),
        FinishReason::MaxTurns => (
            1,
            Some(format!(
                "the run ended at its turn limit ({} turns)",
                outcome.turns
            )),
        ),
        FinishReason::Cancelled => (1, Some("the run was cancelled".to_owned())),
        FinishReason::Failed => (1, Some("the model API failed".to_owned())),
        FinishReason::Timeout => {
            return Err(AxiomataError::AgentTimeout {
                backend: BACKEND_AXH,
                timeout: request.timeout,
            });
        }
    };
    Ok(AgentRunResult {
        stdout: outcome.answer.unwrap_or_default(),
        stderr: notes.join(ended.as_deref()),
        exit_code,
        duration_ms,
        // Metered by the caller from the token counts (`spend::metered_cost_usd`,
        // keyed by this bare model id) — no estimate of ours to record.
        cost_usd: None,
        input_tokens,
        output_tokens,
        num_turns: Some(outcome.turns),
    })
}

/// The model endpoint of an `axh` run (B1): the skill provider's base URL and
/// key, and the run's model — bare, as the API names it (D2).
///
/// # Errors
///
/// [`AxiomataError::InvalidAgentModel`] when the skill provider is missing
/// from the config, has no base URL (Anthropic: no OpenAI-compatible API yet,
/// H4), has no key where one is needed (OpenRouter), or when the run carries
/// no model — a config gap must fail loudly, never silently bill another
/// endpoint.
fn endpoint(config: &Config, model: Option<String>) -> Result<ModelEndpoint, AxiomataError> {
    let id = config.agents.skill_provider;
    let gap = |reason: String| AxiomataError::InvalidAgentModel { reason };
    let settings = config.agents.providers.get(&id).ok_or_else(|| {
        gap(format!(
            "backend axh needs settings for {} in config.toml",
            id.as_str()
        ))
    })?;
    let base_url = settings
        .base_url
        .clone()
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| {
            gap(format!(
                "backend axh needs a provider with a base URL (open_router or ollama); \
                 the skill provider {} has none — set it in Settings → Providers",
                id.as_str()
            ))
        })?;
    let api_key = settings
        .api_key
        .clone()
        .filter(|key| !key.trim().is_empty());
    if id == ProviderId::OpenRouter && api_key.is_none() {
        return Err(gap(
            "OpenRouter has no API key in config.toml (Settings → Providers)".to_owned(),
        ));
    }
    let model = model
        .filter(|model| !model.trim().is_empty())
        .ok_or_else(|| {
            gap(format!(
                "no model for an axh run: set `model:` in the skill's frontmatter \
                 or the skill model of {}",
                id.as_str()
            ))
        })?;
    Ok(ModelEndpoint {
        base_url,
        api_key,
        model,
    })
}

/// The skill's declared `allowed_tools` as rules — parsed once, for the gate (B3) and, in HS8, for whether
/// the run offers `task` at all (HS8, B3): offering and allowing come from one reading of the frontmatter.
///
/// # Errors
///
/// [`AxiomataError::InvalidSkill`] for an `allowed_tools` entry that is no
/// rule — the skill exists but is misconfigured; named by the run, since only
/// the request travels here.
fn declared_rules(
    name: Option<&str>,
    allowed_tools: Option<&str>,
) -> Result<Vec<Rule>, AxiomataError> {
    match allowed_tools.map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => skills::rules_of(text).map_err(|reason| AxiomataError::InvalidSkill {
            path: PathBuf::from(name.unwrap_or("the run")),
            reason: format!("unparsable `allowed_tools`: {reason}"),
        }),
        None => Ok(Vec::new()),
    }
}

/// The run's gate (B3): mode `default` with nobody to answer a question, and
/// the skill's declared `allowed_tools` as both its allow rules and its
/// narrowing — what the skill names is allowed, everything else is refused
/// before any rule or mode is asked. A skill that declares nothing keeps the
/// mode's own word: reads are free, everything that changes something asks
/// and `Nobody` refuses it. [`declared_rules`] parses the list.
fn gate_for(name: Option<&str>, declared: &[Rule]) -> Gate {
    let mut gate = Gate::new(
        RuleSet {
            allow: declared.to_vec(),
            ask: Vec::new(),
            deny: Vec::new(),
        },
        Mode::Default,
        Arc::new(Nobody),
    );
    if !declared.is_empty() {
        gate.restrict_name = name.map(str::to_owned);
        gate.restrict = Some(declared.to_vec());
    }
    gate
}

/// What the run's events leave behind for the record (D7): the errors the
/// model API or the loop reported, the MCP servers that did not come up, and
/// the calls the gate refused — joined into the run's `stderr`, so the
/// dashboard shows *why* a run failed or did less than it should, without an
/// events table of its own.
#[derive(Clone, Default)]
struct Notes(Arc<Mutex<Vec<String>>>);

impl EventSink for Notes {
    fn emit(&mut self, event: Event) {
        if let Some(line) = note_of(event) {
            self.0
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .push(line);
        }
    }
}

/// One event's line for the record, `None` for what the record has nothing to say about (D7). An event of
/// a subagent (HS8, D8) arrives wrapped in `Sub`: its line is kept with the prefix, never lost.
fn note_of(event: Event) -> Option<String> {
    match event {
        Event::Error { message } => Some(message),
        Event::McpServer {
            name,
            status: McpStatus::Failed,
            reason,
            ..
        } => Some(format!(
            "mcp server {name} left out: {}",
            reason.unwrap_or_else(|| "it did not start".to_owned())
        )),
        Event::Permission {
            allowed: false,
            tool,
            reason,
            ..
        } => Some(format!("{tool}: {reason}")),
        // HS7: what a hook did or why it did not (a block's reason, a
        // failure) — the record shows why a call never ran.
        Event::Hook {
            hook_event,
            note: Some(note),
            ..
        } => Some(format!("hook {hook_event}: {note}")),
        Event::Sub { event, .. } => note_of(*event).map(|line| format!("subagent: {line}")),
        _ => None,
    }
}

impl Notes {
    /// A line for the record that no event brought (the project's hooks
    /// left out, B1).
    fn push(&self, line: String) {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(line);
    }

    /// The kept lines, then `why` — the reason a run ended without an answer —
    /// one per line; nothing at all when neither has anything to say.
    fn join(&self, why: Option<&str>) -> String {
        let mut lines = self
            .0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone();
        lines.extend(why.map(str::to_owned));
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ModelCost, ProviderSettings};
    use crate::skills::model::RunStatus;
    use crate::skills::runlog;
    use crate::skills::runner;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use axiomata_harness::Access;
    use rusqlite::Connection;
    use serde_json::{Value, json};
    use std::path::PathBuf;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn config_with_provider(id: ProviderId, settings: ProviderSettings) -> Config {
        let mut config = Config::default();
        config.agents.skill_provider = id;
        config.agents.providers.insert(id, settings);
        config
    }

    fn openrouter_settings() -> ProviderSettings {
        ProviderSettings {
            base_url: Some("https://openrouter.ai/api".into()),
            api_key: Some("sk-or".into()),
            chat_model: "c".into(),
            skill_model: "deepseek/deepseek-chat".into(),
        }
    }

    #[test]
    fn the_endpoint_is_the_skill_providers_own_and_needs_a_base_url() {
        // B1 on the default config: Anthropic, the skill provider Axiomata ships
        // with, has neither base URL nor key — no silent fallback to opencode.
        let err = endpoint(&Config::default(), Some("claude-haiku-5-5".into())).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("base URL"), "{message}");
        assert!(message.contains("anthropic"), "{message}");

        let openrouter = config_with_provider(ProviderId::OpenRouter, openrouter_settings());
        let resolved = endpoint(&openrouter, Some("deepseek/deepseek-chat".into())).unwrap();
        assert_eq!(resolved.base_url, "https://openrouter.ai/api");
        assert_eq!(resolved.api_key.as_deref(), Some("sk-or"));
        assert_eq!(resolved.model, "deepseek/deepseek-chat");

        // A keyless OpenRouter is refused, not tried — as the CLI's `harness run` refuses it.
        let mut keyless = openrouter_settings();
        keyless.api_key = None;
        let keyless = config_with_provider(ProviderId::OpenRouter, keyless);
        assert!(endpoint(&keyless, Some("m".into())).is_err());

        // A local Ollama needs no key.
        let ollama = config_with_provider(
            ProviderId::Ollama,
            ProviderSettings {
                base_url: Some("http://localhost:11434".into()),
                api_key: None,
                chat_model: String::new(),
                skill_model: "qwen3:8b".into(),
            },
        );
        assert_eq!(
            endpoint(&ollama, Some("qwen3:8b".into())).unwrap().model,
            "qwen3:8b"
        );

        // No model on the request (nothing resolved) is an error, not a guess.
        assert!(endpoint(&ollama, None).is_err());
        assert!(endpoint(&ollama, Some("  ".into())).is_err());
    }

    fn bash(command: &str) -> Access {
        Access::Bash {
            command: command.into(),
            folder: String::new(),
            unsandboxed: false,
        }
    }

    #[tokio::test]
    async fn the_gate_allows_what_the_skill_declares_and_refuses_the_rest() {
        // B3: the declaration is the whole permission set, narrowed on top of itself.
        let declared = declared_rules(
            Some("mail-digest"),
            Some("Read mcp__apple-mail__search_emails"),
        )
        .unwrap();
        let gate = gate_for(Some("mail-digest"), &declared);
        assert_eq!(gate.restrict_name.as_deref(), Some("mail-digest"));
        assert!(gate.mode_name() == "default");
        // A declared tool is allowed by its own rule …
        assert!(
            gate.judge(
                "read_file",
                &Access::Read {
                    path: "a.txt".into()
                }
            )
            .await
            .is_ok()
        );
        assert!(
            gate.judge(
                "mcp__apple-mail__search_emails",
                &Access::Mcp {
                    server: "apple-mail".into(),
                    tool: "search_emails".into(),
                }
            )
            .await
            .is_ok()
        );
        // … and an undeclared one is refused before anything is asked, with
        // the skill named in the refusal.
        let refused = gate.judge("bash", &bash("rm -rf /")).await.unwrap_err();
        assert!(refused.contains("mail-digest"), "{refused}");
        assert!(refused.contains("allows only"), "{refused}");
        // Declaring `Read` means all reads below the working folder — that is
        // what the rule says.
        assert!(
            gate.judge(
                "read_file",
                &Access::Read {
                    path: "secret.txt".into()
                }
            )
            .await
            .is_ok()
        );

        // A skill that declares only a command gets no free reads: its
        // declaration is the whole permission set — unlike declaring nothing,
        // where mode default keeps reads free.
        let git_only = gate_for(
            Some("release-notes"),
            &declared_rules(Some("release-notes"), Some("Bash(git log:*)")).unwrap(),
        );
        let refused = git_only
            .judge(
                "read_file",
                &Access::Read {
                    path: "secret.txt".into(),
                },
            )
            .await
            .unwrap_err();
        assert!(refused.contains("release-notes"), "{refused}");
        assert!(refused.contains("allows only"), "{refused}");
        // … and its own command is allowed by its own rule.
        assert!(git_only.judge("bash", &bash("git log -1")).await.is_ok());
    }

    #[tokio::test]
    async fn a_skill_that_declares_nothing_reads_freely_but_changes_nothing() {
        let gate = gate_for(Some("summarize"), &[]);
        assert!(gate.restrict.is_none());
        // Mode default: reads are free …
        assert!(
            gate.judge(
                "read_file",
                &Access::Read {
                    path: "a.txt".into()
                }
            )
            .await
            .is_ok()
        );
        // … and a command asks, which nobody answers.
        let refused = gate.judge("bash", &bash("git status")).await.unwrap_err();
        assert!(refused.contains("no one to ask"), "{refused}");
    }

    #[test]
    fn an_unparsable_declaration_is_a_clear_error_naming_the_run() {
        let err = match declared_rules(Some("weird"), Some("Bash(")) {
            Err(err) => err,
            Ok(_) => panic!("unparsable `allowed_tools` must be refused"),
        };
        assert!(matches!(err, AxiomataError::InvalidSkill { .. }));
        let message = err.to_string();
        assert!(message.contains("weird"), "{message}");
        assert!(message.contains("allowed_tools"), "{message}");
    }

    #[test]
    fn the_collector_keeps_only_what_the_record_needs() {
        let mut notes = Notes::default();
        notes.emit(Event::Started {
            model: "m".into(),
            cwd: ".".into(),
            tools: vec![],
        });
        notes.emit(Event::Error {
            message: "the model API answered 500".into(),
        });
        notes.emit(Event::McpServer {
            name: "apple-mail".into(),
            status: McpStatus::Failed,
            tools: 0,
            reason: Some("it did not start".into()),
        });
        notes.emit(Event::Permission {
            turn: 1,
            id: "c".into(),
            tool: "bash".into(),
            target: "rm /".into(),
            allowed: false,
            reason: "the command /demo allows only: Read".into(),
        });
        // HS7: a hook's note — a block's reason or a failure — reaches the
        // record too.
        notes.emit(Event::Hook {
            turn: Some(1),
            hook_event: "PreToolUse".into(),
            command: "check".into(),
            blocked: true,
            context_chars: 12,
            note: Some("not today".into()),
        });
        notes.emit(Event::Usage {
            turn: 1,
            input_tokens: 5,
            output_tokens: 2,
        });
        let joined = notes.join(Some("the model API failed"));
        assert!(joined.contains("500"), "{joined}");
        assert!(joined.contains("apple-mail"), "{joined}");
        assert!(joined.contains("allows only"), "{joined}");
        assert!(joined.contains("hook PreToolUse: not today"), "{joined}");
        assert!(!joined.contains("token"), "{joined}");
        assert!(joined.ends_with("the model API failed"), "{joined}");
        assert_eq!(Notes::default().join(None), "");
    }

    /// Isolated `AXIOMATA_HOME` **and** `AXH_HOME` — so the harness's own
    /// settings, approvals, MCP config and skill folder are scratch ones and
    /// never the owner's real `~/.axh` — plus a migrated database.
    struct Fixture {
        _guard: std::sync::MutexGuard<'static, ()>,
        home: PathBuf,
        axh_home: PathBuf,
        config: Config,
        db: Mutex<Connection>,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let guard = ENV_MUTEX.lock().unwrap();
            let home = unique_temp_dir(&format!("axiomata-test-axh-{tag}-home"));
            let cwd = unique_temp_dir(&format!("axiomata-test-axh-{tag}-cwd"));
            let axh_home = unique_temp_dir(&format!("axiomata-test-axh-{tag}-axh"));
            std::fs::create_dir_all(home.join("logs")).unwrap();
            std::fs::create_dir_all(&cwd).unwrap();
            std::fs::create_dir_all(&axh_home).unwrap();
            // SAFETY: serialized by `guard`, see `paths::tests`.
            unsafe {
                std::env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
                std::env::set_var("AXH_HOME", &axh_home);
            }
            let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();
            let config = Config {
                workspace_root: cwd,
                ..Config::default()
            };
            Self {
                _guard: guard,
                home,
                axh_home,
                config,
                db: Mutex::new(db),
            }
        }

        fn write_skill(&self, name: &str, text: &str) {
            let dir = self.home.join("skills").join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), text).unwrap();
        }

        fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
            self.db.lock().unwrap_or_else(|poison| poison.into_inner())
        }
    }

    /// A fake OpenAI-compatible endpoint: one canned answer, and the requests
    /// it saw kept for assertions. Raw tokio, in the shape of the harness's
    /// own `FakeApi` — that one is `cfg(test)` over there, and the core may
    /// not depend on the harness's tests.
    struct FakeApi {
        base_url: String,
        /// Per request: the head of the POST and its parsed JSON body.
        seen: Arc<Mutex<Vec<(String, Value)>>>,
    }

    impl FakeApi {
        async fn spawn(answer: Value) -> Self {
            Self::spawn_each(vec![answer]).await
        }

        /// One canned answer per request; the last one repeats when more
        /// requests come than there are answers.
        async fn spawn_each(answers: Vec<Value>) -> Self {
            assert!(!answers.is_empty());
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base_url = format!("http://{}", listener.local_addr().unwrap());
            let seen: Arc<Mutex<Vec<(String, Value)>>> = Arc::new(Mutex::new(Vec::new()));
            let kept = Arc::clone(&seen);
            tokio::spawn(async move {
                loop {
                    let Ok((mut socket, _)) = listener.accept().await else {
                        return;
                    };
                    let (head, body) = Self::read_request(&mut socket).await;
                    let parsed: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    let number = {
                        let mut seen = kept.lock().unwrap();
                        seen.push((head, parsed));
                        seen.len() - 1
                    };
                    let answer = &answers[number.min(answers.len() - 1)];
                    let payload = serde_json::to_string(answer).unwrap();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                         content-length: {}\r\nconnection: close\r\n\r\n{payload}",
                        payload.len()
                    );
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                }
            });
            Self { base_url, seen }
        }

        /// The head and body of one HTTP request, read until `content-length` bytes arrived.
        async fn read_request(socket: &mut tokio::net::TcpStream) -> (String, Vec<u8>) {
            let mut buffer = Vec::new();
            let mut chunk = [0_u8; 4096];
            let head_end = loop {
                if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                    break end + 4;
                }
                let n = socket.read(&mut chunk).await.unwrap_or(0);
                if n == 0 {
                    return (String::from_utf8_lossy(&buffer).into_owned(), Vec::new());
                }
                buffer.extend_from_slice(&chunk[..n]);
            };
            let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
            let length = head
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())?
                })
                .unwrap_or(0);
            let mut body = buffer[head_end..].to_vec();
            while body.len() < length {
                let n = socket.read(&mut chunk).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..n]);
            }
            (head, body)
        }
    }

    /// The whole path HS6b exists for: a `SKILL.md` with `backend: axh` runs
    /// through the own harness against a fake OpenAI-compatible endpoint, and
    /// the `runs` row carries the backend, the provider, the bare model, the
    /// tokens and the metered cost — no model spend, no network, no real
    /// `~/.axh`.
    #[tokio::test]
    async fn an_axh_skill_runs_end_to_end_and_lands_in_the_runs_table() {
        let mut fx = Fixture::new("e2e");
        fx.write_skill(
            "axh-smoke",
            "---\nname: axh-smoke\ndescription: d\nbackend: axh\nallowed_tools: Read\n---\n\
             Say the word ok.\n",
        );
        let api = FakeApi::spawn(json!({
            "choices": [{"message": {"role": "assistant", "content": "ok"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 2}
        }))
        .await;
        fx.config.agents.skill_provider = ProviderId::OpenRouter;
        fx.config.agents.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some(api.base_url.clone()),
                api_key: Some("sk-test".into()),
                chat_model: "c".into(),
                skill_model: "test-model".into(),
            },
        );
        fx.config.agents.costs.insert(
            "test-model".into(),
            ModelCost {
                input_per_m: 1.0,
                output_per_m: 2.0,
            },
        );

        let record = runner::execute_and_record_skill("axh-smoke", &fx.config, &fx.db)
            .await
            .unwrap();
        assert_eq!(record.status, RunStatus::Success, "err: {:?}", record.error);
        assert_eq!(record.backend, "axh");
        assert_eq!(record.exit_code, Some(0));
        assert_eq!(record.provider.as_deref(), Some("open_router"));
        assert_eq!(record.model.as_deref(), Some("test-model"));
        assert_eq!(record.stdout, "ok");
        assert_eq!(record.input_tokens, Some(10));
        assert_eq!(record.output_tokens, Some(2));
        assert_eq!(record.num_turns, Some(1));
        // 10 input tokens × $1/M + 2 output tokens × $2/M — metered from the
        // price table, not estimated (`spend::metered_cost_usd`).
        let cost = record.cost_usd.unwrap();
        assert!((cost - 14.0 / 1e6).abs() < 1e-12, "{cost}");

        let runs = runlog::list_runs(&fx.conn(), 10).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].skill_name, "axh-smoke");
        assert_eq!(runs[0].backend, "axh");
        // The `model` column is not part of `RunSummary`; read it straight
        // from the row the run persisted (D2: bare id, no provider prefix).
        let stored_model: String = fx
            .conn()
            .query_row(
                "SELECT model FROM runs WHERE skill_name = ?1",
                ["axh-smoke"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_model, "test-model");

        // What the API saw: the bearer key, the bare model, the skill's body as
        // the task — and exactly one call, since axh has no repair turn (D3).
        let seen = api.seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "one model call, no repair turn");
        assert!(
            seen[0]
                .0
                .to_ascii_lowercase()
                .contains("authorization: bearer sk-test"),
            "{}",
            seen[0].0
        );
        assert_eq!(seen[0].1["model"], "test-model");
        let messages = seen[0].1["messages"].to_string();
        assert!(messages.contains("Say the word ok."), "{messages}");
    }

    /// HS8, B3 end to end: the model's tool list carries `task` only for a
    /// skill that names it in its `allowed_tools` — the same declaration that
    /// is the gate's whole permission set, so offering and allowing come from
    /// one reading of the frontmatter. A skill that does not declare it never
    /// even sees the tool.
    #[tokio::test]
    async fn only_a_skill_declaring_task_is_offered_the_task_tool() {
        let mut fx = Fixture::new("task-tool");
        fx.write_skill(
            "axh-plain",
            "---\nname: axh-plain\ndescription: d\nbackend: axh\nallowed_tools: Read\n---\n\
             Say ok.\n",
        );
        fx.write_skill(
            "axh-subbed",
            "---\nname: axh-subbed\ndescription: d\nbackend: axh\nallowed_tools: Read task\n---\n\
             Say ok.\n",
        );
        let api = FakeApi::spawn(json!({
            "choices": [{"message": {"role": "assistant", "content": "ok"}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 2}
        }))
        .await;
        fx.config.agents.skill_provider = ProviderId::OpenRouter;
        fx.config.agents.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some(api.base_url.clone()),
                api_key: Some("sk-test".into()),
                chat_model: "c".into(),
                skill_model: "test-model".into(),
            },
        );

        for skill in ["axh-plain", "axh-subbed"] {
            let record = runner::execute_and_record_skill(skill, &fx.config, &fx.db)
                .await
                .unwrap();
            assert_eq!(record.status, RunStatus::Success, "err: {:?}", record.error);
        }

        let seen = api.seen.lock().unwrap();
        assert_eq!(seen.len(), 2, "one model call per skill");
        let offered = |body: &Value| -> Vec<String> {
            body["tools"]
                .as_array()
                .map(|specs| {
                    specs
                        .iter()
                        .filter_map(|spec| spec["function"]["name"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        let plain = offered(&seen[0].1);
        assert!(!plain.contains(&"task".to_owned()), "offered: {plain:?}");
        let subbed = offered(&seen[1].1);
        assert!(subbed.contains(&"task".to_owned()), "offered: {subbed:?}");
    }

    /// B1 end to end: the default skill provider (Anthropic, no base URL)
    /// records a failed run that says so — no model call at all.
    #[tokio::test]
    async fn an_axh_run_without_a_usable_provider_is_recorded_as_failed() {
        let fx = Fixture::new("no-url");
        fx.write_skill(
            "axh-unserved",
            "---\nname: axh-unserved\ndescription: d\nbackend: axh\n---\nSay ok.\n",
        );
        let record = runner::execute_and_record_skill("axh-unserved", &fx.config, &fx.db)
            .await
            .unwrap();
        assert_eq!(record.status, RunStatus::Failed);
        let message = record.error.unwrap();
        assert!(message.contains("base URL"), "{message}");
        assert_eq!(runlog::list_runs(&fx.conn(), 10).unwrap().len(), 1);
    }

    /// HS7, B4 end to end: the owner's hooks run in an unattended core run —
    /// a `PreToolUse` hook exits 2 with stderr, the call is blocked and the
    /// reason lands in the run's `stderr` through the `Notes` collector. A
    /// project's hooks are parsed and left out (B1), and the run says so.
    #[tokio::test]
    async fn hooks_run_in_unattended_core_runs_and_a_projects_are_left_out() {
        let mut fx = Fixture::new("hooks");
        fx.write_skill(
            "axh-hooked",
            "---\nname: axh-hooked\ndescription: d\nbackend: axh\n---\n\
             Look at a.txt, then say ok.\n",
        );
        std::fs::create_dir_all(fx.axh_home.join(".axh")).unwrap();
        std::fs::write(
            fx.axh_home.join(".axh/settings.json"),
            r#"{"hooks": {"PreToolUse": [{"hooks": [{"type": "command", "command": "echo 'hook says no' >&2; exit 2"}]}]}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(fx.config.workspace_root.join(".axh")).unwrap();
        std::fs::write(
            fx.config.workspace_root.join(".axh/settings.json"),
            r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "true"}]}]}}"#,
        )
        .unwrap();
        let api = FakeApi::spawn_each(vec![
            json!({"choices": [{"message": {"role": "assistant", "tool_calls": [
                {"id": "c1", "type": "function",
                 "function": {"name": "read_file", "arguments": "{\"path\":\"a.txt\"}"}}
            ]}}]}),
            json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}],
                   "usage": {"prompt_tokens": 10, "completion_tokens": 2}}),
        ])
        .await;
        fx.config.agents.skill_provider = ProviderId::OpenRouter;
        fx.config.agents.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some(api.base_url.clone()),
                api_key: Some("sk-test".into()),
                chat_model: "c".into(),
                skill_model: "test-model".into(),
            },
        );

        let record = runner::execute_and_record_skill("axh-hooked", &fx.config, &fx.db)
            .await
            .unwrap();
        assert_eq!(record.status, RunStatus::Success, "err: {:?}", record.error);
        assert_eq!(record.stdout, "ok");
        assert_eq!(record.num_turns, Some(2), "the blocked call was a turn");
        assert!(
            record.stderr.contains("hook says no"),
            "the hook's block reached the record: {}",
            record.stderr
        );
        assert!(
            record.stderr.contains(".axh/ hooks are not run"),
            "and so did the project's hooks being left out: {}",
            record.stderr
        );
    }
}
