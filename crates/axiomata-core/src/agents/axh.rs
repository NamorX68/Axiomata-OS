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
//! widen the gate — their `deny` and `ask` rules still hold (an `ask` is a
//! refusal here, since nobody answers), their `allow` rules do not count.
//! HS8 (B3): the `task` tool is offered only for a skill that names it in its
//! `allowed_tools` — the same declaration feeds the gate and the tool list.
//! HS7 (B4): the owner's hooks from `~/.axh/settings.json` run here too —
//! they may only block, never permit (B2), fail open (B3), and a project's
//! hooks are parsed and left out (B1), said so in the run's `stderr`.
//!
//! B1 — no silent fallback: the endpoint is the skill provider's as it is
//! (`config.agents.providers`). A provider without a base URL — Anthropic,
//! the current default, for which the harness has no client yet (H4) — fails
//! the run with a message naming it, recorded as a `Failed` run under
//! `backend: axh`. A run says `axh` in `runs.backend` or it did not run.

use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use axiomata_harness::mcp;
use axiomata_harness::permissions::settings;
use axiomata_harness::permissions::{Gate, Mode, Nobody, Rule, RuleSet};
use axiomata_harness::setup;
use axiomata_harness::skills;
use axiomata_harness::{
    Cancel, Event, EventSink, FinishReason, McpStatus, Message, ModelEndpoint, OpenAiCompat,
    RunConfig, SharedSink,
};

use crate::agents::claude_code::{bash_rule, is_shell_rule};
use crate::agents::opencode::{JSON_REPAIR_PROMPT, JSON_REPAIR_TIMEOUT, find_json_object};
use crate::agents::opencode_instance::{READ_COMMANDS, READ_DENIED_COMMANDS};
use crate::agents::{
    AgentRequest, AgentRunResult, BACKEND_AXH, ChatReply, ChatRequest, agent_slots,
};
use crate::config::{Config, ProviderId, ProviderRole};
use crate::error::AxiomataError;
use crate::paths::global_skills_dir;

/// Runs `request` on this backend — see the module doc for its shape. A turn
/// that ran but was refused its tools, hit the turn or time limit or was
/// cancelled is an `Ok` with `exit_code == 1` and the reason in `stderr`; an
/// `Err` means the run never started (no endpoint, no usable home folder).
pub async fn run(request: AgentRequest, config: &Config) -> Result<AgentRunResult, AxiomataError> {
    // B1: the endpoint as the skill provider has it — before a single token
    // is spent, a provider without a base URL fails here.
    let endpoint = endpoint(config, ProviderRole::Skill, request.model.clone())?;
    // The harness's home (`~/.axh`, overridable as `AXH_HOME`): its settings,
    // approvals, MCP config and the owner's own `skills/` folder live there.
    let home = settings::home_dir()?;
    // D5: one of the four global agent slots, taken before the tools are
    // built and any MCP server — which spawns a process — is started.
    let _permit = agent_slots()
        .acquire()
        .await
        .expect("agent_slots semaphore is never closed");
    let declared = declared_rules(request.name.as_deref(), request.allowed_tools.as_deref())?;
    let rights = if request.read_only {
        read_rights(request.name.as_deref(), &declared)?
    } else {
        Rights {
            name: request.name.as_deref(),
            declared: declared.clone(),
            // A skill keeps the mode's own word for what it does not declare (B3).
            mode: Mode::Default,
            deny: Vec::new(),
        }
    };
    let mut harness =
        Harness::prepare(&home, &request.cwd, &rights, request.timeout, config).await?;
    // No signal handlers: this process is the app, not `axh`, and nothing
    // cancels a recorded run from here — the timeout ends it.
    let cancel = Cancel::new();
    let started = Instant::now();
    let model = OpenAiCompat::new(endpoint)?;
    let (mut outcome, conversation) = axiomata_harness::run_continuing(
        &harness.run_config,
        &model,
        &harness.tools,
        Vec::new(),
        &request.prompt,
        &mut harness.sink,
        &cancel,
    )
    .await;
    // `output: json` (harness-choice CP5, lifting HS6b's D3): an answer without a JSON object is asked once more,
    // in the same conversation, as on Opencode and Claude Code. Usage and turns count twice — both were paid.
    let answer_lacks_json = outcome.reason == FinishReason::Answered
        && outcome
            .answer
            .as_deref()
            .is_none_or(|answer| find_json_object(answer).is_none());
    if request.expects_json && answer_lacks_json {
        tracing::warn!("the reply holds no JSON object; asking once more in the same conversation");
        // Never longer than the skill's own limit, and no subagents: the repair only writes out what is known.
        harness.run_config.timeout = request.timeout.min(JSON_REPAIR_TIMEOUT);
        harness.run_config.subagents = false;
        let (repair, _) = axiomata_harness::run_continuing(
            &harness.run_config,
            &model,
            &harness.tools,
            conversation,
            JSON_REPAIR_PROMPT,
            &mut harness.sink,
            &cancel,
        )
        .await;
        outcome.usage += repair.usage;
        outcome.turns += repair.turns;
        if let Some(answer) = repair
            .answer
            .filter(|answer| find_json_object(answer).is_some())
        {
            outcome.answer = Some(answer);
        }
    }
    let duration_ms = started.elapsed().as_millis() as u64;
    let input_tokens = (outcome.usage.input_tokens > 0).then_some(outcome.usage.input_tokens);
    let output_tokens = (outcome.usage.output_tokens > 0).then_some(outcome.usage.output_tokens);
    let (exit_code, ended) = ending(outcome.reason, outcome.turns, request.timeout);
    Ok(AgentRunResult {
        stdout: outcome.answer.unwrap_or_default(),
        stderr: harness.notes.join(ended.as_deref()),
        exit_code,
        duration_ms,
        // Metered by the caller from the token counts (`spend::metered_cost_usd`,
        // keyed by this bare model id) — no estimate of ours to record.
        cost_usd: None,
        input_tokens,
        cached_input_tokens: input_tokens.map(|_| outcome.usage.cached_input_tokens),
        output_tokens,
        num_turns: Some(outcome.turns),
    })
}

/// One assistant-bar turn on Glut (`docs/plans/harness-choice.md`, CP4): the chat provider's endpoint and chat
/// model, the conversation `session_id` names read from its file and continued, the whole conversation written back
/// after an answer. An id that names no conversation (an Opencode or Claude Code one after a harness switch, a
/// removed file) starts a new one, as on the other harnesses. The vault's `AGENTS.md` and the module manifest are
/// the run's instructions. The rights: the turn's declared `allowed_tools` (a connector's instruct turn), else — with
/// tools pre-approved — everything the owner's `~/.axh` rules do not deny (Opencode's unattended `*` allow), else
/// reads only. A turn that failed or answered nothing is an [`AxiomataError::AgentApi`].
pub(crate) async fn chat(
    request: ChatRequest,
    config: &Config,
) -> Result<ChatReply, AxiomataError> {
    let endpoint = endpoint(config, ProviderRole::Chat, request.model.clone())?;
    let home = settings::home_dir()?;
    let sessions = sessions_dir();
    let session_id = request
        .session_id
        .clone()
        .filter(|id| valid_session_id(id))
        .unwrap_or_else(new_session_id);
    // One turn per conversation at a time, from reading it to writing it back: two overlapping turns would both
    // build on the same history, and the later write would drop the other's.
    let lock = session_lock(&session_id);
    let _held = lock.lock().await;
    let (session_id, earlier) = match load_session(&sessions, &session_id)? {
        Loaded::Conversation(earlier) => (session_id, earlier),
        Loaded::Missing => (session_id, Vec::new()),
        // A file that no longer reads goes on as a new conversation rather than failing every later turn.
        Loaded::Unreadable(reason) => {
            tracing::warn!(%reason, session = %session_id, "a Glut conversation cannot be read; starting a new one");
            (new_session_id(), Vec::new())
        }
    };
    let instructions =
        super::claude_code::run_context(&request.cwd, request.system_prompt_file.as_deref())?;
    let _permit = agent_slots()
        .acquire()
        .await
        .expect("agent_slots semaphore is never closed");
    let declared = declared_rules(None, request.allowed_tools.as_deref())?;
    let rights = Rights {
        name: None,
        declared: declared.clone(),
        mode: chat_mode(&declared, request.auto_approve_tools),
        deny: Vec::new(),
    };
    let mut harness =
        Harness::prepare(&home, &request.cwd, &rights, request.timeout, config).await?;
    harness.run_config.instructions = instructions;
    let continuing = !earlier.is_empty();
    let started = Instant::now();
    let model = OpenAiCompat::new(endpoint)?;
    let (outcome, conversation) = axiomata_harness::run_continuing(
        &harness.run_config,
        &model,
        &harness.tools,
        earlier,
        &request.message,
        &mut harness.sink,
        &Cancel::new(),
    )
    .await;
    let api_err = |message: String| AxiomataError::AgentApi {
        backend: BACKEND_AXH,
        message,
    };
    let (_, ended) = ending(outcome.reason, outcome.turns, request.timeout);
    if let Some(ended) = ended {
        let mut message = harness.notes.join(Some(&ended));
        if continuing && matches!(outcome.reason, FinishReason::Failed | FinishReason::Timeout) {
            message.push_str(" — if a long conversation keeps failing, start a new chat");
        }
        return Err(api_err(message));
    }
    let reply_markdown = outcome.answer.unwrap_or_default().trim().to_owned();
    if reply_markdown.is_empty() {
        return Err(api_err("Glut returned an empty reply".to_owned()));
    }
    // Only an answered turn goes on record: a failed one leaves the conversation as it was.
    save_session(&sessions, &session_id, &conversation)?;
    let usage = outcome.usage;
    let input_tokens = (usage.input_tokens > 0).then_some(usage.input_tokens);
    let output_tokens = (usage.output_tokens > 0).then_some(usage.output_tokens);
    Ok(ChatReply {
        session_id,
        reply_markdown,
        is_error: false,
        // Metered by `chat_and_record` from the token counts and the owner's price table.
        cost_usd: None,
        usage: (input_tokens.is_some() || output_tokens.is_some()).then(|| {
            serde_json::json!({ "input_tokens": usage.input_tokens, "output_tokens": usage.output_tokens })
        }),
        input_tokens,
        cached_input_tokens: input_tokens.map(|_| usage.cached_input_tokens),
        output_tokens,
        num_turns: Some(outcome.turns),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

/// The rights of an `access: read` skill on Glut (`docs/plans/glut-round.md` CP1), the same as on Opencode's read
/// instance and on Claude Code: `Mode::Plan` (no edit, even where a rule would allow one; a command only by rule), all
/// reads, the declared tools without their shell rules, and the shared read commands; the git commands that reach
/// beyond the folder are refused. Stricter than the Opencode read instance on purpose: what the skill does not name —
/// another MCP tool, a subagent — is refused, since the declaration always narrows a read run (it holds at least the
/// reads).
///
/// # Errors
///
/// [`AxiomataError::InvalidSkill`] when a shared read command does not parse as a rule — a programming error, caught
/// by the tests.
fn read_rights<'a>(name: Option<&'a str>, declared: &[Rule]) -> Result<Rights<'a>, AxiomataError> {
    let rule = |text: String| {
        Rule::parse(&text).map_err(|reason| AxiomataError::InvalidSkill {
            path: PathBuf::from(name.unwrap_or("the run")),
            reason: format!("read rule {text:?}: {reason}"),
        })
    };
    let mut allowed: Vec<Rule> = declared
        .iter()
        .filter(|rule| !is_shell_rule(rule.text()))
        .cloned()
        .collect();
    // No web unless the skill names it (`WebFetch`): reads plus untrusted input plus the network would be a way out
    // for what the run read (glut-round CP2, audit).
    allowed.push(rule("Read".to_owned())?);
    for command in READ_COMMANDS {
        allowed.push(rule(bash_rule(command))?);
    }
    let deny = READ_DENIED_COMMANDS
        .iter()
        .map(|command| rule(bash_rule(command)))
        .collect::<Result<_, _>>()?;
    Ok(Rights {
        name,
        declared: allowed,
        mode: Mode::Plan,
        deny,
    })
}

/// The gate's mode for a chat turn: a declared list keeps the mode's own word for the rest (it narrows anyway);
/// without one, pre-approved tools are the owner's auto mode — what no rule denies runs, an `ask` is refused
/// since nobody answers — and otherwise only reads run.
fn chat_mode(declared: &[Rule], auto_approve: bool) -> Mode {
    if declared.is_empty() && auto_approve {
        Mode::Auto
    } else {
        Mode::Default
    }
}

/// What a run or a chat turn is allowed: the declared rules narrow it (B3), the mode speaks for the rest.
struct Rights<'a> {
    name: Option<&'a str>,
    declared: Vec<Rule>,
    mode: Mode,
    /// Refusals of the run's own beside the owner's `~/.axh` ones — a read run's git commands that reach beyond the
    /// folder.
    deny: Vec<Rule>,
}

/// The harness as one run or chat turn uses it: the run's configuration, its tools behind the gate, and the notes
/// its events leave for the record (D7).
struct Harness {
    run_config: RunConfig,
    tools: axiomata_harness::Toolbox,
    notes: Notes,
    sink: SharedSink,
}

impl Harness {
    async fn prepare(
        home: &std::path::Path,
        cwd: &std::path::Path,
        rights: &Rights<'_>,
        timeout: Duration,
        config: &Config,
    ) -> Result<Self, AxiomataError> {
        // B3: a project's settings are never trusted in an unattended run; from
        // them only what makes the run stricter counts — the sandbox's read
        // secrets and the deny/ask rules. HS7 B4: the owner's hooks run here as
        // they do in the CLI; a project's hooks are parsed and left out (B1), the
        // run's `stderr` says so below.
        let loaded = settings::load(home, cwd, false)?;
        let gate = gate_for(rights, &loaded.rules);
        let sandbox = loaded.sandbox.clone();
        // K2/K3: the app's `~/.axiomata/skills` beside the harness's own folders;
        // a skill the catalog cannot use is a note, never an error.
        let skill_dirs = [global_skills_dir()];
        let (catalog, skipped) = skills::Catalog::load(home, cwd, false, &skill_dirs);
        for note in skipped {
            tracing::warn!(note, "skill left out of the harness catalog");
        }
        let catalog = Arc::new(catalog);
        // D1: the prompt the core built is the run's user message; the harness's
        // own short system prompt stays as it is.
        let mut run_config = RunConfig::new(cwd);
        run_config.timeout = timeout;
        run_config.skill_dirs = skill_dirs.to_vec();
        // HS8 (B3): the `task` tool exists only for a run that names it in its
        // `allowed_tools` — the same declaration that is the gate's whole
        // permission set above, so what is offered and what is allowed come from
        // one place; a run that declares nothing never even sees the tool.
        run_config.subagents = rights.declared.iter().any(|rule| rule.names_tool("task"));
        // HS7 (B4): the owner's hooks, and the sandbox their commands run in (D5).
        run_config.hooks = loaded.hooks;
        run_config.sandbox = Some(sandbox.clone());
        // D7: what the record needs, kept as the run reports it.
        let notes = Notes::default();
        if loaded.project_hooks_skipped {
            notes.push(
                "the project's own .axh/ hooks are not run: only the owner's \
                 ~/.axh/settings.json hooks run"
                    .to_owned(),
            );
        }
        let mut sink = SharedSink::new(Box::new(notes.clone()));
        // B2: the harness's own MCP config, never opencode's. A project's server
        // is never asked about here (headless, M2): left out with a note.
        let servers = mcp::config::load(home, cwd)?;
        let servers = mcp::approval::permitted(servers, home, cwd, false)?;
        // The sandbox the settings word on rides along with the shell (HS3b), so
        // commands run guarded even though nobody asks.
        // CP3 of glut-round: the web search with the owner's keys and the app's limits.
        let extras = setup::Extras {
            search: crate::web_search::web_search_tool(config),
            // W8: a declared `axiomata-cli …` command reaches the app's database, as on the other harnesses.
            direct: crate::agent_entry::cli_direct_program()
                .into_iter()
                .collect(),
        };
        let mut tools =
            setup::toolbox(cwd, &catalog, gate, servers, sandbox, extras, &mut sink).await?;
        // A skill run's task is the skill's body already: loading skills would only cost a turn (glut-round CP4,
        // measured in CP0: the model loaded the running skill again).
        if rights.name.is_some() {
            tools.withdraw(skills::tool::NAME);
        }
        Ok(Self {
            run_config,
            tools,
            notes,
            sink,
        })
    }
}

/// Where Glut keeps its chat conversations, one JSON-lines file per conversation.
fn sessions_dir() -> PathBuf {
    crate::paths::axiomata_home()
        .join("harness")
        .join("sessions")
}

/// The prefix of a Glut conversation's id, so it is told apart from Opencode's and Claude Code's at a glance.
const SESSION_PREFIX: &str = "axh-";

/// The largest conversation file read back: beyond it the turn fails and asks for a new chat — no model window
/// holds that much, and a file is never read without bound.
const MAX_SESSION_BYTES: u64 = 8 * 1024 * 1024;

/// Unique names in this process: conversation ids and their partial files.
static NEXT_NAME: AtomicU64 = AtomicU64::new(0);

/// A new conversation id: the prefix, the time and a counter — unique in this process, and plain enough to be a
/// file name.
fn new_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!(
        "{SESSION_PREFIX}{nanos:x}-{:x}-{}",
        std::process::id(),
        NEXT_NAME.fetch_add(1, Ordering::Relaxed)
    )
}

/// Whether `id` can name a conversation file: Glut's prefix, then letters, digits and dashes only — never a path.
fn valid_session_id(id: &str) -> bool {
    id.len() <= 128
        && id.starts_with(SESSION_PREFIX)
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The lock of one conversation, shared by every turn on it in this process.
fn session_lock(id: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    // Locks nobody holds or waits for are dropped, so the map does not grow with every conversation ever held.
    locks.retain(|_, lock| Arc::strong_count(lock) > 1);
    Arc::clone(locks.entry(id.to_owned()).or_default())
}

/// What reading a conversation found.
#[derive(Debug)]
enum Loaded {
    Conversation(Vec<Message>),
    Missing,
    /// The file is there but does not read as a conversation; the reason, for the log.
    Unreadable(String),
}

/// The conversation `id` names.
///
/// # Errors
///
/// [`AxiomataError::Io`] when the file cannot be read for another reason than its absence or its content;
/// [`AxiomataError::AgentApi`] when it is over [`MAX_SESSION_BYTES`].
fn load_session(dir: &Path, id: &str) -> Result<Loaded, AxiomataError> {
    let path = dir.join(format!("{id}.jsonl"));
    let size = match std::fs::metadata(&path) {
        Ok(meta) => meta.len(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Loaded::Missing),
        Err(source) => return Err(AxiomataError::Io { path, source }),
    };
    if size > MAX_SESSION_BYTES {
        return Err(AxiomataError::AgentApi {
            backend: BACKEND_AXH,
            message: "this conversation has grown too long to go on; start a new chat".to_owned(),
        });
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::InvalidData => {
            return Ok(Loaded::Unreadable(err.to_string()));
        }
        Err(source) => return Err(AxiomataError::Io { path, source }),
    };
    let messages = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<Message>)
        .collect::<Result<Vec<_>, _>>();
    Ok(match messages {
        Ok(messages) => Loaded::Conversation(messages),
        Err(err) => Loaded::Unreadable(err.to_string()),
    })
}

/// Writes the whole conversation as `id`'s file, readable by the owner only, replacing the old one in one step.
fn save_session(dir: &Path, id: &str, messages: &[Message]) -> Result<(), AxiomataError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| AxiomataError::Io { path, source }
    };
    std::fs::create_dir_all(dir).map_err(io(dir))?;
    let mut text = String::new();
    for message in messages {
        let line = serde_json::to_string(message).map_err(|err| AxiomataError::AgentApi {
            backend: BACKEND_AXH,
            message: format!("the conversation cannot be written: {err}"),
        })?;
        text.push_str(&line);
        text.push('\n');
    }
    let path = dir.join(format!("{id}.jsonl"));
    // A name of its own per write, so no other write can meet it half done.
    let partial = dir.join(format!(
        "{id}.{}-{}.partial",
        std::process::id(),
        NEXT_NAME.fetch_add(1, Ordering::Relaxed)
    ));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&partial)
        .and_then(|mut file| {
            file.write_all(text.as_bytes())?;
            file.sync_all()
        })
        .map_err(io(&partial))
        .and_then(|()| std::fs::rename(&partial, &path).map_err(io(&path)));
    if written.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    written
}

/// How a run that ran ended, for its record (D6/D7): the exit code and, for
/// anything but an answer, the reason that closes its `stderr`.
///
/// A timeout says what an opencode run's `AgentTimeout` says, but stays a
/// result rather than an `Err`: the tokens spent before the deadline must
/// reach the record, or the daily cap never sees what a run that hit its
/// limit cost.
fn ending(reason: FinishReason, turns: u32, timeout: Duration) -> (i32, Option<String>) {
    match reason {
        FinishReason::Answered => (0, None),
        FinishReason::MaxTurns => (
            1,
            Some(format!("the run ended at its turn limit ({turns} turns)")),
        ),
        FinishReason::Cancelled => (1, Some("the run was cancelled".to_owned())),
        FinishReason::Failed => (1, Some("the model API failed".to_owned())),
        FinishReason::Timeout => (
            1,
            Some(
                AxiomataError::AgentTimeout {
                    backend: BACKEND_AXH,
                    timeout,
                }
                .to_string(),
            ),
        ),
    }
}

/// The model endpoint of an `axh` run (B1): the base URL and key of `role`'s
/// provider (the skill provider for a run, the chat provider for a chat turn),
/// and the run's model — bare, as the API names it (D2).
///
/// # Errors
///
/// [`AxiomataError::InvalidAgentModel`] when the skill provider is missing
/// from the config, has no base URL (Anthropic: no OpenAI-compatible API yet,
/// H4), has no key where one is needed (OpenRouter), or when the run carries
/// no model — a config gap must fail loudly, never silently bill another
/// endpoint.
fn endpoint(
    config: &Config,
    role: ProviderRole,
    model: Option<String>,
) -> Result<ModelEndpoint, AxiomataError> {
    let id = config.agents.provider_for(role);
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
                 the {} provider {} has none — set it in Settings → Providers",
                match role {
                    ProviderRole::Skill => "skill",
                    ProviderRole::Chat => "chat",
                },
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
            gap(match role {
                ProviderRole::Skill => format!(
                    "no model for an axh run: set `model:` in the skill's frontmatter \
                     or the skill model of {}",
                    id.as_str()
                ),
                ProviderRole::Chat => {
                    format!(
                        "no model for an axh chat: set the chat model of {}",
                        id.as_str()
                    )
                }
            })
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
///
/// The settings' `deny` and `ask` rules (the owner's and a project's) beat a
/// declaration as they do everywhere else — a skill cannot declare its way
/// past `deny: Bash(rm:*)`. Their `allow` rules are left out: they would
/// widen a skill beyond what it declares.
fn gate_for(rights: &Rights<'_>, settings: &RuleSet) -> Gate {
    let mut gate = Gate::new(
        RuleSet {
            allow: rights.declared.clone(),
            ask: settings.ask.clone(),
            deny: settings.deny.iter().chain(&rights.deny).cloned().collect(),
        },
        rights.mode,
        Arc::new(Nobody),
    );
    // An empty declaration narrows nothing: the mode speaks for the rest (a skill: reads only; a pre-approved chat
    // turn: what no rule denies).
    gate.narrow(
        match rights.name {
            Some(name) => format!("the skill {name}"),
            None => "this run's declaration".to_owned(),
        },
        rights.declared.clone(),
    );
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
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// A skill's gate as `run` builds it: its declaration, the mode's own word for the rest.
    fn skill_gate(name: Option<&str>, declared: &[Rule], settings: &RuleSet) -> Gate {
        gate_for(
            &Rights {
                name,
                declared: declared.to_vec(),
                mode: Mode::Default,
                deny: Vec::new(),
            },
            settings,
        )
    }

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
        let err = endpoint(
            &Config::default(),
            ProviderRole::Skill,
            Some("claude-haiku-5-5".into()),
        )
        .unwrap_err();
        let message = err.to_string();
        assert!(message.contains("base URL"), "{message}");
        assert!(message.contains("anthropic"), "{message}");

        let openrouter = config_with_provider(ProviderId::OpenRouter, openrouter_settings());
        let resolved = endpoint(
            &openrouter,
            ProviderRole::Skill,
            Some("deepseek/deepseek-chat".into()),
        )
        .unwrap();
        assert_eq!(resolved.base_url, "https://openrouter.ai/api");
        assert_eq!(resolved.api_key.as_deref(), Some("sk-or"));
        assert_eq!(resolved.model, "deepseek/deepseek-chat");

        // A keyless OpenRouter is refused, not tried — as the CLI's `harness run` refuses it.
        let mut keyless = openrouter_settings();
        keyless.api_key = None;
        let keyless = config_with_provider(ProviderId::OpenRouter, keyless);
        assert!(endpoint(&keyless, ProviderRole::Skill, Some("m".into())).is_err());

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
            endpoint(&ollama, ProviderRole::Skill, Some("qwen3:8b".into()))
                .unwrap()
                .model,
            "qwen3:8b"
        );

        // No model on the request (nothing resolved) is an error, not a guess.
        assert!(endpoint(&ollama, ProviderRole::Skill, None).is_err());
        assert!(endpoint(&ollama, ProviderRole::Skill, Some("  ".into())).is_err());
    }

    /// `access: read` on Glut (glut-round CP1): reads, the shared read commands and the declared MCP tool run; edits,
    /// other commands, a declared shell rule, an output redirect and git's `--output`/`--no-index` are refused.
    #[tokio::test]
    async fn a_read_skill_on_glut_only_reads() {
        let declared = declared_rules(
            Some("look"),
            Some("mcp__apple-mail__search_emails Bash Edit"),
        )
        .unwrap();
        let rights = read_rights(Some("look"), &declared).unwrap();
        assert_eq!(rights.mode, Mode::Plan);
        let gate = gate_for(&rights, &RuleSet::default());
        let read = Access::Read {
            path: "notes.md".into(),
        };
        assert!(gate.judge("read_file", &read).await.is_ok());
        for allowed in ["git log --oneline", "git status", "ls", "pwd"] {
            assert!(
                gate.judge("bash", &bash(allowed)).await.is_ok(),
                "{allowed}"
            );
        }
        let mail = Access::Mcp {
            server: "apple-mail".into(),
            tool: "search_emails".into(),
        };
        assert!(
            gate.judge("mcp__apple-mail__search_emails", &mail)
                .await
                .is_ok()
        );
        // The web only when declared: this skill names none, so a page is refused.
        let page = Access::Network {
            host: "docs.rs".into(),
        };
        assert!(gate.judge("web_fetch", &page).await.is_err());
        let declared_web = declared_rules(Some("look"), Some("WebFetch(domain:docs.rs)")).unwrap();
        let gate = gate_for(
            &read_rights(Some("look"), &declared_web).unwrap(),
            &RuleSet::default(),
        );
        assert!(gate.judge("web_fetch", &page).await.is_ok());
        let edit = Access::Edit {
            path: "notes.md".into(),
        };
        assert!(gate.judge("write_file", &edit).await.is_err());
        for refused in [
            "touch x",
            "echo x > f",
            "git log > f",
            "git log --output=f",
            "git diff --no-index a b",
            "rm notes.md",
        ] {
            assert!(
                gate.judge("bash", &bash(refused)).await.is_err(),
                "{refused}"
            );
        }
    }

    /// The bundled skill's declared rights, read from its frontmatter as shipped.
    fn bundled_rights(name: &str, skill_md: &str) -> Gate {
        let declared = skill_md
            .lines()
            .find_map(|line| line.strip_prefix("allowed_tools: "))
            .map(|value| value.trim_matches('"'));
        skill_gate(
            Some(name),
            &declared_rules(Some(name), declared).unwrap(),
            &RuleSet::default(),
        )
    }

    /// W8 (glut-round): what the bundled skills do on Glut is exactly what they declare — the news skills read the
    /// web and write their `News/` file, inbox-sort moves without overwriting, todo-to-kanban lists and adds cards.
    #[tokio::test]
    async fn the_bundled_skills_declare_what_they_do_and_no_more() {
        let page = Access::Network {
            host: "huggingface.co".into(),
        };
        for (name, skill_md, own_file) in [
            (
                "news-rss",
                include_str!("../../resources/news-rss/SKILL.md"),
                "News/KI-News RSS.md",
            ),
            (
                "news-web",
                include_str!("../../resources/news-web/SKILL.md"),
                "News/KI-News Web.md",
            ),
            (
                "model-news",
                include_str!("../../resources/model-news/SKILL.md"),
                "News/KI-Modelle.md",
            ),
        ] {
            let news = bundled_rights(name, skill_md);
            for allowed in ["date '+%Y-%m-%d %H:%M'", "mkdir -p News"] {
                assert!(
                    news.judge("bash", &bash(allowed)).await.is_ok(),
                    "{name}: {allowed}"
                );
            }
            assert!(news.judge("web_fetch", &page).await.is_ok(), "{name}");
            // `News/` read and searched, its own file written — nothing else of the vault or beyond it: untrusted
            // pages plus the web would otherwise be a way out for what the skill can read, or in for what others read.
            assert!(
                news.judge(
                    "read_file",
                    &Access::Read {
                        path: own_file.into()
                    }
                )
                .await
                .is_ok()
            );
            assert!(
                news.judge(
                    "grep",
                    &Access::Read {
                        path: "News/".into()
                    }
                )
                .await
                .is_ok()
            );
            assert!(
                news.judge(
                    "edit_file",
                    &Access::Edit {
                        path: own_file.into()
                    }
                )
                .await
                .is_ok()
            );
            let other = Access::Edit {
                path: "News/AGENTS.md".into(),
            };
            assert!(news.judge("write_file", &other).await.is_err(), "{name}");
            for elsewhere in ["AGENTS.md", "Inbox/note.md", "."] {
                let read = Access::Read {
                    path: elsewhere.into(),
                };
                assert!(
                    news.judge("read_file", &read).await.is_err(),
                    "{name}: {elsewhere}"
                );
                let edit = Access::Edit {
                    path: elsewhere.into(),
                };
                assert!(
                    news.judge("write_file", &edit).await.is_err(),
                    "{name}: {elsewhere}"
                );
            }
            for refused in [
                "grep -rl -F \"https://example.org/a\" News/",
                "rm News/KI-News RSS.md",
                "curl https://example.org",
                "mkdir -p Other",
            ] {
                assert!(
                    news.judge("bash", &bash(refused)).await.is_err(),
                    "{name}: {refused}"
                );
            }
        }

        let inbox = bundled_rights(
            "inbox-sort",
            include_str!("../../resources/inbox-sort/SKILL.md"),
        );
        assert!(
            inbox
                .judge(
                    "bash",
                    &bash("axiomata-cli inbox move \"a.md\" \"Arbeit/Rust\"")
                )
                .await
                .is_ok()
        );
        for refused in [
            "mv -n -f \"Inbox/a.md\" AGENTS.md",
            "mv \"Inbox/a.md\" x",
            "rm Inbox/a.md",
        ] {
            assert!(
                inbox.judge("bash", &bash(refused)).await.is_err(),
                "{refused}"
            );
        }
        assert!(inbox.judge("web_fetch", &page).await.is_err());

        let board = bundled_rights(
            "todo-to-kanban",
            include_str!("../../resources/todo-to-kanban/SKILL.md"),
        );
        for allowed in [
            "axiomata-cli board list",
            "axiomata-cli board list --board 1",
            "axiomata-cli board add --column 3 \"Steuer machen\" --label steuer",
        ] {
            assert!(
                board.judge("bash", &bash(allowed)).await.is_ok(),
                "{allowed}"
            );
        }
        for refused in ["axiomata-cli board delete 1", "$AXIOMATA_CLI board list"] {
            assert!(
                board.judge("bash", &bash(refused)).await.is_err(),
                "{refused}"
            );
        }
    }

    /// A read skill that declares nothing still reads and runs the read commands — and nothing else, compound
    /// commands included; a declared `Write` stays refused under Plan.
    #[tokio::test]
    async fn a_read_skill_without_a_declaration_reads_and_never_chains_a_write() {
        let declared = declared_rules(Some("look"), Some("Write")).unwrap();
        for rights in [
            read_rights(Some("look"), &[]).unwrap(),
            read_rights(Some("look"), &declared).unwrap(),
        ] {
            let gate = gate_for(&rights, &RuleSet::default());
            assert!(gate.judge("bash", &bash("git status")).await.is_ok());
            for refused in ["git log; touch x", "git log && rm f", "ls | tee f"] {
                assert!(
                    gate.judge("bash", &bash(refused)).await.is_err(),
                    "{refused}"
                );
            }
            let write = Access::Edit {
                path: "x.md".into(),
            };
            assert!(gate.judge("write_file", &write).await.is_err());
        }
    }

    /// A skill that does not only look keeps the gate it always had: mode default, no extra denies.
    #[test]
    fn a_skill_that_writes_keeps_its_gate() {
        let gate = skill_gate(Some("w"), &[], &RuleSet::default());
        assert_eq!(gate.mode_name(), "default");
        assert!(gate.rules().deny.is_empty());
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
        let gate = skill_gate(Some("mail-digest"), &declared, &RuleSet::default());
        assert_eq!(gate.narrowed_by(), ["the skill mail-digest"]);
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
        let git_only = skill_gate(
            Some("release-notes"),
            &declared_rules(Some("release-notes"), Some("Bash(git log:*)")).unwrap(),
            &RuleSet::default(),
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
        let gate = skill_gate(Some("summarize"), &[], &RuleSet::default());
        assert!(gate.narrowed_by().is_empty());
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

    #[tokio::test]
    async fn settings_deny_and_ask_beat_a_declaration_and_settings_allow_does_not_count() {
        let declared = declared_rules(Some("cleanup"), Some("Bash(rm:*) Bash(git:*)")).unwrap();
        let settings = RuleSet {
            allow: vec![Rule::parse("Bash(curl:*)").unwrap()],
            ask: vec![Rule::parse("Bash(git push:*)").unwrap()],
            deny: vec![Rule::parse("Bash(rm:*)").unwrap()],
        };
        let gate = skill_gate(Some("cleanup"), &declared, &settings);
        // Declared, but the owner denies it.
        assert!(gate.judge("bash", &bash("rm -rf notes")).await.is_err());
        // Declared, but the owner wants to be asked — and nobody answers.
        assert!(gate.judge("bash", &bash("git push")).await.is_err());
        // Declared and untouched by the settings.
        assert!(gate.judge("bash", &bash("git status")).await.is_ok());
        // The settings allow it, the skill does not declare it: still refused.
        let refused = gate.judge("bash", &bash("curl x")).await.unwrap_err();
        assert!(refused.contains("allows only"), "{refused}");
    }

    #[test]
    fn a_run_that_hit_its_time_limit_is_a_failed_result_not_an_error() {
        // D6: a result, so the tokens it spent reach the record and the
        // daily cap; its reason reads as an opencode timeout's does.
        let (code, why) = ending(FinishReason::Timeout, 3, Duration::from_secs(90));
        assert_eq!(code, 1);
        let expected = AxiomataError::AgentTimeout {
            backend: BACKEND_AXH,
            timeout: Duration::from_secs(90),
        }
        .to_string();
        assert_eq!(why.as_deref(), Some(expected.as_str()));
        assert_eq!(ending(FinishReason::Answered, 1, Duration::ZERO), (0, None));
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

    /// CP4 of harness-choice end to end: two chat turns on Glut against a fake endpoint — the second one sends
    /// the first conversation along, under the same id, and the file holding it is the owner's alone. An id that
    /// names no Glut conversation starts a new one.
    #[tokio::test]
    async fn a_glut_chat_goes_on_with_its_conversation() {
        let mut fx = Fixture::new("chat");
        std::fs::write(
            fx.config.workspace_root.join("AGENTS.md"),
            "The vault has the area Garten.",
        )
        .unwrap();
        let api = FakeApi::spawn_each(vec![
            json!({"choices": [{"message": {"role": "assistant", "content": "Noted: GLUTHEX."}}],
                   "usage": {"prompt_tokens": 10, "completion_tokens": 3}}),
            json!({"choices": [{"message": {"role": "assistant", "content": "GLUTHEX"}}],
                   "usage": {"prompt_tokens": 20, "completion_tokens": 1}}),
            json!({"choices": [{"message": {"role": "assistant", "content": "None."}}]}),
        ])
        .await;
        fx.config.agents.chat_provider = ProviderId::OpenRouter;
        fx.config.agents.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some(api.base_url.clone()),
                api_key: Some("sk-test".into()),
                chat_model: "chat-model".into(),
                skill_model: "s".into(),
            },
        );
        let request = |message: &str, session_id: Option<String>| ChatRequest {
            message: message.to_owned(),
            session_id,
            mode: crate::agents::ChatMode::Chat,
            cwd: fx.config.workspace_root.clone(),
            timeout: Duration::from_secs(30),
            system_prompt_file: None,
            model: Some("chat-model".into()),
            auto_approve_tools: true,
            allowed_tools: None,
        };

        let first = chat(request("Remember GLUTHEX.", None), &fx.config)
            .await
            .unwrap();
        assert!(
            first.session_id.starts_with(SESSION_PREFIX),
            "{}",
            first.session_id
        );
        assert_eq!(first.reply_markdown, "Noted: GLUTHEX.");
        assert_eq!(first.input_tokens, Some(10));
        let file = sessions_dir().join(format!("{}.jsonl", first.session_id));
        assert_eq!(
            std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let second = chat(
            request("Which word?", Some(first.session_id.clone())),
            &fx.config,
        )
        .await
        .unwrap();
        assert_eq!(second.session_id, first.session_id);
        assert_eq!(second.reply_markdown, "GLUTHEX");

        // An Opencode id after a harness switch: a new conversation, not an error.
        let third = chat(request("Which word?", Some("ses_abc".into())), &fx.config)
            .await
            .unwrap();
        assert_ne!(third.session_id, first.session_id);

        let seen = api.seen.lock().unwrap();
        assert_eq!(seen[0].1["model"], "chat-model");
        let opening = seen[0].1["messages"].to_string();
        assert!(
            opening.contains("The vault has the area Garten."),
            "{opening}"
        );
        let messages = seen[1].1["messages"].as_array().unwrap().clone();
        let texts: Vec<String> = messages.iter().map(|m| m["content"].to_string()).collect();
        assert!(
            texts.iter().any(|t| t.contains("Remember GLUTHEX.")),
            "{texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("Noted: GLUTHEX.")),
            "{texts:?}"
        );
        assert!(texts.last().unwrap().contains("Which word?"), "{texts:?}");
        let fresh = seen[2].1["messages"].to_string();
        assert!(!fresh.contains("GLUTHEX"), "{fresh}");
    }

    /// A turn whose model call fails is an error and leaves no conversation behind.
    #[tokio::test]
    async fn a_failed_glut_chat_turn_writes_no_conversation() {
        let mut fx = Fixture::new("chat-failed");
        let api = FakeApi::spawn(json!({"error": "nope"})).await;
        fx.config.agents.chat_provider = ProviderId::OpenRouter;
        fx.config.agents.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some(api.base_url.clone()),
                api_key: Some("sk-test".into()),
                chat_model: "chat-model".into(),
                skill_model: "s".into(),
            },
        );
        let request = ChatRequest {
            message: "hi".to_owned(),
            session_id: None,
            mode: crate::agents::ChatMode::Chat,
            cwd: fx.config.workspace_root.clone(),
            timeout: Duration::from_secs(30),
            system_prompt_file: None,
            model: Some("chat-model".into()),
            auto_approve_tools: true,
            allowed_tools: None,
        };
        let err = chat(request, &fx.config).await.unwrap_err();
        assert!(matches!(err, AxiomataError::AgentApi { .. }), "{err:?}");
        let kept = std::fs::read_dir(sessions_dir()).map_or(0, |entries| entries.count());
        assert_eq!(kept, 0);
    }

    /// A pre-approved chat turn runs in the owner's auto mode — but the owner's `~/.axh` deny rules still refuse,
    /// and an ask is refused since nobody answers.
    #[tokio::test]
    async fn a_chat_turn_in_auto_mode_still_obeys_the_owners_deny_and_ask_rules() {
        let settings = RuleSet {
            allow: Vec::new(),
            ask: vec![Rule::parse("Bash(git push:*)").unwrap()],
            deny: vec![Rule::parse("Bash(rm:*)").unwrap()],
        };
        let rights = Rights {
            name: None,
            declared: Vec::new(),
            mode: chat_mode(&[], true),
            deny: Vec::new(),
        };
        let gate = gate_for(&rights, &settings);
        assert!(gate.narrowed_by().is_empty());
        assert!(gate.judge("bash", &bash("ls -la")).await.is_ok());
        assert!(gate.judge("bash", &bash("rm -rf x")).await.is_err());
        assert!(
            gate.judge("bash", &bash("git push origin main"))
                .await
                .is_err()
        );
    }

    #[test]
    fn a_conversation_reads_back_and_a_broken_or_huge_file_is_told_apart() {
        let dir = unique_temp_dir("axiomata-test-axh-sessions");
        assert!(matches!(
            load_session(&dir, "axh-1").unwrap(),
            Loaded::Missing
        ));
        let messages = vec![Message::system("s"), Message::user("hi")];
        save_session(&dir, "axh-1", &messages).unwrap();
        save_session(&dir, "axh-1", &messages).unwrap();
        assert!(
            matches!(load_session(&dir, "axh-1").unwrap(), Loaded::Conversation(read) if read == messages)
        );
        // No partial file is left behind.
        let names: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 1, "{names:?}");

        std::fs::write(dir.join("axh-2.jsonl"), "{not json\n").unwrap();
        assert!(matches!(
            load_session(&dir, "axh-2").unwrap(),
            Loaded::Unreadable(_)
        ));

        let huge = std::fs::File::create(dir.join("axh-3.jsonl")).unwrap();
        huge.set_len(MAX_SESSION_BYTES + 1).unwrap();
        assert!(matches!(
            load_session(&dir, "axh-3"),
            Err(AxiomataError::AgentApi { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_lock_per_conversation_and_none_kept_for_conversations_nobody_holds() {
        let first = session_lock("axh-lock-a");
        let again = session_lock("axh-lock-a");
        assert!(Arc::ptr_eq(&first, &again));
        assert!(!Arc::ptr_eq(&first, &session_lock("axh-lock-b")));
    }

    #[test]
    fn a_glut_session_id_is_never_a_path_and_a_chat_turn_is_auto_only_when_nothing_is_declared() {
        assert!(valid_session_id(&new_session_id()));
        assert_ne!(new_session_id(), new_session_id());
        for bad in [
            "ses_abc",
            "axh-../x",
            "axh-a/b",
            "4b74f87c-27b3-4a24-88b4-2bc337107fbd",
            "",
        ] {
            assert!(!valid_session_id(bad), "{bad}");
        }
        let declared = declared_rules(None, Some("mcp__apple-reminders__reminders_tasks")).unwrap();
        assert_eq!(chat_mode(&declared, true), Mode::Default);
        assert_eq!(chat_mode(&[], true), Mode::Auto);
        assert_eq!(chat_mode(&[], false), Mode::Default);
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
                cached_input_per_m: None,
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
        // the task — and exactly one call: the skill asks for no JSON, so no repair turn follows.
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

    /// `output: json` on Glut (harness-choice CP5): a prose answer is asked once more in the same conversation, and
    /// the JSON object of the second answer is what the run records — with both calls' tokens.
    #[tokio::test]
    async fn an_axh_json_skill_gets_one_repair_turn() {
        let mut fx = Fixture::new("json-repair");
        fx.write_skill(
            "axh-json",
            "---\nname: axh-json\ndescription: d\nbackend: axh\nallowed_tools: Read\noutput: json\n---\n\
             Answer as JSON.\n",
        );
        let api = FakeApi::spawn_each(vec![
            json!({"choices": [{"message": {"role": "assistant", "content": "Here is what I found."}}],
                   "usage": {"prompt_tokens": 10, "completion_tokens": 2}}),
            json!({"choices": [{"message": {"role": "assistant", "content": "{\"ok\": true}"}}],
                   "usage": {"prompt_tokens": 12, "completion_tokens": 3}}),
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
        let record = runner::execute_and_record_skill("axh-json", &fx.config, &fx.db)
            .await
            .unwrap();
        assert_eq!(record.status, RunStatus::Success, "err: {:?}", record.error);
        assert_eq!(record.stdout, "{\"ok\": true}");
        assert_eq!(record.input_tokens, Some(22));
        assert_eq!(record.num_turns, Some(2));
        let seen = api.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        let repair = seen[1].1["messages"].to_string();
        assert!(repair.contains("Here is what I found."), "{repair}");
        assert!(repair.contains("no JSON object"), "{repair}");
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
