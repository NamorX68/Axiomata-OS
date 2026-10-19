//! The `claude_code` backend: a skill or prompt run as `claude -p` (`docs/plans/harness-choice.md`, CP2).
//!
//! The app's Claude Code is apart from the owner's (D5): every run sets `CLAUDE_CONFIG_DIR` to the app's own login
//! folder ([`paths::app_claude_dir`]), so none of the owner's `~/.claude` permissions, hooks, plugins or global
//! `CLAUDE.md` apply and no transcript lands there. It reads **no settings file at all** (`--setting-sources ""`):
//! measured (CP2), a folder's `.claude/settings.json` runs its hooks and sets its `env` even in a folder never
//! trusted — a run that may write the vault could otherwise start a process in the next one. The working folder's
//! `AGENTS.md` (the vault's router) is handed over by the app instead, with the run's own context, as one appended
//! system prompt (Claude Code keeps only the last `--append-system-prompt-file`, measured). MCP servers are the
//! owner's `~/.axh/mcp.json` with `--strict-mcp-config` (agent isolation D7), passed as a file only the owner can
//! read, never on the command line where `ps` shows it.
//!
//! Permissions, nobody answers (`-p` refuses what is not allowed, it never waits):
//! - a skill that declares `allowed_tools` gets exactly those (they are Claude's spelling already);
//! - `access: read` keeps the declared tools but no declared shell rule, adds the shared read commands and denies
//!   every edit tool and the git commands that reach beyond the folder — measured, Claude Code refuses an output
//!   redirect (`git log > f`) even under `Bash(git log:*)`;
//! - a skill that declares nothing runs like Opencode's unattended `*` allow when `auto_approve_tools` is on: edits
//!   in its folder (`acceptEdits`), the shell, the web and every MCP server — but no edit outside the folder.
//!
//! Anthropic is billed through the owner's subscription (D2); the result's `total_cost_usd` is recorded as reported
//! and the daily cap does not meter it, as on Opencode.

use std::fs::{DirBuilder, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::process::{Pid, Signal, kill_process_group};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use crate::agents::opencode::{JSON_REPAIR_PROMPT, JSON_REPAIR_TIMEOUT, find_json_object};
use crate::agents::opencode_instance::{READ_COMMANDS, READ_DENIED_COMMANDS};
use crate::agents::{
    AgentRequest, AgentRunResult, BACKEND_CLAUDE_CODE, ChatReply, ChatRequest, MAX_RESPONSE_BYTES,
    agent_child_env, agent_slots, truncate_utf8, valid_model_name,
};
use crate::error::AxiomataError;
use crate::paths;

/// Name of the Claude Code binary; expected on `PATH` (the app adopts the login shell's, `login_path`).
pub(crate) const CLAUDE_BIN: &str = "claude";

/// What Claude Code answers in `result` when its config folder has no login (CP0).
const NOT_SIGNED_IN: &str = "Not logged in";

/// What Claude Code prints, instead of a result, for a `--resume` of a conversation it does not have (CP3).
const NO_CONVERSATION: &str = "No conversation found";

/// How much of a call's stdout a failure without a result keeps: enough for Claude Code's one-line reasons.
const MAX_FAILURE_STDOUT: usize = 500;

/// What a run that met [`NOT_SIGNED_IN`] tells the owner to do.
const SIGN_IN_HINT: &str = "the app's Claude Code is not signed in: sign in once with \
    `CLAUDE_CONFIG_DIR=~/.axiomata/claude claude auth login --claudeai`";

/// The tools that change files; a read run denies them all.
const EDIT_TOOLS: [&str; 3] = ["Edit", "Write", "NotebookEdit"];

/// What a run without its own `allowed_tools` may use when tools are pre-approved, beside the edits `acceptEdits`
/// lets through in its folder and the MCP servers: Opencode's unattended `*` allow, minus edits outside the folder.
const UNDECLARED_TOOLS: [&str; 3] = ["Bash", "WebFetch", "WebSearch"];

/// The working folder's instruction file the app hands over (Opencode reads the same one).
const INSTRUCTIONS_FILE: &str = "AGENTS.md";

/// The largest instruction file handed over, as for a skill's `prepend_files`: a bigger one is left out with a warning.
const MAX_INSTRUCTIONS_BYTES: u64 = 256 * 1024;

/// Runs `request` as `claude -p` — see the module doc. A turn that ran but failed (not signed in, an API error) is an
/// `Ok` with `exit_code == 1` and the reason in `stderr`; an `Err` means it could not run (no model, a bad
/// `allowed_tools`, no binary, an unusable MCP file, the time limit).
pub async fn run(request: AgentRequest) -> Result<AgentRunResult, AxiomataError> {
    refuse_in_tests()?;
    let model = run_model(request.model.as_deref())?;
    let declared = declared_tools(request.name.as_deref(), request.allowed_tools.as_deref())?;
    let servers = owner_servers()?;
    let _permit = agent_slots()
        .acquire()
        .await
        .expect("agent_slots semaphore is never closed");
    // Held to the end of the run, the repair call included; dropping it removes the files.
    let setup = Setup::prepare(
        &servers,
        &request.cwd,
        request.system_prompt_file.as_deref(),
    )?;
    let invocation = Invocation {
        model: &model,
        mcp_config: setup.mcp_config.as_deref(),
        servers: &setup.server_names,
        declared: &declared,
        auto_approve: request.auto_approve_tools,
        read_only: request.read_only,
        context_file: setup.context_file.as_deref(),
        resume: None,
    };
    let program = Path::new(CLAUDE_BIN);
    let first = invoke(
        program,
        &arguments(&invocation),
        &request.prompt,
        &request.cwd,
        request.timeout,
    )
    .await?;
    let wants_repair =
        request.expects_json && first.succeeded() && find_json_object(&first.reply).is_none();
    let session = first.session_id.clone().filter(|id| valid_session_id(id));
    let outcome = match session {
        Some(session) if wants_repair => {
            tracing::warn!(%session, "the reply holds no JSON object; asking once more in the same session");
            let repair = arguments(&Invocation {
                resume: Some(&session),
                ..invocation
            });
            match invoke(
                program,
                &repair,
                JSON_REPAIR_PROMPT,
                &request.cwd,
                JSON_REPAIR_TIMEOUT,
            )
            .await
            {
                Ok(second) => first.merge_repair(second),
                Err(err) => {
                    tracing::warn!(%err, "the JSON repair turn failed; keeping the first reply");
                    first
                }
            }
        }
        _ => first,
    };
    Ok(outcome.into_result())
}

/// One assistant-bar turn on Claude Code (harness-choice CP3): the conversation `session_id` names goes on with
/// `--resume`, a new one starts otherwise. An id Claude Code does not know — an Opencode session's (`ses_…`) after a
/// switch of the chat harness, or one whose transcript is gone — starts a new conversation rather than failing, as
/// on Opencode. The module manifest rides in the appended system prompt beside the folder's `AGENTS.md`. Chat and
/// instruct turns run alike, unattended ([`crate::agents::ChatMode`]). A turn that failed or answered nothing is an
/// [`AxiomataError::AgentApi`].
pub(crate) async fn chat(request: ChatRequest) -> Result<ChatReply, AxiomataError> {
    refuse_in_tests()?;
    if request
        .model
        .as_deref()
        .is_none_or(|model| model.trim().is_empty())
    {
        return Err(AxiomataError::InvalidAgentModel {
            reason:
                "chat on claude_code has no model: set the chat model of anthropic in config.toml"
                    .to_owned(),
        });
    }
    let model = run_model(request.model.as_deref())?;
    let declared = declared_tools(None, request.allowed_tools.as_deref())?;
    let servers = owner_servers()?;
    let _permit = agent_slots()
        .acquire()
        .await
        .expect("agent_slots semaphore is never closed");
    let setup = Setup::prepare(
        &servers,
        &request.cwd,
        request.system_prompt_file.as_deref(),
    )?;
    let resume = request
        .session_id
        .as_deref()
        .filter(|id| valid_session_id(id));
    let invocation = Invocation {
        model: &model,
        mcp_config: setup.mcp_config.as_deref(),
        servers: &setup.server_names,
        declared: &declared,
        auto_approve: request.auto_approve_tools,
        read_only: false,
        context_file: setup.context_file.as_deref(),
        resume,
    };
    let outcome = turn_or_start_anew(
        Path::new(CLAUDE_BIN),
        invocation,
        &request.message,
        &request.cwd,
        request.timeout,
    )
    .await?;
    chat_reply(outcome)
}

/// One chat turn: the call as `invocation` says, and — only when it resumed a conversation Claude Code does not have —
/// once more as a new conversation. Never a third call.
async fn turn_or_start_anew(
    program: &Path,
    invocation: Invocation<'_>,
    message: &str,
    cwd: &Path,
    timeout: Duration,
) -> Result<Outcome, AxiomataError> {
    let first = invoke(program, &arguments(&invocation), message, cwd, timeout).await?;
    if invocation.resume.is_none() || !first.unknown_conversation {
        return Ok(first);
    }
    tracing::info!("Claude Code does not know the chat's conversation; starting a new one");
    let fresh = Invocation {
        resume: None,
        ..invocation
    };
    invoke(program, &arguments(&fresh), message, cwd, timeout).await
}

/// A chat turn's [`Outcome`] as the dashboard takes it: a failure or an empty answer is an error the owner sees.
fn chat_reply(outcome: Outcome) -> Result<ChatReply, AxiomataError> {
    let api_err = |message: String| AxiomataError::AgentApi {
        backend: BACKEND_CLAUDE_CODE,
        message,
    };
    if let Some(failure) = outcome.failure {
        return Err(api_err(failure));
    }
    let reply_markdown = outcome.reply.trim().to_owned();
    if reply_markdown.is_empty() {
        return Err(api_err("Claude Code returned an empty reply".to_owned()));
    }
    let session_id = outcome
        .session_id
        .ok_or_else(|| api_err("Claude Code returned no session id".to_owned()))?;
    let input_tokens = (outcome.input_tokens > 0).then_some(outcome.input_tokens);
    let output_tokens = (outcome.output_tokens > 0).then_some(outcome.output_tokens);
    Ok(ChatReply {
        session_id,
        reply_markdown,
        is_error: false,
        cost_usd: outcome.cost,
        usage: (input_tokens.is_some() || output_tokens.is_some()).then(|| {
            serde_json::json!({ "input_tokens": outcome.input_tokens, "output_tokens": outcome.output_tokens })
        }),
        input_tokens,
        output_tokens,
        num_turns: (outcome.turns > 0).then_some(outcome.turns),
        duration_ms: outcome.duration.as_millis() as u64,
    })
}

/// What a call hands Claude Code by path, prepared once per run or chat turn: the MCP file and the appended system
/// prompt, in a [`RunFiles`] folder removed when this is dropped.
struct Setup {
    _files: RunFiles,
    mcp_config: Option<PathBuf>,
    context_file: Option<PathBuf>,
    server_names: Vec<String>,
}

impl Setup {
    fn prepare(
        servers: &[axiomata_harness::mcp::config::ServerEntry],
        cwd: &Path,
        system_prompt_file: Option<&Path>,
    ) -> Result<Self, AxiomataError> {
        let context = run_context(cwd, system_prompt_file)?;
        let files = RunFiles::create(&paths::app_claude_dir().join("runs"))?;
        let mcp_config = match mcp_config_json(servers)? {
            Some(json) => Some(files.write("mcp.json", &json)?),
            None => None,
        };
        let context_file = match context {
            Some(text) => Some(files.write("context.md", &text)?),
            None => None,
        };
        Ok(Self {
            _files: files,
            mcp_config,
            context_file,
            server_names: servers.iter().map(|entry| entry.name.clone()).collect(),
        })
    }
}

/// A real Claude Code is never reached from a unit test.
fn refuse_in_tests() -> Result<(), AxiomataError> {
    if cfg!(test) {
        return Err(AxiomataError::AgentApi {
            backend: BACKEND_CLAUDE_CODE,
            message: "unit tests never reach the real Claude Code".to_owned(),
        });
    }
    Ok(())
}

/// The model for `--model`: the bare Anthropic id (`skills::runner::resolve_run_model`), an `anthropic/` prefix of
/// Opencode's spelling dropped, checked against the id alphabet so it can never read as a flag.
fn run_model(model: Option<&str>) -> Result<String, AxiomataError> {
    let model = model.map(str::trim).filter(|model| !model.is_empty());
    let Some(model) = model else {
        return Err(AxiomataError::InvalidAgentModel {
            reason: "backend claude_code has no model: set `model:` in the skill's frontmatter or the skill \
                     model of anthropic in config.toml"
                .to_owned(),
        });
    };
    let bare = model.strip_prefix("anthropic/").unwrap_or(model);
    if !valid_model_name(bare) {
        return Err(AxiomataError::InvalidAgentModel {
            reason: format!("backend claude_code: {bare:?} is not a valid model id"),
        });
    }
    Ok(bare.to_owned())
}

/// The skill's `allowed_tools` as Claude rules, each checked by the harness's parser first — the one reading of the
/// frontmatter Glut uses too — and each starting with a letter: the list is a variadic flag's values, and a part like
/// `--permission-mode` would be read as a flag of its own.
fn declared_tools(
    name: Option<&str>,
    allowed_tools: Option<&str>,
) -> Result<Vec<String>, AxiomataError> {
    let Some(text) = allowed_tools.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(Vec::new());
    };
    let invalid = |reason: String| AxiomataError::InvalidSkill {
        path: name.unwrap_or("the run").into(),
        reason: format!("unparsable `allowed_tools`: {reason}"),
    };
    axiomata_harness::skills::rules_of(text).map_err(invalid)?;
    let parts = axiomata_harness::skills::split_rules(text);
    if let Some(part) = parts
        .iter()
        .find(|part| !part.starts_with(|c: char| c.is_ascii_alphabetic()))
    {
        return Err(invalid(format!("{part:?} does not name a tool")));
    }
    Ok(parts.into_iter().map(str::to_owned).collect())
}

/// Whether a session id from Claude Code's result can go back on the command line (`--resume`) as one value.
fn valid_session_id(id: &str) -> bool {
    id.len() <= 128
        && id.starts_with(|c: char| c.is_ascii_alphanumeric())
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The owner's MCP servers (`~/.axh/mcp.json`).
fn owner_servers() -> Result<Vec<axiomata_harness::mcp::config::ServerEntry>, AxiomataError> {
    let home = home::home_dir().ok_or_else(|| AxiomataError::AgentApi {
        backend: BACKEND_CLAUDE_CODE,
        message: "no home folder to read ~/.axh/mcp.json from".to_owned(),
    })?;
    axiomata_harness::mcp::config::owner(&home).map_err(|err| AxiomataError::AgentApi {
        backend: BACKEND_CLAUDE_CODE,
        message: format!("the MCP servers for the app's Claude Code: {err}"),
    })
}

/// `servers` as Claude Code's `--mcp-config` file (`{"mcpServers": …}`), or `None` without any.
fn mcp_config_json(
    servers: &[axiomata_harness::mcp::config::ServerEntry],
) -> Result<Option<String>, AxiomataError> {
    if servers.is_empty() {
        return Ok(None);
    }
    let map: serde_json::Map<String, serde_json::Value> = servers
        .iter()
        .map(|entry| serde_json::to_value(&entry.config).map(|config| (entry.name.clone(), config)))
        .collect::<Result<_, _>>()
        .map_err(|err| AxiomataError::AgentApi {
            backend: BACKEND_CLAUDE_CODE,
            message: format!("the MCP servers for the app's Claude Code: {err}"),
        })?;
    Ok(Some(serde_json::json!({ "mcpServers": map }).to_string()))
}

/// The run's appended system prompt: the working folder's [`INSTRUCTIONS_FILE`], then the caller's context file (the
/// chat's module manifest). A missing one is skipped; a link, a special file or an oversized instruction file is left
/// out with a warning, as Opencode would not follow it either.
pub(crate) fn run_context(
    cwd: &Path,
    system_prompt_file: Option<&Path>,
) -> Result<Option<String>, AxiomataError> {
    let mut parts = Vec::new();
    let instructions = cwd.join(INSTRUCTIONS_FILE);
    match std::fs::symlink_metadata(&instructions) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_INSTRUCTIONS_BYTES => {
            parts.push(read_text(&instructions)?);
        }
        Ok(_) => {
            tracing::warn!(
                path = %instructions.display(),
                "instruction file left out: a link, a special file or too big"
            )
        }
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(source) => {
            return Err(AxiomataError::Io {
                path: instructions,
                source,
            });
        }
    }
    if let Some(file) = system_prompt_file {
        parts.push(read_text(file)?);
    }
    Ok((!parts.is_empty()).then(|| parts.join("\n\n")))
}

fn read_text(path: &Path) -> Result<String, AxiomataError> {
    std::fs::read_to_string(path).map_err(|source| AxiomataError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// The files one run hands Claude Code by path, in a folder of their own that only the owner can read; dropping it
/// removes them.
struct RunFiles {
    dir: PathBuf,
}

impl RunFiles {
    fn create(base: &Path) -> Result<Self, AxiomataError> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| AxiomataError::Io { path, source }
        };
        std::fs::create_dir_all(base).map_err(io(base))?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let dir = base.join(format!(
            "{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        DirBuilder::new()
            .mode(0o700)
            .create(&dir)
            .map_err(io(&dir))?;
        Ok(Self { dir })
    }

    fn write(&self, name: &str, text: &str) -> Result<PathBuf, AxiomataError> {
        let path = self.dir.join(name);
        let result = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .and_then(|mut file| file.write_all(text.as_bytes()));
        result.map_err(|source| AxiomataError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }
}

impl Drop for RunFiles {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_dir_all(&self.dir) {
            tracing::warn!(%err, dir = %self.dir.display(), "could not remove a Claude Code run's files");
        }
    }
}

/// What one `claude -p` call is asked to do.
#[derive(Clone, Copy)]
struct Invocation<'a> {
    model: &'a str,
    mcp_config: Option<&'a Path>,
    servers: &'a [String],
    declared: &'a [String],
    auto_approve: bool,
    read_only: bool,
    context_file: Option<&'a Path>,
    resume: Option<&'a str>,
}

/// The permission mode and the allow and deny lists of a run — see the module doc.
#[derive(Debug, PartialEq)]
struct Permissions {
    mode: Option<&'static str>,
    allowed: Vec<String>,
    disallowed: Vec<String>,
}

fn permissions(invocation: &Invocation) -> Permissions {
    if invocation.read_only {
        // A declared shell rule (`Bash`, `Bash(*)`) would reach beyond the read commands — Opencode's read instance
        // refuses every other command whatever the skill says, and so does this.
        let allowed = invocation
            .declared
            .iter()
            .filter(|rule| !is_shell_rule(rule))
            .cloned()
            .chain(READ_COMMANDS.iter().map(|command| bash_rule(command)))
            .collect();
        let disallowed = EDIT_TOOLS
            .iter()
            .map(|tool| (*tool).to_owned())
            .chain(
                READ_DENIED_COMMANDS
                    .iter()
                    .map(|command| bash_rule(command)),
            )
            .collect();
        return Permissions {
            mode: None,
            allowed,
            disallowed,
        };
    }
    if invocation.declared.is_empty() && invocation.auto_approve {
        let allowed = UNDECLARED_TOOLS
            .iter()
            .map(|tool| (*tool).to_owned())
            .chain(
                invocation
                    .servers
                    .iter()
                    .map(|server| format!("mcp__{server}")),
            )
            .collect();
        return Permissions {
            mode: Some("acceptEdits"),
            allowed,
            disallowed: Vec::new(),
        };
    }
    Permissions {
        mode: None,
        allowed: invocation.declared.to_vec(),
        disallowed: Vec::new(),
    }
}

fn is_shell_rule(rule: &str) -> bool {
    rule == "Bash" || rule.starts_with("Bash(")
}

/// One shared shell pattern ([`READ_COMMANDS`]) in Claude's spelling: a trailing ` *` is its `:*` ("with any
/// arguments"); a `*` elsewhere stays a wildcard, which Claude Code matches inside a command too (measured, CP2).
fn bash_rule(command: &str) -> String {
    match command.strip_suffix(" *") {
        Some(prefix) if !prefix.contains('*') => format!("Bash({prefix}:*)"),
        _ => format!("Bash({command})"),
    }
}

/// The arguments of one `claude -p` call; the prompt goes to its stdin.
fn arguments(invocation: &Invocation) -> Vec<String> {
    let mut args: Vec<String> = [
        "-p",
        "--output-format",
        "json",
        "--model",
        invocation.model,
        "--setting-sources",
        "",
        "--strict-mcp-config",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if let Some(file) = invocation.mcp_config {
        args.extend(["--mcp-config".to_owned(), file.display().to_string()]);
    }
    if let Some(file) = invocation.context_file {
        args.extend([
            "--append-system-prompt-file".to_owned(),
            file.display().to_string(),
        ]);
    }
    if let Some(session) = invocation.resume {
        args.extend(["--resume".to_owned(), session.to_owned()]);
    }
    let permissions = permissions(invocation);
    if let Some(mode) = permissions.mode {
        args.extend(["--permission-mode".to_owned(), mode.to_owned()]);
    }
    // Each list is one variadic flag; the next flag ends it, the prompt is on stdin, and every rule starts with a
    // letter ([`declared_tools`]), so nothing in a list is read as a flag.
    if !permissions.allowed.is_empty() {
        args.push("--allowedTools".to_owned());
        args.extend(permissions.allowed);
    }
    if !permissions.disallowed.is_empty() {
        args.push("--disallowedTools".to_owned());
        args.extend(permissions.disallowed);
    }
    args
}

/// `program` as every call of the app's Claude Code starts it: no ambient `ANTHROPIC_API_KEY` or `CLAUDE_CODE_*` may
/// switch the billing or the login (provider hardening), and its config folder is the app's own (D5).
pub(crate) fn app_command(program: &Path) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    command
        .env_clear()
        .envs(agent_child_env())
        .env("CLAUDE_CONFIG_DIR", paths::app_claude_dir());
    command
}

/// Starts `program` with `args` in `cwd` as the leader of its own process group, feeds it `prompt` and reads its
/// result — all within `timeout`. On the limit the whole group is killed, MCP servers and shell children included;
/// after a normal end what is left of the group is swept up too.
async fn invoke(
    program: &Path,
    args: &[String],
    prompt: &str,
    cwd: &Path,
    timeout: Duration,
) -> Result<Outcome, AxiomataError> {
    let spawn_error = |source| AxiomataError::AgentSpawn {
        backend: BACKEND_CLAUDE_CODE,
        program: CLAUDE_BIN,
        source,
    };
    let started = Instant::now();
    let mut child = app_command(program)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(spawn_error)?;
    // The child leads its own group, so its pid is the group's id.
    let group = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(Pid::from_raw);
    let stdin = child.stdin.take();
    let feed = async move {
        let Some(mut stdin) = stdin else {
            return Ok(());
        };
        // A child that ends before reading its prompt is no failure of ours: its own output says why.
        match stdin.write_all(prompt.as_bytes()).await {
            Err(err) if err.kind() == ErrorKind::BrokenPipe => Ok(()),
            other => other,
        }
        // Dropping `stdin` here closes it, which ends the prompt.
    };
    let finished = tokio::time::timeout(timeout, async {
        let (fed, output) = tokio::join!(feed, child.wait_with_output());
        fed.and(output)
    })
    .await;
    let kill_group = || {
        if let Some(group) = group {
            // Already gone is fine: the group ended by itself.
            let _ = kill_process_group(group, Signal::KILL);
        }
    };
    let output = match finished {
        Ok(output) => {
            kill_group();
            output.map_err(spawn_error)?
        }
        Err(_) => {
            kill_group();
            return Err(AxiomataError::AgentTimeout {
                backend: BACKEND_CLAUDE_CODE,
                timeout,
            });
        }
    };
    let duration = started.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(match parse_result(&stdout) {
        Some(event) => Outcome::from_event(event, duration),
        None => {
            // Claude Code says some reasons on stdout and others on stderr; both are kept, stdout cut short.
            let said = truncate_utf8(stdout.trim().to_owned(), MAX_FAILURE_STDOUT);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let reason = [said.as_str(), stderr.trim()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" — ");
            let mut outcome = Outcome::failed(
                format!("Claude Code gave no result ({}): {reason}", output.status),
                duration,
            );
            outcome.unknown_conversation = stdout.contains(NO_CONVERSATION);
            outcome
        }
    })
}

/// The `result` event of `--output-format json`: the last line that parses as an event of type `result`. Claude Code
/// may print a notice before it, so not the whole stdout.
fn parse_result(stdout: &str) -> Option<ResultEvent> {
    stdout
        .lines()
        .rev()
        .filter(|line| line.trim_start().starts_with('{'))
        .filter_map(|line| serde_json::from_str::<ResultEvent>(line).ok())
        .find(|event| event.kind == "result")
}

#[derive(Debug, Deserialize)]
struct ResultEvent {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    total_cost_usd: Option<f64>,
    #[serde(default)]
    num_turns: Option<u32>,
    #[serde(default)]
    usage: Option<Usage>,
    #[serde(default)]
    permission_denials: Vec<Denial>,
}

#[derive(Debug, Default, Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct Denial {
    #[serde(default)]
    tool_name: String,
}

/// The `subtype` of a result whose run ended as it should; others (`error_max_turns`, `error_during_execution`) fail.
const SUCCESS_SUBTYPE: &str = "success";

/// One or two calls (a JSON repair) folded into what the run records.
#[derive(Debug)]
struct Outcome {
    reply: String,
    failure: Option<String>,
    session_id: Option<String>,
    cost: Option<f64>,
    input_tokens: u64,
    output_tokens: u64,
    turns: u32,
    denied: Vec<String>,
    duration: Duration,
    /// The call resumed a conversation Claude Code does not have: it printed [`NO_CONVERSATION`] instead of a
    /// result (measured, CP3). Read from the whole stdout, not the shortened failure text.
    unknown_conversation: bool,
}

impl Outcome {
    fn from_event(event: ResultEvent, duration: Duration) -> Self {
        let usage = event.usage.unwrap_or_default();
        let text = event.result.unwrap_or_default();
        let subtype = event.subtype.unwrap_or_else(|| SUCCESS_SUBTYPE.to_owned());
        let failure = if text.contains(NOT_SIGNED_IN) && event.is_error {
            Some(format!("{text} — {SIGN_IN_HINT}"))
        } else if event.is_error || subtype != SUCCESS_SUBTYPE {
            Some(if text.is_empty() {
                format!("Claude Code ended with {subtype} and no message")
            } else {
                text.clone()
            })
        } else {
            None
        };
        Self {
            reply: if failure.is_some() {
                String::new()
            } else {
                text
            },
            failure,
            session_id: event.session_id,
            cost: event.total_cost_usd,
            // Every input token the model read counts, the cached ones included: that is what the turn used.
            input_tokens: usage.input_tokens
                + usage.cache_creation_input_tokens
                + usage.cache_read_input_tokens,
            output_tokens: usage.output_tokens,
            turns: event.num_turns.unwrap_or(0),
            unknown_conversation: false,
            denied: event
                .permission_denials
                .into_iter()
                .map(|denial| denial.tool_name)
                .collect(),
            duration,
        }
    }

    fn failed(reason: String, duration: Duration) -> Self {
        Self {
            reply: String::new(),
            failure: Some(reason),
            session_id: None,
            cost: None,
            input_tokens: 0,
            output_tokens: 0,
            turns: 0,
            denied: Vec::new(),
            duration,
            unknown_conversation: false,
        }
    }

    fn succeeded(&self) -> bool {
        self.failure.is_none()
    }

    /// Folds a JSON repair call into the call it repairs: its usage is added (both were paid), and its reply replaces
    /// the first one only when it succeeded and holds a JSON object — as on Opencode (`opencode::merge_repair`).
    fn merge_repair(mut self, repair: Outcome) -> Outcome {
        self.cost = match (self.cost, repair.cost) {
            (None, None) => None,
            (first, second) => Some(first.unwrap_or(0.0) + second.unwrap_or(0.0)),
        };
        self.input_tokens += repair.input_tokens;
        self.output_tokens += repair.output_tokens;
        self.turns += repair.turns;
        self.duration += repair.duration;
        if repair.succeeded() && find_json_object(&repair.reply).is_some() {
            self.reply = repair.reply;
        }
        self.denied.extend(repair.denied);
        self
    }

    fn into_result(self) -> AgentRunResult {
        let mut notes = Vec::new();
        if let Some(failure) = &self.failure {
            notes.push(failure.clone());
        }
        if !self.denied.is_empty() {
            notes.push(format!(
                "Claude Code refused {} tool call(s) the run is not allowed: {}",
                self.denied.len(),
                self.denied.join(", ")
            ));
        }
        AgentRunResult {
            stdout: truncate_utf8(self.reply, MAX_RESPONSE_BYTES),
            stderr: notes.join("\n"),
            exit_code: if self.failure.is_some() { 1 } else { 0 },
            duration_ms: self.duration.as_millis() as u64,
            cost_usd: self.cost,
            input_tokens: (self.input_tokens > 0).then_some(self.input_tokens),
            output_tokens: (self.output_tokens > 0).then_some(self.output_tokens),
            num_turns: (self.turns > 0).then_some(self.turns),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn invocation<'a>(declared: &'a [String], servers: &'a [String]) -> Invocation<'a> {
        Invocation {
            model: "claude-haiku-5-5",
            mcp_config: None,
            servers,
            declared,
            auto_approve: true,
            read_only: false,
            context_file: None,
            resume: None,
        }
    }

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    /// A folder of its own under the system's temp folder, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("axiomata-cc-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// An executable shell script standing in for `claude`.
        fn script(&self, body: &str) -> PathBuf {
            let path = self.0.join("fake-claude");
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    const RESULT_LINE: &str = "{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"done\",\
                               \"session_id\":\"s1\"}";

    #[test]
    fn every_call_reads_no_settings_file_and_only_the_given_mcp_servers() {
        let args = arguments(&invocation(&[], &[]));
        assert_eq!(
            &args[..8],
            [
                "-p",
                "--output-format",
                "json",
                "--model",
                "claude-haiku-5-5",
                "--setting-sources",
                "",
                "--strict-mcp-config"
            ]
        );
    }

    #[test]
    fn a_declared_skill_gets_exactly_its_tools() {
        let declared = strings(&["Bash(git log:*)", "Edit"]);
        let servers = strings(&["apple-mail"]);
        let call = invocation(&declared, &servers);
        assert_eq!(
            permissions(&call),
            Permissions {
                mode: None,
                allowed: declared.clone(),
                disallowed: Vec::new()
            }
        );
    }

    #[test]
    fn an_undeclared_pre_approved_skill_runs_like_opencode_but_edits_only_its_folder() {
        let servers = strings(&["apple-mail", "apple-reminders"]);
        let call = invocation(&[], &servers);
        assert_eq!(
            permissions(&call),
            Permissions {
                mode: Some("acceptEdits"),
                allowed: strings(&[
                    "Bash",
                    "WebFetch",
                    "WebSearch",
                    "mcp__apple-mail",
                    "mcp__apple-reminders"
                ]),
                disallowed: Vec::new()
            }
        );
        let mut strict = invocation(&[], &servers);
        strict.auto_approve = false;
        assert_eq!(permissions(&strict).mode, None);
        assert!(permissions(&strict).allowed.is_empty());
    }

    #[test]
    fn a_read_run_adds_the_shared_read_commands_and_denies_every_edit() {
        let declared = strings(&["mcp__apple-mail__search_emails", "Edit", "Bash", "Bash(*)"]);
        let mut call = invocation(&declared, &[]);
        call.read_only = true;
        let rules = permissions(&call);
        assert_eq!(rules.mode, None);
        assert!(rules.allowed.contains(&"Bash(git log:*)".to_owned()));
        assert!(rules.allowed.contains(&"Bash(git log)".to_owned()));
        assert!(
            rules
                .allowed
                .contains(&"mcp__apple-mail__search_emails".to_owned())
        );
        // A declared shell rule would reach beyond the read commands: it is left out.
        assert!(!rules.allowed.contains(&"Bash".to_owned()));
        assert!(!rules.allowed.contains(&"Bash(*)".to_owned()));
        // A deny beats an allow in Claude Code, so a declared `Edit` stays refused.
        for denied in [
            "Edit",
            "Write",
            "NotebookEdit",
            "Bash(git * --no-i*)",
            "Bash(git * --ou*)",
        ] {
            assert!(rules.disallowed.contains(&denied.to_owned()), "{denied}");
        }
    }

    #[test]
    fn bash_rules_spell_a_trailing_wildcard_as_any_arguments() {
        assert_eq!(bash_rule("git status *"), "Bash(git status:*)");
        assert_eq!(bash_rule("ls -la"), "Bash(ls -la)");
        assert_eq!(bash_rule("git * --ou*"), "Bash(git * --ou*)");
    }

    #[test]
    fn the_lists_follow_their_flags_and_resume_mcp_and_context_ride_along() {
        let declared = strings(&["Read"]);
        let servers = strings(&["apple-mail"]);
        let mut call = invocation(&declared, &servers);
        call.mcp_config = Some(Path::new("/run/mcp.json"));
        call.resume = Some("abc");
        call.context_file = Some(Path::new("/run/context.md"));
        let args = arguments(&call);
        let after = |flag: &str| args[args.iter().position(|arg| arg == flag).unwrap() + 1].clone();
        assert_eq!(after("--mcp-config"), "/run/mcp.json");
        assert_eq!(after("--resume"), "abc");
        assert_eq!(after("--append-system-prompt-file"), "/run/context.md");
        assert_eq!(after("--allowedTools"), "Read");
        assert!(!args.contains(&"--disallowedTools".to_owned()));
    }

    #[test]
    fn the_model_is_the_bare_anthropic_id() {
        assert_eq!(
            run_model(Some("anthropic/claude-haiku-5-5")).unwrap(),
            "claude-haiku-5-5"
        );
        assert_eq!(
            run_model(Some(" claude-sonnet-5 ")).unwrap(),
            "claude-sonnet-5"
        );
        assert!(matches!(
            run_model(None),
            Err(AxiomataError::InvalidAgentModel { .. })
        ));
        assert!(matches!(
            run_model(Some("--evil")),
            Err(AxiomataError::InvalidAgentModel { .. })
        ));
    }

    #[test]
    fn declared_tools_are_checked_and_split_like_glut_reads_them() {
        assert_eq!(
            declared_tools(Some("s"), Some("Bash(git log:*) Edit,Write")).unwrap(),
            strings(&["Bash(git log:*)", "Edit", "Write"])
        );
        assert!(declared_tools(Some("s"), None).unwrap().is_empty());
        assert!(matches!(
            declared_tools(Some("s"), Some("Bash(")),
            Err(AxiomataError::InvalidSkill { .. })
        ));
    }

    /// A part that would read as a flag of its own on the command line is refused, not passed on.
    #[test]
    fn a_declared_tool_that_reads_as_a_flag_is_refused() {
        for evil in [
            "Read --dangerously-skip-permissions",
            "--settings",
            "Edit -p",
        ] {
            assert!(
                matches!(
                    declared_tools(Some("s"), Some(evil)),
                    Err(AxiomataError::InvalidSkill { .. })
                ),
                "{evil}"
            );
        }
    }

    #[test]
    fn only_a_plain_session_id_goes_back_on_the_command_line() {
        assert!(valid_session_id("490bf6c1-6f36-4c40-92dd-5dd7f101c709"));
        assert!(!valid_session_id("--settings"));
        assert!(!valid_session_id("a b"));
        assert!(!valid_session_id(""));
    }

    #[test]
    fn the_mcp_file_has_claudes_shape_and_no_servers_means_no_file() {
        use axiomata_harness::mcp::config::{Origin, ServerConfig, ServerEntry};
        assert_eq!(mcp_config_json(&[]).unwrap(), None);
        let entry = ServerEntry {
            name: "apple-mail".to_owned(),
            origin: Origin::Owner,
            config: ServerConfig::Stdio {
                command: "uvx".to_owned(),
                args: vec!["mcp-apple-mail".to_owned()],
                env: Default::default(),
            },
        };
        let json: serde_json::Value =
            serde_json::from_str(&mcp_config_json(&[entry]).unwrap().unwrap()).unwrap();
        assert_eq!(json["mcpServers"]["apple-mail"]["type"], "stdio");
        assert_eq!(json["mcpServers"]["apple-mail"]["command"], "uvx");
    }

    #[test]
    fn run_files_are_private_and_gone_after_the_run() {
        let scratch = Scratch::new("files");
        let dir;
        {
            let files = RunFiles::create(&scratch.0.join("runs")).unwrap();
            let path = files.write("mcp.json", "{}").unwrap();
            dir = files.dir.clone();
            let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&dir), 0o700);
            assert_eq!(mode(&path), 0o600);
        }
        assert!(!dir.exists());
    }

    #[test]
    fn the_context_is_the_folders_agents_md_then_the_callers_file() {
        let scratch = Scratch::new("context");
        assert_eq!(run_context(&scratch.0, None).unwrap(), None);
        std::fs::write(scratch.0.join("AGENTS.md"), "router").unwrap();
        let extra = scratch.0.join("module-context.md");
        std::fs::write(&extra, "modules").unwrap();
        assert_eq!(
            run_context(&scratch.0, Some(&extra)).unwrap().as_deref(),
            Some("router\n\nmodules")
        );
        // A linked instruction file is not followed.
        std::fs::remove_file(scratch.0.join("AGENTS.md")).unwrap();
        std::os::unix::fs::symlink(&extra, scratch.0.join("AGENTS.md")).unwrap();
        assert_eq!(run_context(&scratch.0, None).unwrap(), None);
    }

    /// The result event as measured (CP0), after the notice Claude Code prints for an untrusted folder.
    #[test]
    fn a_result_is_read_after_a_notice_line_with_every_input_token() {
        let stdout = "Ignoring 1 permissions.allow entry from .claude/settings.json: this workspace has not been \
                      trusted.\n{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\
                      \"result\":\"{\\\"a\\\":1}\",\"session_id\":\"s1\",\"total_cost_usd\":0.0033,\"num_turns\":3,\
                      \"usage\":{\"input_tokens\":6,\"output_tokens\":20,\"cache_creation_input_tokens\":100,\
                      \"cache_read_input_tokens\":1000},\"permission_denials\":[{\"tool_name\":\"Bash\",\
                      \"tool_input\":{}}]}\n{}\n";
        let outcome = Outcome::from_event(parse_result(stdout).unwrap(), Duration::from_millis(5));
        assert_eq!(outcome.session_id.as_deref(), Some("s1"));
        let result = outcome.into_result();
        assert_eq!(result.stdout, "{\"a\":1}");
        assert_eq!(result.exit_code, 0);
        assert_eq!(result.input_tokens, Some(1106));
        assert_eq!(result.output_tokens, Some(20));
        assert_eq!(result.num_turns, Some(3));
        assert_eq!(result.cost_usd, Some(0.0033));
        assert!(
            result.stderr.contains("refused 1 tool call(s)"),
            "{}",
            result.stderr
        );
    }

    #[test]
    fn a_stray_object_is_no_result_and_an_error_subtype_fails() {
        assert!(parse_result("{}\n{\"type\":\"assistant\"}").is_none());
        let stdout = "{\"type\":\"result\",\"subtype\":\"error_max_turns\",\"is_error\":false}";
        let result =
            Outcome::from_event(parse_result(stdout).unwrap(), Duration::ZERO).into_result();
        assert_eq!(result.exit_code, 1);
        assert!(
            result.stderr.contains("error_max_turns"),
            "{}",
            result.stderr
        );
    }

    #[test]
    fn a_missing_login_fails_the_run_with_the_sign_in_command() {
        let stdout = "{\"type\":\"result\",\"is_error\":true,\"result\":\"Not logged in · Please run /login\"}";
        let result =
            Outcome::from_event(parse_result(stdout).unwrap(), Duration::ZERO).into_result();
        assert_eq!(result.exit_code, 1);
        assert!(result.stdout.is_empty());
        assert!(
            result.stderr.contains("claude auth login --claudeai"),
            "{}",
            result.stderr
        );
        assert!(parse_result("no json at all").is_none());
    }

    #[test]
    fn a_repair_adds_its_usage_and_replaces_the_reply_only_with_json() {
        let event = |result: &str, cost: f64| ResultEvent {
            kind: "result".to_owned(),
            subtype: None,
            result: Some(result.to_owned()),
            is_error: false,
            session_id: Some("s".to_owned()),
            total_cost_usd: Some(cost),
            num_turns: Some(1),
            usage: Some(Usage {
                input_tokens: 10,
                output_tokens: 2,
                ..Usage::default()
            }),
            permission_denials: Vec::new(),
        };
        let first = Outcome::from_event(event("prose", 0.1), Duration::from_millis(1));
        let merged = first.merge_repair(Outcome::from_event(
            event("{\"ok\":true}", 0.2),
            Duration::from_millis(1),
        ));
        assert_eq!(merged.reply, "{\"ok\":true}");
        assert_eq!(merged.input_tokens, 20);
        assert!((merged.cost.unwrap() - 0.3).abs() < 1e-9);

        let first = Outcome::from_event(event("prose", 0.1), Duration::ZERO);
        let kept = first.merge_repair(Outcome::from_event(
            event("still prose", 0.1),
            Duration::ZERO,
        ));
        assert_eq!(kept.reply, "prose");
    }

    #[tokio::test]
    async fn invoke_feeds_the_prompt_and_reads_the_result() {
        let scratch = Scratch::new("feed");
        // Echoes the prompt's byte count back as the result, so the test sees the whole prompt arrived.
        let program = scratch.script(
            "n=$(wc -c | tr -d ' ')\n\
             printf '{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false,\"result\":\"%s\"}\\n' \"$n\"",
        );
        let prompt = "x".repeat(200_000);
        let outcome = invoke(&program, &[], &prompt, &scratch.0, Duration::from_secs(20))
            .await
            .unwrap();
        assert_eq!(outcome.reply, "200000");
    }

    /// A child that answers without reading its (large) prompt is no spawn error: its own result counts.
    #[tokio::test]
    async fn invoke_takes_the_result_of_a_child_that_never_reads_its_prompt() {
        let scratch = Scratch::new("early");
        let program = scratch.script(&format!("echo '{RESULT_LINE}'"));
        let prompt = "x".repeat(1024 * 1024);
        let outcome = invoke(&program, &[], &prompt, &scratch.0, Duration::from_secs(20))
            .await
            .unwrap();
        assert_eq!(outcome.reply, "done");
    }

    #[tokio::test]
    async fn invoke_ends_the_whole_group_at_the_limit() {
        let scratch = Scratch::new("limit");
        let marker = scratch.0.join("child-survived");
        // A grandchild that would leave a mark after the limit: killing only the leader would let it.
        let program = scratch.script(&format!(
            "(sleep 2; touch '{}') &\nsleep 30",
            marker.display()
        ));
        let started = Instant::now();
        let err = invoke(&program, &[], "", &scratch.0, Duration::from_millis(300))
            .await
            .unwrap_err();
        assert!(matches!(err, AxiomataError::AgentTimeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(5));
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert!(!marker.exists(), "a child of the run outlived the limit");
    }

    /// Measured (CP3): `--resume` of an unknown conversation prints a line instead of a result — found even behind a
    /// notice longer than the shortened failure text.
    #[tokio::test]
    async fn an_unknown_conversation_is_told_apart_from_other_failures() {
        let scratch = Scratch::new("unknown");
        let notice = "n".repeat(MAX_FAILURE_STDOUT * 2);
        let program = scratch.script(&format!(
            "echo '{notice}'\necho 'No conversation found with session ID: 1111'"
        ));
        let outcome = invoke(&program, &[], "hi", &scratch.0, Duration::from_secs(20))
            .await
            .unwrap();
        assert!(outcome.unknown_conversation);
        // The failure text stays short, cut on a character boundary.
        assert!(outcome.failure.unwrap().len() < MAX_FAILURE_STDOUT + 200);
        let program = scratch.script("echo 'something else ü' >&2; exit 1");
        let outcome = invoke(&program, &[], "hi", &scratch.0, Duration::from_secs(20))
            .await
            .unwrap();
        assert!(!outcome.unknown_conversation);
        assert!(outcome.failure.unwrap().contains("something else ü"));
    }

    /// A resumed conversation Claude Code does not have is retried once as a new one — and only that one is.
    #[tokio::test]
    async fn a_chat_turn_starts_anew_once_when_its_conversation_is_unknown() {
        let scratch = Scratch::new("anew");
        let calls = scratch.0.join("calls");
        let program = scratch.script(&format!(
            "echo \"$*\" >> '{calls}'\n\
             case \"$*\" in *--resume*) echo 'No conversation found with session ID: x' ;; \
             *) echo '{RESULT_LINE}' ;; esac",
            calls = calls.display()
        ));
        let call = |resume| Invocation {
            resume,
            ..invocation(&[], &[])
        };
        let outcome = turn_or_start_anew(
            &program,
            call(Some("gone-1")),
            "hi",
            &scratch.0,
            Duration::from_secs(20),
        )
        .await
        .unwrap();
        assert_eq!(outcome.reply, "done");
        let lines = std::fs::read_to_string(&calls).unwrap();
        assert_eq!(lines.lines().count(), 2, "{lines}");
        assert!(lines.lines().next().unwrap().contains("--resume gone-1"));
        assert!(!lines.lines().nth(1).unwrap().contains("--resume"));

        // A failure that is not an unknown conversation is not retried.
        std::fs::remove_file(&calls).unwrap();
        let program = scratch.script(&format!("echo x >> '{}'\nexit 1", calls.display()));
        let outcome = turn_or_start_anew(
            &program,
            call(Some("s-1")),
            "hi",
            &scratch.0,
            Duration::from_secs(20),
        )
        .await
        .unwrap();
        assert!(outcome.failure.is_some());
        assert_eq!(std::fs::read_to_string(&calls).unwrap().lines().count(), 1);
    }

    /// The connector modules' instruct turns name one MCP tool (`calendar.ts`, `reminders.ts`): only that is allowed,
    /// and the turn resumes its conversation.
    #[test]
    fn an_instruct_turn_with_a_declared_mcp_tool_allows_only_that_tool() {
        let declared = declared_tools(None, Some("mcp__apple-reminders__reminders_tasks")).unwrap();
        let servers = strings(&["apple-reminders", "apple-mail"]);
        let call = Invocation {
            resume: Some("s-1"),
            ..invocation(&declared, &servers)
        };
        let args = arguments(&call);
        let after = |flag: &str| args[args.iter().position(|arg| arg == flag).unwrap() + 1].clone();
        assert_eq!(after("--resume"), "s-1");
        assert_eq!(
            after("--allowedTools"),
            "mcp__apple-reminders__reminders_tasks"
        );
        assert_eq!(
            args.last().unwrap(),
            "mcp__apple-reminders__reminders_tasks"
        );
        assert!(!args.contains(&"--permission-mode".to_owned()));
    }

    #[test]
    fn a_chat_reply_carries_the_session_and_refuses_failures_and_silence() {
        let event = |result: &str| ResultEvent {
            kind: "result".to_owned(),
            subtype: None,
            result: Some(result.to_owned()),
            is_error: false,
            session_id: Some("s-1".to_owned()),
            total_cost_usd: Some(0.01),
            num_turns: Some(2),
            usage: Some(Usage {
                input_tokens: 5,
                output_tokens: 7,
                ..Usage::default()
            }),
            permission_denials: Vec::new(),
        };
        let reply = chat_reply(Outcome::from_event(
            event("  Hallo  "),
            Duration::from_millis(3),
        ))
        .unwrap();
        assert_eq!(reply.session_id, "s-1");
        assert_eq!(reply.reply_markdown, "Hallo");
        assert_eq!(reply.input_tokens, Some(5));
        assert_eq!(reply.output_tokens, Some(7));
        assert_eq!(reply.num_turns, Some(2));
        assert!(matches!(
            chat_reply(Outcome::from_event(event("   "), Duration::ZERO)),
            Err(AxiomataError::AgentApi { .. })
        ));
        assert!(matches!(
            chat_reply(Outcome::failed("boom".to_owned(), Duration::ZERO)),
            Err(AxiomataError::AgentApi { message, .. }) if message == "boom"
        ));
    }

    #[tokio::test]
    async fn unit_tests_never_start_claude_code() {
        let request = AgentRequest {
            prompt: "hi".to_owned(),
            name: None,
            cwd: std::env::temp_dir(),
            timeout: Duration::from_secs(1),
            system_prompt_file: None,
            model: Some("claude-haiku-5-5".to_owned()),
            auto_approve_tools: false,
            allowed_tools: None,
            expects_json: false,
            read_only: false,
        };
        assert!(matches!(
            run(request).await,
            Err(AxiomataError::AgentApi { .. })
        ));
    }
}
