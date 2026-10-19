//! `axiomata-cli models …`: the providers' model catalogs (board card #17), the same refresh the app runs at start
//! and once a day.

use anyhow::{Result, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::config::ProviderId;
use axiomata_core::model_catalog;
use clap::Subcommand;

use crate::board_flow;

#[derive(Debug, Subcommand)]
pub enum ModelsAction {
    /// Fetch the model lists and write OpenRouter's prices into `agents.costs` for every model in use.
    Refresh {
        /// Only this provider: `open_router` or `ollama` (Anthropic's list is built in).
        #[arg(long)]
        provider: Option<String>,
    },
    /// Print the stored catalog, one model per line.
    List {
        /// Only this provider: `anthropic`, `open_router` or `ollama`.
        #[arg(long)]
        provider: Option<String>,
    },
}

pub async fn run(core: &AxiomataCore, action: ModelsAction) -> Result<()> {
    match action {
        ModelsAction::Refresh { provider } => {
            // It rewrites prices in the owner's config, which is not an agent's to decide.
            board_flow::owner_only("refreshing the model catalog")?;
            let only = provider.as_deref().map(parse_provider).transpose()?;
            let outcome = model_catalog::refresh_and_price(&core.config, only).await;
            for (id, catalog) in &outcome.catalog.providers {
                match &catalog.error {
                    Some(err) => println!(
                        "{}: {} models (kept), fetch failed: {err}",
                        id.as_str(),
                        catalog.models.len()
                    ),
                    None => println!("{}: {} models", id.as_str(), catalog.models.len()),
                }
            }
            if let Some(err) = outcome.price_error {
                bail!("the prices could not be written: {err}");
            }
            match outcome.repriced.as_slice() {
                [] => println!("prices: unchanged"),
                repriced => println!("prices updated: {}", repriced.join(", ")),
            }
        }
        ModelsAction::List { provider } => {
            let only = provider.as_deref().map(parse_provider).transpose()?;
            for (id, catalog) in model_catalog::load().providers {
                if only.is_some_and(|wanted| wanted != id) {
                    continue;
                }
                for model in catalog.models {
                    match model.price {
                        Some(p) => println!(
                            "{}\t{}\tin {} / out {} $/M",
                            id.as_str(),
                            model.id,
                            p.input_per_m,
                            p.output_per_m
                        ),
                        None => println!("{}\t{}", id.as_str(), model.id),
                    }
                }
            }
        }
    }
    Ok(())
}

/// Takes the names `ProviderId::as_str` prints, plus `openrouter` as Opencode spells it.
fn parse_provider(name: &str) -> Result<ProviderId> {
    match name {
        "anthropic" => Ok(ProviderId::Anthropic),
        "open_router" | "openrouter" => Ok(ProviderId::OpenRouter),
        "ollama" => Ok(ProviderId::Ollama),
        other => bail!("unknown provider {other:?}: anthropic, open_router or ollama"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_names_parse_as_they_are_printed() {
        for id in ProviderId::ALL {
            assert_eq!(parse_provider(id.as_str()).unwrap(), id);
        }
        assert_eq!(
            parse_provider("openrouter").unwrap(),
            ProviderId::OpenRouter
        );
        assert!(parse_provider("OpenRouter").is_err());
    }
}
