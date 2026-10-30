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

/// The models research is for: switched on, and either asked about again by the owner, or with neither a research
/// proposal nor a tier of the owner's.
pub fn todo(config: &Config, sources: &ModelSources) -> ResearchTodo {
    let models = model_tiers::models(config, sources)
        .into_iter()
        .filter(|model| {
            model.enabled
                && (sources.proposals.wanted.contains(&model.key)
                    || (model.proposal.is_none() && model.owner_tier.is_none()))
        })
        .map(|model| ResearchItem {
            output_per_m: model.price.as_ref().map(|price| price.output_per_m),
            price_tier: model.price_tier,
            context_length: model.context_length,
            name: model.short_name,
            key: model.key,
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

/// The models a run was started for, and what marking them changed — so a run that could not start is taken back
/// ([`unmark`]) and one that ended lets go of what it did not answer ([`finish`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marked {
    pub keys: Vec<String>,
    /// Each key's attempt time before the mark.
    previous_attempts: Vec<(String, Option<String>)>,
    /// The keys the mark made wanted that were not before.
    added_wanted: Vec<String>,
}

impl Marked {
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// Marks `keys` attempted at `now` (and wanted, when `want`) in `proposals`.
fn mark(
    proposals: &mut TierProposals,
    keys: Vec<String>,
    want: bool,
    now: DateTime<Utc>,
) -> Marked {
    let mut marked = Marked::default();
    for key in &keys {
        let before = proposals.attempts.insert(key.clone(), now.to_rfc3339());
        marked.previous_attempts.push((key.clone(), before));
        if want && proposals.wanted.insert(key.clone()) {
            marked.added_wanted.push(key.clone());
        }
    }
    marked.keys = keys;
    marked
}

/// The models a research run should start for by itself now, marked as attempted at `now` in the same write; empty
/// when nothing is due — then no run starts.
///
/// # Errors
///
/// A failing write of `model-tiers.json`.
pub fn claim_due(config: &Config, catalog: &ModelCatalog, now: DateTime<Utc>) -> Result<Marked> {
    model_tiers::edit_proposals(|proposals| {
        let sources = ModelSources {
            catalog: catalog.clone(),
            proposals: proposals.clone(),
        };
        let due = due_keys(&todo(config, &sources), proposals, now);
        Ok(mark(proposals, due, false, now))
    })
}

/// The owner's "propose again": asks for `keys` (every switched-on model when `None`) again, even where the owner
/// set a tier, and marks them attempted at `now`. Their proposals stay until the run records new ones.
///
/// # Errors
///
/// A refusal for a key the catalog does not offer; a failing write.
pub fn request(
    config: &Config,
    catalog: &ModelCatalog,
    keys: Option<&[String]>,
    now: DateTime<Utc>,
) -> Result<Marked> {
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
    model_tiers::edit_proposals(|proposals| Ok(mark(proposals, keys, true, now)))
}

/// Takes back a mark whose run could not start: the attempts as they were, the keys it made wanted no longer wanted.
///
/// # Errors
///
/// A failing write.
pub fn unmark(marked: &Marked) -> Result<()> {
    model_tiers::edit_proposals(|proposals| {
        for (key, before) in &marked.previous_attempts {
            match before {
                Some(at) => proposals.attempts.insert(key.clone(), at.clone()),
                None => proposals.attempts.remove(key),
            };
        }
        for key in &marked.added_wanted {
            proposals.wanted.remove(key);
        }
        Ok(())
    })
}

/// After a run for `marked` ended: a key it left without a proposal is no longer wanted — so a model with the owner's
/// tier is not researched again every day — while its attempt time keeps a failed one from starting again at once.
///
/// # Errors
///
/// A failing write.
pub fn finish(marked: &Marked) -> Result<()> {
    model_tiers::edit_proposals(|proposals| {
        for key in &marked.keys {
            proposals.wanted.remove(key);
        }
        Ok(())
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

/// [`propose`] as a skill run may: only for a model research is for ([`todo`]) — a run cannot overwrite the proposal
/// of a model nobody asked about.
///
/// # Errors
///
/// A refusal for a model not in the todo; as [`propose`].
pub fn propose_due(
    config: &Config,
    sources: &ModelSources,
    key: &str,
    tier: Tier,
    reason: &str,
    source: &str,
) -> Result<()> {
    if !todo(config, sources)
        .models
        .iter()
        .any(|model| model.key == key)
    {
        return Err(refusal(
            "model",
            format!("“{key}” is not waiting for research (see `ide models list --unrated`)"),
        ));
    }
    propose(&sources.catalog, key, tier, reason, source)
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
            assert_eq!(first.keys.len(), 4);
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
            assert_eq!(next.keys.len(), 3);
            assert!(!next.keys.contains(&"anthropic/claude-haiku-5-5".to_owned()));
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
            assert_eq!(asked.keys, ["anthropic/claude-opus-5-5"]);
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
            assert_eq!(request(&config, &catalog, None, now).unwrap().keys.len(), 4);
        });
    }

    #[test]
    fn asking_again_keeps_the_proposal_until_a_new_one_and_a_run_without_one_lets_go() {
        with_temp_home(|| {
            let catalog = ModelCatalog::default();
            let mut config = config();
            let opus = "anthropic/claude-opus-5-5";
            config.agents.models.choices.insert(
                opus.into(),
                ModelChoice {
                    enabled: None,
                    tier: Some(Tier::Heavy),
                },
            );
            propose(
                &catalog,
                opus,
                Tier::Heavy,
                "Top of the board.",
                "https://lmarena.ai",
            )
            .unwrap();
            let marked = request(
                &config,
                &catalog,
                Some(&[opus.into()]),
                at("2026-10-30T08:00:00Z"),
            )
            .unwrap();
            let stored = model_tiers::load_proposals();
            assert!(
                stored.proposals.contains_key(opus),
                "kept until a new one is recorded"
            );
            assert!(stored.wanted.contains(opus));
            // The run ended without a proposal: no longer wanted, so the owner-tiered model is left alone.
            finish(&marked).unwrap();
            let sources = ModelSources {
                catalog: catalog.clone(),
                proposals: model_tiers::load_proposals(),
            };
            assert!(!todo(&config, &sources).models.iter().any(|m| m.key == opus));
            assert!(
                claim_due(&config, &catalog, at("2026-11-05T08:00:00Z"))
                    .unwrap()
                    .keys
                    .iter()
                    .all(|k| k != opus)
            );
        });
    }

    #[test]
    fn a_mark_whose_run_could_not_start_is_taken_back() {
        with_temp_home(|| {
            let catalog = ModelCatalog::default();
            let first = claim_due(&config(), &catalog, at("2026-10-30T08:00:00Z")).unwrap();
            unmark(&first).unwrap();
            // Nothing attempted: the next look finds them all due again at once.
            let again = claim_due(&config(), &catalog, at("2026-10-30T08:05:00Z")).unwrap();
            assert_eq!(again.keys, first.keys);
            let asked = request(&config(), &catalog, None, at("2026-10-30T09:00:00Z")).unwrap();
            unmark(&asked).unwrap();
            let stored = model_tiers::load_proposals();
            assert!(stored.wanted.is_empty());
            assert!(
                stored
                    .attempts
                    .values()
                    .all(|at| at.starts_with("2026-10-30T08:05"))
            );
        });
    }

    #[test]
    fn a_skill_run_proposes_only_for_what_waits_for_research() {
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
            let sources = ModelSources {
                catalog,
                proposals: model_tiers::load_proposals(),
            };
            let due = |key: &str| {
                propose_due(
                    &config,
                    &sources,
                    key,
                    Tier::Heavy,
                    "Top.",
                    "https://lmarena.ai",
                )
                .is_ok()
            };
            assert!(
                !due("anthropic/claude-opus-5-5"),
                "the owner's tier, not asked again"
            );
            assert!(due("anthropic/claude-sonnet-5-5"));
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
