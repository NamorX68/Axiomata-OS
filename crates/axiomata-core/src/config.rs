//! Loading and saving `~/.axiomata/config.toml`.
//!
//! Holds the user-configurable Second-Brain workspace root and agent backend
//! defaults (e.g. the default Ollama model).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::agents::claude_code::valid_model_name;
use crate::error::AxiomataError;
use crate::paths;

/// Default Second-Brain workspace root, used when no config file exists yet.
///
/// Deliberately independent of `AXIOMATA_HOME`: the app's own data directory
/// and the user's Second-Brain content are separate concepts (see the plan's
/// "App-eigene Daten vs. Second-Brain-Workspace" section), so this always
/// resolves under the real home directory.
fn default_workspace_root() -> PathBuf {
    home::home_dir()
        .expect("could not determine the current user's home directory")
        .join("Axiomata-Workspace")
}

fn default_ollama_model() -> String {
    "llama3.2".to_string()
}

/// Default value for the legacy flat `claude_model` field. Only used as a
/// migration source now — [`AgentDefaults::migrate_legacy_model_if_needed`]
/// folds it into `providers[Anthropic]`'s `chat_model`/`skill_model` when
/// upgrading a config saved before per-provider models existed. New installs
/// never read this: [`ProviderSettings::default_for`] carries the real
/// per-provider, per-task defaults instead.
fn default_claude_model() -> String {
    "claude-haiku-4-5".to_string()
}

/// Default hard wall-clock limit for a single skill run.
fn default_skill_timeout_secs() -> u64 {
    300
}

/// Default daily spend cap (USD) for paid model-routing providers. Applies to
/// every non-Anthropic active provider combined (the Anthropic path is
/// subscription-billed and never metered here). Deliberately low: the
/// 2026-09-08 incident ran up ~$9.71 unnoticed — $2/day is enough for real
/// use and turns a runaway into a same-day stop. `None` disables the cap.
fn default_daily_usd_cap() -> Option<f64> {
    Some(2.0)
}

/// Which model-routing provider the `claude` child process talks to.
/// Execution always stays on the Claude Code CLI/agent loop — this only
/// selects the upstream API endpoint the CLI is pointed at, the same way
/// `ANTHROPIC_BASE_URL` already lets it reach Bedrock or a proxy today. Not
/// to be confused with `AgentBackend::Ollama` (`agents/ollama.rs`), an
/// entirely separate raw/tool-free completion backend selectable per skill —
/// see `docs/plans/settings-provider-overhaul.md`'s scoping note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    #[default]
    Anthropic,
    OpenRouter,
    Ollama,
}

impl ProviderId {
    /// Every provider offered in the Settings UI, in display order. Loop
    /// over this — never enumerate the variants by hand at a call site — so
    /// adding a fourth provider (e.g. the deferred LM Studio, once its own
    /// Phase 0 spike is done) is a one-line addition here.
    pub const ALL: [ProviderId; 3] = [
        ProviderId::Anthropic,
        ProviderId::OpenRouter,
        ProviderId::Ollama,
    ];

    /// The environment variable this provider's API credential goes into.
    /// Anthropic wants a direct key (`x-api-key` header via
    /// `ANTHROPIC_API_KEY`); OpenRouter and Ollama both front the Anthropic
    /// Messages API through a bearer token instead (`ANTHROPIC_AUTH_TOKEN`) —
    /// setting `ANTHROPIC_API_KEY` alongside a non-empty value there makes
    /// the CLI prefer the (wrong or absent) direct key. Confirmed in the
    /// Phase 0 spike, `docs/plans/settings-provider-overhaul.md`.
    pub fn auth_env_var(self) -> &'static str {
        match self {
            ProviderId::Anthropic => "ANTHROPIC_API_KEY",
            ProviderId::OpenRouter | ProviderId::Ollama => "ANTHROPIC_AUTH_TOKEN",
        }
    }

    /// The stable lowercase token this provider is stored as — matches the
    /// `#[serde(rename_all = "snake_case")]` variant name and the value
    /// written into a run's `provider` column. Kept as an explicit method so
    /// call sites don't reach for `serde_json` just to name a provider.
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderId::Anthropic => "anthropic",
            ProviderId::OpenRouter => "open_router",
            ProviderId::Ollama => "ollama",
        }
    }

    /// Whether a run through this provider redirects the Claude CLI at a paid
    /// third-party endpoint (`ANTHROPIC_BASE_URL`). Anthropic goes through the
    /// CLI's own subscription login and is never metered here; everything else
    /// is subject to the spend guardrail.
    pub fn is_redirected(self) -> bool {
        !matches!(self, ProviderId::Anthropic)
    }
}

/// Which role a `claude` turn is filling, so the provider (and therefore the
/// model, base URL, and credential) can be picked per role: interactive
/// dashboard chat vs. unattended skill / routine runs. See
/// [`AgentDefaults::provider_for`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderRole {
    /// Interactive dashboard-assistant turns.
    Chat,
    /// Skill and routine runs (`SKILL.md` body or a routine's raw prompt).
    Skill,
}

/// Per-provider settings: where its Messages-API-compatible endpoint lives,
/// what credential (if any) it needs, and which model to use for
/// interactive chat vs. unattended skill/routine runs. Kept independently
/// per provider (`AgentDefaults::providers`) so switching the active
/// provider in the Settings UI never discards what was typed for the others.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderSettings {
    /// `ANTHROPIC_BASE_URL` for this provider. `None` for Anthropic itself —
    /// it talks to the real API via the CLI's own subscription login, no
    /// override needed.
    #[serde(default)]
    pub base_url: Option<String>,

    /// Credential for `ProviderId::auth_env_var()`. Plaintext in
    /// `config.toml`, same trust boundary as `agents.claude_env` today.
    /// `None`/empty for Anthropic (billed via the CLI's own subscription
    /// login; no key stored here).
    #[serde(default)]
    pub api_key: Option<String>,

    /// Model passed as `claude --model` for interactive chat turns.
    #[serde(default)]
    pub chat_model: String,

    /// Model passed as `claude --model` for skill/routine runs, unless a
    /// skill's own `SKILL.md` frontmatter pins one.
    #[serde(default)]
    pub skill_model: String,
}

impl ProviderSettings {
    /// Starting values for a provider that hasn't been configured yet: base
    /// URLs confirmed in the Phase 0 spike, models left for the owner to
    /// pick except Anthropic's (which needs no key/URL and so is usable
    /// immediately). There's no single "right" default the way there was
    /// for the old global model — that's the point of splitting it out.
    pub fn default_for(id: ProviderId) -> Self {
        match id {
            ProviderId::Anthropic => Self {
                base_url: None,
                api_key: None,
                chat_model: "claude-sonnet-5".to_string(),
                skill_model: "claude-haiku-4-5".to_string(),
            },
            ProviderId::OpenRouter => Self {
                base_url: Some("https://openrouter.ai/api".to_string()),
                api_key: None,
                chat_model: String::new(),
                skill_model: String::new(),
            },
            ProviderId::Ollama => Self {
                base_url: Some("http://localhost:11434".to_string()),
                // No real credential needed for a local Ollama instance, but
                // the endpoint still requires *a* bearer token to be
                // present; "ollama" is the literal placeholder Ollama's own
                // Claude Code integration docs use — see the Phase 0 spike.
                api_key: Some("ollama".to_string()),
                chat_model: String::new(),
                skill_model: String::new(),
            },
        }
    }
}

/// Defaults for the built-in agent backends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentDefaults {
    /// Legacy flat Claude model field, pre-dating per-provider models.
    /// Superseded by `providers[active_provider].chat_model`/`skill_model`;
    /// kept only so [`Self::migrate_legacy_model_if_needed`] has a value to
    /// migrate from when loading a config saved before this field's
    /// replacement existed. Not read anywhere else, and **not written back**
    /// (`skip_serializing`) — once the migration has folded it into the
    /// providers map it is dead weight, so a fresh `config.toml` no longer
    /// carries it; `serde(default)` still supplies it when reading an old file.
    #[serde(default = "default_claude_model", skip_serializing)]
    pub claude_model: String,

    /// Ollama model used when a skill or routine doesn't specify one. (This
    /// is `AgentBackend::Ollama`'s own raw-completion backend, unrelated to
    /// the `ProviderId::Ollama` provider below — see that enum's doc.)
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,

    /// Hard wall-clock limit for a single skill run, in seconds.
    #[serde(default = "default_skill_timeout_secs")]
    pub skill_timeout_secs: u64,

    /// Extra environment variables passed to the `claude` process for provider
    /// routing (`ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`,
    /// `CLAUDE_CODE_USE_BEDROCK`, …). Empty means the real Anthropic API.
    ///
    /// Only keys matching an allow-list of prefixes (`ANTHROPIC_`,
    /// `CLAUDE_CODE_`, `AWS_`, the proxy variables) are actually forwarded;
    /// loader / `PATH` variables are dropped. Values are stored in plaintext in
    /// `config.toml`, so treat a token here as a plaintext secret. A
    /// power-user escape hatch layered on top of `providers` below (e.g. for
    /// Bedrock, which doesn't fit the provider model at all).
    #[serde(default)]
    pub claude_env: BTreeMap<String, String>,

    /// Per-provider settings, keyed by `ProviderId`. Always has an entry for
    /// every `ProviderId::ALL` member after loading — see
    /// [`Self::migrate_legacy_model_if_needed`] — so UI/runtime code can
    /// index it directly instead of falling back to `default_for`.
    #[serde(default)]
    pub providers: BTreeMap<ProviderId, ProviderSettings>,

    /// Pre-split single provider switch. Superseded by `chat_provider` /
    /// `skill_provider`; kept only so [`Self::migrate_legacy_provider_if_needed`]
    /// can fold it into both new fields when loading a config saved before the
    /// per-role split. Not written back (`skip_serializing`) — a fresh
    /// `config.toml` carries only the two role fields.
    #[serde(default, rename = "active_provider", skip_serializing)]
    pub legacy_active_provider: Option<ProviderId>,

    /// Which provider serves interactive dashboard-chat turns — its
    /// `chat_model`, `base_url`, and credential. See [`Self::provider_for`].
    #[serde(default)]
    pub chat_provider: ProviderId,

    /// Which provider serves unattended skill / routine runs — its
    /// `skill_model`, `base_url`, and credential. A skill's own `SKILL.md`
    /// `model:` frontmatter still overrides the model. See [`Self::provider_for`].
    #[serde(default)]
    pub skill_provider: ProviderId,

    /// Daily spend cap in USD, applied per paid (non-Anthropic) role provider.
    /// Checked before every redirected agent turn against the sum of today's
    /// recorded `cost_usd` (skill runs + chat turns) for *that role's*
    /// provider; over the cap, the turn is refused and not spawned. `None`
    /// disables the cap. See [`crate::spend`] and
    /// `docs/plans/provider-hardening.md` checkpoint 5.
    #[serde(default = "default_daily_usd_cap")]
    pub daily_usd_cap: Option<f64>,
}

/// Every provider seeded with its starting settings — the shared source for
/// both [`AgentDefaults::default`] and [`AgentDefaults::migrate_legacy_model_if_needed`].
fn seeded_providers() -> BTreeMap<ProviderId, ProviderSettings> {
    ProviderId::ALL
        .into_iter()
        .map(|id| (id, ProviderSettings::default_for(id)))
        .collect()
}

impl AgentDefaults {
    /// Upgrades a config saved before per-provider models existed. **Seeds any
    /// `ProviderId::ALL` member missing from the map** — so adding a fourth
    /// provider later backfills it on the next load, which the old
    /// "only when the whole map is empty" guard silently skipped. Only when
    /// the map was *entirely* empty (a true pre-`providers` config) is the
    /// legacy flat `claude_model` folded into Anthropic's model fields, so an
    /// upgrade doesn't reset a customized model choice.
    fn migrate_legacy_model_if_needed(&mut self) {
        let was_empty = self.providers.is_empty();
        for (id, settings) in seeded_providers() {
            self.providers.entry(id).or_insert(settings);
        }
        if was_empty && let Some(anthropic) = self.providers.get_mut(&ProviderId::Anthropic) {
            anthropic.chat_model = self.claude_model.clone();
            anthropic.skill_model = self.claude_model.clone();
        }
    }

    /// Upgrades a config saved before the per-role provider split: a present
    /// `active_provider` (now parsed into `legacy_active_provider`) seeds
    /// **both** `chat_provider` and `skill_provider`. A config carrying that
    /// key is by definition pre-split, so the copy is unconditional; `save()`
    /// then drops the legacy key (`skip_serializing`).
    fn migrate_legacy_provider_if_needed(&mut self) {
        if let Some(legacy) = self.legacy_active_provider.take() {
            self.chat_provider = legacy;
            self.skill_provider = legacy;
        }
    }

    /// The provider serving `role` — the single lookup every model / env /
    /// spend call site should go through instead of reaching for a role field
    /// directly.
    pub fn provider_for(&self, role: ProviderRole) -> ProviderId {
        match role {
            ProviderRole::Chat => self.chat_provider,
            ProviderRole::Skill => self.skill_provider,
        }
    }

    /// Restores a provider entry that exists but is *entirely* blank
    /// (`base_url`/`api_key` both unset, both model fields empty) from
    /// [`ProviderSettings::default_for`] — so the base-URL default reappears
    /// after the 2026-09-08 data-loss incident progressively wiped
    /// `[agents.providers.open_router]`. A configured entry (any field set) is
    /// left untouched.
    fn reseed_blank_providers(&mut self) {
        for id in ProviderId::ALL {
            let blank = self.providers.get(&id).is_some_and(|s| {
                s.base_url.is_none()
                    && s.api_key.is_none()
                    && s.chat_model.is_empty()
                    && s.skill_model.is_empty()
            });
            if blank {
                self.providers.insert(id, ProviderSettings::default_for(id));
            }
        }
    }
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            claude_model: default_claude_model(),
            ollama_model: default_ollama_model(),
            skill_timeout_secs: default_skill_timeout_secs(),
            claude_env: BTreeMap::new(),
            providers: seeded_providers(),
            legacy_active_provider: None,
            chat_provider: ProviderId::default(),
            skill_provider: ProviderId::default(),
            daily_usd_cap: default_daily_usd_cap(),
        }
    }
}

/// Axiomata-OS's own configuration (`~/.axiomata/config.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Display name of the person this OS belongs to, shown in the dashboard
    /// top bar ("<owner> | <workspace>"). Purely cosmetic; empty hides it.
    #[serde(default)]
    pub owner: String,

    /// Root folder of the user's Second-Brain workspace. Freely
    /// choosable and changeable; defaults to `~/Axiomata-Workspace`.
    #[serde(default = "default_workspace_root")]
    pub workspace_root: PathBuf,

    /// Defaults for the built-in agent backends.
    #[serde(default)]
    pub agents: AgentDefaults,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            owner: String::new(),
            workspace_root: default_workspace_root(),
            agents: AgentDefaults::default(),
        }
    }
}

impl Config {
    /// Validates the whole config immediately before it is persisted.
    ///
    /// This is the authoritative gate: `apply_config_update` (the dashboard's
    /// `save_config` path) and every other code path that writes
    /// `config.toml` must call this first. Frontend checks in `Settings.svelte`
    /// stay as a fast-path affordance, but they are not load-bearing — a
    /// half-typed provider block that reaches disk can silently route real
    /// spend through a paid proxy (`docs/plans/provider-hardening.md`, the
    /// 2026-09-08 incident).
    ///
    /// Returns a human-readable message on the first failing rule (it is
    /// surfaced verbatim in a Settings toast), `Ok(())` when the config is
    /// safe to save.
    pub fn validate_for_save(&self) -> Result<(), String> {
        if self.workspace_root.as_os_str().is_empty() {
            return Err("workspace root must not be empty".to_string());
        }

        // Providers, one per role (chat, skills). Each role's provider must
        // exist in the map. Anthropic may leave everything blank — the Claude
        // CLI's own default model is billed through the subscription login,
        // i.e. free. Every other provider redirects the CLI at a paid
        // endpoint, where a blank/malformed model, base URL, or missing token
        // turns into a silent full-rate fallback — so each is checked against
        // *that role's* model field. Both roles are validated even when they
        // resolve to the same provider.
        for (role, label) in [
            (ProviderRole::Chat, "chat_model"),
            (ProviderRole::Skill, "skill_model"),
        ] {
            let id = self.agents.provider_for(role);
            let settings = self.agents.providers.get(&id).ok_or_else(|| {
                format!("{role:?} provider {id:?} has no entry in [agents.providers]")
            })?;
            if id == ProviderId::Anthropic {
                continue;
            }

            let model = match role {
                ProviderRole::Chat => settings.chat_model.trim(),
                ProviderRole::Skill => settings.skill_model.trim(),
            };
            if model.is_empty() {
                return Err(format!(
                    "{role:?} provider {id:?} redirects the Claude CLI but its {label} is empty — \
                     set it so the CLI's built-in default model isn't billed through the proxy"
                ));
            }
            if !valid_model_name(model) {
                return Err(format!(
                    "{role:?} provider {id:?} {label} {model:?} is not a valid model id \
                     (allowed characters: letters, digits, and - _ . : [ ] /)"
                ));
            }

            let base_url = settings
                .base_url
                .as_deref()
                .map(str::trim)
                .filter(|u| !u.is_empty())
                .ok_or_else(|| format!("{role:?} provider {id:?} has no base URL configured"))?;
            validate_base_url(base_url).map_err(|reason| {
                format!("{role:?} provider {id:?} base URL {base_url:?}: {reason}")
            })?;

            // A bearer-token provider needs a token, unless it is the
            // local-Ollama placeholder case (loopback, no real auth).
            if id.auth_env_var() == "ANTHROPIC_AUTH_TOKEN" && id != ProviderId::Ollama {
                let has_key = settings
                    .api_key
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|k| !k.is_empty());
                if !has_key {
                    return Err(format!(
                        "{role:?} provider {id:?} has no API key / auth token"
                    ));
                }
            }
        }

        // Spend cap: `None` disables it; a set value must be a real positive
        // dollar amount (a 0 or negative cap would block every paid turn
        // outright, which is a footgun, not a config).
        if let Some(cap) = self
            .agents
            .daily_usd_cap
            .filter(|c| !c.is_finite() || *c <= 0.0)
        {
            return Err(format!(
                "daily spend cap must be a positive dollar amount (got {cap}) — \
                 leave it unset to disable the cap"
            ));
        }

        Ok(())
    }

    /// Loads the config from `~/.axiomata/config.toml`, or returns the
    /// default config if the file doesn't exist yet.
    pub fn load() -> Result<Self, AxiomataError> {
        let path = paths::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(&path).map_err(|source| AxiomataError::Io {
            path: path.clone(),
            source,
        })?;

        let mut config: Config =
            toml::from_str(&raw).map_err(|source| AxiomataError::ConfigParse { path, source })?;
        config.agents.migrate_legacy_model_if_needed();
        config.agents.migrate_legacy_provider_if_needed();
        config.agents.reseed_blank_providers();
        Ok(config)
    }

    /// Writes the config to `~/.axiomata/config.toml`.
    ///
    /// Atomic: the serialized TOML is written to a sibling `*.tmp` file
    /// created `0o600` from the start (never world-readable for a window),
    /// then `rename`d over the real path so a crash mid-write can't leave a
    /// truncated config. The file may hold a plaintext token in
    /// `agents.claude_env`, hence the up-front mode.
    pub fn save(&self) -> Result<(), AxiomataError> {
        let path = paths::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| AxiomataError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let raw = toml::to_string_pretty(self)
            .map_err(|source| AxiomataError::ConfigSerialize { source })?;

        let tmp = path.with_extension("toml.tmp");
        write_private(&tmp, raw.as_bytes()).map_err(|source| AxiomataError::Io {
            path: tmp.clone(),
            source,
        })?;
        fs::rename(&tmp, &path).map_err(|source| {
            let _ = fs::remove_file(&tmp);
            AxiomataError::Io {
                path: path.clone(),
                source,
            }
        })?;
        Ok(())
    }
}

/// Writes `bytes` to `path`, truncating any existing file. On Unix the file is
/// created `0o600` up front (not created-then-`chmod`); a `set_permissions`
/// fallback for an already-existing temp file is best-effort and only
/// `warn!`ed.
fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(err) = file.set_permissions(fs::Permissions::from_mode(0o600)) {
            tracing::warn!(path = %path.display(), %err, "could not tighten config file permissions");
        }
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Minimal, dependency-free check that `url` is an absolute HTTP(S) URL with a
/// host, and that plain `http` is only used for a loopback host (the local
/// Ollama case). Not a full RFC 3986 parser — it only has to catch the
/// mistakes the incident showed up (blank, `openrouter.ai` with no scheme, a
/// stray `http://` to a remote host) without pulling in the `url` crate.
fn validate_base_url(url: &str) -> Result<(), String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| "must be an absolute http(s) URL".to_string())?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(format!("unsupported URL scheme {scheme:?} (want https)"));
    }

    // Authority is everything up to the first path / query / fragment
    // delimiter; drop any `userinfo@` prefix, then the `:port` suffix.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(stripped) = host_port.strip_prefix('[') {
        // IPv6 literal: `[::1]` or `[::1]:11434`.
        match stripped.split_once(']') {
            Some((inner, _)) => format!("[{inner}]"),
            None => return Err("malformed IPv6 host".to_string()),
        }
    } else {
        host_port
            .rsplit_once(':')
            .map_or(host_port, |(h, _)| h)
            .to_string()
    };

    if host.is_empty() {
        return Err("missing host".to_string());
    }

    let is_loopback = matches!(host.as_str(), "localhost" | "127.0.0.1" | "[::1]");
    if scheme == "http" && !is_loopback {
        return Err("plain http is only allowed for a localhost endpoint — use https".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;

    #[test]
    fn load_defaults_when_missing_then_round_trips_through_save() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let temp_home = unique_temp_dir("axiomata-test-config-home");

        // SAFETY: serialized by `_guard`, see `paths::tests` for the same
        // reasoning.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &temp_home);
        }

        let loaded = Config::load().expect("load should succeed with no file present");
        assert_eq!(loaded, Config::default());

        let custom = Config {
            owner: "Ada".to_string(),
            workspace_root: temp_home.join("MyBrain"),
            agents: AgentDefaults {
                ollama_model: "llama3.2:latest".to_string(),
                skill_timeout_secs: 120,
                ..AgentDefaults::default()
            },
        };
        custom.save().expect("save should succeed");

        let reloaded = Config::load().expect("load should succeed after save");
        assert_eq!(reloaded, custom);

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&temp_home);
    }

    /// A config saved before `providers`/`active_provider` existed — flat
    /// `agents.claude_model`, no `[agents.providers]` table at all — must
    /// migrate its custom model choice into `providers[Anthropic]` on load,
    /// not silently reset it to the new default.
    #[test]
    fn load_migrates_legacy_flat_claude_model_into_anthropic_provider() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let temp_home = unique_temp_dir("axiomata-test-config-migration-home");

        // SAFETY: serialized by `_guard`, see `paths::tests` for the same
        // reasoning.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &temp_home);
        }

        let legacy_toml = r#"
            owner = "Ada"
            workspace_root = "/tmp/somewhere"

            [agents]
            claude_model = "claude-opus-4"
            ollama_model = "llama3.2"
            skill_timeout_secs = 300
        "#;
        let path = paths::config_path();
        fs::create_dir_all(path.parent().unwrap()).expect("create config dir should succeed");
        fs::write(&path, legacy_toml).expect("write legacy config should succeed");

        let loaded = Config::load().expect("load should migrate the legacy config");

        // The old value survives, folded into both Anthropic model fields...
        let anthropic = loaded
            .agents
            .providers
            .get(&ProviderId::Anthropic)
            .expect("migration should seed every ProviderId, including Anthropic");
        assert_eq!(anthropic.chat_model, "claude-opus-4");
        assert_eq!(anthropic.skill_model, "claude-opus-4");
        // ...and every other known provider is backfilled too, so UI/runtime
        // code can index the map directly without falling back to
        // `default_for`.
        assert_eq!(loaded.agents.providers.len(), ProviderId::ALL.len());
        assert_eq!(loaded.agents.chat_provider, ProviderId::Anthropic);
        assert_eq!(loaded.agents.skill_provider, ProviderId::Anthropic);

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&temp_home);
    }

    /// A config saved before the per-role provider split carries a single
    /// `active_provider` key. On load it must seed **both** `chat_provider`
    /// and `skill_provider`, and a subsequent `save()` must drop the legacy
    /// key entirely.
    #[test]
    fn load_migrates_legacy_active_provider_into_both_role_fields() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let temp_home = unique_temp_dir("axiomata-test-config-role-migration-home");
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &temp_home);
        }

        let legacy_toml = r#"
            owner = "Ada"
            workspace_root = "/tmp/somewhere"

            [agents]
            ollama_model = "qwen3:30b"
            skill_timeout_secs = 300
            active_provider = "ollama"

            [agents.providers.ollama]
            base_url = "http://localhost:11434"
            chat_model = "qwen3:30b"
            skill_model = "qwen3:30b"
        "#;
        let path = paths::config_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, legacy_toml).unwrap();

        let loaded = Config::load().expect("load should migrate the legacy provider key");
        assert_eq!(loaded.agents.chat_provider, ProviderId::Ollama);
        assert_eq!(loaded.agents.skill_provider, ProviderId::Ollama);
        assert_eq!(loaded.agents.legacy_active_provider, None);

        loaded.save().expect("save should succeed");
        let raw = fs::read_to_string(&path).unwrap();
        assert!(
            !raw.contains("active_provider"),
            "legacy key should be gone: {raw}"
        );
        assert!(raw.contains("chat_provider = \"ollama\""));
        assert!(raw.contains("skill_provider = \"ollama\""));

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&temp_home);
    }

    /// Anthropic gets the direct-key header env var; both proxying
    /// providers (OpenRouter, Ollama) share the bearer-token one, per the
    /// Phase 0 spike documented on `auth_env_var`'s own doc comment.
    #[test]
    fn auth_env_var_maps_anthropic_to_api_key_and_others_to_auth_token() {
        assert_eq!(ProviderId::Anthropic.auth_env_var(), "ANTHROPIC_API_KEY");
        assert_eq!(
            ProviderId::OpenRouter.auth_env_var(),
            "ANTHROPIC_AUTH_TOKEN"
        );
        assert_eq!(ProviderId::Ollama.auth_env_var(), "ANTHROPIC_AUTH_TOKEN");
    }

    #[test]
    fn default_for_anthropic_has_no_base_url_or_key_but_real_models() {
        let settings = ProviderSettings::default_for(ProviderId::Anthropic);
        assert_eq!(settings.base_url, None);
        assert_eq!(settings.api_key, None);
        assert_eq!(settings.chat_model, "claude-sonnet-5");
        assert_eq!(settings.skill_model, "claude-haiku-4-5");
    }

    #[test]
    fn default_for_openrouter_has_a_base_url_but_no_key_or_models_yet() {
        let settings = ProviderSettings::default_for(ProviderId::OpenRouter);
        assert_eq!(
            settings.base_url.as_deref(),
            Some("https://openrouter.ai/api")
        );
        assert_eq!(settings.api_key, None);
        assert_eq!(settings.chat_model, "");
        assert_eq!(settings.skill_model, "");
    }

    #[test]
    fn default_for_ollama_has_a_local_base_url_and_the_placeholder_key() {
        let settings = ProviderSettings::default_for(ProviderId::Ollama);
        assert_eq!(settings.base_url.as_deref(), Some("http://localhost:11434"));
        assert_eq!(settings.api_key.as_deref(), Some("ollama"));
        assert_eq!(settings.chat_model, "");
        assert_eq!(settings.skill_model, "");
    }

    /// `BTreeMap<ProviderId, _>` serializes as a TOML table keyed by the
    /// `#[serde(rename_all = "snake_case")]` variant name; this must survive
    /// a full serialize/deserialize round trip with every `ALL` member
    /// present as its own distinct key, not collapse or reorder.
    #[test]
    fn provider_id_all_members_round_trip_as_distinct_toml_map_keys() {
        let mut providers = BTreeMap::new();
        for id in ProviderId::ALL {
            providers.insert(id, ProviderSettings::default_for(id));
        }

        let defaults = AgentDefaults {
            providers,
            ..AgentDefaults::default()
        };

        let raw = toml::to_string_pretty(&defaults).expect("serialize should succeed");
        assert!(raw.contains("[providers.anthropic]"));
        assert!(raw.contains("[providers.open_router]"));
        assert!(raw.contains("[providers.ollama]"));

        let round_tripped: AgentDefaults =
            toml::from_str(&raw).expect("deserialize should succeed");
        assert_eq!(round_tripped.providers.len(), ProviderId::ALL.len());
        for id in ProviderId::ALL {
            assert_eq!(
                round_tripped.providers.get(&id),
                Some(&ProviderSettings::default_for(id)),
                "provider {id:?} should round-trip with its own default settings"
            );
        }
    }

    /// When `providers` is already populated (the normal case for any
    /// config saved after the migration first ran, and always for a fresh
    /// default), migration must be a true no-op: it must not touch the
    /// existing per-provider settings, even if they differ from
    /// `default_for` and even if the legacy `claude_model` field disagrees
    /// with them.
    #[test]
    fn migrate_legacy_model_if_needed_is_a_no_op_once_providers_is_populated() {
        let mut defaults = AgentDefaults {
            claude_model: "claude-opus-4".to_string(),
            chat_provider: ProviderId::OpenRouter,
            skill_provider: ProviderId::OpenRouter,
            ..AgentDefaults::default()
        };
        // Hand-customize Anthropic's chat model so a wrongful re-migration
        // would be observable.
        defaults
            .providers
            .get_mut(&ProviderId::Anthropic)
            .expect("default providers should include Anthropic")
            .chat_model = "custom-model".to_string();
        let before = defaults.clone();

        defaults.migrate_legacy_model_if_needed();

        assert_eq!(defaults, before);
    }

    /// The old `if !providers.is_empty() { return }` guard never backfilled a
    /// provider added to `ProviderId::ALL` after the map was first written.
    /// Migration must now seed *any* missing member — without re-folding the
    /// legacy `claude_model` (the map wasn't empty, so it's not a legacy
    /// config).
    #[test]
    fn migrate_backfills_a_single_missing_provider_without_refolding_claude_model() {
        let mut defaults = AgentDefaults {
            claude_model: "claude-opus-4".to_string(),
            ..AgentDefaults::default()
        };
        defaults.providers.remove(&ProviderId::Ollama);
        defaults
            .providers
            .get_mut(&ProviderId::Anthropic)
            .unwrap()
            .chat_model = "kept-custom".to_string();

        defaults.migrate_legacy_model_if_needed();

        assert_eq!(defaults.providers.len(), ProviderId::ALL.len());
        assert_eq!(
            defaults.providers.get(&ProviderId::Ollama),
            Some(&ProviderSettings::default_for(ProviderId::Ollama))
        );
        // Not a legacy config → Anthropic's hand-set model is left alone.
        assert_eq!(
            defaults.providers[&ProviderId::Anthropic].chat_model,
            "kept-custom"
        );
    }

    #[test]
    fn reseed_blank_providers_restores_a_wiped_entry_but_leaves_a_configured_one() {
        let mut defaults = AgentDefaults::default();
        // Simulate the data-loss incident: OpenRouter progressively emptied.
        defaults.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: None,
                api_key: None,
                chat_model: String::new(),
                skill_model: String::new(),
            },
        );
        // Ollama stays configured (its default base_url is set).
        let ollama_before = defaults.providers[&ProviderId::Ollama].clone();

        defaults.reseed_blank_providers();

        assert_eq!(
            defaults.providers[&ProviderId::OpenRouter],
            ProviderSettings::default_for(ProviderId::OpenRouter),
            "a fully-blank entry is restored from default_for"
        );
        assert_eq!(
            defaults.providers[&ProviderId::Ollama],
            ollama_before,
            "an entry with any field set is left untouched"
        );
    }

    #[test]
    fn save_writes_owner_only_0o600_and_omits_the_legacy_claude_model_key() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let temp_home = unique_temp_dir("axiomata-test-config-save-perms");
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &temp_home);
        }

        Config::default().save().expect("save should succeed");

        let path = paths::config_path();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(
            !raw.contains("claude_model"),
            "the migration-only field must not be written back: {raw}"
        );
        assert!(
            !path.with_extension("toml.tmp").exists(),
            "the temp file must be renamed away, not left behind"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "config file must be owner-only");
        }

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&temp_home);
    }

    // --- Checkpoint 2: `validate_for_save` -------------------------------

    /// A config with **both** role providers pointed at `id` and that
    /// provider's settings hand-set to a known-good triple, everything else
    /// default.
    fn config_with_active(id: ProviderId, settings: ProviderSettings) -> Config {
        let mut config = Config::default();
        config.agents.chat_provider = id;
        config.agents.skill_provider = id;
        config.agents.providers.insert(id, settings);
        config
    }

    fn valid_openrouter_settings() -> ProviderSettings {
        ProviderSettings {
            base_url: Some("https://openrouter.ai/api".to_string()),
            api_key: Some("sk-or-v1-deadbeef".to_string()),
            chat_model: "z-ai/glm-5.3-flash".to_string(),
            skill_model: "deepseek/deepseek-v4-flash".to_string(),
        }
    }

    #[test]
    fn validate_for_save_accepts_the_default_anthropic_config() {
        assert_eq!(Config::default().validate_for_save(), Ok(()));
    }

    #[test]
    fn validate_for_save_rejects_an_empty_workspace_root() {
        let config = Config {
            workspace_root: PathBuf::new(),
            ..Config::default()
        };
        assert!(
            config
                .validate_for_save()
                .unwrap_err()
                .contains("workspace root")
        );
    }

    #[test]
    fn validate_for_save_allows_anthropic_with_blank_models() {
        // Anthropic via the CLI's own login — a blank model uses the CLI's
        // built-in default, which is billed through the subscription, i.e.
        // free. That must stay allowed.
        let config = config_with_active(
            ProviderId::Anthropic,
            ProviderSettings {
                base_url: None,
                api_key: None,
                chat_model: String::new(),
                skill_model: String::new(),
            },
        );
        assert_eq!(config.validate_for_save(), Ok(()));
    }

    /// The point of the feature: chat on Anthropic (blank models OK — CLI
    /// login) while skills route through a fully-configured Ollama. Only the
    /// skill role's `skill_model` needs to be set; the chat role is exempt.
    #[test]
    fn validate_for_save_allows_split_anthropic_chat_and_ollama_skills() {
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::Anthropic;
        config.agents.skill_provider = ProviderId::Ollama;
        config.agents.providers.insert(
            ProviderId::Ollama,
            ProviderSettings {
                base_url: Some("http://localhost:11434".to_string()),
                api_key: None,
                chat_model: String::new(),
                skill_model: "qwen3:30b".to_string(),
            },
        );
        assert_eq!(config.validate_for_save(), Ok(()));
    }

    /// With that same split, a blank `skill_model` on the skill-role provider
    /// is rejected — even though the chat-role provider is perfectly fine.
    #[test]
    fn validate_for_save_rejects_a_blank_skill_model_on_the_skill_role_provider() {
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::Anthropic;
        config.agents.skill_provider = ProviderId::Ollama;
        config.agents.providers.insert(
            ProviderId::Ollama,
            ProviderSettings {
                base_url: Some("http://localhost:11434".to_string()),
                api_key: None,
                chat_model: String::new(),
                skill_model: String::new(),
            },
        );
        let err = config.validate_for_save().unwrap_err();
        assert!(
            err.contains("Skill") && err.contains("skill_model"),
            "{err}"
        );
    }

    #[test]
    fn validate_for_save_rejects_a_role_provider_missing_from_the_map() {
        let mut config = Config::default();
        config.agents.chat_provider = ProviderId::OpenRouter;
        config.agents.skill_provider = ProviderId::OpenRouter;
        config.agents.providers.remove(&ProviderId::OpenRouter);
        assert!(
            config
                .validate_for_save()
                .unwrap_err()
                .contains("no entry in [agents.providers]")
        );
    }

    #[test]
    fn validate_for_save_rejects_a_non_anthropic_provider_with_a_blank_model() {
        let mut settings = valid_openrouter_settings();
        settings.skill_model = "   ".to_string();
        let err = config_with_active(ProviderId::OpenRouter, settings)
            .validate_for_save()
            .unwrap_err();
        assert!(
            err.contains("skill_model") && err.contains("empty"),
            "{err}"
        );
    }

    /// The exact one-character typo from the 2026-09-08 incident:
    /// `z-ai/glm-5.3-flash[1m)` — a `)` where a `]` was meant. It must be
    /// rejected at the save boundary, not silently dropped downstream.
    #[test]
    fn validate_for_save_rejects_the_incident_model_typo() {
        let mut settings = valid_openrouter_settings();
        settings.chat_model = "z-ai/glm-5.3-flash[1m)".to_string();
        let err = config_with_active(ProviderId::OpenRouter, settings)
            .validate_for_save()
            .unwrap_err();
        assert!(err.contains("not a valid model id"), "{err}");
    }

    #[test]
    fn validate_for_save_rejects_a_non_anthropic_provider_without_a_base_url() {
        let mut settings = valid_openrouter_settings();
        settings.base_url = None;
        let err = config_with_active(ProviderId::OpenRouter, settings)
            .validate_for_save()
            .unwrap_err();
        assert!(err.contains("no base URL"), "{err}");
    }

    #[test]
    fn validate_for_save_rejects_a_remote_http_base_url() {
        let mut settings = valid_openrouter_settings();
        settings.base_url = Some("http://openrouter.ai/api".to_string());
        let err = config_with_active(ProviderId::OpenRouter, settings)
            .validate_for_save()
            .unwrap_err();
        assert!(err.contains("https"), "{err}");
    }

    #[test]
    fn validate_for_save_rejects_a_base_url_with_no_scheme() {
        let mut settings = valid_openrouter_settings();
        settings.base_url = Some("openrouter.ai".to_string());
        assert!(
            config_with_active(ProviderId::OpenRouter, settings)
                .validate_for_save()
                .is_err()
        );
    }

    #[test]
    fn validate_for_save_rejects_a_bearer_provider_without_a_key() {
        let mut settings = valid_openrouter_settings();
        settings.api_key = None;
        let err = config_with_active(ProviderId::OpenRouter, settings)
            .validate_for_save()
            .unwrap_err();
        assert!(err.contains("API key"), "{err}");
    }

    #[test]
    fn validate_for_save_rejects_a_nonpositive_or_nonfinite_daily_cap() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut config = Config::default();
            config.agents.daily_usd_cap = Some(bad);
            assert!(
                config.validate_for_save().is_err(),
                "cap {bad} should be rejected"
            );
        }
        // Unset (cap disabled) and a real positive value are both fine.
        let mut config = Config::default();
        config.agents.daily_usd_cap = None;
        assert_eq!(config.validate_for_save(), Ok(()));
        config.agents.daily_usd_cap = Some(5.0);
        assert_eq!(config.validate_for_save(), Ok(()));
    }

    #[test]
    fn validate_for_save_accepts_a_fully_configured_openrouter() {
        assert_eq!(
            config_with_active(ProviderId::OpenRouter, valid_openrouter_settings())
                .validate_for_save(),
            Ok(())
        );
    }

    /// Ollama is non-Anthropic (so it needs concrete models and a base URL),
    /// but its loopback endpoint needs no real credential — an empty
    /// `api_key` there must still validate.
    #[test]
    fn validate_for_save_accepts_local_ollama_without_a_key() {
        let config = config_with_active(
            ProviderId::Ollama,
            ProviderSettings {
                base_url: Some("http://localhost:11434".to_string()),
                api_key: None,
                chat_model: "llama3.2".to_string(),
                skill_model: "llama3.2".to_string(),
            },
        );
        assert_eq!(config.validate_for_save(), Ok(()));
    }

    #[test]
    fn validate_for_save_round_trips_a_valid_three_provider_config_through_save() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let temp_home = unique_temp_dir("axiomata-test-config-validate-roundtrip");
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &temp_home);
        }

        let mut config = Config {
            workspace_root: temp_home.join("Brain"),
            ..Config::default()
        };
        config.agents.chat_provider = ProviderId::OpenRouter;
        config.agents.skill_provider = ProviderId::OpenRouter;
        config
            .agents
            .providers
            .insert(ProviderId::OpenRouter, valid_openrouter_settings());

        assert_eq!(config.validate_for_save(), Ok(()));
        config.save().expect("a validated config should save");
        let reloaded = Config::load().expect("reload should succeed");
        assert_eq!(reloaded, config);
        assert_eq!(reloaded.validate_for_save(), Ok(()));

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&temp_home);
    }

    #[test]
    fn validate_base_url_accepts_https_and_loopback_http_only() {
        assert!(validate_base_url("https://openrouter.ai/api").is_ok());
        assert!(validate_base_url("https://example.com:8443/v1").is_ok());
        assert!(validate_base_url("http://localhost:11434").is_ok());
        assert!(validate_base_url("http://127.0.0.1:11434").is_ok());
        assert!(validate_base_url("http://[::1]:11434").is_ok());

        assert!(validate_base_url("http://example.com").is_err());
        assert!(validate_base_url("ftp://example.com").is_err());
        assert!(validate_base_url("openrouter.ai/api").is_err());
        assert!(validate_base_url("https://").is_err());
        assert!(validate_base_url("").is_err());
    }
}
