//! `axiomata-cli harness …`: the own harness (`axiomata-harness`, `docs/plans/harness.md`) with a provider from
//! `~/.axiomata/config.toml` — its base URL, key and skill model (H5). The way to try the harness before it has a
//! place in the app.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::config::{Config, ProviderId};
use axiomata_harness::tools::files::file_tools;
use axiomata_harness::{
    Cancel, Event, EventSink, FinishReason, ModelEndpoint, OpenAiCompat, RunConfig, Toolbox,
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
        prompt: String,
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
        prompt,
    } = action;
    // It spends the owner's money with the owner's key: not something an agent session starts by itself.
    owner_only("running the own harness")?;
    let config = crate::read_config(core);
    let endpoint = endpoint(&config, provider, model)?;
    let cwd = cwd
        .canonicalize()
        .with_context(|| format!("cannot use {}", cwd.display()))?;
    let model = OpenAiCompat::new(endpoint)?;
    let mut tools = Toolbox::new();
    tools.extend(file_tools(&cwd)?);
    let mut run_config = RunConfig::new(&cwd);
    if let Some(turns) = max_turns {
        run_config.max_turns = turns;
    }
    let mut sink: Box<dyn EventSink> = if json {
        Box::new(JsonLines)
    } else {
        Box::new(Readable)
    };
    // Ctrl-C ends the run with its last line instead of killing it mid-step.
    let cancel = Cancel::new();
    let on_signal = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            on_signal.cancel();
        }
    });
    let outcome =
        axiomata_harness::run(&run_config, &model, &tools, &prompt, sink.as_mut(), &cancel).await;
    match outcome.reason {
        FinishReason::Answered => Ok(()),
        other => bail!("the run ended: {other:?}"),
    }
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
            Event::AssistantText { text, .. } => format!("\n{}\n", plain(&text)),
            Event::ToolCall {
                name, arguments, ..
            } => format!("→ {} {}", plain(&name), preview(&arguments)),
            Event::ToolResult { ok, output, .. } => {
                format!("  {} {}", if ok { "✓" } else { "✗" }, preview(&output))
            }
            Event::Usage { .. } => return,
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
