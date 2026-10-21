//! The providers' model catalogs (board card #17): which models each provider offers, kept in
//! `~/.axiomata/model-catalog.json`, so every place that names a model offers a list instead of a text field.
//!
//! OpenRouter's list carries the prices — with the app's key only the models the account allows (`/models/user`) —,
//! so a refresh also writes them into `agents.costs` for every
//! model the app uses (owner, 2026-10-19: always overwrite — the meter follows the provider's list price). Ollama
//! lists what is pulled on this machine; Anthropic has no list the app could ask without an API key (the CLI login is
//! a subscription), so its catalog is the fixed [`ANTHROPIC_MODELS`].
//!
//! A failed fetch keeps the provider's last list and records the error beside it: a catalog that went offline still
//! offers what it offered before.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::RwLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{self, Config, Effort, ModelCost, ProviderId, ProviderSettings};
use crate::error::AxiomataError;
use crate::json_state;
use crate::paths;

/// On-disk schema version of `model-catalog.json`.
pub const CATALOG_VERSION: u64 = 1;

/// Anthropic's models as the Claude Code CLI names them. No price: Anthropic runs on the subscription.
pub const ANTHROPIC_MODELS: [(&str, &str); 5] = [
    ("claude-fable-5-1", "Claude Fable 5.1"),
    ("claude-opus-5-5", "Claude Opus 5.5"),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
    ("claude-sonnet-5", "Claude Sonnet 5"),
    ("claude-haiku-5-5", "Claude Haiku 5.5"),
];

/// How long one provider may take to answer; the catalog is a convenience and must never hold up the app.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);

/// The largest answer read, in bytes. OpenRouter's whole list was ~0.8 MB on 2026-10-19; a body far beyond that is
/// not a model list, and buffering it unbounded would let a misbehaving endpoint fill the memory.
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// The highest list price taken, in USD per million tokens. Real prices stay far below; anything above is a broken
/// or hostile answer, and an infinite one would make every later Settings save fail validation.
const MAX_USD_PER_M: f64 = 1000.0;

/// Longest model id or name kept, in characters — the picker shows them, the event carries them to the webview.
const MAX_ID_CHARS: usize = 200;

/// Serialises catalog refreshes in this process (the daily timer and the ↻ button), so two never interleave their
/// read-modify-write of `model-catalog.json` and drop each other's provider.
static REFRESH_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// One model a provider offers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CatalogModel {
    /// The id the provider's settings take (`deepseek/deepseek-v4.1-flash`, `gemma4:12b-mlx`, `claude-haiku-5-5`).
    pub id: String,
    /// A readable name; the id when the provider gives none.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<u64>,
    /// The list price; `None` when the provider gives none or prices the model per request (OpenRouter's routers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub price: Option<ModelCost>,
    /// Its effort levels (OpenRouter's `reasoning`); `None` when the provider does not say — every level is offered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ModelReasoning>,
}

/// What a reasoning model knows of Glut's effort levels (`docs/plans/glut-effort.md`, F1): the pickers offer these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelReasoning {
    /// The levels it takes, the least first; never empty.
    pub efforts: Vec<Effort>,
    /// The level it reasons at when none is sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<Effort>,
    /// Its reasoning cannot be turned off.
    #[serde(default)]
    pub mandatory: bool,
}

/// One provider's list and when it was last fetched.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProviderCatalog {
    #[serde(default)]
    pub models: Vec<CatalogModel>,
    /// RFC 3339 time of the last successful fetch; `None` for a list that is built in or never arrived.
    #[serde(default)]
    pub fetched_at: Option<String>,
    /// Why the last fetch failed; cleared by the next one that works.
    #[serde(default)]
    pub error: Option<String>,
}

/// Every provider's catalog, the content of `model-catalog.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCatalog {
    pub version: u64,
    #[serde(default)]
    pub providers: BTreeMap<ProviderId, ProviderCatalog>,
}

impl Default for ModelCatalog {
    fn default() -> Self {
        let mut catalog = Self {
            version: CATALOG_VERSION,
            providers: BTreeMap::new(),
        };
        catalog.fill_builtin();
        catalog
    }
}

impl ModelCatalog {
    /// Puts the fixed Anthropic list in place; it is code, so a stale file never decides it.
    fn fill_builtin(&mut self) {
        let models = ANTHROPIC_MODELS
            .iter()
            .map(|(id, name)| CatalogModel {
                id: (*id).to_string(),
                name: (*name).to_string(),
                context_length: None,
                price: None,
                reasoning: None,
            })
            .collect();
        self.providers.insert(
            ProviderId::Anthropic,
            ProviderCatalog {
                models,
                fetched_at: None,
                error: None,
            },
        );
    }
}

/// The stored catalog; the built-in one when the file is missing or unreadable (it is only a cache).
pub fn load() -> ModelCatalog {
    let loaded = json_state::load(&paths::model_catalog_path(), || {
        serde_json::to_string(&ModelCatalog::default()).unwrap_or_default()
    });
    let mut catalog = loaded
        .ok()
        .and_then(|state| serde_json::from_str::<ModelCatalog>(&state.json).ok())
        .unwrap_or_default();
    catalog.fill_builtin();
    catalog
}

fn save(catalog: &ModelCatalog) -> Result<(), AxiomataError> {
    let json =
        serde_json::to_string(catalog).map_err(|err| AxiomataError::InvalidDashboardState {
            path: paths::model_catalog_path(),
            reason: err.to_string(),
        })?;
    json_state::save(&paths::model_catalog_path(), &json)
}

/// Fetches the lists of `only` (every provider when `None`), stores the catalog and returns it.
///
/// A provider that fails keeps its previous models with the error noted; the stored file is written either way.
pub async fn refresh(config: &Config, only: Option<ProviderId>) -> ModelCatalog {
    let _refreshing = REFRESH_LOCK.lock().await;
    let mut catalog = load();
    // Neither endpoint redirects; following one would let a compromised endpoint bounce the request elsewhere.
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build();
    for id in ProviderId::ALL {
        if only.is_some_and(|wanted| wanted != id) || id == ProviderId::Anthropic {
            continue;
        }
        let settings = config
            .agents
            .providers
            .get(&id)
            .cloned()
            .unwrap_or_else(|| ProviderSettings::default_for(id));
        let fetched = match &client {
            Ok(client) => fetch(client, id, &settings).await,
            Err(err) => Err(err.to_string()),
        };
        let entry = catalog.providers.entry(id).or_default();
        match fetched {
            Ok(models) => {
                entry.models = models;
                entry.fetched_at = Some(chrono::Utc::now().to_rfc3339());
                entry.error = None;
            }
            Err(err) => {
                tracing::warn!(provider = ?id, %err, "model catalog fetch failed");
                entry.error = Some(err);
            }
        }
    }
    if let Err(err) = save(&catalog) {
        tracing::warn!(%err, "could not store the model catalog");
    }
    catalog
}

async fn fetch(
    client: &reqwest::Client,
    id: ProviderId,
    settings: &ProviderSettings,
) -> Result<Vec<CatalogModel>, String> {
    let base = settings
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or("no base URL set")?;
    // With a key, OpenRouter answers `/models/user`: only the models the account allows (its "Only allow" list,
    // provider preferences, guardrails — owner, 2026-10-19), priced alike. Without one, the public list. The key goes
    // over https only.
    let key = settings
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty() && base.starts_with("https://"));
    let (url, key) = match id {
        ProviderId::OpenRouter => {
            let path = if key.is_some() {
                "models/user"
            } else {
                "models"
            };
            (format!("{}/v1/{path}", base.trim_end_matches('/')), key)
        }
        // A local Ollama needs no key.
        ProviderId::Ollama => (format!("{}/api/tags", ollama_root(base)), None),
        ProviderId::Anthropic => return Err("Anthropic has no model list to fetch".into()),
    };
    let mut request = client.get(&url);
    if let Some(key) = key {
        request = request.bearer_auth(key);
    }
    // Errors drop the URL: a base URL may carry credentials (`user:pass@host`), and the error is stored, logged and
    // shown.
    let mut response = request
        .send()
        .await
        .map_err(|err| err.without_url().to_string())?;
    if !response.status().is_success() {
        return Err(format!("the provider answered {}", response.status()));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|err| err.without_url().to_string())?
    {
        if bytes.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(format!("the answer is larger than {MAX_BODY_BYTES} bytes"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: Value = serde_json::from_slice(&bytes).map_err(|err| err.to_string())?;
    match id {
        ProviderId::OpenRouter => parse_openrouter(&body),
        _ => parse_ollama(&body),
    }
}

/// Ollama's own API root: the settings may name its OpenAI-compatible `/v1` endpoint instead.
fn ollama_root(base: &str) -> &str {
    let base = base.trim_end_matches('/');
    base.strip_suffix("/v1").unwrap_or(base)
}

/// OpenRouter's `GET /api/v1/models`: `{ data: [{ id, name, context_length, pricing: { prompt, completion,
/// input_cache_read }, reasoning: { supported_efforts, default_effort, mandatory } }] }`, prices in USD per token as
/// strings.
fn parse_openrouter(body: &Value) -> Result<Vec<CatalogModel>, String> {
    let data = body
        .get("data")
        .and_then(Value::as_array)
        .ok_or("OpenRouter's answer has no model list")?;
    // An empty list is not "OpenRouter offers nothing"; keeping the last good one is the honest reading.
    if data.is_empty() {
        return Err("OpenRouter's model list is empty".into());
    }
    let mut models: Vec<CatalogModel> = data
        .iter()
        .filter_map(|model| {
            let id = sane_text(model.get("id")?.as_str()?)?;
            let name = model
                .get("name")
                .and_then(Value::as_str)
                .and_then(sane_text)
                .unwrap_or_else(|| id.clone());
            Some(CatalogModel {
                name,
                context_length: model.get("context_length").and_then(Value::as_u64),
                price: model.get("pricing").and_then(openrouter_price),
                reasoning: model.get("reasoning").and_then(openrouter_reasoning),
                id,
            })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

/// `text` as an id or name: `None` when empty, overlong or holding control characters.
fn sane_text(text: &str) -> Option<String> {
    let ok = !text.is_empty()
        && text.chars().count() <= MAX_ID_CHARS
        && !text.chars().any(char::is_control);
    ok.then(|| text.to_string())
}

fn openrouter_price(pricing: &Value) -> Option<ModelCost> {
    let per_m = |key: &str| -> Option<f64> {
        let per_m = pricing.get(key)?.as_str()?.parse::<f64>().ok()? * 1e6;
        // A negative price is OpenRouter's "decided per request" (its auto router): there is no list price. `parse`
        // also takes "inf" and "NaN", which the range check refuses with the absurd ones.
        (0.0..=MAX_USD_PER_M)
            .contains(&per_m)
            .then(|| round_price(per_m))
    };
    Some(ModelCost {
        input_per_m: per_m("prompt")?,
        cached_input_per_m: per_m("input_cache_read"),
        output_per_m: per_m("completion")?,
    })
}

/// A model's effort levels; `None` when it names no level Glut knows (a model that does not reason, or one that
/// only says whether it does). A level of a newer API that Glut lacks is left out rather than failing the list.
fn openrouter_reasoning(reasoning: &Value) -> Option<ModelReasoning> {
    let named = |value: &Value| value.as_str().and_then(Effort::parse);
    let listed: Vec<Effort> = reasoning
        .get("supported_efforts")?
        .as_array()?
        .iter()
        .filter_map(named)
        .collect();
    let efforts: Vec<Effort> = Effort::ALL
        .into_iter()
        .filter(|level| listed.contains(level))
        .collect();
    if efforts.is_empty() {
        return None;
    }
    Some(ModelReasoning {
        efforts,
        default_effort: reasoning.get("default_effort").and_then(named),
        mandatory: reasoning
            .get("mandatory")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Drops the float noise of `per token × 10⁶` (0.14 arriving as 0.13999999999999999).
fn round_price(usd_per_m: f64) -> f64 {
    (usd_per_m * 1e9).round() / 1e9
}

/// Ollama's `GET /api/tags`: `{ models: [{ name, … }] }`. Local models carry no price.
fn parse_ollama(body: &Value) -> Result<Vec<CatalogModel>, String> {
    let list = body
        .get("models")
        .and_then(Value::as_array)
        .ok_or("Ollama's answer has no model list")?;
    let mut models: Vec<CatalogModel> = list
        .iter()
        .filter_map(|model| {
            let id = sane_text(model.get("name")?.as_str()?)?;
            Some(CatalogModel {
                name: id.clone(),
                id,
                context_length: None,
                price: None,
                reasoning: None,
            })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

/// The OpenRouter models the app uses, bare ids as `agents.costs` keys them: the ones already priced, the
/// OpenRouter provider's chat and skill models, and every Opencode engine on `openrouter/…`.
fn openrouter_models_in_use(config: &Config) -> BTreeSet<String> {
    let mut used: BTreeSet<String> = config.agents.costs.keys().cloned().collect();
    if let Some(settings) = config.agents.providers.get(&ProviderId::OpenRouter) {
        used.extend(
            [&settings.chat_model, &settings.skill_model]
                .into_iter()
                .map(|model| model.trim().to_string())
                .filter(|model| !model.is_empty()),
        );
    }
    used.extend(config.agents.engines.values().filter_map(|engine| {
        engine
            .model
            .as_deref()?
            .trim()
            .strip_prefix("openrouter/")
            .map(str::to_string)
    }));
    used
}

/// Whether `price` charges nothing at all.
fn is_free(price: &ModelCost) -> bool {
    price.input_per_m == 0.0 && price.output_per_m == 0.0
}

/// The prices that differ from `config.agents.costs`: every model in use that OpenRouter's catalog lists with a
/// price. A model the catalog does not know keeps whatever the owner wrote for it.
///
/// Two exceptions to "the list price wins": a paid price is never replaced by a free one — the meter returns nothing
/// for a free model, which would quietly take it out of the daily cap — and a cache price the list does not give
/// keeps the one the owner set, rather than falling back to the full input price.
pub fn price_updates(config: &Config, catalog: &ModelCatalog) -> BTreeMap<String, ModelCost> {
    let Some(listed) = catalog.providers.get(&ProviderId::OpenRouter) else {
        return BTreeMap::new();
    };
    let prices: BTreeMap<&str, &ModelCost> = listed
        .models
        .iter()
        .filter_map(|model| Some((model.id.as_str(), model.price.as_ref()?)))
        .collect();
    openrouter_models_in_use(config)
        .into_iter()
        .filter_map(|id| {
            let mut price = (*prices.get(id.as_str())?).clone();
            let current = config.agents.costs.get(&id);
            if let Some(current) = current {
                if is_free(&price) && !is_free(current) {
                    tracing::warn!(model = %id, "the catalog lists a paid model as free; its price is kept");
                    return None;
                }
                if price.cached_input_per_m.is_none() {
                    price.cached_input_per_m = current.cached_input_per_m;
                }
            }
            (current != Some(&price)).then_some((id, price))
        })
        .collect()
}

/// Writes the catalog's prices into the file config's `agents.costs` and the live `config`; returns the models whose
/// price changed.
///
/// Applied to the file config, freshly read, not by saving `config`: the live config keeps this session's workspace
/// root while the file may hold one queued for the next start (the same reason as `roster::save_engine`).
pub fn apply_prices(
    config: &mut Config,
    catalog: &ModelCatalog,
) -> Result<Vec<String>, AxiomataError> {
    let _guard = config::file_edit_lock();
    let mut disk = Config::load()?;
    let updates = price_updates(&disk, catalog);
    if !updates.is_empty() {
        disk.agents.costs.extend(updates.clone());
        // The same gate as a Settings save: a config this write would break is never written.
        disk.validate_for_save()
            .map_err(|reason| AxiomataError::InvalidDashboardState {
                path: paths::config_path(),
                reason,
            })?;
        disk.save()?;
    }
    config.agents.costs = disk.agents.costs;
    Ok(updates.into_keys().collect())
}

/// What [`refresh_and_price`] did.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefreshOutcome {
    pub catalog: ModelCatalog,
    /// The models whose price in `agents.costs` changed.
    pub repriced: Vec<String>,
    /// Why the prices could not be written, when they could not; the catalog itself was refreshed.
    pub price_error: Option<String>,
}

impl RefreshOutcome {
    /// Whether a provider's list could not be fetched — the caller should try again soon rather than in a day.
    pub fn any_failed(&self) -> bool {
        self.catalog.providers.values().any(|p| p.error.is_some())
    }
}

/// Refreshes the catalog of `only` (every provider when `None`) and writes the new prices — the one path the app's
/// timer, its refresh button and the CLI share. The live `config` is locked only for the synchronous price write,
/// never across the fetch.
pub async fn refresh_and_price(
    config: &RwLock<Config>,
    only: Option<ProviderId>,
) -> RefreshOutcome {
    let snapshot = config
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    let catalog = refresh(&snapshot, only).await;
    let mut live = config.write().unwrap_or_else(|poison| poison.into_inner());
    let (repriced, price_error) = match apply_prices(&mut live, &catalog) {
        Ok(repriced) => (repriced, None),
        Err(err) => (Vec::new(), Some(err.to_string())),
    };
    RefreshOutcome {
        catalog,
        repriced,
        price_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use serde_json::json;

    fn cost(input: f64, cached: Option<f64>, output: f64) -> ModelCost {
        ModelCost {
            input_per_m: input,
            cached_input_per_m: cached,
            output_per_m: output,
        }
    }

    fn openrouter_catalog(models: &[(&str, Option<ModelCost>)]) -> ModelCatalog {
        let mut catalog = ModelCatalog::default();
        catalog.providers.insert(
            ProviderId::OpenRouter,
            ProviderCatalog {
                models: models
                    .iter()
                    .map(|(id, price)| CatalogModel {
                        id: (*id).into(),
                        name: (*id).into(),
                        context_length: None,
                        price: price.clone(),
                        reasoning: None,
                    })
                    .collect(),
                fetched_at: None,
                error: None,
            },
        );
        catalog
    }

    #[test]
    fn openrouter_prices_arrive_per_million_tokens_and_routers_have_none() {
        let body = json!({ "data": [
            { "id": "z-ai/glm-5.3-flash", "name": "GLM 5.3 Flash", "context_length": 131072,
              "pricing": { "prompt": "0.00000015", "completion": "0.0000005", "input_cache_read": "0.000000006" } },
            { "id": "openrouter/auto", "pricing": { "prompt": "-1", "completion": "-1" } },
            { "id": "a/free:free", "pricing": { "prompt": "0", "completion": "0" } },
            { "name": "no id, skipped" }
        ]});
        let models = parse_openrouter(&body).unwrap();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            ["a/free:free", "openrouter/auto", "z-ai/glm-5.3-flash"]
        );
        assert_eq!(models[2].name, "GLM 5.3 Flash");
        assert_eq!(models[2].context_length, Some(131072));
        assert_eq!(models[2].price, Some(cost(0.15, Some(0.006), 0.5)));
        assert_eq!(models[1].price, None);
        assert_eq!(models[1].name, "openrouter/auto");
        assert_eq!(models[0].price, Some(cost(0.0, None, 0.0)));
        assert!(parse_openrouter(&json!({ "error": "x" })).is_err());
    }

    #[test]
    fn effort_levels_arrive_least_first_and_unknown_or_missing_ones_say_nothing() {
        let body = json!({ "data": [
            { "id": "deepseek/v4", "reasoning": { "mandatory": false, "default_enabled": true,
              "supported_efforts": ["max", "high", "low", "none"], "default_effort": "high" } },
            { "id": "glm/flash", "reasoning": { "mandatory": true, "supported_efforts": ["max", "ultra", 3] } },
            { "id": "plain/model", "reasoning": { "mandatory": false } },
            { "id": "odd/model", "reasoning": { "supported_efforts": ["ultra"], "default_effort": "high" } },
            { "id": "old/model" }
        ]});
        let models = parse_openrouter(&body).unwrap();
        let reasoning = |id: &str| {
            models
                .iter()
                .find(|m| m.id == id)
                .unwrap()
                .reasoning
                .clone()
        };
        assert_eq!(
            reasoning("deepseek/v4"),
            Some(ModelReasoning {
                efforts: vec![Effort::None, Effort::Low, Effort::High, Effort::Max],
                default_effort: Some(Effort::High),
                mandatory: false,
            })
        );
        let glm = reasoning("glm/flash").unwrap();
        assert_eq!(
            (glm.efforts, glm.default_effort, glm.mandatory),
            (vec![Effort::Max], None, true)
        );
        assert_eq!(reasoning("plain/model"), None);
        // OpenRouter's default need not be among the listed levels; it is what the model does unasked, kept as said.
        let other = parse_openrouter(&json!({ "data": [
            { "id": "x/y", "reasoning": { "supported_efforts": ["low"], "default_effort": "medium" } }
        ]}))
        .unwrap();
        let other = other[0].reasoning.clone().unwrap();
        assert_eq!(
            (other.efforts, other.default_effort),
            (vec![Effort::Low], Some(Effort::Medium))
        );
        assert_eq!(reasoning("odd/model"), None);
        assert_eq!(reasoning("old/model"), None);
    }

    #[test]
    fn a_catalog_written_before_the_levels_still_loads() {
        let old = r#"{ "version": 1, "providers": { "open_router": { "models": [ { "id": "a/b", "name": "A" } ] } } }"#;
        let catalog: ModelCatalog = serde_json::from_str(old).unwrap();
        assert_eq!(
            catalog.providers[&ProviderId::OpenRouter].models[0].reasoning,
            None
        );
    }

    #[test]
    fn absurd_prices_and_odd_ids_are_dropped_and_an_empty_list_is_an_error() {
        let body = json!({ "data": [
            { "id": "inf/model", "pricing": { "prompt": "inf", "completion": "0.000001" } },
            { "id": "nan/model", "pricing": { "prompt": "NaN", "completion": "0.000001" } },
            { "id": "huge/model", "pricing": { "prompt": "1e300", "completion": "0.000001" } },
            { "id": "bad\u{7}id", "pricing": { "prompt": "0", "completion": "0" } },
            { "id": "x".repeat(MAX_ID_CHARS + 1) }
        ]});
        let models = parse_openrouter(&body).unwrap();
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["huge/model", "inf/model", "nan/model"]);
        assert!(models.iter().all(|m| m.price.is_none()));
        assert!(parse_openrouter(&json!({ "data": [] })).is_err());
    }

    #[test]
    fn a_paid_price_is_never_replaced_by_a_free_one_and_a_known_cache_price_stays() {
        let mut config = Config::default();
        config
            .agents
            .costs
            .insert("paid/model".into(), cost(0.1, None, 0.2));
        config
            .agents
            .costs
            .insert("cached/model".into(), cost(0.1, Some(0.01), 0.2));
        let catalog = openrouter_catalog(&[
            ("paid/model", Some(cost(0.0, None, 0.0))),
            ("cached/model", Some(cost(0.3, None, 0.4))),
        ]);
        let updates = price_updates(&config, &catalog);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates["cached/model"], cost(0.3, Some(0.01), 0.4));
        assert!(price_updates(&config, &ModelCatalog::default()).is_empty());
    }

    #[test]
    fn ollama_lists_its_pulled_models_without_prices() {
        let body =
            json!({ "models": [{ "name": "qwen3.8:27b-mlx" }, { "name": "gemma4:12b-mlx" }] });
        let models = parse_ollama(&body).unwrap();
        assert_eq!(models[0].id, "gemma4:12b-mlx");
        assert_eq!(models[1].price, None);
        assert!(parse_ollama(&json!({})).is_err());
        assert_eq!(
            ollama_root("http://localhost:11434/v1/"),
            "http://localhost:11434"
        );
        assert_eq!(
            ollama_root("http://localhost:11434"),
            "http://localhost:11434"
        );
    }

    #[test]
    fn prices_follow_the_list_for_every_model_in_use_and_only_those() {
        let mut config = Config::default();
        config
            .agents
            .costs
            .insert("old/priced".into(), cost(9.0, None, 9.0));
        config
            .agents
            .costs
            .insert("same/price".into(), cost(1.0, None, 2.0));
        config
            .agents
            .costs
            .insert("unlisted/kept".into(), cost(3.0, None, 3.0));
        config
            .agents
            .providers
            .get_mut(&ProviderId::OpenRouter)
            .unwrap()
            .chat_model = " chat/model ".into();
        let engine = axiomata_roster::Engine {
            id: "e".into(),
            label: "E".into(),
            harness: axiomata_roster::Harness::Opencode,
            command: String::new(),
            model: Some("openrouter/engine/model".into()),
            env: String::new(),
            billing: axiomata_roster::Billing::Metered,
        };
        config.agents.engines.insert("e".into(), engine);

        let catalog = openrouter_catalog(&[
            ("old/priced", Some(cost(0.1, Some(0.01), 0.2))),
            ("same/price", Some(cost(1.0, None, 2.0))),
            ("chat/model", Some(cost(0.3, None, 0.4))),
            ("engine/model", Some(cost(0.5, None, 0.6))),
            ("not/used", Some(cost(7.0, None, 7.0))),
            ("router/none", None),
        ]);
        let updates = price_updates(&config, &catalog);
        let ids: Vec<&str> = updates.keys().map(String::as_str).collect();
        assert_eq!(ids, ["chat/model", "engine/model", "old/priced"]);
        assert_eq!(updates["old/priced"], cost(0.1, Some(0.01), 0.2));
    }

    #[test]
    fn a_refreshed_price_reaches_the_file_and_the_live_config_but_not_the_workspace_root() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-catalog");
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe { std::env::set_var(paths::AXIOMATA_HOME_ENV, &home) };

        let mut on_disk = Config {
            workspace_root: home.join("queued-vault"),
            ..Config::default()
        };
        on_disk
            .agents
            .costs
            .insert("a/model".into(), cost(1.0, None, 1.0));
        on_disk.save().unwrap();
        let mut live = on_disk.clone();
        live.workspace_root = home.join("this-session");

        let catalog = openrouter_catalog(&[("a/model", Some(cost(0.2, None, 0.4)))]);
        assert_eq!(apply_prices(&mut live, &catalog).unwrap(), ["a/model"]);
        assert_eq!(live.agents.costs["a/model"], cost(0.2, None, 0.4));
        let reread = Config::load().unwrap();
        assert_eq!(reread.agents.costs["a/model"], cost(0.2, None, 0.4));
        assert_eq!(reread.workspace_root, home.join("queued-vault"));
        assert!(apply_prices(&mut live, &catalog).unwrap().is_empty());

        save(&catalog).unwrap();
        assert_eq!(load(), catalog);

        unsafe { std::env::remove_var(paths::AXIOMATA_HOME_ENV) };
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_anthropic_list_is_built_in_even_over_a_stale_file() {
        let mut catalog = ModelCatalog::default();
        catalog
            .providers
            .get_mut(&ProviderId::Anthropic)
            .unwrap()
            .models
            .clear();
        catalog.fill_builtin();
        assert_eq!(
            catalog.providers[&ProviderId::Anthropic].models.len(),
            ANTHROPIC_MODELS.len()
        );
        let json = serde_json::to_string(&catalog).unwrap();
        assert!(json.contains("\"anthropic\""), "{json}");
    }
}
