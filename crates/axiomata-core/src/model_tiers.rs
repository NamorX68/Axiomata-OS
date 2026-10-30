//! Models as the Studio sees them (`docs/plans/agents-model.md`, AM1): which models of the providers' catalogs are
//! switched on, and which tier — light, medium, heavy — each one has.
//!
//! Three sources say what a model is, and the first that speaks wins ([`effective_tier`]):
//!
//! 1. the **owner's** value (`config.agents.models.choices`, saved into the file config like engines are),
//! 2. the app's **research** proposal (`~/.axiomata/model-tiers.json`, written by the `model-tiering` skill of AM2 —
//!    app-owned, never the config, so a proposal can never pass for the owner's word),
//! 3. the **price** proposal: the output price against the owner's limits (Q12). Ollama has no price, so a local model
//!    stays *unrated* until research has spoken — and an unrated model is not offered as an agent.
//!
//! A model is keyed provider-qualified, `anthropic/<id>`, `openrouter/<id>` or `ollama/<id>` ([`model_key`]): the same
//! id can exist at two providers, and the Glut engines name their model in this spelling already.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use axiomata_roster::{Tier, check_slug};
use serde::{Deserialize, Serialize};

use crate::config::{self, Config, ModelCost, ProviderId};
use crate::error::AxiomataError;
use crate::model_catalog::{self, CatalogModel, ModelCatalog};
use crate::roster::refusal;
use crate::{json_state, paths};

type Result<T> = std::result::Result<T, AxiomataError>;

/// The longest model list that is switched on as a whole (Q3). A longer one starts switched off, and the owner picks.
pub const SMALL_LIST_MAX: usize = 20;

/// Below this output price (USD per million tokens) the price proposes `light` (Q12).
pub const DEFAULT_LIGHT_BELOW_USD: f64 = 1.0;

/// Above this output price (USD per million tokens) the price proposes `heavy` (Q12).
pub const DEFAULT_HEAVY_ABOVE_USD: f64 = 10.0;

/// On-disk schema version of `model-tiers.json`.
pub const PROPOSALS_VERSION: u64 = 1;

/// Longest proposal reason kept, in characters: one sentence with its source, shown beside the tier.
const MAX_REASON_CHARS: usize = 400;

/// Longest proposal source kept, in characters (a site's name or a URL).
const MAX_SOURCE_CHARS: usize = 200;

/// Longest model key accepted, in characters: the provider prefix plus the catalog's longest id.
const MAX_KEY_CHARS: usize = 240;

/// Digits from which a part of an Anthropic model id is a date (`20260101`), not a version number.
const DATE_PART_DIGITS: usize = 6;

/// Longest model name shown in an agent's name, in characters (the catalog's names come from a remote list).
pub const MAX_MODEL_NAME_CHARS: usize = 60;

/// Serialises the read-modify-write of `model-tiers.json` in this process; [`ProposalsLock`] does it across processes.
static PROPOSALS_LOCK: Mutex<()> = Mutex::new(());

/// The proposals file's lock across processes — the app and a skill run's CLI both write it: an advisory lock
/// (`flock`) on `model-tiers.json.lock`, held until dropped, taken while this process holds [`PROPOSALS_LOCK`].
struct ProposalsLock {
    _thread: std::sync::MutexGuard<'static, ()>,
    file: std::fs::File,
}

impl ProposalsLock {
    fn take() -> Result<Self> {
        let thread = PROPOSALS_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let path = paths::model_tiers_lock_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| AxiomataError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let io = |source| AxiomataError::Io {
            path: path.clone(),
            source,
        };
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(io)?;
        file.lock().map_err(io)?;
        Ok(Self {
            _thread: thread,
            file,
        })
    }
}

impl Drop for ProposalsLock {
    fn drop(&mut self) {
        // Closing the file releases the lock as well; unlocking first says so.
        let _ = self.file.unlock();
    }
}

// ---- The owner's settings (`[agents.models]`) ----

/// The owner's word on one model. Both fields `None` = the defaults, and the entry is dropped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelChoice {
    /// `None` = the default of Q3 (on for a short list, off for a long one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// `None` = no value of the owner's; a proposal applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier: Option<Tier>,
}

impl ModelChoice {
    /// Whether this says nothing the defaults would not.
    pub fn is_default(&self) -> bool {
        self.enabled.is_none() && self.tier.is_none()
    }
}

fn default_light_below() -> f64 {
    DEFAULT_LIGHT_BELOW_USD
}

fn default_heavy_above() -> f64 {
    DEFAULT_HEAVY_ABOVE_USD
}

/// The price limits of the price proposal, in USD per million **output** tokens (Q12).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TierLimits {
    /// Cheaper than this is `light`.
    #[serde(default = "default_light_below")]
    pub light_below: f64,
    /// Dearer than this is `heavy`; in between is `medium`.
    #[serde(default = "default_heavy_above")]
    pub heavy_above: f64,
}

impl Default for TierLimits {
    fn default() -> Self {
        Self {
            light_below: DEFAULT_LIGHT_BELOW_USD,
            heavy_above: DEFAULT_HEAVY_ABOVE_USD,
        }
    }
}

impl TierLimits {
    /// The tier an output price proposes: below `light_below` light, above `heavy_above` heavy, else medium — so with
    /// the defaults exactly 1.0 and exactly 10.0 are both medium.
    pub fn propose(&self, output_per_m: f64) -> Tier {
        if output_per_m < self.light_below {
            Tier::Light
        } else if output_per_m > self.heavy_above {
            Tier::Heavy
        } else {
            Tier::Medium
        }
    }

    /// Checks that both limits are prices and in order.
    ///
    /// # Errors
    ///
    /// A message naming what is wrong.
    pub fn validate(&self) -> std::result::Result<(), String> {
        let price = 0.0..=model_catalog::MAX_USD_PER_M;
        for (name, value) in [
            ("light_below", self.light_below),
            ("heavy_above", self.heavy_above),
        ] {
            if !price.contains(&value) {
                return Err(format!(
                    "the tier limit {name} must be a price from 0 to {} USD per million tokens",
                    model_catalog::MAX_USD_PER_M
                ));
            }
        }
        if self.light_below > self.heavy_above {
            return Err(
                "the tier limit light_below must not be above heavy_above (medium would be empty)"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

/// The owner's default engine per tier (Q13). `None` = the cheapest engine of the tier.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierEngines {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub medium: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heavy: Option<String>,
}

impl TierEngines {
    /// The default engine of `tier`, if the owner set one.
    pub fn get(&self, tier: Tier) -> Option<&str> {
        match tier {
            Tier::Light => self.light.as_deref(),
            Tier::Medium => self.medium.as_deref(),
            Tier::Heavy => self.heavy.as_deref(),
        }
    }

    /// Sets (or with `None` clears) the default engine of `tier`.
    pub fn set(&mut self, tier: Tier, engine: Option<String>) {
        let slot = match tier {
            Tier::Light => &mut self.light,
            Tier::Medium => &mut self.medium,
            Tier::Heavy => &mut self.heavy,
        };
        *slot = engine;
    }
}

/// `[agents.models]`: what the owner said about the models (AM1).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelSettings {
    /// By provider-qualified model key ([`model_key`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub choices: BTreeMap<String, ModelChoice>,
    #[serde(default)]
    pub tier_limits: TierLimits,
    #[serde(default)]
    pub default_engines: TierEngines,
}

impl ModelSettings {
    /// Checks every key, the limits and the default engines' ids.
    ///
    /// # Errors
    ///
    /// A message naming the first bad entry.
    pub fn validate(&self) -> std::result::Result<(), String> {
        if let Some(key) = self
            .choices
            .keys()
            .find(|key| parse_model_key(key).is_none())
        {
            return Err(format!(
                "model {key:?} is no model key (anthropic/…, openrouter/… or ollama/…)"
            ));
        }
        self.tier_limits.validate()?;
        for tier in Tier::ALL {
            if let Some(id) = self.default_engines.get(tier) {
                check_slug("default_engines", id)
                    .map_err(|err| format!("default engine of {}: {err}", tier.as_str()))?;
            }
        }
        Ok(())
    }
}

// ---- Model keys ----

/// The prefix of `provider`'s models in a model key — the spelling Glut engines use for their model.
pub fn provider_prefix(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::Anthropic => "anthropic",
        ProviderId::OpenRouter => "openrouter",
        ProviderId::Ollama => "ollama",
    }
}

/// The key of model `id` at `provider`: `openrouter/deepseek/deepseek-v4.1-flash`.
pub fn model_key(provider: ProviderId, id: &str) -> String {
    format!("{}/{id}", provider_prefix(provider))
}

/// Splits a model key into its provider and the provider's own id. `None` for anything else: an unknown prefix, an
/// empty id, control characters or an overlong key.
pub fn parse_model_key(key: &str) -> Option<(ProviderId, &str)> {
    if key.chars().count() > MAX_KEY_CHARS || key.chars().any(char::is_control) {
        return None;
    }
    let (prefix, id) = key.split_once('/')?;
    let provider = ProviderId::ALL
        .into_iter()
        .find(|provider| provider_prefix(*provider) == prefix)?;
    let id_ok = !id.trim().is_empty() && id.trim() == id;
    id_ok.then_some((provider, id))
}

// ---- What each provider offers ----

/// `claude-sonnet-5-5` → `("sonnet", [5, 5])`; `None` for an id not in that shape.
fn anthropic_family(id: &str) -> Option<(&str, Vec<u32>)> {
    let (family, version) = id.strip_prefix("claude-")?.split_once('-')?;
    // A dated snapshot (`claude-sonnet-5-20260101`) ends its version at the date: the date is no part of it, else it
    // would outrank every real version.
    let version = version
        .split('-')
        .take_while(|part| part.len() < DATE_PART_DIGITS)
        .map(|part| part.parse().ok())
        .collect::<Option<Vec<u32>>>()?;
    (!version.is_empty()).then_some((family, version))
}

/// Anthropic's models as Claude Code offers them (Q9): only the newest of each family (Fable, Opus, Sonnet, Haiku), by
/// the version in the id — Sonnet 5 drops out while Sonnet 5.5 exists. An id that names no family and version is its
/// own family. The list's order is kept.
pub fn anthropic_offered(models: &[CatalogModel]) -> Vec<&CatalogModel> {
    let mut newest: BTreeMap<&str, (&[u32], usize)> = BTreeMap::new();
    let parsed: Vec<_> = models
        .iter()
        .map(|model| anthropic_family(&model.id))
        .collect();
    for (at, family) in parsed.iter().enumerate() {
        let Some((family, version)) = family else {
            continue;
        };
        match newest.get(family) {
            Some((best, _)) if *best >= version.as_slice() => {}
            _ => {
                newest.insert(family, (version.as_slice(), at));
            }
        }
    }
    models
        .iter()
        .enumerate()
        .filter(|(at, _)| match &parsed[*at] {
            Some((family, _)) => newest.get(family).is_some_and(|(_, best)| best == at),
            None => true,
        })
        .map(|(_, model)| model)
        .collect()
}

/// The models `provider` offers the Studio: Anthropic's newest per family, every other provider's whole list.
pub fn offered(catalog: &ModelCatalog, provider: ProviderId) -> Vec<&CatalogModel> {
    let Some(listed) = catalog.providers.get(&provider) else {
        return Vec::new();
    };
    match provider {
        ProviderId::Anthropic => anthropic_offered(&listed.models),
        _ => listed.models.iter().collect(),
    }
}

/// Whether a model is on: the owner's word, else `by_default` — whether its provider's list is short (Q3).
fn is_enabled(choice: Option<&ModelChoice>, by_default: bool) -> bool {
    choice
        .and_then(|choice| choice.enabled)
        .unwrap_or(by_default)
}

/// The models of `provider` that are switched on.
pub fn enabled_models<'a>(
    config: &Config,
    catalog: &'a ModelCatalog,
    provider: ProviderId,
) -> Vec<&'a CatalogModel> {
    let offered = offered(catalog, provider);
    let by_default = offered.len() <= SMALL_LIST_MAX;
    offered
        .into_iter()
        .filter(|model| {
            let key = model_key(provider, &model.id);
            is_enabled(config.agents.models.choices.get(&key), by_default)
        })
        .collect()
}

// ---- Proposals (`~/.axiomata/model-tiers.json`) ----

/// A tier the app proposes for a model from research (AM2's `model-tiering` skill), never the owner's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierProposal {
    pub tier: Tier,
    /// One sentence why, for the owner to read beside the tier.
    pub reason: String,
    /// Where it comes from: a ranking's name or its URL.
    pub source: String,
    /// RFC 3339 time it was made.
    pub at: String,
}

impl TierProposal {
    /// Checks the texts: one line each, bounded, no control characters.
    ///
    /// # Errors
    ///
    /// A refusal naming the field.
    pub fn validate(&self) -> Result<()> {
        let one_line = |field: &'static str, text: &str, max: usize| {
            if text.trim().is_empty()
                || text.chars().count() > max
                || text.chars().any(char::is_control)
            {
                return Err(refusal(
                    field,
                    format!("one line of up to {max} characters"),
                ));
            }
            Ok(())
        };
        one_line("reason", &self.reason, MAX_REASON_CHARS)?;
        one_line("source", &self.source, MAX_SOURCE_CHARS)?;
        if chrono::DateTime::parse_from_rfc3339(&self.at).is_err() {
            return Err(refusal("at", "an RFC 3339 time".to_owned()));
        }
        Ok(())
    }
}

/// The content of `model-tiers.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TierProposals {
    pub version: u64,
    /// By model key.
    #[serde(default)]
    pub proposals: BTreeMap<String, TierProposal>,
    /// When research was last started for a model (RFC 3339), by key: a run that fails does not start again for it
    /// before a day has passed (`crate::model_research`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attempts: BTreeMap<String, String>,
    /// Models the owner asked to be researched again ("Neu vorschlagen"), even with a tier of their own; a proposal
    /// recorded for one takes it off.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub wanted: BTreeSet<String>,
}

impl Default for TierProposals {
    fn default() -> Self {
        Self {
            version: PROPOSALS_VERSION,
            proposals: BTreeMap::new(),
            attempts: BTreeMap::new(),
            wanted: BTreeSet::new(),
        }
    }
}

/// The stored proposals; none when the file is missing or unreadable (a proposal is advice, not state to lose sleep
/// over: the next research run writes it again).
///
/// Each entry is checked on its own — a key that is no model key, a tier that does not parse, a text over its bound —
/// and one that fails is skipped with a warning: the file is written by a skill (AM2), so it is read as untrusted.
pub fn load_proposals() -> TierProposals {
    /// The file as stored, each entry still unread.
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        proposals: BTreeMap<String, serde_json::Value>,
        #[serde(default)]
        attempts: BTreeMap<String, serde_json::Value>,
        #[serde(default)]
        wanted: Vec<serde_json::Value>,
    }
    let Some(raw) = json_state::load(&paths::model_tiers_path(), || {
        serde_json::to_string(&TierProposals::default()).unwrap_or_default()
    })
    .ok()
    .and_then(|state| serde_json::from_str::<Raw>(&state.json).ok()) else {
        return TierProposals::default();
    };
    let mut proposals = TierProposals::default();
    for (key, value) in raw.proposals {
        let read = serde_json::from_value::<TierProposal>(value)
            .map_err(|err| err.to_string())
            .and_then(|proposal| {
                if parse_model_key(&key).is_none() {
                    return Err("no model key".to_owned());
                }
                proposal.validate().map_err(|err| err.to_string())?;
                Ok(proposal)
            });
        match read {
            Ok(proposal) => {
                proposals.proposals.insert(key, proposal);
            }
            Err(err) => tracing::warn!(model = %key, %err, "a tier proposal was skipped"),
        }
    }
    for (key, at) in raw.attempts {
        let at = at
            .as_str()
            .filter(|at| chrono::DateTime::parse_from_rfc3339(at).is_ok());
        match (parse_model_key(&key), at) {
            (Some(_), Some(at)) => {
                proposals.attempts.insert(key, at.to_owned());
            }
            _ => tracing::warn!(model = %key, "a research attempt was skipped"),
        }
    }
    proposals.wanted = raw
        .wanted
        .iter()
        .filter_map(serde_json::Value::as_str)
        .filter(|key| parse_model_key(key).is_some())
        .map(str::to_owned)
        .collect();
    proposals
}

/// Applies `change` to the stored proposals under the file's lock and writes them back.
///
/// # Errors
///
/// What `change` returns; a failing write.
pub(crate) fn edit_proposals<T>(change: impl FnOnce(&mut TierProposals) -> Result<T>) -> Result<T> {
    let _lock = ProposalsLock::take()?;
    let mut proposals = load_proposals();
    let before = proposals.clone();
    let out = change(&mut proposals)?;
    // Nothing changed, nothing written: the check after every catalog refresh stays a read.
    if proposals != before {
        write_proposals(&proposals)?;
    }
    Ok(out)
}

fn write_proposals(proposals: &TierProposals) -> Result<()> {
    let json =
        serde_json::to_string(proposals).map_err(|err| AxiomataError::InvalidDashboardState {
            path: paths::model_tiers_path(),
            reason: err.to_string(),
        })?;
    json_state::save(&paths::model_tiers_path(), &json)
}

/// Records the research proposal for model `key`, replacing an earlier one.
///
/// # Errors
///
/// A refusal for a bad key or proposal; a failing write.
pub fn record_proposal(key: &str, proposal: TierProposal) -> Result<()> {
    if parse_model_key(key).is_none() {
        return Err(refusal("model", format!("“{key}” is no model key")));
    }
    proposal.validate()?;
    edit_proposals(|proposals| {
        proposals.wanted.remove(key);
        proposals.proposals.insert(key.to_owned(), proposal);
        Ok(())
    })
}

/// Drops the research proposal for model `key`. `false` if there was none.
///
/// # Errors
///
/// A failing write.
pub fn forget_proposal(key: &str) -> Result<bool> {
    edit_proposals(|proposals| Ok(proposals.proposals.remove(key).is_some()))
}

/// Everything besides the config that says what a model is: the catalog and the research proposals, both read from
/// their files (cheap, read-only).
#[derive(Debug, Clone, Default)]
pub struct ModelSources {
    pub catalog: ModelCatalog,
    pub proposals: TierProposals,
}

impl ModelSources {
    /// Both files, as stored.
    ///
    /// In this crate's tests only under a scratch `AXIOMATA_HOME`: without one a test gets the built-in catalog and no
    /// proposals, so a test that resolves an engine id never reads — or, through a corrupt file's `.bak`, writes —
    /// the owner's `~/.axiomata`.
    pub fn load() -> Self {
        if cfg!(test) && std::env::var_os(paths::AXIOMATA_HOME_ENV).is_none() {
            return Self::default();
        }
        Self {
            catalog: model_catalog::load(),
            proposals: load_proposals(),
        }
    }
}

// ---- Tiers ----

/// Where a model's tier comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TierSource {
    /// The owner set it.
    Owner,
    /// The app's research proposed it.
    Research,
    /// The price proposed it.
    Price,
    /// Nothing says yet — a local model before research. Not offered as an agent.
    Unrated,
}

/// A model's tier and where it comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Rated {
    pub tier: Option<Tier>,
    pub source: TierSource,
}

/// The tier that applies: the owner's, else the research proposal's, else the price's; unrated when none speaks.
pub fn effective_tier(
    owner: Option<Tier>,
    research: Option<&TierProposal>,
    price: Option<Tier>,
) -> Rated {
    let (tier, source) = match (owner, research, price) {
        (Some(tier), ..) => (Some(tier), TierSource::Owner),
        (None, Some(proposal), _) => (Some(proposal.tier), TierSource::Research),
        (None, None, Some(tier)) => (Some(tier), TierSource::Price),
        (None, None, None) => (None, TierSource::Unrated),
    };
    Rated { tier, source }
}

/// The price of model `id` at `provider` that ranks it: the catalog's list price, else what the owner wrote into
/// `agents.costs`, else Anthropic's list price. `None` for a local model.
pub fn model_price(
    config: &Config,
    provider: ProviderId,
    id: &str,
    listed: Option<&CatalogModel>,
) -> Option<ModelCost> {
    if provider == ProviderId::Ollama {
        return None;
    }
    listed
        .and_then(|model| model.price.clone())
        .or_else(|| config.agents.costs.get(id).cloned())
        .or_else(|| {
            (provider == ProviderId::Anthropic)
                .then(|| model_catalog::anthropic_list_price(id))
                .flatten()
        })
}

/// What one model is: its price and its tier with the source.
#[derive(Debug, Clone, PartialEq)]
pub struct Rating {
    pub price: Option<ModelCost>,
    pub owner_tier: Option<Tier>,
    pub price_tier: Option<Tier>,
    pub proposal: Option<TierProposal>,
    pub rated: Rated,
}

/// Rates model `id` at `provider` — also one that is not offered (an own engine may still run Sonnet 5).
pub fn rate(config: &Config, sources: &ModelSources, provider: ProviderId, id: &str) -> Rating {
    let key = model_key(provider, id);
    let listed = sources
        .catalog
        .providers
        .get(&provider)
        .and_then(|listed| listed.models.iter().find(|model| model.id == id));
    let price = model_price(config, provider, id, listed);
    let limits = &config.agents.models.tier_limits;
    let price_tier = price
        .as_ref()
        .map(|price| limits.propose(price.output_per_m));
    let owner_tier = config
        .agents
        .models
        .choices
        .get(&key)
        .and_then(|choice| choice.tier);
    let proposal = sources.proposals.proposals.get(&key).cloned();
    let rated = effective_tier(owner_tier, proposal.as_ref(), price_tier);
    Rating {
        price,
        owner_tier,
        price_tier,
        proposal,
        rated,
    }
}

/// A short name to show for a model: the catalog's name without a vendor prefix ("DeepSeek: …") and without
/// "Claude " — "Claude Sonnet 5.5" reads "Sonnet 5.5" in an agent's name. The name itself when nothing is left.
pub fn short_name(name: &str) -> String {
    let name = name.trim();
    let name = name
        .split_once(": ")
        .map_or(name, |(_, rest)| rest.trim())
        .trim();
    display_text(name.strip_prefix("Claude ").unwrap_or(name))
}

/// Whether `ch` changes how text around it reads without being seen: bidi overrides and isolates, zero-width marks.
fn is_invisible_format(ch: char) -> bool {
    matches!(ch, '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2069}' | '\u{FEFF}')
}

/// `text` fit to stand in an agent's name: printable characters only (no control or invisible format characters),
/// runs of space as one, at most [`MAX_MODEL_NAME_CHARS`] characters (cut with "…"). Model names come from a remote
/// catalog, and a session's name is built from them.
pub fn display_text(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|ch| if ch.is_whitespace() { ' ' } else { ch })
        .filter(|ch| !ch.is_control() && !is_invisible_format(*ch))
        .collect();
    let joined = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() <= MAX_MODEL_NAME_CHARS {
        return joined;
    }
    let head: String = joined.chars().take(MAX_MODEL_NAME_CHARS - 1).collect();
    format!("{}…", head.trim_end())
}

// ---- The list the Models tab and `ide models list` show ----

/// One model as the Models tab shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelEntry {
    pub key: String,
    pub provider: ProviderId,
    pub id: String,
    pub name: String,
    /// [`short_name`] of the name.
    pub short_name: String,
    pub enabled: bool,
    /// What `enabled` is without the owner's word (Q3).
    pub enabled_by_default: bool,
    /// The tier that applies, `None` while unrated.
    pub tier: Option<Tier>,
    pub tier_source: TierSource,
    /// The owner's value, the research proposal and the price's proposal, each as it stands.
    pub owner_tier: Option<Tier>,
    pub proposal: Option<TierProposal>,
    pub price_tier: Option<Tier>,
    pub price: Option<ModelCost>,
    /// The context window in tokens, when the catalog says.
    pub context_length: Option<u64>,
}

/// Every model the providers offer, provider by provider (Anthropic, OpenRouter, Ollama), in the catalog's order.
pub fn models(config: &Config, sources: &ModelSources) -> Vec<ModelEntry> {
    let mut out = Vec::new();
    for provider in ProviderId::ALL {
        let offered = offered(&sources.catalog, provider);
        let by_default = offered.len() <= SMALL_LIST_MAX;
        for model in offered {
            let key = model_key(provider, &model.id);
            let choice = config.agents.models.choices.get(&key);
            let rating = rate(config, sources, provider, &model.id);
            out.push(ModelEntry {
                enabled: is_enabled(choice, by_default),
                enabled_by_default: by_default,
                provider,
                id: model.id.clone(),
                name: model.name.clone(),
                short_name: short_name(&model.name),
                tier: rating.rated.tier,
                tier_source: rating.rated.source,
                owner_tier: rating.owner_tier,
                proposal: rating.proposal,
                price_tier: rating.price_tier,
                price: rating.price,
                context_length: model.context_length,
                key,
            });
        }
    }
    out
}

/// What the Models tab shows in one read: every model, the price limits and the default engine per tier.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelsOverview {
    pub models: Vec<ModelEntry>,
    pub tier_limits: TierLimits,
    pub default_engines: TierEngines,
}

/// [`models`] with the limits and the default engines.
pub fn overview(config: &Config, sources: &ModelSources) -> ModelsOverview {
    ModelsOverview {
        models: models(config, sources),
        tier_limits: config.agents.models.tier_limits,
        default_engines: config.agents.models.default_engines.clone(),
    }
}

/// The models that are switched on but have no tier yet: what AM2's research runs on.
pub fn unrated(config: &Config, sources: &ModelSources) -> Vec<ModelEntry> {
    models(config, sources)
        .into_iter()
        .filter(|model| model.enabled && model.tier.is_none())
        .collect()
}

// ---- Setting (into the file config, like engines) ----

/// Applies `change` to the file config's `[agents.models]`, freshly read under the config lock, saves it, and hands
/// the result to `config`. Never by saving `config`: the live config keeps this session's workspace root while the
/// file may carry one queued for the next start (the same reason as `roster::save_engine`).
fn edit_models(
    config: &mut Config,
    change: impl FnOnce(&Config, &mut ModelSettings) -> Result<()>,
) -> Result<()> {
    let _guard = config::file_edit_lock();
    let mut disk = Config::load()?;
    let mut models = disk.agents.models.clone();
    change(&disk, &mut models)?;
    models
        .validate()
        .map_err(|reason| refusal("models", reason))?;
    disk.agents.models = models;
    disk.save()?;
    config.agents.models = disk.agents.models;
    Ok(())
}

/// Sets the owner's word on model `key` (a full replace; both fields `None` drops the entry).
///
/// # Errors
///
/// A refusal for a bad key; a failing read or save (then `config` is unchanged).
pub fn save_choice(config: &mut Config, key: &str, choice: ModelChoice) -> Result<()> {
    edit_choice(config, key, |current| *current = choice)
}

/// Changes the owner's word on model `key` as it stands in the file config, read under the config lock — so a change
/// of one field (the CLI's `--enable`) never writes back a stale value of the other.
///
/// # Errors
///
/// As [`save_choice`].
pub fn edit_choice(
    config: &mut Config,
    key: &str,
    change: impl FnOnce(&mut ModelChoice),
) -> Result<()> {
    if parse_model_key(key).is_none() {
        return Err(refusal(
            "model",
            format!("“{key}” is no model key (anthropic/…, openrouter/… or ollama/…)"),
        ));
    }
    edit_models(config, |_, models| {
        let mut choice = models.choices.get(key).copied().unwrap_or_default();
        change(&mut choice);
        if choice.is_default() {
            models.choices.remove(key);
        } else {
            models.choices.insert(key.to_owned(), choice);
        }
        Ok(())
    })
}

/// Sets the price limits of the price proposal.
///
/// # Errors
///
/// A refusal for limits that are no prices or out of order; a failing read or save.
pub fn save_tier_limits(config: &mut Config, limits: TierLimits) -> Result<()> {
    limits
        .validate()
        .map_err(|reason| refusal("tier_limits", reason))?;
    edit_models(config, |_, models| {
        models.tier_limits = limits;
        Ok(())
    })
}

/// Sets (or with `None` clears) the owner's default engine of `tier`. The engine must exist — own or automatic.
///
/// # Errors
///
/// A refusal for an engine that does not exist; a failing read or save.
pub fn save_default_engine(config: &mut Config, tier: Tier, engine: Option<String>) -> Result<()> {
    edit_models(config, |disk, models| {
        if let Some(id) = engine.as_deref()
            && !crate::engines::Lookup::new(disk).exists(id)
        {
            return Err(refusal("engine", format!("there is no engine “{id}”")));
        }
        models.default_engines.set(tier, engine);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_catalog::{ANTHROPIC_MODELS, ProviderCatalog};
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};

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

    fn catalog_with(provider: ProviderId, models: Vec<CatalogModel>) -> ModelCatalog {
        let mut catalog = ModelCatalog::default();
        catalog.providers.insert(
            provider,
            ProviderCatalog {
                models,
                fetched_at: None,
                error: None,
            },
        );
        catalog
    }

    #[test]
    fn claude_code_offers_the_newest_model_of_each_family_and_sonnet_5_drops_out() {
        let catalog = ModelCatalog::default();
        let ids: Vec<&str> = offered(&catalog, ProviderId::Anthropic)
            .iter()
            .map(|model| model.id.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "claude-fable-5-1",
                "claude-opus-5-5",
                "claude-sonnet-5-5",
                "claude-haiku-5-5"
            ]
        );
        // The order of the list does not decide: an older model listed first still loses to the newer one.
        let models = vec![
            listed("claude-opus-5", "Opus 5", None),
            listed("claude-opus-5-5", "Opus 5.5", None),
            listed("claude-opus-4-8", "Opus 4.8", None),
            listed("something-else", "Other", None),
        ];
        let ids: Vec<&str> = anthropic_offered(&models)
            .iter()
            .map(|model| model.id.as_str())
            .collect();
        assert_eq!(ids, ["claude-opus-5-5", "something-else"]);
    }

    #[test]
    fn a_dated_snapshot_does_not_outrank_a_newer_version() {
        let models = vec![
            listed("claude-sonnet-5-20260101", "Sonnet 5 (dated)", None),
            listed("claude-sonnet-5-5", "Sonnet 5.5", None),
        ];
        let ids: Vec<&str> = anthropic_offered(&models)
            .iter()
            .map(|model| model.id.as_str())
            .collect();
        assert_eq!(ids, ["claude-sonnet-5-5"]);
        assert_eq!(
            anthropic_family("claude-sonnet-5-20260101"),
            Some(("sonnet", vec![5]))
        );
    }

    #[test]
    fn a_remote_name_is_bounded_and_printable_before_it_names_an_agent() {
        assert_eq!(
            short_name("Evil: Mo\u{202E}del\u{200B} \n\tX\u{7}"),
            "Model X"
        );
        let long = short_name(&"y".repeat(200));
        assert_eq!(long.chars().count(), MAX_MODEL_NAME_CHARS);
        assert!(long.ends_with('…'));
    }

    #[test]
    fn every_anthropic_model_has_a_list_price() {
        for (id, _) in ANTHROPIC_MODELS {
            assert!(model_catalog::anthropic_list_price(id).is_some(), "{id}");
        }
        assert!(model_catalog::anthropic_list_price("claude-unknown").is_none());
    }

    #[test]
    fn a_short_list_is_on_as_a_whole_and_a_long_one_waits_for_the_owner() {
        let mut config = Config::default();
        let short: Vec<_> = (0..SMALL_LIST_MAX)
            .map(|n| listed(&format!("v/m{n}"), "M", Some(1.0)))
            .collect();
        let long: Vec<_> = (0..=SMALL_LIST_MAX)
            .map(|n| listed(&format!("v/m{n}"), "M", Some(1.0)))
            .collect();
        let short = catalog_with(ProviderId::OpenRouter, short);
        let long = catalog_with(ProviderId::OpenRouter, long);
        assert_eq!(
            enabled_models(&config, &short, ProviderId::OpenRouter).len(),
            SMALL_LIST_MAX
        );
        assert!(enabled_models(&config, &long, ProviderId::OpenRouter).is_empty());

        config.agents.models.choices.insert(
            "openrouter/v/m3".into(),
            ModelChoice {
                enabled: Some(true),
                tier: None,
            },
        );
        config.agents.models.choices.insert(
            "openrouter/v/m4".into(),
            ModelChoice {
                enabled: Some(false),
                tier: None,
            },
        );
        let on: Vec<_> = enabled_models(&config, &long, ProviderId::OpenRouter)
            .iter()
            .map(|model| model.id.clone())
            .collect();
        assert_eq!(on, ["v/m3"]);
        assert_eq!(
            enabled_models(&config, &short, ProviderId::OpenRouter).len(),
            SMALL_LIST_MAX - 1
        );
        let sources = ModelSources {
            catalog: long,
            proposals: TierProposals::default(),
        };
        let entries = models(&config, &sources);
        let m3 = entries.iter().find(|m| m.key == "openrouter/v/m3").unwrap();
        assert!(m3.enabled && !m3.enabled_by_default);
    }

    fn proposal(tier: Tier) -> TierProposal {
        TierProposal {
            tier,
            reason: "Ranks near the top of the coding board.".into(),
            source: "LMArena".into(),
            at: "2026-10-30T10:00:00Z".into(),
        }
    }

    #[test]
    fn the_owner_beats_research_which_beats_the_price() {
        let research = proposal(Tier::Heavy);
        assert_eq!(
            effective_tier(Some(Tier::Light), Some(&research), Some(Tier::Medium)),
            Rated {
                tier: Some(Tier::Light),
                source: TierSource::Owner
            }
        );
        assert_eq!(
            effective_tier(None, Some(&research), Some(Tier::Medium)),
            Rated {
                tier: Some(Tier::Heavy),
                source: TierSource::Research
            }
        );
        assert_eq!(
            effective_tier(None, None, Some(Tier::Medium)).source,
            TierSource::Price
        );
        assert_eq!(
            effective_tier(None, None, None),
            Rated {
                tier: None,
                source: TierSource::Unrated
            }
        );
    }

    #[test]
    fn the_price_proposes_at_the_limits_and_a_local_model_stays_unrated() {
        let limits = TierLimits::default();
        assert_eq!(limits.propose(0.99), Tier::Light);
        assert_eq!(limits.propose(DEFAULT_LIGHT_BELOW_USD), Tier::Medium);
        assert_eq!(limits.propose(DEFAULT_HEAVY_ABOVE_USD), Tier::Medium);
        assert_eq!(limits.propose(10.01), Tier::Heavy);

        let config = Config::default();
        let sources = ModelSources::default();
        let tier = |id: &str| {
            rate(&config, &sources, ProviderId::Anthropic, id)
                .rated
                .tier
        };
        assert_eq!(tier("claude-haiku-5-5"), Some(Tier::Light));
        assert_eq!(tier("claude-sonnet-5-5"), Some(Tier::Medium));
        assert_eq!(tier("claude-opus-5-5"), Some(Tier::Heavy));

        let mut sources = ModelSources {
            catalog: catalog_with(
                ProviderId::Ollama,
                vec![listed("qwen3:32b", "qwen3:32b", None)],
            ),
            proposals: TierProposals::default(),
        };
        let local = rate(&config, &sources, ProviderId::Ollama, "qwen3:32b");
        assert_eq!(local.rated.source, TierSource::Unrated);
        sources
            .proposals
            .proposals
            .insert("ollama/qwen3:32b".into(), proposal(Tier::Light));
        let local = rate(&config, &sources, ProviderId::Ollama, "qwen3:32b");
        assert_eq!(local.rated.tier, Some(Tier::Light));
        assert_eq!(local.rated.source, TierSource::Research);
    }

    #[test]
    fn limits_out_of_order_or_beyond_any_price_are_refused() {
        let bad = |light_below, heavy_above| {
            TierLimits {
                light_below,
                heavy_above,
            }
            .validate()
            .is_err()
        };
        assert!(bad(11.0, 10.0));
        assert!(bad(-1.0, 10.0));
        assert!(bad(1.0, f64::INFINITY));
        assert!(bad(f64::NAN, 10.0));
        assert!(!bad(2.0, 2.0));
    }

    #[test]
    fn model_keys_name_a_provider_and_an_id() {
        assert_eq!(
            parse_model_key("openrouter/deepseek/deepseek-v4.1-flash"),
            Some((ProviderId::OpenRouter, "deepseek/deepseek-v4.1-flash"))
        );
        assert_eq!(
            parse_model_key(&model_key(ProviderId::Ollama, "gemma4:12b")),
            Some((ProviderId::Ollama, "gemma4:12b"))
        );
        for bad in [
            "deepseek/x",
            "openrouter/",
            "anthropic",
            "open_router/x",
            "ollama/ x",
            "ollama/a\nb",
        ] {
            assert_eq!(parse_model_key(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn names_lose_their_vendor_prefix() {
        assert_eq!(
            short_name("DeepSeek: DeepSeek V4.1 Flash"),
            "DeepSeek V4.1 Flash"
        );
        assert_eq!(short_name("Anthropic: Claude Sonnet 5.5"), "Sonnet 5.5");
        assert_eq!(short_name("Claude Opus 5.5"), "Opus 5.5");
        assert_eq!(short_name("qwen3:32b"), "qwen3:32b");
    }

    /// Runs `body` with `AXIOMATA_HOME` pointed at a fresh folder, and a workspace beside it rather than the owner's.
    fn with_temp_home(body: impl FnOnce(&std::path::Path)) {
        let _guard = ENV_MUTEX.lock().unwrap_or_else(|p| p.into_inner());
        let home = unique_temp_dir("axiomata-test-model-tiers");
        // SAFETY: serialised by ENV_MUTEX; no other thread of this test reads the environment meanwhile.
        unsafe {
            std::env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }
        Config {
            workspace_root: home.join("workspace"),
            ..Config::default()
        }
        .save()
        .unwrap();
        body(&home);
        unsafe {
            std::env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn choices_limits_and_defaults_reach_the_file_config_and_the_live_one() {
        with_temp_home(|home| {
            let mut live = Config {
                workspace_root: home.join("live"),
                ..Config::default()
            };
            let choice = ModelChoice {
                enabled: Some(true),
                tier: Some(Tier::Heavy),
            };
            save_choice(&mut live, "openrouter/a/b", choice).unwrap();
            save_tier_limits(
                &mut live,
                TierLimits {
                    light_below: 0.5,
                    heavy_above: 20.0,
                },
            )
            .unwrap();
            save_default_engine(
                &mut live,
                Tier::Light,
                Some("claude-code-claude-haiku-5-5".into()),
            )
            .unwrap();
            assert!(save_default_engine(&mut live, Tier::Heavy, Some("nowhere".into())).is_err());
            assert!(save_choice(&mut live, "nope", choice).is_err());

            let disk = Config::load().unwrap();
            assert_eq!(
                disk.workspace_root,
                home.join("workspace"),
                "the file's root stays"
            );
            assert_eq!(disk.agents.models, live.agents.models);
            assert_eq!(disk.agents.models.choices["openrouter/a/b"], choice);
            assert_eq!(disk.agents.models.tier_limits.light_below, 0.5);
            assert_eq!(
                disk.agents.models.default_engines.get(Tier::Light),
                Some("claude-code-claude-haiku-5-5")
            );

            save_choice(&mut live, "openrouter/a/b", ModelChoice::default()).unwrap();
            assert!(Config::load().unwrap().agents.models.choices.is_empty());
        });
    }

    #[test]
    fn proposals_live_in_their_own_file_and_bad_ones_are_refused() {
        with_temp_home(|_| {
            assert!(load_proposals().proposals.is_empty());
            record_proposal("ollama/qwen3:32b", proposal(Tier::Medium)).unwrap();
            let mut long = proposal(Tier::Light);
            long.reason = "x".repeat(MAX_REASON_CHARS + 1);
            assert!(record_proposal("ollama/qwen3:32b", long).is_err());
            let mut undated = proposal(Tier::Light);
            undated.at = "yesterday".into();
            assert!(record_proposal("ollama/qwen3:32b", undated).is_err());
            assert!(record_proposal("qwen3:32b", proposal(Tier::Light)).is_err());

            let stored = load_proposals();
            assert_eq!(stored.proposals["ollama/qwen3:32b"].tier, Tier::Medium);
            assert!(paths::model_tiers_path().exists());
            // The app and a skill run's CLI both write it: the write took the lock file beside it.
            assert!(paths::model_tiers_lock_path().exists());
            assert!(
                !std::fs::read_to_string(paths::config_path())
                    .unwrap()
                    .contains("qwen3"),
                "a proposal never lands in the config"
            );
            assert!(forget_proposal("ollama/qwen3:32b").unwrap());
            assert!(!forget_proposal("ollama/qwen3:32b").unwrap());
        });
    }

    #[test]
    fn a_bad_proposal_in_the_file_is_skipped_and_the_good_ones_stay() {
        with_temp_home(|_| {
            let good = serde_json::to_value(proposal(Tier::Light)).unwrap();
            let mut bad_tier = good.clone();
            bad_tier["tier"] = "huge".into();
            let mut long_reason = good.clone();
            long_reason["reason"] = "x".repeat(MAX_REASON_CHARS + 1).into();
            let file = serde_json::json!({ "version": PROPOSALS_VERSION, "proposals": {
                "ollama/good": good.clone(),
                "ollama/bad-tier": bad_tier,
                "ollama/long": long_reason,
                "nokey": good,
            }});
            std::fs::write(paths::model_tiers_path(), file.to_string()).unwrap();
            let keys: Vec<String> = load_proposals().proposals.into_keys().collect();
            assert_eq!(keys, ["ollama/good"]);
        });
    }

    #[test]
    fn a_change_of_one_field_keeps_the_other_as_the_file_has_it() {
        with_temp_home(|home| {
            let mut stale = Config {
                workspace_root: home.join("live"),
                ..Config::default()
            };
            let mut other = stale.clone();
            save_choice(
                &mut other,
                "anthropic/claude-opus-5-5",
                ModelChoice {
                    enabled: None,
                    tier: Some(Tier::Medium),
                },
            )
            .unwrap();
            // `stale` never saw the tier; switching the model off through it keeps the tier on disk.
            edit_choice(&mut stale, "anthropic/claude-opus-5-5", |choice| {
                choice.enabled = Some(false);
            })
            .unwrap();
            assert_eq!(
                Config::load().unwrap().agents.models.choices["anthropic/claude-opus-5-5"],
                ModelChoice {
                    enabled: Some(false),
                    tier: Some(Tier::Medium)
                }
            );
        });
    }
}
