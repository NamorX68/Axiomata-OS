//! Every engine the Studio can start (`docs/plans/agents-model.md`, AM1, Q9, Q13): the owner's own engines
//! (`config.agents.engines`) and the **automatic** ones, computed — never stored — from the model catalog and which of
//! its models are switched on.
//!
//! An automatic engine is one model on the harness that can run it: Claude Code for Anthropic's models (the newest of
//! each family), Glut for OpenRouter's (`openrouter/<id>`) and Ollama's (`ollama/<id>`), the spelling
//! `glut_card::endpoint` picks the provider by. Its id is [`auto_engine_id`]: the harness, a dash, the model as handed
//! to the harness made a slug — `claude-code-claude-sonnet-5-5`, `glut-openrouter-deepseek-deepseek-v4-1-flash`,
//! `glut-ollama-qwen3-32b` — so it is stable across runs, the same on every machine with the same catalog, and the id
//! `roster::derive_engines` already gave an engine of that profile. An id over the slug limit keeps its head and ends
//! in eight hex digits of the model's SHA-256, so two long ids never meet by being cut.
//!
//! Automatic engines are read-only. An own engine whose id an automatic one would have wins, and the automatic one is
//! left out. A model that leaves the catalog takes its engine with it; sessions made on it keep their profile columns
//! (`roster::fields_from_engine`), so they still run.
//!
//! Every place that resolves an engine id goes through here: [`Lookup`] for a single id (it reads the catalog only
//! when the id is no own engine), [`all`] for the list.

use std::cell::OnceCell;

use axiomata_roster::{Billing, Engine, Harness, MAX_IDENT_LEN, MAX_LABEL_CHARS, Tier};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::config::{Config, ProviderId};
use crate::model_tiers::{self, ModelSources, TierSource};

/// Hex digits of the model's hash that end an automatic engine id too long to be a slug.
const ID_HASH_HEX: usize = 8;

/// Where an engine comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineOrigin {
    /// Computed from a model that is switched on; read-only.
    Auto,
    /// The owner's own (`config.agents.engines`).
    Own,
}

/// An engine with what the Studio needs to offer it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineInfo {
    #[serde(flatten)]
    pub engine: Engine,
    pub origin: EngineOrigin,
    /// The tier that applies: an own engine's own, else its model's. `None` while unrated — then it is no agent.
    pub rated_tier: Option<Tier>,
    pub tier_source: TierSource,
    /// The model's key (`openrouter/…`), when the engine names a model of a known provider.
    pub model_key: Option<String>,
    /// A short name of the model for an agent's name ("Sonnet 5.5"), when the catalog knows it.
    pub model_name: Option<String>,
    /// USD per million output tokens — the order of [`engine_for`]. Claude Code at its list price, a local model 0.
    pub output_per_m: Option<f64>,
    /// Why it cannot run right now (no key for OpenRouter, no base URL), `None` when it can.
    pub unusable: Option<String>,
}

/// Lower-case slug of `text`: letters and digits, single dashes between them.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn harness_slug(harness: Harness) -> &'static str {
    match harness {
        Harness::ClaudeCode => "claude-code",
        Harness::Opencode => "opencode",
        Harness::Glut => "glut",
    }
}

fn harness_label(harness: Harness) -> &'static str {
    match harness {
        Harness::ClaudeCode => "Claude Code",
        Harness::Opencode => "Opencode",
        Harness::Glut => "Glut",
    }
}

/// The id of the automatic engine that runs `model` (as handed to the harness) on `harness`. See the module docs.
///
/// This is the id of a model alone; within a catalog, two models whose ids meet both take [`hashed_engine_id`] instead
/// (see [`auto_engines`]).
pub fn auto_engine_id(harness: Harness, model: &str) -> String {
    let id = plain_engine_id(harness, model);
    if id.len() <= MAX_IDENT_LEN {
        return id;
    }
    hashed_engine_id(harness, model)
}

/// `<harness>-<slug of the model>`, of any length.
fn plain_engine_id(harness: Harness, model: &str) -> String {
    format!("{}-{}", harness_slug(harness), slug(model))
}

/// The plain id's head, cut to leave room, and eight hex digits of the model's SHA-256: unique per model string
/// however its slug reads.
pub fn hashed_engine_id(harness: Harness, model: &str) -> String {
    let id = plain_engine_id(harness, model);
    let hash = Sha256::digest(model.as_bytes());
    let hex: String = hash
        .iter()
        .flat_map(|byte| [byte >> 4, byte & 0x0f])
        .take(ID_HASH_HEX)
        .map(|nibble| char::from_digit(u32::from(nibble), 16).unwrap_or('0'))
        .collect();
    // The id is ASCII, so cutting by bytes cuts at a character.
    let head = id[..id.len().min(MAX_IDENT_LEN - ID_HASH_HEX - 1)].trim_end_matches('-');
    format!("{head}-{hex}")
}

/// The harness and the model string an engine of `provider`'s model `id` runs.
fn auto_profile(provider: ProviderId, id: &str) -> (Harness, String) {
    match provider {
        ProviderId::Anthropic => (Harness::ClaudeCode, id.to_owned()),
        _ => (Harness::Glut, model_tiers::model_key(provider, id)),
    }
}

/// The provider and the provider's own id an engine's model names: `openrouter/…` and `ollama/…` for Glut, a bare id
/// on Claude Code. `None` for a Glut model without a prefix (it follows the chat provider) or no model at all.
fn model_of(engine: &Engine) -> Option<(ProviderId, String)> {
    let model = engine.model.as_deref()?.trim();
    match engine.harness {
        Harness::ClaudeCode if !model.is_empty() => Some((ProviderId::Anthropic, model.to_owned())),
        Harness::Glut | Harness::Opencode => model_tiers::parse_model_key(model)
            .filter(|(provider, _)| *provider != ProviderId::Anthropic)
            .map(|(provider, id)| (provider, id.to_owned())),
        Harness::ClaudeCode => None,
    }
}

/// Why `engine` cannot run now, offline and cheap: a Glut engine needs its provider's base URL (and OpenRouter a key),
/// Claude Code needs nothing the app could check.
fn unusable_reason(config: &Config, engine: &Engine) -> Option<String> {
    match engine.harness {
        Harness::ClaudeCode => None,
        Harness::Opencode => Some("Opencode is no engine any more".to_owned()),
        Harness::Glut => crate::glut_card::endpoint(config, engine.model.clone())
            .err()
            .map(|err| err.to_string()),
    }
}

/// An engine of the catalog with its tier, price and model name filled in.
fn describe(
    config: &Config,
    sources: &ModelSources,
    engine: Engine,
    origin: EngineOrigin,
) -> EngineInfo {
    let model = model_of(&engine);
    let rating = model
        .as_ref()
        .map(|(provider, id)| model_tiers::rate(config, sources, *provider, id));
    let (rated_tier, tier_source) = match (engine.tier, &rating) {
        (Some(tier), _) => (Some(tier), TierSource::Owner),
        (None, Some(rating)) => (rating.rated.tier, rating.rated.source),
        (None, None) => (None, TierSource::Unrated),
    };
    let model_name = model.as_ref().and_then(|(provider, id)| {
        let listed = sources.catalog.providers.get(provider)?;
        let model = listed.models.iter().find(|model| &model.id == id)?;
        Some(model_tiers::short_name(&model.name))
    });
    let output_per_m = match &model {
        Some((ProviderId::Ollama, _)) => Some(0.0),
        _ => rating
            .as_ref()
            .and_then(|rating| rating.price.as_ref())
            .map(|price| price.output_per_m),
    };
    EngineInfo {
        unusable: unusable_reason(config, &engine),
        model_key: model.map(|(provider, id)| model_tiers::model_key(provider, &id)),
        engine,
        origin,
        rated_tier,
        tier_source,
        model_name,
        output_per_m,
    }
}

/// The automatic engines, in the models' order, without the ids an own engine has.
///
/// Two models whose ids slug alike (`x/y.z` and `x/y-z`) both take [`hashed_engine_id`]: neither may own the plain id,
/// or a lookalike entry arriving in a remote catalog could take over an id that roles, defaults and sessions name.
fn auto_engines(config: &Config, sources: &ModelSources) -> Vec<Engine> {
    let mut candidates = Vec::new();
    for provider in ProviderId::ALL {
        for model in model_tiers::enabled_models(config, &sources.catalog, provider) {
            // The id reaches a harness's argument list or its spec: one the app would not take as a model name (a
            // remote catalog's odd entry) makes no engine.
            if !crate::agents::valid_model_name(&model.id) {
                continue;
            }
            let (harness, handed) = auto_profile(provider, &model.id);
            let id = auto_engine_id(harness, &handed);
            candidates.push((id, harness, handed, model));
        }
    }
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (id, ..) in &candidates {
        *seen.entry(id.clone()).or_default() += 1;
    }
    let mut out: Vec<Engine> = Vec::new();
    for (id, harness, handed, model) in candidates {
        let id = if seen[&id] > 1 {
            hashed_engine_id(harness, &handed)
        } else {
            id
        };
        // Own wins.
        if config.agents.engines.contains_key(&id) || out.iter().any(|engine| engine.id == id) {
            continue;
        }
        let mut label = format!(
            "{} · {}",
            harness_label(harness),
            model_tiers::display_text(&model.name)
        );
        if label.chars().count() > MAX_LABEL_CHARS {
            label = label.chars().take(MAX_LABEL_CHARS - 1).collect::<String>() + "…";
        }
        let engine = Engine {
            id,
            label,
            harness,
            command: String::new(),
            model: Some(handed),
            env: String::new(),
            billing: if harness == Harness::ClaudeCode {
                Billing::Subscription
            } else {
                Billing::Metered
            },
            tier: None,
        };
        if engine.validate().is_ok() {
            out.push(engine);
        }
    }
    out
}

/// Every engine: the owner's own (by id), then the automatic ones (in the models' order).
pub fn all(config: &Config, sources: &ModelSources) -> Vec<EngineInfo> {
    let own = config
        .agents
        .engines
        .values()
        .cloned()
        .map(|engine| describe(config, sources, engine, EngineOrigin::Own));
    let auto = auto_engines(config, sources)
        .into_iter()
        .map(|engine| describe(config, sources, engine, EngineOrigin::Auto));
    own.chain(auto).collect()
}

/// The engine `id`, own or automatic.
pub fn find(config: &Config, sources: &ModelSources, id: &str) -> Option<EngineInfo> {
    if let Some(engine) = config.agents.engines.get(id) {
        return Some(describe(config, sources, engine.clone(), EngineOrigin::Own));
    }
    auto_engines(config, sources)
        .into_iter()
        .find(|engine| engine.id == id)
        .map(|engine| describe(config, sources, engine, EngineOrigin::Auto))
}

/// Whether `id` is an automatic engine (and no own one).
pub fn is_auto(config: &Config, sources: &ModelSources, id: &str) -> bool {
    !config.agents.engines.contains_key(id)
        && auto_engines(config, sources).iter().any(|e| e.id == id)
}

/// Resolves engine ids against one config: an own engine at once, an automatic one after reading the catalog — once,
/// and only when an id is no own engine, so a lookup of own engines costs what it did before.
pub struct Lookup<'a> {
    config: &'a Config,
    sources: OnceCell<ModelSources>,
}

impl<'a> Lookup<'a> {
    /// A lookup that reads the catalog and the proposals from their files when it first needs them.
    pub fn new(config: &'a Config) -> Self {
        Self {
            config,
            sources: OnceCell::new(),
        }
    }

    /// A lookup over sources already in hand (tests, or a caller that loaded them anyway).
    pub fn with_sources(config: &'a Config, sources: ModelSources) -> Self {
        Self {
            config,
            sources: OnceCell::from(sources),
        }
    }

    fn sources(&self) -> &ModelSources {
        self.sources.get_or_init(ModelSources::load)
    }

    /// The engine `id`, own or automatic.
    pub fn engine(&self, id: &str) -> Option<Engine> {
        if let Some(engine) = self.config.agents.engines.get(id) {
            return Some(engine.clone());
        }
        auto_engines(self.config, self.sources())
            .into_iter()
            .find(|engine| engine.id == id)
    }

    /// Whether there is an engine `id`.
    pub fn exists(&self, id: &str) -> bool {
        self.engine(id).is_some()
    }
}

/// The engines of `tier` a start may take, in the order to try them (Q13): the owner's default engine of the tier
/// first, when it exists and can run; then the other engines of the tier by output price, the cheapest first
/// (subscription-billed Claude Code at its list price, a local model as free, an unpriced one last; ties by id). An
/// engine that cannot run now is left out — so the next one of the tier takes its place.
pub fn engine_for(tier: Tier, config: &Config, sources: &ModelSources) -> Vec<EngineInfo> {
    let usable: Vec<EngineInfo> = all(config, sources)
        .into_iter()
        .filter(|info| info.unusable.is_none())
        .collect();
    let default = config.agents.models.default_engines.get(tier);
    let mut first: Vec<EngineInfo> = default
        .and_then(|id| usable.iter().find(|info| info.engine.id == id))
        .cloned()
        .into_iter()
        .collect();
    let mut rest: Vec<EngineInfo> = usable
        .into_iter()
        .filter(|info| info.rated_tier == Some(tier) && Some(info.engine.id.as_str()) != default)
        .collect();
    rest.sort_by(|a, b| {
        let price = |info: &EngineInfo| info.output_per_m.unwrap_or(f64::INFINITY);
        price(a)
            .total_cmp(&price(b))
            .then_with(|| a.engine.id.cmp(&b.engine.id))
    });
    first.append(&mut rest);
    first
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ModelCost, ProviderSettings};
    use crate::model_catalog::{CatalogModel, ModelCatalog, ProviderCatalog};
    use crate::model_tiers::{ModelChoice, TierProposals};

    fn listed(id: &str, name: &str, output: Option<f64>) -> CatalogModel {
        CatalogModel {
            id: id.into(),
            name: name.into(),
            context_length: None,
            price: output.map(|output| ModelCost {
                input_per_m: output / 4.0,
                cached_input_per_m: None,
                output_per_m: output,
            }),
            reasoning: None,
        }
    }

    /// The built-in Anthropic list, two OpenRouter models and one local model.
    fn sources() -> ModelSources {
        let mut catalog = ModelCatalog::default();
        catalog.providers.insert(
            ProviderId::OpenRouter,
            ProviderCatalog {
                models: vec![
                    listed(
                        "deepseek/deepseek-v4.1-flash",
                        "DeepSeek: DeepSeek V4.1 Flash",
                        Some(0.4),
                    ),
                    listed("z-ai/glm-5.3", "Z.ai: GLM 5.3", Some(2.0)),
                ],
                fetched_at: None,
                error: None,
            },
        );
        catalog.providers.insert(
            ProviderId::Ollama,
            ProviderCatalog {
                models: vec![listed("qwen3:32b", "qwen3:32b", None)],
                fetched_at: None,
                error: None,
            },
        );
        ModelSources {
            catalog,
            proposals: TierProposals::default(),
        }
    }

    /// A config whose OpenRouter and Ollama can run (a key, base URLs), the workspace kept out of the owner's.
    fn config() -> Config {
        let mut config = Config {
            workspace_root: std::env::temp_dir().join("axiomata-engines-test-workspace"),
            ..Config::default()
        };
        let open_router = config
            .agents
            .providers
            .get_mut(&ProviderId::OpenRouter)
            .unwrap();
        open_router.api_key = Some("sk-test".into());
        config.agents.providers.insert(
            ProviderId::Ollama,
            ProviderSettings::default_for(ProviderId::Ollama),
        );
        config
    }

    fn own(id: &str, harness: Harness, model: &str, tier: Option<Tier>) -> Engine {
        Engine {
            id: id.into(),
            label: id.into(),
            harness,
            command: String::new(),
            model: Some(model.into()),
            env: String::new(),
            billing: Billing::Metered,
            tier,
        }
    }

    fn ids(infos: &[EngineInfo]) -> Vec<&str> {
        infos.iter().map(|info| info.engine.id.as_str()).collect()
    }

    #[test]
    fn automatic_engines_have_stable_ids_one_per_enabled_model_and_harness() {
        let infos = all(&config(), &sources());
        assert_eq!(
            ids(&infos),
            [
                "claude-code-claude-fable-5-1",
                "claude-code-claude-opus-5-5",
                "claude-code-claude-sonnet-5-5",
                "claude-code-claude-haiku-5-5",
                "glut-openrouter-deepseek-deepseek-v4-1-flash",
                "glut-openrouter-z-ai-glm-5-3",
                "glut-ollama-qwen3-32b",
            ]
        );
        assert!(infos.iter().all(|info| info.origin == EngineOrigin::Auto));
        assert!(infos.iter().all(|info| info.engine.validate().is_ok()));
        let flash = &infos[4];
        assert_eq!(flash.engine.harness, Harness::Glut);
        assert_eq!(
            flash.engine.model.as_deref(),
            Some("openrouter/deepseek/deepseek-v4.1-flash")
        );
        assert_eq!(flash.engine.billing, Billing::Metered);
        assert_eq!(flash.model_name.as_deref(), Some("DeepSeek V4.1 Flash"));
        assert_eq!(infos[2].engine.model.as_deref(), Some("claude-sonnet-5-5"));
        assert_eq!(infos[2].engine.billing, Billing::Subscription);
        // The same inputs, the same ids — and what `roster::derive_engines` names an engine of that profile.
        assert_eq!(ids(&all(&config(), &sources())), ids(&infos));
        assert_eq!(
            auto_engine_id(Harness::Glut, "openrouter/deepseek/v4"),
            "glut-openrouter-deepseek-v4"
        );
    }

    #[test]
    fn models_whose_ids_slug_alike_both_take_a_hashed_id() {
        let mut sources = sources();
        let open_router = sources
            .catalog
            .providers
            .get_mut(&ProviderId::OpenRouter)
            .unwrap();
        open_router
            .models
            .push(listed("x/y.z", "Dotted", Some(1.0)));
        open_router
            .models
            .push(listed("x-y-z", "Dashed", Some(1.0)));
        let infos = all(&config(), &sources);
        let id_of = |model: &str| {
            infos
                .iter()
                .find(|info| info.engine.model.as_deref() == Some(model))
                .map(|info| info.engine.id.clone())
                .unwrap()
        };
        let (dotted, dashed) = (id_of("openrouter/x/y.z"), id_of("openrouter/x-y-z"));
        assert_ne!(dotted, dashed);
        assert_ne!(dotted, "glut-openrouter-x-y-z", "neither owns the plain id");
        assert_ne!(dashed, "glut-openrouter-x-y-z");
        assert!(dotted.starts_with("glut-openrouter-x-y-z-"));
        assert!(infos.iter().all(|info| info.engine.validate().is_ok()));
        // Alone, a model keeps its plain id.
        assert_eq!(
            id_of("openrouter/z-ai/glm-5.3"),
            "glut-openrouter-z-ai-glm-5-3"
        );
    }

    #[test]
    fn a_catalog_id_that_is_no_model_name_makes_no_engine() {
        let mut sources = sources();
        let open_router = sources
            .catalog
            .providers
            .get_mut(&ProviderId::OpenRouter)
            .unwrap();
        open_router
            .models
            .push(listed("--dangerous", "Odd", Some(1.0)));
        open_router
            .models
            .push(listed("a b/c", "Spaced", Some(1.0)));
        let infos = all(&config(), &sources);
        assert!(infos.iter().all(|info| {
            let model = info.engine.model.as_deref().unwrap_or_default();
            !model.contains("dangerous") && !model.contains(' ')
        }));
    }

    #[test]
    fn an_overlong_automatic_id_keeps_its_head_and_ends_in_a_hash() {
        let long = "openrouter/some-vendor/an-extraordinarily-long-model-name-preview";
        let other = "openrouter/some-vendor/an-extraordinarily-long-model-name-preview-2";
        let id = auto_engine_id(Harness::Glut, long);
        assert!(id.len() <= MAX_IDENT_LEN, "{id}");
        assert!(id.starts_with("glut-openrouter-some-vendor"));
        assert!(axiomata_roster::check_slug("id", &id).is_ok());
        assert_eq!(id, auto_engine_id(Harness::Glut, long));
        assert_ne!(id, auto_engine_id(Harness::Glut, other));
    }

    #[test]
    fn a_switched_off_model_has_no_engine_and_an_own_engine_wins_its_id() {
        let mut config = config();
        config.agents.models.choices.insert(
            "openrouter/z-ai/glm-5.3".into(),
            ModelChoice {
                enabled: Some(false),
                tier: None,
            },
        );
        let mine = own(
            "glut-ollama-qwen3-32b",
            Harness::Glut,
            "ollama/qwen3:32b",
            Some(Tier::Heavy),
        );
        config.agents.engines.insert(mine.id.clone(), mine);
        let infos = all(&config, &sources());
        assert!(!ids(&infos).contains(&"glut-openrouter-z-ai-glm-5-3"));
        let clash: Vec<_> = infos
            .iter()
            .filter(|info| info.engine.id == "glut-ollama-qwen3-32b")
            .collect();
        assert_eq!(clash.len(), 1);
        assert_eq!(clash[0].origin, EngineOrigin::Own);
        assert_eq!(
            (clash[0].rated_tier, clash[0].tier_source),
            (Some(Tier::Heavy), TierSource::Owner)
        );
        assert!(!is_auto(&config, &sources(), "glut-ollama-qwen3-32b"));
        assert!(is_auto(&config, &sources(), "claude-code-claude-opus-5-5"));
        let lookup = Lookup::with_sources(&config, sources());
        assert_eq!(
            lookup.engine("glut-ollama-qwen3-32b").unwrap().tier,
            Some(Tier::Heavy)
        );
        assert!(lookup.exists("glut-openrouter-deepseek-deepseek-v4-1-flash"));
        assert!(!lookup.exists("glut-openrouter-z-ai-glm-5-3"));
    }

    #[test]
    fn an_own_engine_without_a_tier_takes_its_models() {
        let mut config = config();
        let mine = own("mine", Harness::Glut, "openrouter/z-ai/glm-5.3", None);
        config.agents.engines.insert(mine.id.clone(), mine);
        let info = find(&config, &sources(), "mine").unwrap();
        assert_eq!(
            (info.rated_tier, info.tier_source),
            (Some(Tier::Medium), TierSource::Price)
        );
        assert_eq!(info.model_key.as_deref(), Some("openrouter/z-ai/glm-5.3"));
    }

    #[test]
    fn a_tier_lists_the_default_engine_first_then_the_cheapest_and_skips_what_cannot_run() {
        let mut config = config();
        let sources = sources();
        // Medium: GLM (2.0) and Sonnet 5.5 (10.0, list price) by price.
        assert_eq!(
            ids(&engine_for(Tier::Medium, &config, &sources)),
            [
                "glut-openrouter-z-ai-glm-5-3",
                "claude-code-claude-sonnet-5-5"
            ]
        );
        // The owner's default goes first.
        config
            .agents
            .models
            .default_engines
            .set(Tier::Medium, Some("claude-code-claude-sonnet-5-5".into()));
        assert_eq!(
            ids(&engine_for(Tier::Medium, &config, &sources)),
            [
                "claude-code-claude-sonnet-5-5",
                "glut-openrouter-z-ai-glm-5-3"
            ]
        );
        // Light: Haiku (0.5) and the flash model (0.4); the local model is unrated and no candidate.
        assert_eq!(
            ids(&engine_for(Tier::Light, &config, &sources)),
            [
                "glut-openrouter-deepseek-deepseek-v4-1-flash",
                "claude-code-claude-haiku-5-5"
            ]
        );
        // Without a key OpenRouter cannot run: its engines drop out, a default on it too, and the next one stands in.
        config.agents.models.default_engines.set(
            Tier::Light,
            Some("glut-openrouter-deepseek-deepseek-v4-1-flash".into()),
        );
        config
            .agents
            .providers
            .get_mut(&ProviderId::OpenRouter)
            .unwrap()
            .api_key = None;
        assert_eq!(
            ids(&engine_for(Tier::Light, &config, &sources)),
            ["claude-code-claude-haiku-5-5"]
        );
        let flash = find(
            &config,
            &sources,
            "glut-openrouter-deepseek-deepseek-v4-1-flash",
        )
        .unwrap();
        assert!(flash.unusable.unwrap().contains("API key"));
    }

    #[test]
    fn a_rated_local_model_is_free_and_comes_first() {
        let config = config();
        let mut sources = sources();
        sources.proposals.proposals.insert(
            "ollama/qwen3:32b".into(),
            model_tiers::TierProposal {
                tier: Tier::Light,
                reason: "Small and quick.".into(),
                source: "LMArena".into(),
                at: "2026-10-30T10:00:00Z".into(),
            },
        );
        assert_eq!(
            ids(&engine_for(Tier::Light, &config, &sources))[0],
            "glut-ollama-qwen3-32b"
        );
    }
}
