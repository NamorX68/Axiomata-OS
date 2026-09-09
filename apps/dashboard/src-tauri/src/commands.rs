//! Tauri command handlers exposed to the dashboard frontend.
//!
//! The managed state is the `AxiomataCore` itself: `config` sits behind an
//! `Arc<RwLock<Config>>` (writable at runtime since `save_config` exists),
//! `core.db` behind a `Mutex`. Every command reads config by cloning it out
//! from under the lock in its own statement (`state.config.read()...clone()`)
//! before doing anything else, so the guard is never held across an
//! `.await` — the same discipline `core.db`'s `Mutex` already needed.

use axiomata_core::AxiomataCore;
use axiomata_core::agents::{self, ChatMode, ChatReply};
use axiomata_core::bridge::{self, ActionRequest, ActionResponse, ManifestEntry};
use axiomata_core::config::{Config, ProviderId, ProviderSettings};
use axiomata_core::dashboard::{self, LoadedState};
use axiomata_core::graph::{self, WorkspaceGraph};
use axiomata_core::importer;
use axiomata_core::memory::{self, MemoryStatus, SyncReport};
use axiomata_core::notes;
use axiomata_core::routines::{self, NewRoutine, Routine, RoutineRun};
use axiomata_core::skills::{self, RunRecord, RunSummary, Skill, SkippedSkill};
use axiomata_core::spend;
use axiomata_core::workspace::{self, SearchHit, WorkspaceFile, WorkspaceImage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::RwLock;
use tauri::State;

/// The Tauri-managed core engine.
pub type CoreState = AxiomataCore;

/// Static facts the shell shows in its top bar. Read once at startup.
#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    /// `config.owner`; empty when the user hasn't set one.
    pub owner: String,
    /// Last path component of `config.workspace_root` (e.g. "Axiomata-Workspace").
    pub workspace_name: String,
    /// Absolute workspace root, for tooltips / settings.
    pub workspace_root: String,
    /// The dashboard crate version.
    pub version: String,
}

/// Returns the owner line and workspace facts for the top bar.
#[tauri::command]
pub fn get_app_info(state: State<'_, CoreState>) -> AppInfo {
    let config = read_config(&state.config);
    let root = &config.workspace_root;
    AppInfo {
        owner: config.owner.clone(),
        workspace_name: root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        workspace_root: root.to_string_lossy().into_owned(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

// ---- Redacted config view for the webview (provider-hardening CP7) ----
//
// `get_config` used to hand the renderer the whole `Config`, including every
// `providers[*].api_key` and every `agents.claude_env` value. Before the
// model-provider feature, nothing sensitive was reachable from a (possibly
// XSS-compromised) webview at all. These view/update types keep the raw
// secrets on the Rust side: the renderer sees only a `has_key` flag and the
// *names* of the `claude_env` overrides, and sends key changes back as an
// explicit [`KeyUpdate`] rather than round-tripping a value it never had.

/// One provider's settings as sent to the renderer — credential redacted.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderSettingsView {
    pub base_url: Option<String>,
    /// Whether a non-blank `api_key` is stored. The value itself is never sent.
    pub has_key: bool,
    pub chat_model: String,
    pub skill_model: String,
}

/// `agents` as sent to the renderer. Omits `claude_model` (migration-only)
/// and the `claude_env` *values*.
#[derive(Debug, Clone, Serialize)]
pub struct AgentDefaultsView {
    pub ollama_model: String,
    pub skill_timeout_secs: u64,
    pub providers: BTreeMap<ProviderId, ProviderSettingsView>,
    pub active_provider: ProviderId,
    pub daily_usd_cap: Option<f64>,
    /// Names only of the `agents.claude_env` power-user overrides — their
    /// values can be Bedrock / proxy tokens, so they never cross the IPC line.
    pub claude_env_keys: Vec<String>,
}

/// The config the Settings dialog renders. See the module note above.
#[derive(Debug, Clone, Serialize)]
pub struct ConfigView {
    pub owner: String,
    pub workspace_root: String,
    pub agents: AgentDefaultsView,
}

impl ConfigView {
    fn from_config(c: &Config) -> Self {
        let providers = c
            .agents
            .providers
            .iter()
            .map(|(id, s)| {
                (
                    *id,
                    ProviderSettingsView {
                        base_url: s.base_url.clone(),
                        has_key: s.api_key.as_deref().is_some_and(|k| !k.trim().is_empty()),
                        chat_model: s.chat_model.clone(),
                        skill_model: s.skill_model.clone(),
                    },
                )
            })
            .collect();
        ConfigView {
            owner: c.owner.clone(),
            workspace_root: c.workspace_root.to_string_lossy().into_owned(),
            agents: AgentDefaultsView {
                ollama_model: c.agents.ollama_model.clone(),
                skill_timeout_secs: c.agents.skill_timeout_secs,
                providers,
                active_provider: c.agents.active_provider,
                daily_usd_cap: c.agents.daily_usd_cap,
                claude_env_keys: c.agents.claude_env.keys().cloned().collect(),
            },
        }
    }
}

/// How the renderer wants one provider's stored API key changed.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum KeyUpdate {
    /// The key field wasn't edited — keep whatever is stored.
    Keep,
    /// The key field was emptied — clear the stored key.
    Clear,
    /// The key field holds a new value.
    Set { value: String },
}

/// One provider's settings coming back from the dialog. `api_key` is a
/// [`KeyUpdate`], not a value.
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderSettingsUpdate {
    pub base_url: Option<String>,
    pub api_key: KeyUpdate,
    pub chat_model: String,
    pub skill_model: String,
}

/// `agents` coming back from the dialog. No `claude_env` (no editor for it;
/// preserved server-side) and no `claude_model`.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentDefaultsUpdate {
    pub ollama_model: String,
    pub skill_timeout_secs: u64,
    pub providers: BTreeMap<ProviderId, ProviderSettingsUpdate>,
    pub active_provider: ProviderId,
    pub daily_usd_cap: Option<f64>,
}

/// The payload `save_config` accepts. See the module note above.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfigUpdate {
    pub owner: String,
    pub workspace_root: String,
    pub agents: AgentDefaultsUpdate,
}

/// Folds a renderer [`ConfigUpdate`] onto `current`: everything the dialog can
/// edit is taken from the update, but each provider's `api_key` is resolved
/// against what's already stored (the renderer never had the raw value), and
/// `agents.claude_env` + the migration-only `claude_model` are carried over
/// from `current` untouched.
fn merge_view_update(current: &Config, update: ConfigUpdate) -> Config {
    let mut merged = current.clone();
    merged.owner = update.owner;
    merged.workspace_root = PathBuf::from(update.workspace_root);
    merged.agents.ollama_model = update.agents.ollama_model;
    merged.agents.skill_timeout_secs = update.agents.skill_timeout_secs;
    merged.agents.active_provider = update.agents.active_provider;
    merged.agents.daily_usd_cap = update.agents.daily_usd_cap;

    let mut providers = BTreeMap::new();
    for (id, u) in update.agents.providers {
        let stored_key = current
            .agents
            .providers
            .get(&id)
            .and_then(|s| s.api_key.clone());
        let api_key = match u.api_key {
            KeyUpdate::Keep => stored_key,
            KeyUpdate::Clear => None,
            KeyUpdate::Set { value } => Some(value),
        };
        providers.insert(
            id,
            ProviderSettings {
                base_url: u.base_url,
                api_key,
                chat_model: u.chat_model,
                skill_model: u.skill_model,
            },
        );
    }
    // A provider the dialog didn't send stays as-is (defensive; it always
    // sends all three).
    for (id, s) in &current.agents.providers {
        providers.entry(*id).or_insert_with(|| s.clone());
    }
    merged.agents.providers = providers;
    merged
}

/// Returns the redacted editable config for the Settings dialog — see
/// [`ConfigView`]. The raw `api_key` / `claude_env` values are **not** sent.
#[tauri::command]
pub fn get_config(state: State<'_, CoreState>) -> ConfigView {
    ConfigView::from_config(&read_config(&state.config))
}

/// Today's / this month's recorded agent spend for the active model-routing
/// provider, plus the configured daily cap — shown in the Settings provider
/// section (provider-hardening checkpoint 4). `metered` is `false` for the
/// subscription-billed Anthropic provider.
#[tauri::command]
pub fn get_spend_summary(state: State<'_, CoreState>) -> Result<spend::SpendSummary, String> {
    let config = read_config(&state.config);
    let db = state.db.lock().expect("run-log database mutex is poisoned");
    spend::active_provider_summary_now(&db, &config).map_err(|err| err.to_string())
}

/// Validates and saves `new_config`: writes the full config to
/// `config.toml` (via `Config::save`, which also fixes up its `0o600`
/// permissions) and updates the live in-memory config other commands read.
///
/// **Workspace root is a special case.** A live workspace swap has too wide
/// a blast radius — the memory router, the particle graph, module manifests,
/// and every open note all assume `config.workspace_root` is fixed for the
/// process's lifetime. So a changed `workspace_root` is written to disk
/// immediately (a restart will pick it up) but is *not* swapped into the
/// live in-memory config; every other field (owner, agent providers/models,
/// `claude_env`) applies live, effective on the very next `claude -p` spawn.
/// Returns `true` if the workspace root actually changed, so the frontend
/// can prompt for a restart instead of quietly no-op'ing that part of the
/// save.
///
/// Takes a [`ConfigUpdate`], not a `Config`: the renderer never holds the raw
/// `api_key` / `claude_env` values, so those are merged in from the current
/// live config here (see [`merge_view_update`]).
#[tauri::command]
pub fn save_config(state: State<'_, CoreState>, new_config: ConfigUpdate) -> Result<bool, String> {
    // Merge against the current live config so the retained secrets come from
    // Rust-side state. The read guard is dropped before `apply_config_update`
    // takes the write guard; a single Settings dialog is the only caller, so
    // that read→write gap can't realistically race.
    let merged = merge_view_update(&read_config(&state.config), new_config);
    apply_config_update(&state.config, merged)
}

/// Clones `Config` out from under `config`'s lock. Every command does this
/// instead of holding the `RwLockReadGuard`, so the guard is never held
/// across an `.await` (irrelevant for a sync command, but keeping one
/// pattern everywhere means a command that later becomes `async` can't
/// silently reintroduce that bug). Takes `&RwLock<Config>` rather than
/// `State` so it — and [`apply_config_update`] below — are plain, unit-
/// testable functions with no Tauri test harness needed.
fn read_config(config: &RwLock<Config>) -> Config {
    config.read().expect("config lock poisoned").clone()
}

/// [`save_config`]'s actual logic, factored out from the `State` extraction
/// so it's directly unit-testable. See that command's doc comment for the
/// workspace-root special case this implements.
fn apply_config_update(config: &RwLock<Config>, mut new_config: Config) -> Result<bool, String> {
    // The authoritative save-time gate — see `Config::validate_for_save`.
    // Rejects here (empty workspace root, half-typed paid provider, …)
    // happen before the write lock is ever taken.
    new_config.validate_for_save()?;

    let mut current = config
        .write()
        .map_err(|err| format!("config lock poisoned: {err}"))?;
    let workspace_changed = new_config.workspace_root != current.workspace_root;

    new_config.save().map_err(|err| err.to_string())?;
    if workspace_changed {
        // Disk has the new root now; the live copy keeps the old one until
        // restart — see `save_config`'s doc comment.
        new_config.workspace_root = current.workspace_root.clone();
    }
    *current = new_config;

    Ok(workspace_changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiomata_core::config::ProviderId;
    use axiomata_core::paths;
    use std::env;
    use std::path::PathBuf;

    /// Serializes tests in this module that set `AXIOMATA_HOME` — same
    /// reasoning as `axiomata_core::test_support::ENV_MUTEX`, which isn't
    /// exported from that crate, so this module keeps its own.
    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// A fresh, unique scratch directory under the OS temp dir, mirroring
    /// `axiomata_core::test_support::unique_temp_dir` (also not exported).
    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
    }

    // ---- CP7: redacted config view + key-preserving merge ----

    /// A `Config` with OpenRouter active, a real key, and a `claude_env`
    /// override — the shape that used to leak wholesale to the webview.
    fn config_with_secrets() -> Config {
        let mut c = Config {
            workspace_root: PathBuf::from("/ws"),
            ..Config::default()
        };
        c.agents.active_provider = ProviderId::OpenRouter;
        c.agents.providers.insert(
            ProviderId::OpenRouter,
            ProviderSettings {
                base_url: Some("https://openrouter.ai/api".to_string()),
                api_key: Some("sk-or-v1-SECRET".to_string()),
                chat_model: "z-ai/glm-5.3-flash".to_string(),
                skill_model: "deepseek/deepseek-v4-flash".to_string(),
            },
        );
        c.agents.claude_env.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            "BEARER-SECRET".to_string(),
        );
        c
    }

    #[test]
    fn config_view_redacts_api_keys_and_claude_env_values() {
        let view = ConfigView::from_config(&config_with_secrets());
        let json = serde_json::to_string(&view).unwrap();

        assert!(!json.contains("SECRET"), "no raw secret may appear: {json}");
        assert!(!json.contains("BEARER-SECRET"));
        // has_key still tells the UI a credential is stored.
        assert!(view.agents.providers[&ProviderId::OpenRouter].has_key);
        assert!(!view.agents.providers[&ProviderId::Anthropic].has_key);
        // claude_env is reduced to its key names only.
        assert_eq!(view.agents.claude_env_keys, vec!["ANTHROPIC_AUTH_TOKEN"]);
    }

    #[test]
    fn merge_view_update_keeps_set_and_clears_the_stored_key_per_keyupdate() {
        let current = config_with_secrets();
        let base_update = |key: KeyUpdate| ConfigUpdate {
            owner: "Roman".to_string(),
            workspace_root: "/ws".to_string(),
            agents: AgentDefaultsUpdate {
                ollama_model: current.agents.ollama_model.clone(),
                skill_timeout_secs: current.agents.skill_timeout_secs,
                active_provider: ProviderId::OpenRouter,
                daily_usd_cap: current.agents.daily_usd_cap,
                providers: BTreeMap::from([(
                    ProviderId::OpenRouter,
                    ProviderSettingsUpdate {
                        base_url: Some("https://openrouter.ai/api".to_string()),
                        api_key: key,
                        chat_model: "new-chat-model".to_string(),
                        skill_model: "deepseek/deepseek-v4-flash".to_string(),
                    },
                )]),
            },
        };

        let kept = merge_view_update(&current, base_update(KeyUpdate::Keep));
        let or = |c: &Config| c.agents.providers[&ProviderId::OpenRouter].api_key.clone();
        assert_eq!(
            or(&kept).as_deref(),
            Some("sk-or-v1-SECRET"),
            "Keep retains"
        );
        // ...and an unrelated edit in the same save still lands.
        assert_eq!(
            kept.agents.providers[&ProviderId::OpenRouter].chat_model,
            "new-chat-model"
        );
        // claude_env is preserved untouched (the update carries none).
        assert_eq!(
            kept.agents
                .claude_env
                .get("ANTHROPIC_AUTH_TOKEN")
                .map(String::as_str),
            Some("BEARER-SECRET")
        );

        let set = merge_view_update(
            &current,
            base_update(KeyUpdate::Set {
                value: "sk-or-v1-NEW".to_string(),
            }),
        );
        assert_eq!(or(&set).as_deref(), Some("sk-or-v1-NEW"));

        let cleared = merge_view_update(&current, base_update(KeyUpdate::Clear));
        assert_eq!(or(&cleared), None);
    }

    #[test]
    fn save_path_with_keep_still_validates_the_merged_config() {
        // A required provider whose stored key is empty + a Keep update must
        // still be rejected by `validate_for_save` on the merged result.
        let mut current = Config {
            workspace_root: PathBuf::from("/ws"),
            ..Config::default()
        };
        current.agents.active_provider = ProviderId::OpenRouter;
        current
            .agents
            .providers
            .get_mut(&ProviderId::OpenRouter)
            .unwrap()
            .api_key = None;

        let update = ConfigUpdate {
            owner: String::new(),
            workspace_root: "/ws".to_string(),
            agents: AgentDefaultsUpdate {
                ollama_model: "llama3.2".to_string(),
                skill_timeout_secs: 300,
                active_provider: ProviderId::OpenRouter,
                daily_usd_cap: Some(2.0),
                providers: BTreeMap::from([(
                    ProviderId::OpenRouter,
                    ProviderSettingsUpdate {
                        base_url: Some("https://openrouter.ai/api".to_string()),
                        api_key: KeyUpdate::Keep,
                        chat_model: "m".to_string(),
                        skill_model: "m".to_string(),
                    },
                )]),
            },
        };
        let merged = merge_view_update(&current, update);
        assert!(merged.validate_for_save().unwrap_err().contains("API key"));
    }

    #[test]
    fn apply_config_update_rejects_an_empty_workspace_root() {
        let original_root = PathBuf::from("/original/workspace");
        let lock = RwLock::new(Config {
            workspace_root: original_root.clone(),
            ..Config::default()
        });
        let new_config = Config {
            workspace_root: PathBuf::new(),
            ..Config::default()
        };

        let err = apply_config_update(&lock, new_config).unwrap_err();
        assert!(err.contains("workspace root"));
        // Nothing was touched — the reject happens before the lock is ever
        // taken for write.
        assert_eq!(lock.read().unwrap().workspace_root, original_root);
    }

    #[test]
    fn apply_config_update_writes_the_new_root_to_disk_but_keeps_the_old_one_live() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-cmd-config-home");
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, matching `axiomata_core::paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }

        let old_root = home.join("OldWorkspace");
        let lock = RwLock::new(Config {
            workspace_root: old_root.clone(),
            ..Config::default()
        });

        let new_root = home.join("NewWorkspace");
        let new_config = Config {
            workspace_root: new_root.clone(),
            owner: "Ada".to_string(),
            ..Config::default()
        };

        let changed = apply_config_update(&lock, new_config).expect("save should succeed");
        assert!(changed, "the workspace root did change");

        // The live copy keeps the OLD root (no live workspace swap)...
        let live = lock.read().unwrap();
        assert_eq!(live.workspace_root, old_root);
        // ...but every other field did apply live.
        assert_eq!(live.owner, "Ada");
        drop(live);

        // ...while disk has the NEW root, for the next restart to pick up.
        let on_disk = Config::load().expect("load should succeed");
        assert_eq!(on_disk.workspace_root, new_root);

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn apply_config_update_applies_everything_live_when_the_workspace_root_is_unchanged() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-cmd-config-home-unchanged");
        std::fs::create_dir_all(&home).unwrap();
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }

        let root = home.join("Workspace");
        let lock = RwLock::new(Config {
            workspace_root: root.clone(),
            ..Config::default()
        });

        let new_config = Config {
            workspace_root: root.clone(),
            owner: "Bea".to_string(),
            ..Config::default()
        };

        let changed = apply_config_update(&lock, new_config).expect("save should succeed");
        assert!(!changed, "the workspace root did not change");
        assert_eq!(lock.read().unwrap().owner, "Bea");
        assert_eq!(lock.read().unwrap().workspace_root, root);

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn read_config_clones_out_from_under_the_lock() {
        let lock = RwLock::new(Config {
            owner: "Cleo".to_string(),
            ..Config::default()
        });
        assert_eq!(read_config(&lock).owner, "Cleo");
    }

    #[test]
    fn apply_config_update_leaves_the_live_config_untouched_when_save_fails() {
        let _guard = ENV_MUTEX.lock().unwrap();
        // Point `AXIOMATA_HOME` at a path that is itself a plain *file*, not
        // a directory. `config_path()`'s parent is then that same file, so
        // `Config::save()`'s `fs::create_dir_all(parent)` fails
        // deterministically — a save failure with nothing OS-specific or
        // timing-dependent about it.
        let home_as_file = unique_temp_dir("axiomata-test-cmd-config-save-fails");
        std::fs::write(&home_as_file, b"not a directory").unwrap();
        // SAFETY: serialized by `_guard`, matching `axiomata_core::paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home_as_file);
        }

        let original_root = PathBuf::from("/original/workspace");
        let lock = RwLock::new(Config {
            owner: "Original".to_string(),
            workspace_root: original_root.clone(),
            ..Config::default()
        });
        let new_config = Config {
            owner: "Changed".to_string(),
            workspace_root: PathBuf::from("/new/workspace"),
            ..Config::default()
        };

        let err = apply_config_update(&lock, new_config).unwrap_err();
        assert!(!err.is_empty());

        // The write lock was taken (this isn't the empty-root reject path,
        // which never takes it), but the failed `save()` must still leave
        // the live copy exactly as it was — no partial/half-applied update.
        let live = lock.read().unwrap();
        assert_eq!(live.owner, "Original");
        assert_eq!(live.workspace_root, original_root);
        drop(live);

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_file(&home_as_file);
    }

    #[test]
    fn apply_config_update_applies_provider_and_env_settings_live_too() {
        // The existing "keeps the old root live" test only checks `owner`
        // for "every other field does apply live" — this exercises the
        // `agents` sub-struct specifically, since that's the actual payload
        // the Settings dialog's provider section writes.
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-cmd-config-agents-live");
        std::fs::create_dir_all(&home).unwrap();
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }

        let root = home.join("Workspace");
        let lock = RwLock::new(Config {
            workspace_root: root.clone(),
            ..Config::default()
        });

        let mut new_config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };
        new_config.agents.active_provider = ProviderId::Ollama;
        // Ollama is non-Anthropic, so `validate_for_save` now requires
        // concrete models on the active provider (its seeded defaults leave
        // them blank for the owner to fill in).
        if let Some(ollama) = new_config.agents.providers.get_mut(&ProviderId::Ollama) {
            ollama.chat_model = "llama3.2".to_string();
            ollama.skill_model = "llama3.2".to_string();
        }
        new_config.agents.claude_env.insert(
            "ANTHROPIC_BASE_URL".to_string(),
            "http://localhost:11434".to_string(),
        );

        let changed = apply_config_update(&lock, new_config).expect("save should succeed");
        assert!(!changed, "the workspace root did not change");

        let live = lock.read().unwrap();
        assert_eq!(live.agents.active_provider, ProviderId::Ollama);
        assert_eq!(
            live.agents
                .claude_env
                .get("ANTHROPIC_BASE_URL")
                .map(String::as_str),
            Some("http://localhost:11434")
        );
        drop(live);

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn concurrent_saves_serialize_without_corrupting_the_live_config() {
        // `apply_config_update` holds the `RwLock` write guard across both
        // the disk write and the live-memory update, so two concurrent
        // Settings-dialog saves can never interleave into a torn state: the
        // live config and the on-disk config must always agree on whichever
        // write happened last, never a mix of the two.
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-cmd-config-concurrent");
        std::fs::create_dir_all(&home).unwrap();
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }

        let root = home.join("Workspace");
        let lock = RwLock::new(Config {
            workspace_root: root.clone(),
            ..Config::default()
        });

        std::thread::scope(|scope| {
            for owner in ["Thread-A", "Thread-B"] {
                let lock_ref = &lock;
                let root = root.clone();
                scope.spawn(move || {
                    let new_config = Config {
                        workspace_root: root,
                        owner: owner.to_string(),
                        ..Config::default()
                    };
                    apply_config_update(lock_ref, new_config).expect("save should succeed");
                });
            }
        });

        let live_owner = lock.read().unwrap().owner.clone();
        assert!(
            live_owner == "Thread-A" || live_owner == "Thread-B",
            "live owner should be a whole value from one writer, got {live_owner:?}"
        );
        let on_disk = Config::load().expect("load should succeed");
        assert_eq!(
            on_disk.owner, live_owner,
            "disk and live config must agree on whichever write happened last"
        );

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}

/// Reads `~/.axiomata/dashboard.json` (raw text) or the defaults; a corrupt
/// file is moved to `.bak` and reported in `recovered_backup`.
#[tauri::command]
pub fn get_dashboard_state() -> Result<LoadedState, String> {
    dashboard::load_state().map_err(|err| err.to_string())
}

/// Validates and atomically writes the dashboard state handed in by the
/// frontend. The core only checks "object with numeric `version`".
#[tauri::command]
pub fn save_dashboard_state(json: String) -> Result<(), String> {
    dashboard::save_state(&json).map_err(|err| err.to_string())
}

/// The workspace as a graph for the particle view: files (with area, title,
/// size, mtime), wiki/relative links, skills, routines, the CLAUDE.md hub.
/// Walks the workspace and reads Markdown heads; takes the DB lock only for
/// the routine list.
#[tauri::command]
pub fn get_workspace_graph(state: State<'_, CoreState>) -> Result<WorkspaceGraph, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    graph::build(&read_config(&state.config), &db).map_err(|err| err.to_string())
}

/// Renders the mounted-module manifest into `~/.axiomata/module-context.md`
/// for the agent. Returns `true` if the file changed.
#[tauri::command]
pub fn write_module_manifest(entries: Vec<ManifestEntry>) -> Result<bool, String> {
    bridge::write_manifest(&entries).map_err(|err| err.to_string())
}

/// Takes every pending agent → module action request out of the inbox queue
/// (each is returned exactly once). The frontend dispatches them and answers
/// with `complete_module_action`.
#[tauri::command]
pub fn poll_module_actions() -> Result<Vec<ActionRequest>, String> {
    bridge::drain_inbox().map_err(|err| err.to_string())
}

/// Writes the frontend's answer to a polled request into the outbox queue.
#[tauri::command]
pub fn complete_module_action(response: ActionResponse) -> Result<(), String> {
    bridge::complete(&response).map_err(|err| err.to_string())
}

/// One assistant turn. `mode` is `"chat"` (never asks, read-mostly) or
/// `"instruct"` (may edit workspace files). Pass the `session_id` from the
/// previous reply to continue that conversation. `allowed_tools` is the
/// `--allowedTools` value a connector module's write action needs to reach
/// its MCP tool at all (see `AgentRequest::allowed_tools`) — `None` for a
/// plain assistant-bar turn. Runs the agent with no lock held. The turn is
/// checked against the daily spend cap first and logged to `chat_turns`
/// afterwards (`agents::chat_and_record`).
#[tauri::command]
pub async fn assistant_send(
    state: State<'_, CoreState>,
    message: String,
    session_id: Option<String>,
    mode: String,
    allowed_tools: Option<String>,
) -> Result<ChatReply, String> {
    let mode = match mode.as_str() {
        "chat" => ChatMode::Chat,
        "instruct" => ChatMode::Instruct,
        other => return Err(format!("unknown assistant mode {other:?}")),
    };
    let config = read_config(&state.config);
    agents::chat_and_record(&config, &state.db, message, session_id, mode, allowed_tools)
        .await
        .map_err(|err| err.to_string())
}

/// Reads the user's custom theme CSS (`~/.axiomata/theme.css`, or the
/// absolute `.css` path from the dashboard settings). `None` if absent. The
/// frontend validates it before injecting.
#[tauri::command]
pub fn load_custom_css(path: Option<String>) -> Result<Option<String>, String> {
    dashboard::load_custom_css(path.as_deref().map(std::path::Path::new))
        .map_err(|err| err.to_string())
}

/// Case-insensitive full-text search over the workspace's text files; every
/// word must occur on one line. At most `limit` files, most hits first.
#[tauri::command]
pub fn search_workspace(
    state: State<'_, CoreState>,
    query: String,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    workspace::search(&read_config(&state.config), &query, limit.clamp(1, 200))
        .map_err(|err| err.to_string())
}

/// Reads a UTF-8 file by workspace-relative path (≤ 1 MiB, no `..`, no
/// symlinks, must resolve inside `config.workspace_root`).
#[tauri::command]
pub fn read_workspace_file(
    state: State<'_, CoreState>,
    rel: String,
) -> Result<WorkspaceFile, String> {
    workspace::read_file(&read_config(&state.config), &rel).map_err(|err| err.to_string())
}

/// Reads a raster image (png/jpeg/gif/webp, ≤ 8 MiB) by workspace-relative
/// path, base64-encoded — the binary counterpart to `read_workspace_file`,
/// used by the Markdown viewer to inline a note's own relatively-referenced
/// images. Same path guard (no `..`, no symlinks, must resolve inside
/// `config.workspace_root`).
#[tauri::command]
pub fn read_workspace_image(
    state: State<'_, CoreState>,
    rel: String,
) -> Result<WorkspaceImage, String> {
    workspace::read_image(&read_config(&state.config), &rel).map_err(|err| err.to_string())
}

/// Atomically writes a workspace file under the same guard as
/// `read_workspace_file`. May create at most one new top-level directory as
/// `rel`'s immediate parent (see `workspace::ensure_immediate_parent_dir`);
/// never creates anything deeper.
#[tauri::command]
pub fn write_workspace_file(
    state: State<'_, CoreState>,
    rel: String,
    content: String,
) -> Result<(), String> {
    workspace::write_file(&read_config(&state.config), &rel, &content)
        .map_err(|err| err.to_string())
}

/// Creates a new Markdown note from `content` alone — no separate title:
/// if `content` starts with its own `#` heading that wins, otherwise the
/// agent proposes one as part of the same turn. The agent also picks which
/// workspace area it belongs in — reusing an existing one, proposing a new
/// one, or `Inbox` as a last resort (see `notes::placement_prompt`) — and a
/// file name; this command then does the actual write (sanitized, never
/// overwriting), the same "agent decides, code writes" split
/// `axiomata-cli import obsidian` uses. Returns the workspace-relative path
/// written.
#[tauri::command]
pub async fn create_note(state: State<'_, CoreState>, content: String) -> Result<String, String> {
    let config = read_config(&state.config);
    let existing = importer::existing_areas(&config.workspace_root);
    let prompt = notes::placement_prompt(&content, &existing);
    let reply = agents::chat_and_record(&config, &state.db, prompt, None, ChatMode::Instruct, None)
        .await
        .map_err(|err| err.to_string())?;
    let placement = notes::parse_placement(&reply.reply_markdown).map_err(|err| err.to_string())?;
    notes::write_placed_note(
        &config.workspace_root,
        &content,
        &placement.area,
        &placement.file_name,
        placement.title.as_deref(),
    )
    .map_err(|err| err.to_string())
}

/// Lists every discovered skill (`~/.axiomata/skills/`).
#[tauri::command]
pub fn list_skills() -> Result<Vec<Skill>, String> {
    skills::list_skills().map_err(|err| err.to_string())
}

/// Lists every skill directory that was skipped during discovery, and why
/// (a broken `SKILL.md`, a symlink, …) — the Skills Deck shows these as a
/// warning instead of a broken skill just quietly not being there.
#[tauri::command]
pub fn list_skipped_skills() -> Result<Vec<SkippedSkill>, String> {
    skills::list_skipped_skills().map_err(|err| err.to_string())
}

/// Returns the most recent skill runs as slim summaries, newest first. `limit`
/// is clamped to `skills::MAX_RUN_LIMIT` in the core.
#[tauri::command]
pub fn list_runs(state: State<'_, CoreState>, limit: usize) -> Result<Vec<RunSummary>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    skills::list_runs(&db, limit).map_err(|err| err.to_string())
}

/// Returns one full run (with captured output) by id, or `null` if unknown.
#[tauri::command]
pub fn get_run(state: State<'_, CoreState>, id: i64) -> Result<Option<RunRecord>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    skills::get_run(&db, id).map_err(|err| err.to_string())
}

/// Runs a skill by name and returns the persisted run record.
#[tauri::command]
pub async fn run_skill(state: State<'_, CoreState>, name: String) -> Result<RunRecord, String> {
    let config = read_config(&state.config);
    skills::execute_and_record_skill(&name, &config, &state.db)
        .await
        .map_err(|err| err.to_string())
}

/// Regenerates the workspace router `CLAUDE.md` blocks. Reads no database.
#[tauri::command]
pub fn sync_memory(state: State<'_, CoreState>) -> Result<SyncReport, String> {
    memory::sync(&read_config(&state.config)).map_err(|err| err.to_string())
}

/// Reports whether the memory router is stale — a plain walk-and-compare.
#[tauri::command]
pub fn get_memory_status(state: State<'_, CoreState>) -> Result<MemoryStatus, String> {
    memory::status(&read_config(&state.config)).map_err(|err| err.to_string())
}

/// Lists every routine, soonest next-fire first.
#[tauri::command]
pub fn list_routines(state: State<'_, CoreState>) -> Result<Vec<Routine>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    routines::store::list(&db).map_err(|err| err.to_string())
}

/// Creates a routine. `new.target` arrives as `{ "type": "skill" | "prompt",
/// "value": "..." }`. Returns the stored routine (with its computed next fire).
#[tauri::command]
pub fn add_routine(state: State<'_, CoreState>, new: NewRoutine) -> Result<Routine, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    routines::store::add(&db, new).map_err(|err| err.to_string())
}

/// Enables or disables a routine by id. Returns `false` if there is no such
/// routine. Re-enabling recomputes the next fire from now.
#[tauri::command]
pub fn set_routine_enabled(
    state: State<'_, CoreState>,
    id: i64,
    enabled: bool,
) -> Result<bool, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    routines::store::set_enabled(&db, id, enabled).map_err(|err| err.to_string())
}

/// Replaces a routine's name, cron expression, target, backend, and enabled
/// flag — a full replace like `add_routine`, not a partial patch. Returns
/// `None` if there is no such routine.
#[tauri::command]
pub fn update_routine(
    state: State<'_, CoreState>,
    id: i64,
    new: NewRoutine,
) -> Result<Option<Routine>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    routines::store::update(&db, id, new).map_err(|err| err.to_string())
}

/// Permanently deletes a routine and its firing history. Returns `false` if
/// there is no such routine.
#[tauri::command]
pub fn delete_routine(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    routines::store::delete(&db, id).map_err(|err| err.to_string())
}

/// Returns a routine's firing history, newest first. `limit` is clamped in the
/// core to `routines::store::MAX_ROUTINE_RUN_LIMIT`.
#[tauri::command]
pub fn routine_history(
    state: State<'_, CoreState>,
    id: i64,
    limit: usize,
) -> Result<Vec<RoutineRun>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    routines::store::list_runs(&db, id, limit).map_err(|err| err.to_string())
}
