//! Research of model tiers (`docs/plans/agents-model.md`, AM2, Q2, Q11): the bundled skill [`SKILL`] looks up how a
//! model stands on rating sites and records one tier proposal per model through `axiomata-cli ide models propose` —
//! a proposal, never the owner's value.
//!
//! What a run works on is [`todo`]: the switched-on models without a research proposal that have no tier of the
//! owner's — or that the owner asked about again. A run starts by itself when the catalog holds such models
//! ([`claim_due`]), at most once a day per model: the time research was started for a model is kept beside the
//! proposals (`model-tiers.json`), so a run that fails does not start again and again. The owner's "propose again"
//! ([`request`]) forgets the models' proposals and asks for them at once.
//!
//! Every run goes through the ordinary skill path (`skills::execute_and_record_skill`: the spend guard, a recorded
//! run, the skill harness); nothing here starts a process.

use axiomata_roster::Tier;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use crate::config::Config;
use crate::error::AxiomataError;
use crate::model_catalog::ModelCatalog;
use crate::model_tiers::{self, ModelSources, TierLimits, TierProposal, TierProposals};
use crate::roster::refusal;

type Result<T> = std::result::Result<T, AxiomataError>;

/// The bundled skill that researches tiers.
pub const SKILL: &str = "model-tiering";

/// How long after research was started for a model it may start by itself again.
pub const RETRY_AFTER_HOURS: i64 = 24;

/// One model a research run rates.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResearchItem {
    pub key: String,
    pub name: String,
    /// USD per million output tokens, when the model has a price.
    pub output_per_m: Option<f64>,
    pub context_length: Option<u64>,
    /// What the price alone proposes.
    pub price_tier: Option<Tier>,
}

/// What a research run reads: the models and the price limits (`ide models list --unrated --json`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResearchTodo {
    pub tier_limits: TierLimits,
    pub models: Vec<ResearchItem>,
}

/// The models research is for: switched on, no research proposal, and no tier of the owner's unless the owner asked.
pub fn todo(config: &Config, sources: &ModelSources) -> ResearchTodo {
    let models = model_tiers::models(config, sources)
        .into_iter()
        .filter(|model| {
            model.enabled
                && model.proposal.is_none()
                && (model.owner_tier.is_none() || sources.proposals.wanted.contains(&model.key))
        })
        .map(|model| {
            let context_length = sources
                .catalog
                .providers
                .get(&model.provider)
                .and_then(|listed| listed.models.iter().find(|m| m.id == model.id))
                .and_then(|listed| listed.context_length);
            ResearchItem {
                output_per_m: model.price.as_ref().map(|price| price.output_per_m),
                price_tier: model.price_tier,
                context_length,
                name: model.short_name,
                key: model.key,
            }
        })
        .collect();
    ResearchTodo {
        tier_limits: config.agents.models.tier_limits,
        models,
    }
}

/// The keys of `todo` whose research has not started within [`RETRY_AFTER_HOURS`] of `now`.
fn due_keys(todo: &ResearchTodo, proposals: &TierProposals, now: DateTime<Utc>) -> Vec<String> {
    let retry = Duration::hours(RETRY_AFTER_HOURS);
    todo.models
        .iter()
        .filter(|model| {
            proposals
                .attempts
                .get(&model.key)
                .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                .is_none_or(|at| now.signed_duration_since(at) >= retry)
        })
        .map(|model| model.key.clone())
        .collect()
}

/// The models a research run should start for by itself now, marked as attempted at `now` in the same write; empty
/// when nothing is due — then no run starts.
///
/// # Errors
///
/// A failing write of `model-tiers.json`.
pub fn claim_due(
    config: &Config,
    catalog: &ModelCatalog,
    now: DateTime<Utc>,
) -> Result<Vec<String>> {
    model_tiers::edit_proposals(|proposals| {
        let sources = ModelSources {
            catalog: catalog.clone(),
            proposals: proposals.clone(),
        };
        let due = due_keys(&todo(config, &sources), proposals, now);
        for key in &due {
            proposals.attempts.insert(key.clone(), now.to_rfc3339());
        }
        Ok(due)
    })
}

/// The owner's "propose again": forgets the proposals of `keys` (every switched-on model when `None`), asks for them
/// again even where the owner set a tier, and marks them attempted at `now`. Returns the keys asked for.
///
/// # Errors
///
/// A refusal for a key the catalog does not offer; a failing write.
pub fn request(
    config: &Config,
    catalog: &ModelCatalog,
    keys: Option<&[String]>,
    now: DateTime<Utc>,
) -> Result<Vec<String>> {
    let sources = ModelSources {
        catalog: catalog.clone(),
        proposals: TierProposals::default(),
    };
    let models = model_tiers::models(config, &sources);
    let keys: Vec<String> = match keys {
        Some(keys) => {
            if let Some(unknown) = keys
                .iter()
                .find(|key| !models.iter().any(|m| &m.key == *key))
            {
                return Err(refusal(
                    "model",
                    format!("the catalog offers no model “{unknown}”"),
                ));
            }
            keys.to_vec()
        }
        None => models
            .iter()
            .filter(|model| model.enabled)
            .map(|model| model.key.clone())
            .collect(),
    };
    model_tiers::edit_proposals(|proposals| {
        for key in &keys {
            proposals.proposals.remove(key);
            proposals.wanted.insert(key.clone());
            proposals.attempts.insert(key.clone(), now.to_rfc3339());
        }
        Ok(keys.clone())
    })
}

/// Records what a research run found for `key` (`ide models propose`): the key must be a model the catalog offers,
/// the source a web address, the texts bounded (as [`model_tiers::record_proposal`] checks).
///
/// # Errors
///
/// A refusal naming what is wrong; a failing write.
pub fn propose(
    catalog: &ModelCatalog,
    key: &str,
    tier: Tier,
    reason: &str,
    source: &str,
) -> Result<()> {
    let Some((provider, id)) = model_tiers::parse_model_key(key) else {
        return Err(refusal("model", format!("“{key}” is no model key")));
    };
    if !model_tiers::offered(catalog, provider)
        .iter()
        .any(|model| model.id == id)
    {
        return Err(refusal(
            "model",
            format!("the catalog offers no model “{key}”"),
        ));
    }
    let source = source.trim();
    let is_url = (source.starts_with("https://") || source.starts_with("http://"))
        && !source.contains(char::is_whitespace);
    if !is_url {
        return Err(refusal(
            "source",
            "the web address the evidence comes from (https://…)".to_owned(),
        ));
    }
    model_tiers::record_proposal(
        key,
        TierProposal {
            tier,
            reason: reason.trim().to_owned(),
            source: source.to_owned(),
            at: Utc::now().to_rfc3339(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_tiers::{MAX_MODEL_NAME_CHARS, ModelChoice};
    use crate::paths;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};

    fn with_temp_home(body: impl FnOnce()) {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        let home = unique_temp_dir("axiomata-test-model-research");
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: serialised by ENV_MUTEX.
        unsafe {
            std::env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }
        body();
        unsafe {
            std::env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    fn config() -> Config {
        Config {
            workspace_root: std::env::temp_dir().join("axiomata-model-research-test-workspace"),
            ..Config::default()
        }
    }

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn research_starts_once_per_model_and_again_only_after_a_day() {
        with_temp_home(|| {
            let catalog = ModelCatalog::default();
            let morning = at("2026-10-30T08:00:00Z");
            // The four offered Anthropic models have a price proposal only: all of them are due.
            let first = claim_due(&config(), &catalog, morning).unwrap();
            assert_eq!(first.len(), 4);
            // Attempted: a failing run does not loop.
            assert!(
                claim_due(&config(), &catalog, at("2026-10-30T20:00:00Z"))
                    .unwrap()
                    .is_empty()
            );
            // A proposal takes a model off; the rest come back after a day.
            propose(
                &catalog,
                "anthropic/claude-haiku-5-5",
                Tier::Light,
                "Cheapest Anthropic model, ranks with the small ones.",
                "https://artificialanalysis.ai/models",
            )
            .unwrap();
            let next = claim_due(&config(), &catalog, at("2026-10-31T08:00:00Z")).unwrap();
            assert_eq!(next.len(), 3);
            assert!(!next.contains(&"anthropic/claude-haiku-5-5".to_owned()));
        });
    }

    #[test]
    fn a_model_with_the_owners_tier_is_researched_only_when_asked_again() {
        with_temp_home(|| {
            let catalog = ModelCatalog::default();
            let mut config = config();
            config.agents.models.choices.insert(
                "anthropic/claude-opus-5-5".into(),
                ModelChoice {
                    enabled: None,
                    tier: Some(Tier::Heavy),
                },
            );
            let sources = |proposals| ModelSources {
                catalog: catalog.clone(),
                proposals,
            };
            let keys =
                |todo: ResearchTodo| todo.models.into_iter().map(|m| m.key).collect::<Vec<_>>();
            assert!(
                !keys(todo(&config, &sources(model_tiers::load_proposals())))
                    .contains(&"anthropic/claude-opus-5-5".to_owned())
            );

            let now = at("2026-10-30T08:00:00Z");
            let asked = request(
                &config,
                &catalog,
                Some(&["anthropic/claude-opus-5-5".into()]),
                now,
            )
            .unwrap();
            assert_eq!(asked, ["anthropic/claude-opus-5-5"]);
            let todo_now = todo(&config, &sources(model_tiers::load_proposals()));
            let opus = todo_now
                .models
                .iter()
                .find(|m| m.key == "anthropic/claude-opus-5-5")
                .unwrap();
            assert_eq!(
                (opus.output_per_m, opus.price_tier),
                (Some(20.0), Some(Tier::Heavy))
            );
            assert!(opus.name.chars().count() <= MAX_MODEL_NAME_CHARS);
            assert!(request(&config, &catalog, Some(&["openrouter/none".into()]), now).is_err());
            // None asks for every model that is on.
            assert_eq!(request(&config, &catalog, None, now).unwrap().len(), 4);
        });
    }

    #[test]
    fn a_proposal_names_an_offered_model_and_a_web_source() {
        with_temp_home(|| {
            let catalog = ModelCatalog::default();
            let ok = |key: &str, source: &str| {
                propose(&catalog, key, Tier::Medium, "Mid of the board.", source).is_ok()
            };
            assert!(ok(
                "anthropic/claude-sonnet-5-5",
                "https://lmarena.ai/leaderboard"
            ));
            assert!(
                !ok("anthropic/claude-sonnet-5", "https://lmarena.ai"),
                "not offered"
            );
            assert!(
                !ok("openrouter/x/y", "https://lmarena.ai"),
                "not in the catalog"
            );
            assert!(
                !ok("anthropic/claude-opus-5-5", "LMArena"),
                "no web address"
            );
            assert!(!ok("anthropic/claude-opus-5-5", "https://a b"));
            assert!(
                propose(
                    &catalog,
                    "anthropic/claude-opus-5-5",
                    Tier::Heavy,
                    "",
                    "https://x.org"
                )
                .is_err(),
                "a reason is needed"
            );
            let stored = model_tiers::load_proposals();
            assert_eq!(stored.proposals.len(), 1);
            assert!(stored.wanted.is_empty());
        });
    }
}
