//! `axiomata-cli harness …`: the own harness (`axiomata-harness`, `docs/plans/harness.md`) with a provider from
//! `~/.axiomata/config.toml` — its base URL, key and skill model (H5). The way to try the harness before it has a
//! place in the app.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::config::{Config, ProviderId};
use axiomata_harness::commands::{Expanded, PermissionsView, Registry, Source};
use axiomata_harness::hooks::Hooks;
use axiomata_harness::mcp;
use axiomata_harness::permissions::settings::{self, SandboxSettings, Trust};
use axiomata_harness::permissions::{
    Asker, Gate, GateOptions, Mode, Nobody, Terminal, build_gate, printable,
};
use axiomata_harness::skills::{self, SkillTool};
use axiomata_harness::tools::Tool;
use axiomata_harness::tools::files::{SkillRoots, file_tools};
use axiomata_harness::tools::shell::{ShellOptions, shell_tools};
use axiomata_harness::{
    Cancel, Event, EventSink, FinishReason, McpStatus, ModelEndpoint, OpenAiCompat, RunConfig,
    SharedSink, SkillHow, Toolbox,
};
use clap::{Subcommand, ValueEnum};

use crate::board_flow::owner_only;

/// How many characters of a tool's result the readable output shows: one glance, the JSON form has the rest.
const PREVIEW_CHARS: usize = 160;

#[derive(Debug, Subcommand)]
pub enum HarnessAction {
    /// Run one prompt in a folder; prints each step, or JSON lines with `--json`.
    Run {
        /// The provider whose base URL and key are used; default: the skill provider, else OpenRouter.
        #[arg(long, value_enum)]
        provider: Option<Provider>,
        /// The model; default: the provider's skill model.
        #[arg(long)]
        model: Option<String>,
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
        /// The prompt — extra text beside `--skill`, or a slash command (`/review src/`) of `~/.axh/commands/`
        /// and, in a trusted folder, the project's `.axh/commands/` (HS4). Without `--skill` a prompt is required;
        /// with one it defaults to "Run the skill.".
        prompt: Option<String>,
    },
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

pub async fn run(core: &AxiomataCore, action: HarnessAction) -> Result<()> {
    let HarnessAction::Run {
        provider,
        model,
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
    } = action;
    // It spends the owner's money with the owner's key: not something an agent session starts by itself.
    owner_only("running the own harness")?;
    if prompt.is_none() && skill.is_none() {
        bail!("name a prompt, or run a skill with --skill <name>");
    }
    let config = crate::read_config(core);
    let mut endpoint = endpoint(&config, provider, model)?;
    let cwd = cwd
        .canonicalize()
        .with_context(|| format!("cannot use {}", cwd.display()))?;
    let mut built = gate(&cwd, permission_mode, allow, ask, deny, role)?;
    // Every event through one shared sink: the loop writes to it, and the `skill` tool reports from inside a call
    // (HS6, K1.2).
    let inner: Box<dyn EventSink> = if json {
        Box::new(JsonLines)
    } else {
        Box::new(Readable)
    };
    let mut sink = SharedSink::new(inner);
    // Skills (HS6, K2): the app's own `~/.axiomata/skills` (this CLI knows where that is, the harness does not),
    // the folders of `--skill-dir` and — in a folder the owner trusted once, the same trust the project's allow
    // rules need — the project's `.axh/skills`. A skill that cannot be used is a note, never an error (K3).
    let mut skill_dirs = vec![axiomata_core::paths::global_skills_dir()];
    skill_dirs.extend(skill_dir);
    let (catalog, notes) = skills::Catalog::load(&built.home, &cwd, built.trusted, &skill_dirs);
    for note in notes {
        eprintln!("note: skill left out: {note}");
    }
    let catalog = Arc::new(catalog);
    let mut tools = Toolbox::new();
    tools.extend(file_tools(&cwd, SkillRoots::new(&catalog.folders()))?);
    // The `skill` tool only while there is at least one skill to load (K1.2).
    if !catalog.is_empty() {
        tools.extend([
            Box::new(SkillTool::new(Arc::clone(&catalog), sink.clone())) as Box<dyn Tool>
        ]);
    }
    // Slash commands (HS4), after the gate and with the same home, folder and trust it was built with (C1).
    let mut registry = Registry::load(&built.home, &cwd, built.trusted)?;
    registry.set_permissions(PermissionsView::of(&built.gate, &built.sandbox));
    let mut prompt = prompt.unwrap_or_default();
    built.gate = match registry.expand(&prompt) {
        // An ordinary prompt: no command was named (C6).
        None => built.gate,
        Some(Err(message)) => bail!(message),
        // A built-in is answered without a model: its text is the answer (C5).
        Some(Ok(Expanded::Builtin { name, text })) => {
            sink.emit(Event::Command {
                name,
                source: Source::Builtin.as_str().to_owned(),
                prompt_chars: 0,
            });
            sink.emit(Event::AssistantText { turn: 0, text });
            return Ok(());
        }
        Some(Ok(Expanded::Prompt {
            name,
            prompt: expanded,
            allowed_tools,
            model,
        })) => {
            let source = registry.source(&name).unwrap_or(Source::User);
            sink.emit(Event::Command {
                source: source.as_str().to_owned(),
                name: name.clone(),
                prompt_chars: expanded.chars().count(),
            });
            // C3: the command's tools narrow the gate on top of its rules, never in their place.
            if !allowed_tools.is_empty() {
                built.gate.restrict_name = Some(name);
                built.gate.restrict = Some(allowed_tools);
            }
            prompt = expanded;
            // The command's own model, where it names one (C2).
            if let Some(model) = model {
                endpoint.model = model;
            }
            built.gate
        }
    };
    let mut run_config = RunConfig::new(&cwd);
    if let Some(turns) = max_turns {
        run_config.max_turns = turns;
    }
    // HS7: the owner's hooks (B1, the only ones that ever run), and the sandbox their commands run in (D5).
    run_config.hooks = built.hooks;
    run_config.sandbox = Some(built.sandbox.clone());
    // A skill as the run (HS6, K1.1): its body is the run's instructions, its frontmatter may narrow the tools,
    // pick the model and set the time limit (K5). After the commands, so a command's own word wins where both
    // name one.
    if let Some(name) = skill {
        let Some(chosen) = catalog.get(&name) else {
            bail!("no skill named {name:?}; known: {}", catalog.known());
        };
        chosen.start_run(&cwd, &mut run_config, &mut endpoint, &mut built.gate)?;
        sink.send(Event::Skill {
            name,
            how: SkillHow::Run,
        });
        if prompt.trim().is_empty() {
            prompt = skills::DEFAULT_PROMPT.to_owned();
        }
    }
    // The model is built after the commands and the skill: one of them may name its own.
    let model = OpenAiCompat::new(endpoint)?;
    tools.set_gate(Arc::new(built.gate));
    // MCP servers (HS5), after the gate and before the shell (M1–M3): a project's server the owner has not
    // approved yet is asked about in the terminal, or left out with a note when there is none (M2).
    let entries = mcp::config::load(&built.home, &cwd)?;
    let entries =
        mcp::approval::permitted(entries, &built.home, &cwd, mcp::approval::interactive())?;
    tools.extend(mcp::start(entries, &mut sink).await);
    // The shell after the gate: the settings that built it also say how the sandbox runs (HS3b).
    tools.extend(shell_tools(
        &cwd,
        ShellOptions {
            sandbox: Some(built.sandbox),
            ..ShellOptions::from_environment()
        },
    )?);
    // Ctrl-C, SIGTERM and SIGHUP end the run with its last line and every background job stopped.
    let cancel = Cancel::new();
    axiomata_harness::run::cancel_on_signals(&cancel);
    let outcome =
        axiomata_harness::run(&run_config, &model, &tools, &prompt, &mut sink, &cancel).await;
    match outcome.reason {
        FinishReason::Answered => Ok(()),
        other => bail!("the run ended: {other:?}"),
    }
}

/// What the run's gate was built with (HS3a), for the caller that still needs it: the settings' word on the sandbox
/// (HS3b), the home folder and whether the project counts as trusted — the command registry needs the same trust
/// the gate was built on (HS4, C1) — and the owner's hooks (HS7).
struct Built {
    gate: Gate,
    sandbox: SandboxSettings,
    home: PathBuf,
    trusted: bool,
    hooks: Hooks,
}

/// The run's gate (HS3a): the settings files, the flags and a role's rules; asks in the terminal when there is one.
/// Also what the settings say about the sandbox (HS3b), and the home folder and trust the command registry of HS4
/// is read with — a project's commands count where its allow rules count (C1).
/// A project with `.axh/` settings in a folder not yet trusted is asked about first (P5).
fn gate(
    cwd: &std::path::Path,
    mode: Option<String>,
    mut allow: Vec<String>,
    ask: Vec<String>,
    deny: Vec<String>,
    role: Option<String>,
) -> Result<Built> {
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
            eprintln!("{folder} has its own harness settings (.axh/). Trusting it adds:");
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
    let (gate, untrusted, sandbox, hooks, project_hooks) = build_gate(&home, cwd, options, asker)?;
    if untrusted {
        eprintln!(
            "note: the project's .axh/ allow rules and mode are left out: this folder is not trusted"
        );
    }
    // HS7, B1: the owner's hooks run; a project's are parsed and left out, and said so here.
    if project_hooks {
        eprintln!(
            "note: the project's own .axh/ hooks are not run: only ~/.axh/settings.json hooks run"
        );
    }
    if !sandbox.enabled || !axiomata_harness::sandbox::available() {
        eprintln!("note: commands run without a sandbox here; only the rules judge them");
    }
    Ok(Built {
        gate,
        sandbox,
        home,
        trusted,
        hooks,
    })
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
struct Readable;

impl EventSink for Readable {
    fn emit(&mut self, event: Event) {
        let line = match event {
            Event::Started { model, cwd, tools } => {
                format!("▸ {} in {cwd} · {} tools", plain(&model), tools.len())
            }
            // The command a prompt came from, before the run's own lines (HS4).
            Event::Command { name, source, .. } => format!("/{name} ({source})"),
            Event::AssistantText { text, .. } => format!("\n{}\n", plain(&text)),
            Event::ToolCall {
                name, arguments, ..
            } => format!("→ {} {}", plain(&name), preview(&arguments)),
            Event::ToolResult { ok, output, .. } => {
                format!("  {} {}", if ok { "✓" } else { "✗" }, preview(&output))
            }
            Event::Usage { .. } => return,
            Event::Permission {
                allowed, reason, ..
            } => {
                format!("  {} {}", if allowed { "·" } else { "⊘" }, plain(&reason))
            }
            // An MCP server of this run, connected or failed (HS5).
            Event::McpServer {
                name,
                status,
                tools,
                reason,
            } => match status {
                McpStatus::Connected => format!("◇ mcp {} · {tools} tools", plain(&name)),
                McpStatus::Failed => format!(
                    "◇ mcp {} ✗ {}",
                    plain(&name),
                    plain(reason.as_deref().unwrap_or("it did not start"))
                ),
            },
            // A skill of this run: as the run itself (K1.1) or loaded by the model (K1.2).
            Event::Skill { name, how } => {
                let how = match how {
                    SkillHow::Run => "as the run",
                    SkillHow::Loaded => "loaded",
                };
                format!("✳ skill {} · {how}", plain(&name))
            }
            // A hook of this run (HS7): its event, whether it blocked, and what it had to say.
            Event::Hook {
                hook_event,
                blocked,
                note,
                ..
            } => format!(
                "△ hook {hook_event}{} · {}",
                if blocked { " ⊘" } else { "" },
                plain(note.as_deref().unwrap_or("ran"))
            ),
            Event::Error { message } => {
                eprintln!("error: {}", plain(&message));
                return;
            }
            Event::Finished {
                reason,
                turns,
                usage,
            } => format!(
                "■ {reason:?} · {turns} turns · {} in / {} out tokens",
                usage.input_tokens, usage.output_tokens
            ),
        };
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "{line}");
        let _ = out.flush();
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
}
