//! The app's own Opencode instances (`docs/plans/agent-isolation.md`, D2, CP1): services of the app's, never the
//! owner's.
//!
//! Opencode keeps everything — configuration, database, sessions, saved "always" answers, its service's discovery
//! file — in the four `XDG_*` folders, and moves all of it with them (measured on 2.0.32, CP0). An instance is one
//! set of those folders below `~/.axiomata/opencode/<instance>/`, with a service of its own on a port of its own;
//! the owner's `~/.config/opencode`, `~/.local/share/opencode` and their service are never written, started or
//! stopped from here (D1).
//!
//! What the instance knows is generated at every connect from the app's sources ([`prepare`]): the provider keys
//! and endpoints from `config.toml` (the one source of credentials, D2), the MCP servers from `~/.axh/mcp.json`
//! (D7), and the owner's model catalog — the model entries of their own `opencode.json`, read and stripped of every
//! credential, so the models, variants and routing preferences the owner tuned are the same here.
//!
//! The service runs with the instance's folders, so would every shell command it starts — `gh` or `git` would look
//! for their configuration in the instance. A session therefore gets the owner's environment back for its commands
//! ([`session_env`], `Service::with_session_environment`). A pane's terminal UI only needs to find the service:
//! the state folder is where the discovery file lives ([`tui_env_prefix`]); its own configuration stays the owner's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::agent_child_env;
use crate::config::{Config, ProviderId};
use crate::error::AxiomataError;
use crate::paths;

/// The folder below the app's home the instances live in.
const INSTANCES_DIR: &str = "opencode";
/// The `XDG_*` variables Opencode takes its folders from, in the order [`Places::env`] lists them.
const XDG_VARS: [&str; 4] = [
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "XDG_CACHE_HOME",
];
/// Opencode's own folder inside each `XDG_*` folder.
const OPENCODE_DIR: &str = "opencode";
/// The configuration file Opencode reads at start.
const CONFIG_FILE: &str = "opencode.json";
/// Where `opencode service set port` keeps the port (and the service its password), inside the config folder.
const SERVICE_FILE: &str = "service.json";
/// The schema the generated configuration names.
const CONFIG_SCHEMA: &str = "https://opencode.ai/v2/config.json";
/// Opencode's provider ids for the providers `config.toml` knows.
const OPENROUTER: &str = "openrouter";
const OLLAMA: &str = "ollama";
/// The package Opencode runs an OpenAI-compatible endpoint with — what a local Ollama is.
const OPENAI_COMPATIBLE: &str = "aisdk:@ai-sdk/openai-compatible";
/// Field names that hold a credential, compared lowercased and without `-` and `_` (`apiKey`, `api_key`,
/// `x-api-key` …); such a field of the owner's catalog is never copied. Exact names, not parts: `maxTokens` is
/// tuning the catalog must keep.
const CREDENTIAL_NAMES: [&str; 18] = [
    "apikey",
    "xapikey",
    "key",
    "accesskey",
    "secretkey",
    "token",
    "accesstoken",
    "refreshtoken",
    "idtoken",
    "authtoken",
    "bearer",
    "secret",
    "clientsecret",
    "password",
    "passphrase",
    "authorization",
    "auth",
    "cookie",
];
/// The marker of the configuration the running service was started with, in the instance's folder.
const STARTED_WITH_FILE: &str = "started-with.json";
/// The largest owner's `opencode.json` read for its catalog.
const MAX_OWNER_CONFIG_BYTES: u64 = 1024 * 1024;

/// One of the app's Opencode instances. The discriminants index per-instance state (`Instance as usize`), in
/// [`Instance::ALL`]'s order and then the master's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum Instance {
    /// Everything that may write: card workers, chat, skills that do not declare `access: read`.
    Write = 0,
    /// Sessions that only look (D3, CP2): reviewers, planners, skills declaring `access: read`. Its configuration
    /// refuses every edit and every command but the read-only ones ([`read_rules`]), and its database keeps its own
    /// saved "always" answers — none of the writers' and none of the owner's reach it.
    Read = 1,
    /// The master pane's (D4, D6, CP3): the owner's own configuration — agents, commands, rules, MCP servers — with
    /// the app's keys, its own sessions and database. The owner works on it by hand; nothing of the app's runs there
    /// but the team tools' registration.
    Master = 2,
}

impl Instance {
    /// The instances the app's automation runs on, in the order a session is looked for on them. The master's is not
    /// among them: nothing the app watches or limits runs there.
    pub const ALL: [Instance; 2] = [Instance::Write, Instance::Read];
    /// How many instances there are, for state kept per instance.
    pub const COUNT: usize = 3;

    /// The instance a session runs on: the read instance when it may only look.
    pub fn for_access(read_only: bool) -> Self {
        if read_only {
            Instance::Read
        } else {
            Instance::Write
        }
    }

    fn name(self) -> &'static str {
        match self {
            Instance::Write => "write",
            Instance::Read => "read",
            Instance::Master => "master",
        }
    }
}

/// The rights of the read instance, as (action, resource, effect) in Opencode's order — the later rule counts. Its
/// configuration holds them for a session without rules of its own (a pane the owner opens on it), and every session
/// the app makes there gets them again in front of its own rules ([`read_session_rules`]): measured on 2.0.32
/// (`docs/plans/agent-isolation.md`, CP2), a project's `opencode.json` or `.opencode/opencode.json` and a saved
/// "always" both beat the configuration's rules, and a session's own rules beat them both.
pub(crate) fn read_rules() -> Vec<(&'static str, &'static str, &'static str)> {
    let mut rules = vec![
        ("edit", "*", "deny"),
        ("shell", "*", "deny"),
        ("external_directory", "*", "deny"),
    ];
    rules.extend(
        READ_COMMANDS
            .iter()
            .map(|command| ("shell", *command, "allow")),
    );
    rules.extend(
        READ_DENIED_COMMANDS
            .iter()
            .map(|pattern| ("shell", *pattern, "deny")),
    );
    // Measured (CP2): a redirect does not make a command another one — `git log > f` passes as `git log *` and writes
    // the file. Last, so nothing above lifts it.
    rules.push(("shell", "*>*", "deny"));
    rules
}

/// The read-only git commands that reach beyond the checkout: `--no-index` reads any file, `--output` writes one —
/// matched in their shortest unambiguous forms, which git takes too (`--outp=f`). Denied after [`READ_COMMANDS`]
/// allow them, on every harness (`docs/plans/harness-choice.md`, "Read access").
pub(crate) const READ_DENIED_COMMANDS: [&str; 2] = ["git * --no-i*", "git * --ou*"];

/// The commands a reader runs without asking: git's read-only ones and a look at the folder it is in. Nothing that
/// takes a program or a path to write to as an argument (`find -exec`, `rg --pre`, a pager), which a pattern cannot
/// tell apart, and `ls` without a path, so it does not list folders outside the checkout. Shared by every harness: a
/// trailing ` *` means "with any arguments".
pub(crate) const READ_COMMANDS: [&str; 13] = [
    "git status",
    "git status *",
    "git log",
    "git log *",
    "git diff",
    "git diff *",
    "git show",
    "git show *",
    "git rev-parse *",
    "git branch --show-current",
    "ls",
    "ls -la",
    "pwd",
];

/// The four folders of an instance.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Places {
    instance: Instance,
    root: PathBuf,
    config: PathBuf,
    data: PathBuf,
    state: PathBuf,
    cache: PathBuf,
}

impl Places {
    fn of(instance: Instance) -> Self {
        Self::below(
            instance,
            &paths::axiomata_home()
                .join(INSTANCES_DIR)
                .join(instance.name()),
        )
    }

    fn below(instance: Instance, root: &Path) -> Self {
        Self {
            instance,
            root: root.to_path_buf(),
            config: root.join("config"),
            data: root.join("data"),
            state: root.join("state"),
            cache: root.join("cache"),
        }
    }

    /// The `XDG_*` assignments, in [`XDG_VARS`]' order.
    fn env(&self) -> Vec<(String, String)> {
        [&self.config, &self.data, &self.state, &self.cache]
            .into_iter()
            .zip(XDG_VARS)
            .map(|(path, var)| (var.to_owned(), path.display().to_string()))
            .collect()
    }

    /// Opencode's configuration folder of this instance.
    fn opencode_config(&self) -> PathBuf {
        self.config.join(OPENCODE_DIR)
    }

    /// The state folder Opencode itself must report for this instance (`opencode debug paths state`).
    fn opencode_state(&self) -> PathBuf {
        self.state.join(OPENCODE_DIR)
    }
}

/// The state folder `instance`'s service must run from — what the caller checks against Opencode's own answer
/// before it starts, restarts or stops anything, so a command meant for the app's service can never reach the
/// owner's.
pub(crate) fn expected_state_dir(instance: Instance) -> PathBuf {
    Places::of(instance).opencode_state()
}

/// The complete environment of the `opencode` commands for `instance` (`debug paths`, `service start`): the app's
/// allowlist without any `XDG_*` folder of the owner's, and the instance's folders in their place.
pub(crate) fn service_env(instance: Instance) -> Vec<(String, String)> {
    service_env_from(agent_child_env(), &Places::of(instance))
}

fn service_env_from(ambient: Vec<(String, String)>, places: &Places) -> Vec<(String, String)> {
    let mut env: Vec<_> = ambient
        .into_iter()
        .filter(|(key, _)| !XDG_VARS.contains(&key.as_str()))
        .collect();
    env.extend(places.env());
    env
}

/// The environment a session's shell commands run with: the app's allowlist, the owner's own folders included —
/// what the owner's terminal would give them, not the instance's.
pub(crate) fn session_env() -> BTreeMap<String, String> {
    agent_child_env().into_iter().collect()
}

/// What goes in front of a pane's `opencode` command so its terminal UI attaches to `instance`'s service: the
/// instance's state, data and cache folders. The UI's own configuration (theme, keys) stays the owner's.
pub(crate) fn tui_env_prefix(instance: Instance) -> String {
    tui_env_prefix_for(&Places::of(instance))
}

fn tui_env_prefix_for(places: &Places) -> String {
    // The state folder is where the discovery file is. Data and cache go along: should the UI ever find no service
    // and start one itself, that one keeps its sessions in the instance's database, never in the owner's (D1).
    let quoted = |path: &Path| crate::agent_entry::shell_quote(&path.display().to_string());
    format!(
        "env XDG_STATE_HOME={} XDG_DATA_HOME={} XDG_CACHE_HOME={} ",
        quoted(&places.state),
        quoted(&places.data),
        quoted(&places.cache)
    )
}

/// Makes `instance` ready to start: its folders, its generated configuration and its port. Blocking file work;
/// the caller runs it off the async runtime.
///
/// Returns whether the configuration differs from the one the instance's service was last started with
/// ([`mark_started`]) — a running service reads it only at its start, so it must be restarted to see it.
///
/// Errors:
///     [`AxiomataError::Io`] when a folder or file cannot be written, the app's home is no absolute path or the
///     instance's folder is a symbolic link; [`AxiomataError::AgentApi`] when `~/.axh/mcp.json` cannot be used (an
///     MCP file that does not parse would otherwise leave every connector skill without its tools, silently).
pub(crate) fn prepare(instance: Instance, config: &Config) -> Result<bool, AxiomataError> {
    let home = home::home_dir().ok_or_else(|| AxiomataError::AgentApi {
        backend: super::BACKEND_OPENCODE,
        message: "no home folder to read ~/.axh/mcp.json from".to_owned(),
    })?;
    prepare_at(&Places::of(instance), &home, config)
}

fn prepare_at(places: &Places, home: &Path, config: &Config) -> Result<bool, AxiomataError> {
    make_instance_dirs(places)?;
    let generated = if places.instance == Instance::Master {
        link_owner_files(places, &owner_config_dir(home))?;
        link_owner_config_home(places, &owner_config_home(home))?;
        master_config(owner_config(home), config)
    } else {
        let servers =
            axiomata_harness::mcp::config::owner(home).map_err(|err| AxiomataError::AgentApi {
                backend: super::BACKEND_OPENCODE,
                message: format!("the MCP servers for the app's Opencode: {err}"),
            })?;
        generated_config(owner_catalog(home), config, &servers, places.instance)
    };
    let text = serde_json::to_string_pretty(&generated).map_err(|err| AxiomataError::AgentApi {
        backend: super::BACKEND_OPENCODE,
        message: format!("the generated Opencode configuration: {err}"),
    })?;
    let file = places.opencode_config().join(CONFIG_FILE);
    if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
        write_private_atomic(&file, &text)?;
    }
    ensure_port(&places.opencode_config().join(SERVICE_FILE))?;
    let started_with = std::fs::read_to_string(places.root.join(STARTED_WITH_FILE)).ok();
    Ok(started_with.as_deref() != Some(text.as_str()))
}

/// Records that `instance`'s service now runs with the configuration [`prepare`] wrote. Only after a start or
/// restart that worked: a restart that failed leaves the marker behind, so the next connect tries again.
///
/// Errors:
///     [`AxiomataError::Io`] when the configuration cannot be read or the marker not written.
pub(crate) fn mark_started(instance: Instance) -> Result<(), AxiomataError> {
    let places = Places::of(instance);
    let file = places.opencode_config().join(CONFIG_FILE);
    let text = std::fs::read_to_string(&file).map_err(|source| io_error(&file, source))?;
    write_private_atomic(&places.root.join(STARTED_WITH_FILE), &text)
}

/// The configuration an instance runs with: the owner's model catalog under the app's credentials, the MCP
/// servers of `~/.axh/mcp.json`, and for the read instance its rights ([`read_rules`]).
fn generated_config(
    catalog: Map<String, Value>,
    config: &Config,
    servers: &[axiomata_harness::mcp::config::ServerEntry],
    instance: Instance,
) -> Value {
    let providers = with_app_credentials(catalog, config);
    let servers: Map<String, Value> = servers
        .iter()
        .filter_map(|entry| opencode_server(entry).map(|server| (entry.name.clone(), server)))
        .collect();
    let mut generated = json!({
        "$schema": CONFIG_SCHEMA,
        "providers": providers,
        "mcp": { "servers": servers },
    });
    if instance == Instance::Read {
        generated["permissions"] = read_rules()
            .into_iter()
            .map(|(action, resource, effect)| json!({ "action": action, "resource": resource, "effect": effect }))
            .collect();
    }
    generated
}

/// The master instance's configuration (D6): the owner's own `opencode.json` as it is — agents, rules, MCP servers,
/// default model — with its providers under the app's credentials, since the owner's logins (`auth.json`) live in
/// their own data folder, which the master does not use.
fn master_config(mut owner: Map<String, Value>, config: &Config) -> Value {
    let catalog = match owner.remove("providers") {
        Some(Value::Object(mut providers)) => {
            for provider in providers.values_mut() {
                strip_credentials(provider);
            }
            providers
        }
        _ => Map::new(),
    };
    owner.insert(
        "providers".to_owned(),
        Value::Object(with_app_credentials(catalog, config)),
    );
    owner
        .entry("$schema")
        .or_insert_with(|| json!(CONFIG_SCHEMA));
    Value::Object(owner)
}

/// `catalog`, the owner's providers stripped of credentials, with the keys and endpoints of `config.toml` (D2).
fn with_app_credentials(catalog: Map<String, Value>, config: &Config) -> Map<String, Value> {
    let mut providers = catalog;
    let settings = |id: ProviderId| config.agents.providers.get(&id);
    if let Some(key) = settings(ProviderId::OpenRouter)
        .and_then(|p| p.api_key.as_deref())
        .filter(|key| !key.trim().is_empty())
    {
        // The app's key goes to Opencode's own OpenRouter endpoint only, never to one the owner's catalog names.
        let settings = provider_settings(&mut providers, OPENROUTER);
        settings.remove("baseURL");
        settings.insert("apiKey".to_owned(), json!(key));
        if let Some(openrouter) = providers.get_mut(OPENROUTER).and_then(Value::as_object_mut) {
            openrouter.remove("package");
        }
    }
    if let Some(base) = settings(ProviderId::Ollama)
        .and_then(|p| p.base_url.as_deref())
        .filter(|url| !url.trim().is_empty())
    {
        provider_settings(&mut providers, OLLAMA)
            .insert("baseURL".to_owned(), json!(openai_base(base)));
        let ollama = providers
            .get_mut(OLLAMA)
            .and_then(Value::as_object_mut)
            .expect("provider_settings just made the entry an object");
        ollama
            .entry("package")
            .or_insert_with(|| json!(OPENAI_COMPATIBLE));
        ollama.entry("name").or_insert_with(|| json!("Ollama"));
    }
    providers
}

/// The `settings` object of `provider` in `providers`, made where missing.
fn provider_settings<'a>(
    providers: &'a mut Map<String, Value>,
    provider: &str,
) -> &'a mut Map<String, Value> {
    let entry = providers.entry(provider).or_insert_with(|| json!({}));
    if !entry.is_object() {
        *entry = json!({});
    }
    let settings = entry
        .as_object_mut()
        .expect("made an object above")
        .entry("settings")
        .or_insert_with(|| json!({}));
    if !settings.is_object() {
        *settings = json!({});
    }
    settings.as_object_mut().expect("made an object above")
}

/// Ollama's OpenAI-compatible endpoint below the base URL `config.toml` keeps (`http://localhost:11434`).
fn openai_base(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    if base.ends_with("/v1") {
        base.to_owned()
    } else {
        format!("{base}/v1")
    }
}

/// One server of `~/.axh/mcp.json` as Opencode spells it. An HTTP server whose header needs a `${VAR}` is left
/// out: the harness expands it from its own environment, and the service has no such variable.
fn opencode_server(entry: &axiomata_harness::mcp::config::ServerEntry) -> Option<Value> {
    use axiomata_harness::mcp::config::ServerConfig;
    match &entry.config {
        ServerConfig::Stdio { command, args, env } => {
            if env.values().any(|value| value.contains("${")) {
                tracing::warn!(
                    server = %entry.name,
                    "MCP server left out of the app's Opencode: its env needs a variable the service does not have"
                );
                return None;
            }
            let mut line = vec![command.clone()];
            line.extend(args.iter().cloned());
            let mut server = json!({ "type": "local", "command": line });
            if !env.is_empty() {
                server["environment"] = json!(env);
            }
            Some(server)
        }
        ServerConfig::Http { url, headers } => {
            if headers.values().any(|value| value.contains("${")) {
                tracing::warn!(
                    server = %entry.name,
                    "MCP server left out of the app's Opencode: a header needs a variable the service does not have"
                );
                return None;
            }
            Some(json!({ "type": "remote", "url": url, "headers": headers }))
        }
    }
}

/// The `providers` of the owner's own `opencode.json`, every credential removed; empty when there is none or it
/// cannot be read — the instance then has Opencode's built-in catalog and what Ollama lists.
fn owner_catalog(home: &Path) -> Map<String, Value> {
    match owner_config(home).remove("providers") {
        Some(Value::Object(mut providers)) => {
            for provider in providers.values_mut() {
                strip_credentials(provider);
            }
            providers
        }
        _ => Map::new(),
    }
}

/// The owner's configuration folder (`$XDG_CONFIG_HOME`, else `~/.config`).
fn owner_config_home(home: &Path) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| home.join(".config"))
}

/// The owner's own Opencode configuration folder (`$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`).
fn owner_config_dir(home: &Path) -> PathBuf {
    owner_config_home(home).join(OPENCODE_DIR)
}

/// Links every entry of the owner's configuration folder but Opencode's into the master's `XDG_CONFIG_HOME`. The
/// master's service runs its shell commands with that folder — the owner's sessions there are the TUI's own, which
/// get no environment from the app — so `gh`, `git` and the like find their configuration as they do outside.
fn link_owner_config_home(places: &Places, owner_home: &Path) -> Result<(), AxiomataError> {
    let wanted: Vec<std::ffi::OsString> = match std::fs::read_dir(owner_home) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .filter(|name| name.to_str() != Some(OPENCODE_DIR))
            .collect(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(source) => return Err(io_error(owner_home, source)),
    };
    sync_links(&places.config, owner_home, &wanted)
}

/// The owner's `opencode.json` as a whole, read only; empty when it is missing, too large or does not parse.
fn owner_config(home: &Path) -> Map<String, Value> {
    let file = owner_config_dir(home).join(CONFIG_FILE);
    let text = match read_capped(&file, MAX_OWNER_CONFIG_BYTES) {
        Ok(text) => text,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(file = %file.display(), %err, "the owner's Opencode configuration is not used");
            }
            return Map::new();
        }
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(root)) => root,
        Ok(_) => Map::new(),
        Err(err) => {
            tracing::warn!(file = %file.display(), %err, "the owner's Opencode configuration does not parse");
            Map::new()
        }
    }
}

/// What of the owner's configuration folder the master links: what Opencode reads to know how to work — instructions,
/// agents, commands, modes, skills, themes, tools, plugins. Not the files it keeps up itself there (`.gitignore`,
/// `package.json`, `node_modules`): written through a link, they would be the owner's (D1).
const LINKED_FROM_OWNER: [&str; 15] = [
    "AGENTS.md",
    "agents",
    "agent",
    "commands",
    "command",
    "modes",
    "mode",
    "skills",
    "skill",
    "themes",
    "tools",
    "tool",
    "plugins",
    "plugin",
    "prompts",
];

fn linked_from_owner(name: &std::ffi::OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| LINKED_FROM_OWNER.contains(&name))
}

/// Links the owner's agents, commands, `AGENTS.md` and the rest of their configuration folder into the master's, so
/// Opencode reads them where it reads its configuration — read in place, never copied or written (D1, D6). A link the
/// owner's folder no longer backs is removed; a real file in the master's folder is left alone.
fn link_owner_files(places: &Places, owner_dir: &Path) -> Result<(), AxiomataError> {
    let wanted: Vec<std::ffi::OsString> = match std::fs::read_dir(owner_dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .filter(|name| linked_from_owner(name))
            .collect(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(source) => return Err(io_error(owner_dir, source)),
    };
    sync_links(&places.opencode_config(), owner_dir, &wanted)
}

/// Makes `target_dir` hold a link to `source_dir/<name>` for every name in `wanted`: a link nothing wants any more is
/// removed, one pointing elsewhere replaced, a real file of `target_dir`'s own left alone.
fn sync_links(
    target_dir: &Path,
    source_dir: &Path,
    wanted: &[std::ffi::OsString],
) -> Result<(), AxiomataError> {
    let existing = std::fs::read_dir(target_dir).map_err(|source| io_error(target_dir, source))?;
    for entry in existing.filter_map(Result::ok) {
        let path = entry.path();
        let is_link =
            std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink());
        if is_link && !wanted.contains(&entry.file_name()) {
            std::fs::remove_file(&path).map_err(|source| io_error(&path, source))?;
        }
    }
    for name in wanted {
        let link = target_dir.join(name);
        let target = source_dir.join(name);
        match std::fs::symlink_metadata(&link) {
            Ok(meta) if meta.file_type().is_symlink() => {
                if std::fs::read_link(&link).ok().as_deref() == Some(target.as_path()) {
                    continue;
                }
                std::fs::remove_file(&link).map_err(|source| io_error(&link, source))?;
            }
            Ok(_) => {
                tracing::warn!(file = %link.display(), "the master's folder has a file of its own here; not linked");
                continue;
            }
            Err(_) => {}
        }
        std::os::unix::fs::symlink(&target, &link).map_err(|source| io_error(&link, source))?;
    }
    Ok(())
}

/// Removes, at any depth, every field named like a credential ([`CREDENTIAL_NAMES`]), every `headers` object
/// (where an `Authorization` would be) and every URL that carries a user or password (`https://user:pw@host`).
fn strip_credentials(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|key, value| {
                let name: String = key
                    .chars()
                    .filter(|c| *c != '-' && *c != '_')
                    .flat_map(char::to_lowercase)
                    .collect();
                name != "headers"
                    && !CREDENTIAL_NAMES.contains(&name.as_str())
                    && !url_with_userinfo(value)
            });
            map.values_mut().for_each(strip_credentials);
        }
        Value::Array(items) => {
            items.retain(|item| !url_with_userinfo(item));
            items.iter_mut().for_each(strip_credentials);
        }
        _ => {}
    }
}

/// Whether `value` is a URL with a user part (`scheme://user[:pw]@host…`).
fn url_with_userinfo(value: &Value) -> bool {
    let Some(text) = value.as_str() else {
        return false;
    };
    let Some((_, rest)) = text.split_once("://") else {
        return false;
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    authority.contains('@')
}

/// Gives the instance a port of its own in `service.json`, keeping what the service wrote there (its password). A
/// second service on Opencode's default port does not start (CP0).
fn ensure_port(file: &Path) -> Result<(), AxiomataError> {
    let mut service = match std::fs::read_to_string(file) {
        Ok(text) => serde_json::from_str::<Map<String, Value>>(&text).unwrap_or_default(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Map::new(),
        Err(source) => return Err(io_error(file, source)),
    };
    if service.get("port").and_then(Value::as_u64).is_some() {
        return Ok(());
    }
    service.insert(
        "port".to_owned(),
        json!(free_port().map_err(|source| io_error(file, source))?),
    );
    let text = serde_json::to_string_pretty(&service).expect("a JSON object always serialises");
    write_private_atomic(file, &text)
}

/// A port nobody listens on right now, from the system.
fn free_port() -> std::io::Result<u16> {
    Ok(std::net::TcpListener::bind(("127.0.0.1", 0))?
        .local_addr()?
        .port())
}

/// The instance's folders, private to the owner. The instance's own folder must be a real folder below an
/// absolute home: a relative one would make Opencode ignore the `XDG_*` variables (the spec says so) and fall
/// back to the owner's places, and a symbolic link could point the generated files anywhere.
fn make_instance_dirs(places: &Places) -> Result<(), AxiomataError> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let refuse = |reason: &str| {
        io_error(
            &places.root,
            std::io::Error::new(std::io::ErrorKind::InvalidInput, reason.to_owned()),
        )
    };
    if !places.root.is_absolute() {
        return Err(refuse("the app's Opencode folder must be an absolute path"));
    }
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&places.root)
        .map_err(|source| io_error(&places.root, source))?;
    let meta =
        std::fs::symlink_metadata(&places.root).map_err(|source| io_error(&places.root, source))?;
    if !meta.is_dir() {
        return Err(refuse("the app's Opencode folder is not a plain folder"));
    }
    // A folder made earlier keeps the mode it had; the one holding the generated keys is tightened every time.
    std::fs::set_permissions(&places.root, std::fs::Permissions::from_mode(0o700))
        .map_err(|source| io_error(&places.root, source))?;
    for dir in [&places.config, &places.data, &places.state, &places.cache] {
        let dir = dir.join(OPENCODE_DIR);
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .map_err(|source| io_error(&dir, source))?;
    }
    Ok(())
}

/// Writes `text` to `file` through a new private file and a rename, so the service never reads half of it. The
/// temporary name is fresh and created exclusively: nothing planted under it (a link) is ever written through.
fn write_private_atomic(file: &Path, text: &str) -> Result<(), AxiomataError> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temp = file.with_file_name(format!(".{name}.{}.{nanos}.tmp", std::process::id()));
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut handle| {
            handle.write_all(text.as_bytes())?;
            handle.sync_all()
        });
    if let Err(source) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(io_error(&temp, source));
    }
    std::fs::rename(&temp, file).map_err(|source| {
        let _ = std::fs::remove_file(&temp);
        io_error(file, source)
    })
}

fn read_capped(file: &Path, max: u64) -> std::io::Result<String> {
    use std::io::Read as _;
    let opened = std::fs::File::open(file)?;
    if !opened.metadata()?.is_file() {
        return Err(std::io::Error::other("not a regular file"));
    }
    let mut text = String::new();
    opened.take(max + 1).read_to_string(&mut text)?;
    if text.len() as u64 > max {
        return Err(std::io::Error::other(format!("larger than {max} bytes")));
    }
    Ok(text)
}

fn io_error(path: &Path, source: std::io::Error) -> AxiomataError {
    AxiomataError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProviderSettings;
    use axiomata_harness::mcp::config::{Origin, ServerConfig, ServerEntry};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "axiomata-instance-{label}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_service_gets_the_instance_folders_in_place_of_the_owners() {
        let places = Places::below(Instance::Write, Path::new("/app/opencode/write"));
        let ambient = vec![
            ("PATH".to_owned(), "/usr/bin".to_owned()),
            ("XDG_CONFIG_HOME".to_owned(), "/owner/config".to_owned()),
            ("XDG_RUNTIME_DIR".to_owned(), "/run".to_owned()),
        ];
        let env = service_env_from(ambient, &places);
        let get = |key: &str| {
            env.iter()
                .filter(|(k, _)| k == key)
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            get("XDG_CONFIG_HOME"),
            ["/app/opencode/write/config"],
            "the owner's folder is replaced"
        );
        assert_eq!(get("XDG_STATE_HOME"), ["/app/opencode/write/state"]);
        assert_eq!(get("XDG_DATA_HOME"), ["/app/opencode/write/data"]);
        assert_eq!(get("XDG_CACHE_HOME"), ["/app/opencode/write/cache"]);
        assert_eq!(get("XDG_RUNTIME_DIR"), ["/run"], "other XDG variables stay");
        assert_eq!(get("PATH"), ["/usr/bin"]);
    }

    #[test]
    fn a_pane_attaches_through_the_instances_folders_but_keeps_the_owners_look() {
        let places = Places::below(Instance::Write, Path::new("/app/it's/write"));
        let prefix = tui_env_prefix_for(&places);
        assert!(
            prefix.starts_with(r"env XDG_STATE_HOME='/app/it'\''s/write/state' "),
            "{prefix}"
        );
        assert!(
            prefix.contains(r"XDG_DATA_HOME='/app/it'\''s/write/data'"),
            "{prefix}"
        );
        assert!(
            prefix.contains(r"XDG_CACHE_HOME='/app/it'\''s/write/cache'"),
            "{prefix}"
        );
        assert!(
            !prefix.contains("XDG_CONFIG_HOME"),
            "theme and keys stay the owner's"
        );
    }

    fn config_with(openrouter_key: Option<&str>, ollama_base: Option<&str>) -> Config {
        let mut config = Config::default();
        config.agents.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                api_key: openrouter_key.map(str::to_owned),
                ..ProviderSettings::default_for(ProviderId::OpenRouter)
            },
        );
        config.agents.providers.insert(
            ProviderId::Ollama,
            ProviderSettings {
                base_url: ollama_base.map(str::to_owned),
                ..ProviderSettings::default_for(ProviderId::Ollama)
            },
        );
        config
    }

    #[test]
    fn keys_and_endpoints_come_from_the_apps_config_over_the_owners_catalog() {
        let mut catalog = json!({
            "openrouter": {
                "settings": { "apiKey": "owner-key", "timeout": 30 },
                "headers": { "Authorization": "Bearer owner" },
                "body": { "provider": { "quantizations": ["fp8"], "ignore": ["morph"] } },
                "models": { "deepseek/deepseek-v4.1-flash": { "body": { "provider": { "sort": "throughput" } } } }
            },
            "ollama": { "settings": { "baseURL": "http://elsewhere:1/v1" }, "models": { "q": { "name": "Q" } } }
        });
        strip_credentials(&mut catalog);
        let Value::Object(catalog) = catalog else {
            unreachable!()
        };
        let generated = generated_config(
            catalog,
            &config_with(Some("app-key"), Some("http://localhost:11434/")),
            &[],
            Instance::Write,
        );
        assert!(
            generated.get("permissions").is_none(),
            "the write instance keeps Opencode's defaults"
        );
        let openrouter = &generated["providers"]["openrouter"];
        assert_eq!(openrouter["settings"]["apiKey"], "app-key");
        assert_eq!(
            openrouter["settings"]["timeout"], 30,
            "the owner's tuning stays"
        );
        assert!(
            openrouter.get("headers").is_none(),
            "headers never come along"
        );
        assert_eq!(
            openrouter["models"]["deepseek/deepseek-v4.1-flash"]["body"]["provider"]["sort"],
            "throughput"
        );
        assert_eq!(
            openrouter["body"],
            json!({ "provider": { "quantizations": ["fp8"], "ignore": ["morph"] } }),
            "the routing for every model of the provider comes along"
        );
        let ollama = &generated["providers"]["ollama"];
        assert_eq!(ollama["settings"]["baseURL"], "http://localhost:11434/v1");
        assert_eq!(ollama["package"], OPENAI_COMPATIBLE);
        assert_eq!(ollama["models"]["q"]["name"], "Q");
        assert_eq!(generated["$schema"], CONFIG_SCHEMA);
    }

    #[test]
    fn without_an_app_key_no_owner_key_is_left_behind() {
        let mut catalog =
            json!({ "openrouter": { "settings": { "apiKey": "owner-key", "api_key": "x" } } });
        strip_credentials(&mut catalog);
        let Value::Object(catalog) = catalog else {
            unreachable!()
        };
        let generated = generated_config(catalog, &config_with(None, None), &[], Instance::Write);
        assert!(!generated.to_string().contains("owner-key"));
        assert!(generated["providers"].get("ollama").is_none());
    }

    #[test]
    fn tuning_named_like_a_part_of_a_credential_stays() {
        let mut value = json!({
            "maxTokens": 8, "budget_tokens": 2, "keywords": ["k"],
            "x-api-key": "k", "Auth": "a", "baseURL": "https://user:pw@proxy.example/v1",
            "mirrors": ["https://a:b@c.example", "https://c.example"]
        });
        strip_credentials(&mut value);
        assert_eq!(
            value,
            json!({ "maxTokens": 8, "budget_tokens": 2, "keywords": ["k"], "mirrors": ["https://c.example"] })
        );
    }

    #[test]
    fn the_apps_key_never_goes_to_an_endpoint_of_the_owners_catalog() {
        let catalog = json!({ "openrouter": {
            "package": "aisdk:@ai-sdk/openai-compatible",
            "settings": { "baseURL": "https://proxy.example/v1", "timeout": 5 }
        }});
        let Value::Object(catalog) = catalog else {
            unreachable!()
        };
        let generated = generated_config(
            catalog,
            &config_with(Some("app-key"), None),
            &[],
            Instance::Write,
        );
        let openrouter = &generated["providers"]["openrouter"];
        assert_eq!(
            openrouter["settings"],
            json!({ "apiKey": "app-key", "timeout": 5 })
        );
        assert!(openrouter.get("package").is_none());
    }

    fn write_mcp(home: &Path, text: &str) {
        std::fs::create_dir_all(home.join(".axh")).unwrap();
        std::fs::write(home.join(".axh/mcp.json"), text).unwrap();
    }

    #[test]
    fn prepare_writes_a_private_config_and_tells_when_the_service_runs_another() {
        use std::os::unix::fs::PermissionsExt;
        let base = temp_dir("prepare");
        let home = base.join("home");
        write_mcp(
            &home,
            r#"{"mcpServers": {"mail": {"command": "uvx", "args": ["mail"]}}}"#,
        );
        let places = Places::below(Instance::Write, &base.join("app/opencode/write"));
        let config = config_with(Some("app-key"), None);

        assert!(
            prepare_at(&places, &home, &config).unwrap(),
            "never started: stale"
        );
        let file = places.opencode_config().join(CONFIG_FILE);
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(&places.root), 0o700);
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(
            written["mcp"]["servers"]["mail"]["command"],
            json!(["uvx", "mail"])
        );
        assert!(places.opencode_config().join(SERVICE_FILE).exists());

        std::fs::copy(&file, places.root.join(STARTED_WITH_FILE)).unwrap();
        assert!(
            !prepare_at(&places, &home, &config).unwrap(),
            "started with this one"
        );
        let rotated = config_with(Some("rotated"), None);
        assert!(
            prepare_at(&places, &home, &rotated).unwrap(),
            "a new key needs a restart"
        );
        let leftovers = std::fs::read_dir(places.opencode_config())
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp")
            })
            .count();
        assert_eq!(leftovers, 0, "no temporary file stays behind");

        write_mcp(&home, "{ not json");
        assert!(matches!(
            prepare_at(&places, &home, &config),
            Err(AxiomataError::AgentApi { .. })
        ));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn prepare_refuses_a_relative_or_linked_instance_folder() {
        let base = temp_dir("refuse");
        let config = Config::default();
        let relative = Places::below(Instance::Write, Path::new("relative/write"));
        assert!(matches!(
            prepare_at(&relative, &base, &config),
            Err(AxiomataError::Io { .. })
        ));
        std::fs::create_dir_all(base.join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(base.join("elsewhere"), base.join("linked")).unwrap();
        assert!(matches!(
            prepare_at(
                &Places::below(Instance::Write, &base.join("linked")),
                &base,
                &config
            ),
            Err(AxiomataError::Io { .. })
        ));
        assert!(
            std::fs::read_dir(base.join("elsewhere"))
                .unwrap()
                .next()
                .is_none(),
            "nothing written through the link"
        );
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn the_read_instance_refuses_edits_and_all_but_reading_commands() {
        let generated = generated_config(Map::new(), &Config::default(), &[], Instance::Read);
        let rules = generated["permissions"]
            .as_array()
            .expect("the read instance has rules");
        let effect = |action: &str, resource: &str| {
            rules
                .iter()
                .find(|rule| rule["action"] == action && rule["resource"] == resource)
                .map(|rule| rule["effect"].as_str().unwrap().to_owned())
        };
        assert_eq!(effect("edit", "*").as_deref(), Some("deny"));
        assert_eq!(effect("shell", "*").as_deref(), Some("deny"));
        assert_eq!(effect("external_directory", "*").as_deref(), Some("deny"));
        assert_eq!(effect("shell", "git diff *").as_deref(), Some("allow"));
        let first_allow = rules
            .iter()
            .position(|rule| rule["effect"] == "allow")
            .unwrap();
        let last_deny_all = rules
            .iter()
            .rposition(|rule| rule["resource"] == "*" && rule["effect"] == "deny")
            .unwrap();
        assert!(
            last_deny_all < first_allow,
            "the read-only commands come after the refusal they lift"
        );
        assert_eq!(
            rules.last().unwrap()["effect"],
            "deny",
            "the ways past the checkout come last"
        );
    }

    #[test]
    fn each_instance_indexes_its_own_state() {
        for (index, instance) in Instance::ALL.into_iter().enumerate() {
            assert_eq!(instance as usize, index);
        }
        assert_eq!(
            Instance::Master as usize + 1,
            Instance::COUNT,
            "the master is the last one"
        );
    }

    #[test]
    fn the_masters_shell_finds_the_owners_other_configuration() {
        let base = temp_dir("config-home");
        let owner_home = base.join("owner");
        for dir in ["gh", "git", OPENCODE_DIR] {
            std::fs::create_dir_all(owner_home.join(dir)).unwrap();
        }
        let places = Places::below(Instance::Master, &base.join("app/opencode/master"));
        make_instance_dirs(&places).unwrap();
        link_owner_config_home(&places, &owner_home).unwrap();
        assert_eq!(
            std::fs::read_link(places.config.join("gh")).ok(),
            Some(owner_home.join("gh"))
        );
        assert!(
            !std::fs::symlink_metadata(places.config.join(OPENCODE_DIR))
                .unwrap()
                .file_type()
                .is_symlink(),
            "Opencode's own folder is the master's"
        );
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn each_instance_has_folders_of_its_own() {
        assert_ne!(
            expected_state_dir(Instance::Read),
            expected_state_dir(Instance::Write)
        );
        assert_eq!(Instance::for_access(true), Instance::Read);
        assert_eq!(Instance::for_access(false), Instance::Write);
        assert!(tui_env_prefix(Instance::Read).contains("/opencode/read/state"));
    }

    #[test]
    fn the_master_runs_the_owners_configuration_under_the_apps_keys() {
        let owner = json!({
            "model": "openrouter/x/y",
            "agents": { "build": { "model": "ollama/q" } },
            "permissions": [{ "action": "shell", "resource": "*", "effect": "ask" }],
            "mcp": { "servers": { "own": { "type": "local", "command": ["own-server"] } } },
            "providers": { "openrouter": {
                "settings": { "apiKey": "owner-key" },
                "body": { "provider": { "sort": "throughput" } }
            } }
        });
        let Value::Object(owner) = owner else {
            unreachable!()
        };
        let master = master_config(owner, &config_with(Some("app-key"), None));
        assert_eq!(master["agents"]["build"]["model"], "ollama/q");
        assert_eq!(
            master["permissions"][0]["effect"], "ask",
            "the owner's rules, as outside"
        );
        assert_eq!(
            master["mcp"]["servers"]["own"]["command"],
            json!(["own-server"])
        );
        assert_eq!(master["model"], "openrouter/x/y");
        let openrouter = &master["providers"]["openrouter"];
        assert_eq!(
            openrouter["settings"]["apiKey"], "app-key",
            "one source of keys"
        );
        assert_eq!(openrouter["body"]["provider"]["sort"], "throughput");
        assert_eq!(master["$schema"], CONFIG_SCHEMA);
    }

    #[test]
    fn the_master_links_the_owners_files_and_keeps_its_own_configuration_and_port() {
        let base = temp_dir("links");
        let owner = base.join("owner/opencode");
        std::fs::create_dir_all(owner.join("agents")).unwrap();
        std::fs::create_dir_all(owner.join("skills")).unwrap();
        std::fs::create_dir_all(owner.join("commands")).unwrap();
        for file in [
            "AGENTS.md",
            CONFIG_FILE,
            SERVICE_FILE,
            ".gitignore",
            "package.json",
        ] {
            std::fs::write(owner.join(file), "x").unwrap();
        }
        let places = Places::below(Instance::Master, &base.join("app/opencode/master"));
        make_instance_dirs(&places).unwrap();
        let own = places.opencode_config();
        std::fs::write(own.join("commands"), "the master's own").unwrap();

        link_owner_files(&places, &owner).unwrap();
        let link = |name: &str| std::fs::read_link(own.join(name)).ok();
        assert_eq!(link("agents"), Some(owner.join("agents")));
        assert_eq!(link("AGENTS.md"), Some(owner.join("AGENTS.md")));
        assert_eq!(link(CONFIG_FILE), None, "generated, never the owner's");
        assert_eq!(link(SERVICE_FILE), None, "a port of its own");
        for kept_by_opencode in [".gitignore", "package.json"] {
            assert_eq!(
                link(kept_by_opencode),
                None,
                "Opencode writes it: never through a link"
            );
        }
        assert_eq!(
            link("commands"),
            None,
            "a file of the master's own is left alone"
        );

        std::fs::remove_dir(owner.join("skills")).unwrap();
        link_owner_files(&places, &owner).unwrap();
        assert!(
            std::fs::symlink_metadata(own.join("skills")).is_err(),
            "a link nothing backs goes"
        );
        assert!(
            owner.join("AGENTS.md").exists(),
            "the owner's files are only read"
        );
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn credentials_are_stripped_at_any_depth() {
        let mut value = json!({
            "a": { "accessToken": "t", "nested": [{ "client_secret": "s", "keep": 1 }] },
            "Password": "p",
            "name": "n"
        });
        strip_credentials(&mut value);
        assert_eq!(
            value,
            json!({ "a": { "nested": [{ "keep": 1 }] }, "name": "n" })
        );
    }

    #[test]
    fn mcp_servers_are_written_the_way_opencode_spells_them() {
        let entry = |name: &str, config: ServerConfig| ServerEntry {
            name: name.to_owned(),
            origin: Origin::Owner,
            config,
        };
        let servers = [
            entry(
                "mail",
                ServerConfig::Stdio {
                    command: "uvx".into(),
                    args: vec!["mcp-apple-mail".into()],
                    env: BTreeMap::from([("LANG".to_owned(), "C".to_owned())]),
                },
            ),
            entry(
                "web",
                ServerConfig::Http {
                    url: "https://example.com/mcp".into(),
                    headers: BTreeMap::new(),
                },
            ),
            entry(
                "secret",
                ServerConfig::Http {
                    url: "https://example.com/mcp".into(),
                    headers: BTreeMap::from([(
                        "Authorization".to_owned(),
                        "Bearer ${TOKEN}".to_owned(),
                    )]),
                },
            ),
            entry(
                "envvar",
                ServerConfig::Stdio {
                    command: "x".into(),
                    args: vec![],
                    env: BTreeMap::from([("TOKEN".to_owned(), "${TOKEN}".to_owned())]),
                },
            ),
        ];
        let generated = generated_config(Map::new(), &Config::default(), &servers, Instance::Write);
        let servers = &generated["mcp"]["servers"];
        assert_eq!(
            servers["mail"],
            json!({ "type": "local", "command": ["uvx", "mcp-apple-mail"], "environment": { "LANG": "C" } })
        );
        assert_eq!(servers["web"]["type"], "remote");
        assert!(
            servers.get("secret").is_none(),
            "a header variable the service lacks leaves the server out"
        );
        assert!(servers.get("envvar").is_none(), "so does an env variable");
    }

    #[test]
    fn ollamas_endpoint_is_its_openai_compatible_one() {
        assert_eq!(
            openai_base("http://localhost:11434"),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            openai_base("http://localhost:11434/v1/"),
            "http://localhost:11434/v1"
        );
    }

    #[test]
    fn a_port_is_chosen_once_and_the_services_password_is_kept() {
        let dir = temp_dir("port");
        let file = dir.join(SERVICE_FILE);
        ensure_port(&file).unwrap();
        let first: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        let port = first["port"].as_u64().expect("a port was written");
        std::fs::write(&file, format!(r#"{{"port": {port}, "password": "pw"}}"#)).unwrap();
        ensure_port(&file).unwrap();
        let second: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(second, json!({ "port": port, "password": "pw" }));
        std::fs::write(&file, r#"{"password": "pw"}"#).unwrap();
        ensure_port(&file).unwrap();
        let third: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(third["password"], "pw");
        assert!(third["port"].as_u64().is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_owners_catalog_is_read_without_credentials() {
        let home = temp_dir("catalog");
        let dir = home.join(".config/opencode");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(CONFIG_FILE),
            r#"{"providers": {"openrouter": {"settings": {"apiKey": "k"}, "models": {"m": {}}}}, "model": "x"}"#,
        )
        .unwrap();
        // The test process may carry an XDG_CONFIG_HOME of its own; only a home without one is checked here.
        if std::env::var_os("XDG_CONFIG_HOME").is_none() {
            let catalog = owner_catalog(&home);
            assert_eq!(
                Value::Object(catalog),
                json!({ "openrouter": { "settings": {}, "models": { "m": {} } } })
            );
        }
        assert!(owner_catalog(&home.join("nowhere")).is_empty());
        std::fs::remove_dir_all(&home).unwrap();
    }
}
