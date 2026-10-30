//! `axiomata-cli ide models …` (`docs/plans/agents-model.md`, AM1): which models are switched on, their tiers, the
//! price limits of the price proposal and the default engine per tier — the same data the Studio's Models tab shows.

use anyhow::{Result, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::config::Config;
use axiomata_core::model_tiers::{self, ModelChoice, ModelSources, TierLimits, TierSource};
use axiomata_core::{model_catalog, model_research};
use axiomata_roster::Tier;
use clap::{ArgGroup, Subcommand};

#[derive(Debug, Subcommand)]
pub enum ModelAction {
    /// List every model the providers offer: on or off, its tier and where the tier comes from, its price.
    List {
        /// Print JSON instead of a table.
        #[arg(long)]
        json: bool,
        /// Only what tier research is for — switched on, no research proposal, no tier of yours unless you asked
        /// again — with the price limits (what the `model-tiering` skill reads).
        #[arg(long)]
        unrated: bool,
    },
    /// Record a research proposal for a model's tier (the `model-tiering` skill's step; never your own value).
    Propose {
        /// The model key, as `ide models list --unrated` prints it.
        key: String,
        /// light | medium | heavy.
        #[arg(long)]
        tier: String,
        /// One sentence naming the evidence.
        #[arg(long)]
        reason: String,
        /// The web address the evidence comes from.
        #[arg(long)]
        source: String,
    },
    /// Switch a model on or off, or set or clear its tier. A flag left out keeps the current value.
    #[command(group(ArgGroup::new("switch").args(["enable", "disable"])))]
    #[command(group(ArgGroup::new("rating").args(["tier", "clear_tier"])))]
    Set {
        /// The model key: `anthropic/<id>`, `openrouter/<id>` or `ollama/<id>` (as `ide models list` prints it).
        key: String,
        #[arg(long)]
        enable: bool,
        #[arg(long)]
        disable: bool,
        /// light | medium | heavy — the owner's value, which beats every proposal.
        #[arg(long)]
        tier: Option<String>,
        /// Drop the owner's tier, so the proposals apply again.
        #[arg(long)]
        clear_tier: bool,
    },
    /// Show the price limits of the price proposal, or change them (USD per million output tokens).
    Limits {
        /// Cheaper than this is light.
        #[arg(long)]
        light_below: Option<f64>,
        /// Dearer than this is heavy.
        #[arg(long)]
        heavy_above: Option<f64>,
    },
    /// Show the default engine of a tier, or set or clear it.
    #[command(group(ArgGroup::new("change").args(["engine", "clear"])))]
    Default {
        /// light | medium | heavy.
        #[arg(long)]
        tier: String,
        /// An engine id (own or automatic, as `ide engines list` prints it).
        #[arg(long)]
        engine: Option<String>,
        /// Back to "the cheapest engine of the tier".
        #[arg(long)]
        clear: bool,
    },
}

impl ModelAction {
    /// Whether the action is no owner's decision: it reads, or it records a research proposal (which never becomes
    /// the owner's value). Open beyond the owner's terminal; `propose` is still closed to an agent session (in [`run`])
    /// and to a chat turn (`chat_turn_refusal`).
    pub fn reads_only(&self) -> bool {
        match self {
            ModelAction::List { .. } | ModelAction::Propose { .. } => true,
            ModelAction::Set { .. } => false,
            ModelAction::Limits {
                light_below,
                heavy_above,
            } => light_below.is_none() && heavy_above.is_none(),
            ModelAction::Default { engine, clear, .. } => engine.is_none() && !clear,
        }
    }
}

/// Refuses a tier proposal in an agent session: a studio card session works on its card, and the owner's model list
/// is none of its business.
fn not_in_a_session(session: Option<&str>) -> Result<()> {
    if let Some(own) = session {
        bail!("{own} does not propose model tiers; the `model-tiering` skill does");
    }
    Ok(())
}

pub fn parse_tier(raw: &str) -> Result<Tier> {
    Tier::parse(raw)
        .ok_or_else(|| anyhow::anyhow!("unknown tier {raw:?} — expected light, medium or heavy"))
}

fn source_name(source: TierSource) -> &'static str {
    match source {
        TierSource::Owner => "owner",
        TierSource::Research => "research",
        TierSource::Price => "price",
        TierSource::Unrated => "unrated",
    }
}

fn snapshot(core: &AxiomataCore) -> Config {
    core.config
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

/// Stores `config`'s model settings back into the live config after a setter changed the file.
fn adopt(core: &AxiomataCore, config: Config) {
    core.config
        .write()
        .unwrap_or_else(|poison| poison.into_inner())
        .agents
        .models = config.agents.models;
}

pub fn run(core: &AxiomataCore, action: ModelAction) -> Result<()> {
    match action {
        ModelAction::List {
            json,
            unrated: true,
        } => {
            let todo = model_research::todo(&snapshot(core), &ModelSources::load());
            if json {
                println!("{}", serde_json::to_string_pretty(&todo)?);
                return Ok(());
            }
            for model in &todo.models {
                println!(
                    "{:<52} {:<16} price tier {}",
                    model.key,
                    model
                        .output_per_m
                        .map_or_else(|| "no price".to_owned(), |p| format!("out {p} $/M")),
                    model.price_tier.map_or("-", Tier::as_str),
                );
            }
            Ok(())
        }
        ModelAction::Propose {
            key,
            tier,
            reason,
            source,
        } => {
            not_in_a_session(axiomata_core::session::session_actor().as_deref())?;
            let tier = parse_tier(&tier)?;
            model_research::propose(&model_catalog::load(), &key, tier, &reason, &source)?;
            println!("proposed {} for {key}", tier.as_str());
            Ok(())
        }
        ModelAction::List {
            json,
            unrated: false,
        } => {
            let config = snapshot(core);
            let models = model_tiers::models(&config, &ModelSources::load());
            if json {
                println!("{}", serde_json::to_string_pretty(&models)?);
                return Ok(());
            }
            for model in &models {
                let price = model.price.as_ref().map_or_else(
                    || "no price".to_owned(),
                    |price| format!("in {} / out {} $/M", price.input_per_m, price.output_per_m),
                );
                println!(
                    "{:<52} {:<3} {:<7} {:<9} {}",
                    model.key,
                    if model.enabled { "on" } else { "off" },
                    model.tier.map_or("-", Tier::as_str),
                    source_name(model.tier_source),
                    price,
                );
            }
            Ok(())
        }
        ModelAction::Set {
            key,
            enable,
            disable,
            tier,
            clear_tier,
        } => {
            let tier = tier.as_deref().map(parse_tier).transpose()?;
            let mut config = snapshot(core);
            // Only the fields asked for change, on the file config as it is now (read under its lock).
            model_tiers::edit_choice(&mut config, &key, |choice: &mut ModelChoice| {
                if enable || disable {
                    choice.enabled = Some(enable);
                }
                if tier.is_some() {
                    choice.tier = tier;
                } else if clear_tier {
                    choice.tier = None;
                }
            })?;
            adopt(core, config);
            println!("updated model {key}");
            Ok(())
        }
        ModelAction::Limits {
            light_below,
            heavy_above,
        } => {
            let mut config = snapshot(core);
            let current = config.agents.models.tier_limits;
            if light_below.is_some() || heavy_above.is_some() {
                let limits = TierLimits {
                    light_below: light_below.unwrap_or(current.light_below),
                    heavy_above: heavy_above.unwrap_or(current.heavy_above),
                };
                model_tiers::save_tier_limits(&mut config, limits)?;
                adopt(core, config.clone());
            }
            let limits = config.agents.models.tier_limits;
            println!(
                "light below {} $/M output, heavy above {} $/M output",
                limits.light_below, limits.heavy_above
            );
            Ok(())
        }
        ModelAction::Default {
            tier,
            engine,
            clear,
        } => {
            let tier = parse_tier(&tier)?;
            let mut config = snapshot(core);
            if engine.is_some() || clear {
                if engine.as_deref().is_some_and(|id| id.trim().is_empty()) {
                    bail!("--engine needs an engine id; use --clear for none");
                }
                model_tiers::save_default_engine(&mut config, tier, engine)?;
                adopt(core, config.clone());
            }
            match config.agents.models.default_engines.get(tier) {
                Some(id) => println!("default engine of {}: {id}", tier.as_str()),
                None => println!(
                    "default engine of {}: none (the cheapest of the tier)",
                    tier.as_str()
                ),
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_change_needs_the_owner() {
        assert!(
            ModelAction::List {
                json: false,
                unrated: false
            }
            .reads_only()
        );
        assert!(
            ModelAction::Propose {
                key: "anthropic/x".into(),
                tier: "light".into(),
                reason: "r".into(),
                source: "https://x".into(),
            }
            .reads_only(),
            "a proposal is no owner's decision"
        );
        let limits = |light_below| ModelAction::Limits {
            light_below,
            heavy_above: None,
        };
        assert!(limits(None).reads_only());
        assert!(!limits(Some(0.5)).reads_only());
        let default = |engine: Option<&str>, clear| ModelAction::Default {
            tier: "light".into(),
            engine: engine.map(String::from),
            clear,
        };
        assert!(default(None, false).reads_only());
        assert!(!default(Some("e"), false).reads_only());
        assert!(!default(None, true).reads_only());
        assert!(
            !ModelAction::Set {
                key: "anthropic/x".into(),
                enable: true,
                disable: false,
                tier: None,
                clear_tier: false,
            }
            .reads_only()
        );
        assert!(parse_tier("heavy").is_ok() && parse_tier("Heavy").is_err());
        assert!(not_in_a_session(Some("agent:allrounder-12")).is_err());
        assert!(not_in_a_session(None).is_ok());
    }
}
