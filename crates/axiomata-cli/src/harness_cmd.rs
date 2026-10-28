//! `axiomata-cli harness …`: the own harness (`glut`, `docs/plans/harness.md`) with a provider from
//! `~/.axiomata/config.toml` — its base URL, key and skill model (H5). The way to try the harness before it has a
//! place in the app.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::config::{Config, ProviderId};
use clap::{Args, Subcommand, ValueEnum};
use glut::commands::{PermissionsView, Registry};
use glut::mcp;
use glut::permissions::settings::{self, Trust};
use glut::permissions::{
    Asker, BuiltGate, GateOptions, Mode, Nobody, Terminal, build_gate, printable,
};
use glut::setup::{self, Draft, Prepared};
use glut::skills;
use glut::{
    Cancel, Effort, Event, EventSink, FinishReason, McpStatus, ModelEndpoint, OpenAiCompat,
    RunConfig, SharedSink, SkillHow,
};

use crate::board_flow::owner_only;

/// How many characters of a tool's result the readable output shows: one glance, the JSON form has the rest.
const PREVIEW_CHARS: usize = 160;

#[derive(Debug, Subcommand)]
pub enum HarnessAction {
    /// Run one prompt in a folder; prints each step, or JSON lines with `--json`.
    Run(Box<RunFlags>),
    /// A Studio session on Glut, in its pane (`docs/plans/glut-cards.md`): started by the studio, which names the
    /// session in the environment and wrote what it is started with into its channel.
    Card,
    /// A project's Glut master, in its pane (`docs/plans/glut-tui.md` in Glut's repository, TUI8c): the owner's own
    /// Glut in the project folder with the team server added; started by the studio, which names the session in the
    /// environment.
    Master,
    /// Follow a Glut session of the Studio read-only from its event log (TUI8d): a card's from its channel, a
    /// master's newest session in the project folder.
    Watch {
        /// The session's id (`ide agents list`).
        #[arg(long)]
        agent: i64,
    },
    /// What Glut lean saved (`docs/plans/glut-lean.md`, N9): compacted outputs, unchanged re-reads and relocated
    /// results per kind — every Glut run's, from `~/.glut/lean.jsonl`; the same table as `glut gain`.
    Gain {
        /// Only the last N days.
        #[arg(long)]
        days: Option<u32>,
    },
}

/// `harness run`'s flags.
#[derive(Debug, Args)]
pub struct RunFlags {
    /// The provider whose base URL and key are used; default: the skill provider, else OpenRouter.
    #[arg(long, value_enum)]
    provider: Option<Provider>,
    /// The model; default: the provider's skill model.
    #[arg(long)]
    model: Option<String>,
    /// How hard the model reasons: none, minimal, low, medium, high, xhigh or max. Otherwise a skill's or a
    /// command's `effort:`, else `[agents.glut] skill_effort`, else `effort` in ~/.glut/settings.json.
    #[arg(long)]
    effort: Option<String>,
    /// The folder the run works in.
    #[arg(long, default_value = ".")]
    cwd: PathBuf,
    #[arg(long)]
    max_turns: Option<u32>,
    /// Print the events as JSON lines, as `axh` does.
    #[arg(long)]
    json: bool,
    /// default, accept_edits, plan, dont_ask or auto; otherwise the settings files', else default.
    #[arg(long)]
    permission_mode: Option<String>,
    /// A rule the run allows (`Bash(cargo test:*)`, `Edit(src/**)`); repeatable.
    #[arg(long)]
    allow: Vec<String>,
    /// A rule the run asks about; repeatable.
    #[arg(long)]
    ask: Vec<String>,
    /// A rule the run refuses; repeatable.
    #[arg(long)]
    deny: Vec<String>,
    /// Take a role's `permissions` (`~/.axiomata/agents/<role>/AGENT.md`) as allow rules.
    #[arg(long)]
    role: Option<String>,
    /// Run a skill as the run's task (HS6, K1.1): its SKILL.md body becomes the run's instructions, and its
    /// frontmatter may narrow the tools, pick the model and set the time limit (K5).
    #[arg(long)]
    skill: Option<String>,
    /// A folder of skills beside the app's `~/.axiomata/skills` (HS6, K2); repeatable.
    #[arg(long)]
    skill_dir: Vec<PathBuf>,
    /// The prompt — extra text beside `--skill`, or a slash command (`/review src/`) of `~/.glut/commands/`
    /// and, in a trusted folder, the project's `.glut/commands/` (HS4). Without `--skill` a prompt is required;
    /// with one it defaults to "Run the skill.".
    prompt: Option<String>,
}

/// The providers the harness can talk to today: the OpenAI-compatible ones (H4). Anthropic follows with its own
/// client.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Provider {
    OpenRouter,
    Ollama,
}

impl Provider {
    fn id(self) -> ProviderId {
        match self {
            Provider::OpenRouter => ProviderId::OpenRouter,
            Provider::Ollama => ProviderId::Ollama,
        }
    }
}

pub async fn run(core: &std::sync::Arc<AxiomataCore>, action: HarnessAction) -> Result<()> {
    let flags = match action {
        HarnessAction::Run(flags) => *flags,
        HarnessAction::Gain { days } => return gain(days),
        HarnessAction::Card => return crate::harness_card::card(std::sync::Arc::clone(core)).await,
        HarnessAction::Master => return crate::harness_master::master(core).await,
        HarnessAction::Watch { agent } => return crate::harness_watch::watch(core, agent).await,
    };
    let RunFlags {
        provider,
        model,
        effort,
        cwd,
        max_turns,
        json,
        permission_mode,
        allow,
        ask,
        deny,
        role,
        skill,
        skill_dir,
        prompt,
    } = flags;
    // It spends the owner's money with the owner's key: not something an agent session starts by itself.
    owner_only("running the own harness")?;
    if prompt.is_none() && skill.is_none() {
        bail!("name a prompt, or run a skill with --skill <name>");
    }
    let config = crate::read_config(core);
    let mut endpoint = endpoint(&config, provider, model)?;
    if let Some(text) = effort {
        endpoint.effort =
            Some(Effort::parse_or_explain(&text).map_err(|reason| anyhow!("--effort: {reason}"))?);
    }
    let cwd = cwd
        .canonicalize()
        .with_context(|| format!("cannot use {}", cwd.display()))?;
    let (built, home, trusted) = gate(&cwd, permission_mode, allow, ask, deny, role)?;
    // Every event through one shared sink: the loop writes to it, and the `skill` tool reports from inside a call
    // (HS6, K1.2).
    let inner: Box<dyn EventSink> = if json {
        Box::new(JsonLines)
    } else {
        Box::new(Readable::default())
    };
    let mut sink = SharedSink::new(inner);
    // Skills (HS6, K2): the app's own `~/.axiomata/skills` (this CLI knows where that is, the harness does not),
    // the folders of `--skill-dir` and — in a folder the owner trusted once, the same trust the project's allow
    // rules need — the project's `.glut/skills`. A skill that cannot be used is a note, never an error (K3).
    let mut skill_dirs = vec![axiomata_core::paths::global_skills_dir()];
    skill_dirs.extend(skill_dir);
    let (catalog, notes) = skills::Catalog::load(&home, &cwd, trusted, &skill_dirs);
    for note in notes {
        eprintln!("note: skill left out: {note}");
    }
    let catalog = Arc::new(catalog);
    // Slash commands (HS4), with the same home, folder and trust the gate was built with (C1).
    let mut registry = Registry::load(&home, &cwd, trusted)?;
    registry.set_permissions(PermissionsView::of(&built.gate, &built.sandbox));
    let mut run_config = RunConfig::new(&cwd);
    if let Some(turns) = max_turns {
        run_config.max_turns = turns;
    }
    // HS7: the owner's hooks (B1, the only ones that ever run), and the sandbox their commands run in (D5).
    // Glut's own limits and compaction, as `axh` takes them (#112 R6, #111 K2/K6).
    run_config.refusals = built.refusals;
    run_config.compaction = built.compaction(&endpoint.model);
    run_config.hooks = built.hooks;
    run_config.sandbox = Some(built.sandbox.clone());
    // Glut 2 (G8): code mode as the owner set it in `[agents.glut]`, as the app's runs have it.
    run_config.codemode = config.agents.glut.codemode;
    // G7 (revised), as the app: a skill's declared MCP tools stay direct.
    run_config.mcp_behind_code = skill.is_none();
    // Glut 2 (D): snapshots before changes, in the harness's own store, for `glut undo`.
    run_config.snapshots = Some(home.join(".glut").join("snapshots"));
    // Glut lean (N9): what the run saves is counted with the app's runs.
    run_config.savings = Some(home.join(".glut").join(glut::lean::FILE));
    // The flag, else the app's skill effort, else the harness's own settings; a skill's or a command's `effort:`
    // still wins in `prepare`.
    endpoint.effort = endpoint
        .effort
        .or(config.agents.glut.skill_effort)
        .or(built.effort);
    let mut draft = Draft {
        config: run_config,
        endpoint,
        gate: built.gate,
    };
    let prepared = setup::prepare(
        &mut draft,
        &registry,
        &catalog,
        prompt,
        skill.as_deref(),
        &mut sink,
    )?;
    // A built-in is answered without a model: its text is the answer (C5).
    let Prepared::Prompt(prompt) = prepared else {
        return Ok(());
    };
    let Draft {
        config: run_config,
        endpoint,
        gate,
    } = draft;
    // The model is built after the command and the skill: either may name its own (C2, K5).
    let model = OpenAiCompat::new(endpoint)?;
    // MCP servers (HS5, M1–M3): a project's server the owner has not approved yet is asked about in the
    // terminal, or left out with a note when there is none (M2).
    let servers = mcp::config::load(&home, &cwd)?;
    let servers = mcp::approval::permitted(servers, &home, &cwd, mcp::approval::interactive())?;
    // The web search with the owner's keys and the app's limits, as in the app's own runs (glut-round CP3).
    let extras = setup::Extras {
        search: axiomata_core::web_search::web_search_tool(&config),
        direct: axiomata_core::agent_entry::cli_direct_program(
            axiomata_core::agent_entry::HARNESS_RUN_SKILL,
        )
        .into_iter()
        .collect(),
        memory: None,
        // Glut T37b: making skills, agents and hooks is the owner's, in Glut itself.
        glut_files: None,
    };
    let mut tools = setup::toolbox(
        &cwd,
        &catalog,
        gate,
        servers,
        built.sandbox,
        extras,
        &mut sink,
    )
    .await?;
    // A skill run's task is the skill's body already: loading skills would only cost a turn (glut-round CP4).
    if skill.is_some() {
        tools.withdraw(glut::skills::tool::NAME);
    }
    // Ctrl-C, SIGTERM and SIGHUP end the run with its last line and every background job stopped.
    let cancel = Cancel::new();
    glut::run::cancel_on_signals(&cancel);
    let outcome = glut::run(&run_config, &model, &tools, &prompt, &mut sink, &cancel).await;
    match outcome.reason {
        FinishReason::Answered => Ok(()),
        other => bail!("the run ended: {other:?}"),
    }
}

/// The run's gate (HS3a): the settings files, the flags and a role's rules; asks in the terminal when there is one.
/// Beside it the home folder and whether the project counts as trusted — the skills and the command registry of HS4
/// are read with the same trust (C1), a project's commands count where its allow rules count.
/// A project with `.glut/` settings in a folder not yet trusted is asked about first (P5).
fn gate(
    cwd: &std::path::Path,
    mode: Option<String>,
    mut allow: Vec<String>,
    ask: Vec<String>,
    deny: Vec<String>,
    role: Option<String>,
) -> Result<(BuiltGate, PathBuf, bool)> {
    use std::io::IsTerminal;
    let mode = match mode {
        None => None,
        Some(text) => {
            Some(Mode::parse(&text).with_context(|| format!("unknown permission mode {text:?}"))?)
        }
    };
    if let Some(name) = role {
        let roles = axiomata_core::roster::list_roles()?;
        let role = roles
            .roles
            .into_iter()
            .find(|role| role.name == name)
            .with_context(|| format!("no role {name:?} in ~/.axiomata/agents"))?;
        allow.extend(role.permissions);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .context("HOME is not set to an absolute folder")?;
    let interactive = std::io::stdin().is_terminal();
    let mut trust = Trust::load(&home)?;
    let mut trusted = trust.contains(cwd);
    if !trusted && interactive && settings::project_has_settings(cwd) {
        let (allow, mode) = settings::project_summary(cwd)?;
        if !allow.is_empty() || mode.is_some() {
            // What trusting would let in, shown before the question; the folder's name may hold anything, so it is
            // shown as plain text.
            let folder = printable(&cwd.display().to_string());
            eprintln!("{folder} has its own harness settings (.glut/). Trusting it adds:");
            for rule in &allow {
                eprintln!("  allow {}", printable(rule));
            }
            if let Some(mode) = mode {
                eprintln!("  mode  {mode:?} (only where it is no looser than your own)");
            }
            eprint!("Trust this folder? [y/N]: ");
            let _ = std::io::stderr().flush();
            let mut line = String::new();
            std::io::stdin().read_line(&mut line)?;
            if matches!(
                line.trim().to_lowercase().as_str(),
                "y" | "j" | "yes" | "ja"
            ) {
                trust.add(&home, cwd)?;
                trusted = true;
            }
        }
    }
    let asker: Arc<dyn Asker> = if interactive {
        Arc::new(Terminal)
    } else {
        Arc::new(Nobody)
    };
    let options = GateOptions {
        mode,
        allow,
        ask,
        deny,
        trusted,
    };
    let built = build_gate(&home, cwd, options, asker)?;
    for note in built.notes() {
        eprintln!("note: {note}");
    }
    Ok((built, home, trusted))
}

/// The endpoint for `provider` (or the configured default) from the config: base URL, key and model.
fn endpoint(
    config: &Config,
    provider: Option<Provider>,
    model: Option<String>,
) -> Result<ModelEndpoint> {
    let id = match provider {
        Some(provider) => provider.id(),
        None => match config.agents.skill_provider {
            ProviderId::Ollama => ProviderId::Ollama,
            _ => ProviderId::OpenRouter,
        },
    };
    let settings = config
        .agents
        .providers
        .get(&id)
        .with_context(|| format!("no settings for {} in config.toml", id.as_str()))?;
    let base_url = settings
        .base_url
        .clone()
        .filter(|url| !url.trim().is_empty())
        .with_context(|| format!("{} has no base URL in config.toml", id.as_str()))?;
    let api_key = settings
        .api_key
        .clone()
        .filter(|key| !key.trim().is_empty());
    if id == ProviderId::OpenRouter && api_key.is_none() {
        bail!("OpenRouter has no API key in config.toml (Settings → Providers)");
    }
    let model = model
        .or_else(|| Some(settings.skill_model.clone()).filter(|m| !m.trim().is_empty()))
        .with_context(|| {
            format!(
                "no model: pass --model or set the skill model of {}",
                id.as_str()
            )
        })?;
    Ok(ModelEndpoint {
        base_url,
        api_key,
        model,
        effort: None,
        versioned: false,
    })
}

struct JsonLines;

impl EventSink for JsonLines {
    fn emit(&mut self, event: Event) {
        if let Ok(line) = serde_json::to_string(&event) {
            // A reader that went away (`| head`) is no reason to panic.
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    }
}

/// One line per step, for a person at the terminal. Everything the model wrote goes through [`plain`] first: an
/// escape sequence in its text would otherwise be the terminal's to act on (a title, a fake prompt, the clipboard).
///
/// Subagents that run at once (Glut TUI T40) interleave their lines: each is headed by its task's label, which the
/// parent's `task` call gave.
#[derive(Default)]
pub(crate) struct Readable {
    labels: std::collections::HashMap<String, String>,
}

impl Readable {
    /// The label of the subagent an event comes from, when it names its `task` call.
    fn label_of(&mut self, event: &Event) -> Option<String> {
        match event {
            Event::ToolCall {
                id,
                name,
                arguments,
                ..
            } if name == "task" => {
                let label = serde_json::from_str::<serde_json::Value>(arguments)
                    .ok()
                    .and_then(|value| value.get("description")?.as_str().map(plain))
                    .unwrap_or_else(|| "subagent".into());
                self.labels.insert(id.clone(), preview(&label));
                None
            }
            Event::Sub {
                task: Some(task), ..
            } => self.labels.get(task).cloned(),
            _ => None,
        }
    }
}

impl EventSink for Readable {
    fn emit(&mut self, event: Event) {
        let label = self.label_of(&event);
        let headed = |line: String| match &label {
            Some(label) => {
                let rest = line.trim_start();
                let indent = &line[..line.len() - rest.len()];
                format!("{indent}⬡ {label} · {rest}")
            }
            None => line,
        };
        // Errors go to stderr — stdout carries a command run's reply (HS4) — also from inside a
        // subagent, where they keep their indent (HS8, D8).
        if let Some((indent, message)) = error_of(&event) {
            eprintln!("{}", headed(format!("{indent}error: {message}")));
            return;
        }
        if let Some(line) = render(&event).map(headed) {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    }
}

/// `harness gain`: the count of savings as a table — from the folder `harness run` counts into (`$HOME/.glut`).
fn gain(days: Option<u32>) -> Result<()> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .context("HOME is not set to an absolute folder")?;
    let path = home.join(".glut").join(glut::lean::FILE);
    let table =
        glut::lean::report(&path, days).with_context(|| format!("reading {}", path.display()))?;
    print!("{table}");
    Ok(())
}

/// One event's line for the terminal, `None` for what is not printed: a `Usage` (as before) and an
/// `Error` (stderr's, [`emit`] takes those before this is asked).
fn render(event: &Event) -> Option<String> {
    let line = match event {
        Event::Started {
            model, cwd, tools, ..
        } => {
            format!("▸ {} in {cwd} · {} tools", plain(model), tools.len())
        }
        // The command a prompt came from, before the run's own lines (HS4).
        Event::Command { name, source, .. } => format!("/{name} ({source})"),
        Event::AssistantText { text, .. } => format!("\n{}\n", plain(text)),
        Event::ToolCall {
            name, arguments, ..
        } => format!("→ {} {}", plain(name), preview(arguments)),
        Event::ToolResult { ok, output, .. } => {
            format!("  {} {}", if *ok { "✓" } else { "✗" }, preview(output))
        }
        Event::Permission {
            allowed, reason, ..
        } => {
            format!("  {} {}", if *allowed { "·" } else { "⊘" }, plain(reason))
        }
        // An MCP server of this run, connected or failed (HS5).
        Event::McpServer {
            name,
            status,
            tools,
            reason,
        } => match status {
            McpStatus::Connected => format!("◇ mcp {} · {tools} tools", plain(name)),
            McpStatus::Failed => format!(
                "◇ mcp {} ✗ {}",
                plain(name),
                plain(reason.as_deref().unwrap_or("it did not start"))
            ),
        },
        // A skill of this run: as the run itself (K1.1) or loaded by the model (K1.2).
        Event::Skill { name, how } => {
            let how = match how {
                SkillHow::Run => "as the run",
                SkillHow::Loaded => "loaded",
            };
            format!("✳ skill {} · {how}", plain(name))
        }
        // A hook of this run (HS7): its event, whether it blocked, and what it had to say.
        Event::Hook {
            hook_event,
            blocked,
            note,
            ..
        } => format!(
            "△ hook {hook_event}{} · {}",
            if *blocked { " ⊘" } else { "" },
            plain(note.as_deref().unwrap_or("ran"))
        ),
        Event::Finished {
            reason,
            turns,
            usage,
        } => format!(
            "■ {reason:?} · {turns} turns · {} in / {} out tokens",
            usage.input_tokens, usage.output_tokens
        ),
        // Glut 2, D: the run's task list, as the open items; and the run leaving Plan mode.
        Event::Todo { items, .. } => {
            let open = items
                .iter()
                .filter(|item| item.status != glut::planning::TodoStatus::Done)
                .count();
            format!("☐ todo · {open} open of {}", items.len())
        }
        Event::ModeChanged { mode, .. } => format!("◆ plan approved · mode {}", plain(mode)),
        // Glut lean (N9): what a call spared.
        Event::Lean {
            what,
            chars_before,
            chars_after,
            ..
        } => format!("  ≈ {} · {chars_before} → {chars_after} chars", plain(what)),
        // Glut T58: keys in a call's result were hidden before the model saw them.
        Event::Masked { hidden, .. } => format!("  ⊘ keys hidden: {}", hidden.len()),
        // Glut robust (#111, K5): the older steps became a summary.
        Event::Compacted {
            before_tokens,
            after_tokens,
            replaced,
            ..
        } => format!(
            "⇣ conversation compacted · {before_tokens} → {after_tokens} tokens · {replaced} messages summarised"
        ),
        // Glut TUI3b (T10): a message the owner sent while the run worked, taken in before this step.
        Event::Steered { text, .. } => format!("› {}", plain(text)),
        // Glut's own word to the model (T73): why the run went on.
        Event::Nudged { kind, .. } => match kind {
            glut::NudgeKind::Reminder => "· reminded of its task list".to_owned(),
            glut::NudgeKind::NoList => "· asked to write a task list".to_owned(),
            glut::NudgeKind::Pressed => "· pressed to update its task list".to_owned(),
            glut::NudgeKind::OpenItems => {
                "· asked to go on: its task list has items open".to_owned()
            }
            glut::NudgeKind::Announced => "· asked to take the step it announced".to_owned(),
            glut::NudgeKind::Repeated => "· its last step sent again: not run".to_owned(),
            glut::NudgeKind::Reading => "· told it has only read for many steps".to_owned(),
        },
        Event::Snapshot { turn, run, .. } => format!(
            "⟲ snapshot before step {turn} · glut undo --run {}",
            plain(run)
        ),
        // The readable lines show the whole answer once; its pieces, and the model's thinking, are for a `--json`
        // reader.
        Event::Usage { .. }
        | Event::Error { .. }
        | Event::TextDelta { .. }
        | Event::ReasoningDelta { .. } => return None,
        // A subagent's events, indented by their depth (HS8, D8). Its own finished line reads `↳`, not
        // `■`: two run-looking finished lines would have a reader count the run's end twice.
        Event::Sub { depth, event, .. } => {
            let inner = match &**event {
                Event::Finished { turns, usage, .. } => format!(
                    "↳ subagent · {turns} turns · {} in / {} out tokens",
                    usage.input_tokens, usage.output_tokens
                ),
                inner => render(inner)?,
            };
            return Some(format!("{}{inner}", "  ".repeat(*depth as usize)));
        }
    };
    Some(line)
}

/// `(indent, message)` of an error event wherever it sits in the stream — a subagent's errors arrive
/// wrapped in `Sub` and keep their depth's indent (HS8, D8); `None` for every other event.
fn error_of(event: &Event) -> Option<(String, String)> {
    match event {
        Event::Error { message } => Some((String::new(), plain(message))),
        Event::Sub { depth, event, .. } => {
            let (indent, message) = error_of(event)?;
            Some((format!("{}{indent}", "  ".repeat(*depth as usize)), message))
        }
        _ => None,
    }
}

/// `text` without control characters other than line breaks and tabs.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

/// The first line of `text`, cut to [`PREVIEW_CHARS`], without control characters.
fn preview(text: &str) -> String {
    let text = plain(text);
    let text = text.as_str();
    let first = text.lines().next().unwrap_or("");
    let mut shown: String = first.chars().take(PREVIEW_CHARS).collect();
    if first.chars().count() > PREVIEW_CHARS || text.lines().nth(1).is_some() {
        shown.push('…');
    }
    shown
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiomata_core::config::ProviderSettings;

    fn config_with(id: ProviderId, settings: ProviderSettings) -> Config {
        let mut config = Config::default();
        config.agents.providers.insert(id, settings);
        config
    }

    #[test]
    fn a_subagents_lines_are_headed_by_its_task_while_others_run_beside_it() {
        let mut readable = Readable::default();
        assert_eq!(
            readable.label_of(&Event::ToolCall {
                turn: 1,
                id: "t1".into(),
                name: "task".into(),
                arguments: r#"{"description":"find the callers","prompt":"p"}"#.into(),
            }),
            None
        );
        let sub = |task: Option<&str>| Event::Sub {
            depth: 1,
            task: task.map(Into::into),
            event: Box::new(Event::AssistantText {
                turn: 1,
                text: "two".into(),
            }),
        };
        assert_eq!(
            readable.label_of(&sub(Some("t1"))).as_deref(),
            Some("find the callers")
        );
        assert_eq!(
            readable.label_of(&sub(None)),
            None,
            "a log from before names none"
        );
    }

    #[test]
    fn the_endpoint_takes_url_key_and_skill_model_from_the_config() {
        let config = config_with(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some("https://openrouter.ai/api".into()),
                api_key: Some("sk-or".into()),
                chat_model: "chat".into(),
                skill_model: "deepseek/deepseek-chat".into(),
            },
        );
        let endpoint = endpoint(&config, Some(Provider::OpenRouter), None).unwrap();
        assert_eq!(endpoint.base_url, "https://openrouter.ai/api");
        assert_eq!(endpoint.api_key.as_deref(), Some("sk-or"));
        assert_eq!(endpoint.model, "deepseek/deepseek-chat");
        assert_eq!(
            super::endpoint(&config, Some(Provider::OpenRouter), Some("other".into()))
                .unwrap()
                .model,
            "other"
        );
    }

    #[test]
    fn openrouter_without_a_key_or_any_provider_without_a_model_is_refused() {
        let keyless = config_with(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some("https://openrouter.ai/api".into()),
                api_key: None,
                chat_model: String::new(),
                skill_model: "m".into(),
            },
        );
        assert!(endpoint(&keyless, Some(Provider::OpenRouter), None).is_err());
        let modelless = config_with(
            ProviderId::Ollama,
            ProviderSettings {
                base_url: Some("http://localhost:11434".into()),
                api_key: None,
                chat_model: String::new(),
                skill_model: String::new(),
            },
        );
        assert!(endpoint(&modelless, Some(Provider::Ollama), None).is_err());
        assert!(endpoint(&modelless, Some(Provider::Ollama), Some("qwen3".into())).is_ok());
    }

    #[test]
    fn escape_sequences_from_the_model_never_reach_the_terminal() {
        assert_eq!(plain("a\u{1b}]0;title\u{7}b\nc\td"), "a]0;titleb\nc\td");
        assert!(!preview("\u{1b}[2Jcleared").contains('\u{1b}'));
    }

    #[test]
    fn a_preview_is_one_short_line() {
        assert_eq!(preview("a\nb"), "a…");
        assert_eq!(preview("short"), "short");
        assert_eq!(
            preview(&"x".repeat(PREVIEW_CHARS + 5)).chars().count(),
            PREVIEW_CHARS + 1
        );
    }

    #[test]
    fn a_subagents_events_are_indented_and_their_finished_reads_its_own() {
        use glut::Usage;

        // HS8, D8: the subagent's own finished line is a `↳`, never the run's `■`.
        let finished = Event::Sub {
            depth: 1,
            task: None,
            event: Box::new(Event::Finished {
                reason: FinishReason::Answered,
                turns: 3,
                usage: Usage {
                    input_tokens: 30,
                    cached_input_tokens: 0,
                    output_tokens: 6,
                },
            }),
        };
        assert_eq!(
            render(&finished).as_deref(),
            Some("  ↳ subagent · 3 turns · 30 in / 6 out tokens")
        );
        let own = Event::Finished {
            reason: FinishReason::Answered,
            turns: 3,
            usage: Usage {
                input_tokens: 30,
                cached_input_tokens: 0,
                output_tokens: 6,
            },
        };
        assert_eq!(
            render(&own).as_deref(),
            Some("■ Answered · 3 turns · 30 in / 6 out tokens")
        );

        // A `Usage` is never printed, wrapped or not.
        let usage = Event::Sub {
            depth: 1,
            task: None,
            event: Box::new(Event::Usage {
                turn: 1,
                input_tokens: 1,
                cached_input_tokens: 0,
                output_tokens: 2,
            }),
        };
        assert_eq!(render(&usage), None);

        // Errors stay stderr's, wherever they sit, and a subagent's keeps its indent.
        let error = Event::Error {
            message: "boom".into(),
        };
        assert_eq!(error_of(&error), Some((String::new(), "boom".into())));
        let wrapped = Event::Sub {
            depth: 2,
            task: None,
            event: Box::new(Event::Error {
                message: "boom".into(),
            }),
        };
        assert_eq!(error_of(&wrapped), Some(("    ".into(), "boom".into())));
        assert_eq!(error_of(&own), None);
    }
}
