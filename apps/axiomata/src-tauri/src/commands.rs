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
use axiomata_core::board;
use axiomata_core::board_mirror;
use axiomata_core::bridge::{self, ActionRequest, ActionResponse, ManifestEntry};
use axiomata_core::config::{Config, ProviderId, ProviderSettings};
use axiomata_core::dashboard::{self, LoadedState};
use axiomata_core::editor_settings::{self, LoadedEditorSettings};
use axiomata_core::editor_vi;
use axiomata_core::graph::{self, WorkspaceGraph};
use axiomata_core::ide;
use axiomata_core::importer;
use axiomata_core::json_state::LoadedJsonState;
use axiomata_core::memory::{self, MemoryStatus, SyncReport};
use axiomata_core::notes;
use axiomata_core::routines::{self, NewRoutine, Routine, RoutineRun};
use axiomata_core::skills::{self, RunRecord, RunSummary, Skill, SkippedSkill};
use axiomata_core::spend;
use axiomata_core::terminal_settings::{self, LoadedTerminalSettings};
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
    pub chat_provider: ProviderId,
    pub skill_provider: ProviderId,
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
                chat_provider: c.agents.chat_provider,
                skill_provider: c.agents.skill_provider,
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
    pub chat_provider: ProviderId,
    pub skill_provider: ProviderId,
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
    merged.agents.chat_provider = update.agents.chat_provider;
    merged.agents.skill_provider = update.agents.skill_provider;
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
///
/// `workspace_root` is taken from the config **on disk**, which may already
/// hold a change queued by an earlier save this session that only takes
/// effect on restart — so the dialog shows (and round-trips) the pending
/// value, not the frozen live one.
#[tauri::command]
pub fn get_config(state: State<'_, CoreState>) -> ConfigView {
    let mut view = ConfigView::from_config(&read_config(&state.config));
    if let Ok(on_disk) = Config::load() {
        view.workspace_root = on_disk.workspace_root.to_string_lossy().into_owned();
    }
    view
}

/// Today's / this month's recorded agent spend, one entry per distinct
/// model-routing provider across the chat and skill role selectors, plus the
/// configured daily cap — shown in the Settings provider section
/// (provider-hardening checkpoint 4). `metered` is `false` for the
/// subscription-billed Anthropic provider.
#[tauri::command]
pub fn get_spend_summary(state: State<'_, CoreState>) -> Result<Vec<spend::SpendSummary>, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    spend::role_spend_summaries_now(&db, &config).map_err(|err| err.to_string())
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
/// `claude_env`) applies live, effective on the very next `opencode run`.
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
    config
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone()
}

/// [`save_config`]'s actual logic, factored out from the `State` extraction
/// so it's directly unit-testable. See that command's doc comment for the
/// workspace-root special case this implements.
fn apply_config_update(config: &RwLock<Config>, mut new_config: Config) -> Result<bool, String> {
    // The authoritative save-time gate — see `Config::validate_for_save`.
    // Rejects here (empty workspace root, half-typed paid provider, …)
    // happen before the write lock is ever taken.
    new_config.validate_for_save()?;

    let mut current = config.write().unwrap_or_else(|poison| poison.into_inner());

    // Judge "did the workspace root change" against what's **on disk** — which
    // may already hold a change queued by an earlier save this session — not
    // against the frozen live root. Otherwise a later unrelated save (which
    // echoes back the root `get_config` surfaced) would silently revert a
    // pending workspace switch by rewriting the old root over the queued one.
    let on_disk_root = Config::load().ok().map(|c| c.workspace_root);
    let baseline_root = on_disk_root
        .as_deref()
        .unwrap_or(current.workspace_root.as_path());
    let workspace_changed = new_config.workspace_root.as_path() != baseline_root;

    new_config.save().map_err(|err| err.to_string())?;
    // The live workspace root is frozen for the process's lifetime — a live
    // swap has too wide a blast radius (see `save_config`'s doc). Whatever
    // was just written to disk, the in-memory copy keeps its root until
    // restart.
    new_config.workspace_root = current.workspace_root.clone();
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

    #[test]
    fn changing_a_plan_without_naming_its_goal_keeps_the_goal() {
        // Built from JSON: the shape the webview sends, and the one a `Plan` is read back from.
        let current: board::Plan = serde_json::from_value(serde_json::json!({
            "id": 1, "board_id": 1, "name": "P", "goal": "Add a dark mode", "status": "draft",
            "project_id": 3, "base_branch": null,
            "auto_start_max": null, "max_cost_usd": null, "max_tokens": null,
            "created_at": "2026-10-05T10:00:00Z", "updated_at": "2026-10-05T10:00:00Z", "approved_at": null
        }))
        .unwrap();
        let keep: PlanUpdate = serde_json::from_value(serde_json::json!({"name": "P2"})).unwrap();
        let kept = keep.into_fields(&current);
        assert_eq!(
            (kept.name.as_str(), kept.goal.as_str()),
            ("P2", "Add a dark mode")
        );

        let change: PlanUpdate =
            serde_json::from_value(serde_json::json!({"name": "P", "goal": "Another"})).unwrap();
        assert_eq!(change.into_fields(&current).goal, "Another");

        let clear: PlanUpdate =
            serde_json::from_value(serde_json::json!({"name": "P", "goal": ""})).unwrap();
        assert_eq!(
            clear.into_fields(&current).goal,
            "",
            "an empty goal is a goal the owner cleared"
        );
    }

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
        c.agents.chat_provider = ProviderId::OpenRouter;
        c.agents.skill_provider = ProviderId::OpenRouter;
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
                chat_provider: ProviderId::OpenRouter,
                skill_provider: ProviderId::OpenRouter,
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
        current.agents.chat_provider = ProviderId::OpenRouter;
        current.agents.skill_provider = ProviderId::OpenRouter;
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
                chat_provider: ProviderId::OpenRouter,
                skill_provider: ProviderId::OpenRouter,
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
        // There is always a config on disk after first run — seed one whose
        // root matches, so `apply_config_update`'s on-disk baseline is `root`.
        Config {
            workspace_root: root.clone(),
            ..Config::default()
        }
        .save()
        .unwrap();
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

    /// A workspace-root change queued by one save must survive a later,
    /// unrelated save in the same session (which echoes back the pending
    /// root, since that's what `get_config` now surfaces) — the second save
    /// must not rewrite the old root over the queued one.
    #[test]
    fn a_pending_workspace_root_survives_a_later_unrelated_save() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-cmd-config-pending-root");
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, matching `axiomata_core::paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }

        let old_root = home.join("Old");
        let new_root = home.join("New");
        Config {
            workspace_root: old_root.clone(),
            ..Config::default()
        }
        .save()
        .unwrap();
        let lock = RwLock::new(Config {
            workspace_root: old_root.clone(),
            ..Config::default()
        });

        // Save 1: change the root. Queued on disk, live keeps the old one.
        let changed = apply_config_update(
            &lock,
            Config {
                workspace_root: new_root.clone(),
                ..Config::default()
            },
        )
        .unwrap();
        assert!(changed);
        assert_eq!(Config::load().unwrap().workspace_root, new_root);
        assert_eq!(lock.read().unwrap().workspace_root, old_root);

        // Save 2: change something else. The frontend echoes back the root
        // `get_config` surfaces — the pending `new_root` — not the live one.
        let changed = apply_config_update(
            &lock,
            Config {
                workspace_root: new_root.clone(),
                owner: "Later".to_string(),
                ..Config::default()
            },
        )
        .unwrap();
        assert!(!changed, "the root didn't change relative to what's queued");
        assert_eq!(
            Config::load().unwrap().workspace_root,
            new_root,
            "the queued root must not be clobbered"
        );
        assert_eq!(lock.read().unwrap().owner, "Later");

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
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
        Config {
            workspace_root: root.clone(),
            ..Config::default()
        }
        .save()
        .unwrap();
        let lock = RwLock::new(Config {
            workspace_root: root.clone(),
            ..Config::default()
        });

        let mut new_config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };
        new_config.agents.chat_provider = ProviderId::Ollama;
        new_config.agents.skill_provider = ProviderId::Ollama;
        // Ollama is non-Anthropic, so `validate_for_save` now requires
        // concrete models on both role providers (its seeded defaults leave
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
        assert_eq!(live.agents.chat_provider, ProviderId::Ollama);
        assert_eq!(live.agents.skill_provider, ProviderId::Ollama);
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

    // ---- App Ring: `list_installed_apps`'s scan core (`scan_apps`) ----

    fn make_app(dir: &std::path::Path, name: &str) {
        std::fs::create_dir_all(dir.join(name)).unwrap();
    }

    #[test]
    fn scan_apps_finds_top_level_bundles_sorted_by_name() {
        let root = unique_temp_dir("scan-apps-basic");
        std::fs::create_dir_all(&root).unwrap();
        make_app(&root, "Zebra.app");
        make_app(&root, "Alpha.app");
        std::fs::write(root.join("readme.txt"), "not an app").unwrap();

        let result = scan_apps(std::slice::from_ref(&root), 300);
        assert_eq!(
            result
                .apps
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "Zebra"]
        );
        assert!(!result.truncated);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scan_apps_skips_a_missing_root_without_failing_the_others() {
        let root = unique_temp_dir("scan-apps-missing-root");
        std::fs::create_dir_all(&root).unwrap();
        make_app(&root, "Real.app");
        let ghost = unique_temp_dir("scan-apps-ghost"); // deliberately never created

        let result = scan_apps(&[ghost, root.clone()], 300);
        assert_eq!(result.apps.len(), 1);
        assert_eq!(result.apps[0].name, "Real");
        std::fs::remove_dir_all(&root).ok();
    }

    /// A `.app` bundle's own `Contents/…` can contain further
    /// `.app`-suffixed helper/plugin bundles — the scan must not surface
    /// those as their own top-level rows.
    #[test]
    fn scan_apps_does_not_recurse_into_a_bundle_s_own_nested_app() {
        let root = unique_temp_dir("scan-apps-nested");
        let frameworks = root.join("Outer.app/Contents/Frameworks");
        std::fs::create_dir_all(&frameworks).unwrap();
        make_app(&frameworks, "Helper.app");

        let result = scan_apps(std::slice::from_ref(&root), 300);
        assert_eq!(result.apps.len(), 1, "{:?}", result.apps);
        assert_eq!(result.apps[0].name, "Outer");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scan_apps_deduplicates_the_same_path_seen_via_two_roots() {
        let root = unique_temp_dir("scan-apps-dedup");
        std::fs::create_dir_all(&root).unwrap();
        make_app(&root, "Once.app");

        // The same root listed twice (e.g. `~/Applications` resolving to a
        // path already covered by another root) must not double the result.
        let result = scan_apps(&[root.clone(), root.clone()], 300);
        assert_eq!(result.apps.len(), 1);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scan_apps_strips_only_the_app_suffix_and_trims() {
        let root = unique_temp_dir("scan-apps-name");
        std::fs::create_dir_all(&root).unwrap();
        make_app(&root, "Visual Studio Code.app");

        let result = scan_apps(std::slice::from_ref(&root), 300);
        assert_eq!(result.apps[0].name, "Visual Studio Code");
        assert_eq!(
            result.apps[0].path,
            root.join("Visual Studio Code.app")
                .to_string_lossy()
                .into_owned()
        );
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scan_apps_caps_and_reports_truncated() {
        let root = unique_temp_dir("scan-apps-cap");
        std::fs::create_dir_all(&root).unwrap();
        for i in 0..5 {
            make_app(&root, &format!("App{i}.app"));
        }

        let result = scan_apps(std::slice::from_ref(&root), 3);
        assert_eq!(result.apps.len(), 3);
        assert!(result.truncated);

        let full = scan_apps(std::slice::from_ref(&root), 10);
        assert_eq!(full.apps.len(), 5);
        assert!(!full.truncated);
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn scan_apps_ignores_non_app_entries_and_a_plain_file_named_like_one() {
        let root = unique_temp_dir("scan-apps-ignore");
        std::fs::create_dir_all(&root).unwrap();
        make_app(&root, "Real.app");
        std::fs::create_dir_all(root.join("NotAnApp")).unwrap();
        // A plain file happening to be named like a bundle is not one.
        std::fs::write(root.join("Fake.app"), b"not a directory").unwrap();

        let result = scan_apps(std::slice::from_ref(&root), 300);
        assert_eq!(result.apps.len(), 1);
        assert_eq!(result.apps[0].name, "Real");
        std::fs::remove_dir_all(&root).ok();
    }

    /// The plan explicitly calls for treating a symlinked `.app` like any
    /// other entry — `path.is_dir()` follows symlinks (unlike
    /// `DirEntry::file_type()`), so this pins that platform-API assumption
    /// down with a real symlink instead of leaving it true only by
    /// inspection of `scan_apps`'s own comment.
    #[test]
    fn scan_apps_includes_a_symlinked_app_like_a_real_one() {
        let target_root = unique_temp_dir("scan-apps-symlink-target");
        std::fs::create_dir_all(&target_root).unwrap();
        make_app(&target_root, "Real.app");

        let root = unique_temp_dir("scan-apps-symlink-root");
        std::fs::create_dir_all(&root).unwrap();
        std::os::unix::fs::symlink(target_root.join("Real.app"), root.join("Linked.app")).unwrap();

        let result = scan_apps(std::slice::from_ref(&root), 300);
        assert_eq!(result.apps.len(), 1, "{:?}", result.apps);
        assert_eq!(result.apps[0].name, "Linked");

        std::fs::remove_dir_all(&target_root).ok();
        std::fs::remove_dir_all(&root).ok();
    }

    // ---- M7.3 CP8a: `BaseFile::from_blob` ----

    #[test]
    fn base_file_from_blob_reports_absence_and_size_without_reading_content() {
        assert!(matches!(
            BaseFile::from_blob("x.txt", ide::git::BaseBlob::Absent),
            BaseFile::Absent
        ));
        assert!(matches!(
            BaseFile::from_blob("x.txt", ide::git::BaseBlob::TooLarge { size: 999 }),
            BaseFile::TooLarge { size: 999 }
        ));
    }

    #[test]
    fn base_file_from_blob_reads_utf8_bytes_as_text() {
        let blob = ide::git::BaseBlob::Bytes(b"hello\nworld".to_vec());
        match BaseFile::from_blob("notes.md", blob) {
            BaseFile::Text { text } => assert_eq!(text, "hello\nworld"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn base_file_from_blob_treats_nul_bytes_as_binary_even_though_they_are_valid_utf8() {
        let blob = ide::git::BaseBlob::Bytes(b"a\0b".to_vec());
        match BaseFile::from_blob("notes.md", blob) {
            BaseFile::Binary { size } => assert_eq!(size, 3),
            other => panic!("expected Binary, got {other:?}"),
        }
    }

    #[test]
    fn base_file_from_blob_treats_non_utf8_bytes_as_binary() {
        let blob = ide::git::BaseBlob::Bytes(vec![0xff, 0xfe, 0x00, 0x01]);
        match BaseFile::from_blob("notes.md", blob) {
            BaseFile::Binary { size } => assert_eq!(size, 4),
            other => panic!("expected Binary, got {other:?}"),
        }
    }

    #[test]
    fn base_file_from_blob_reads_a_known_image_extension_as_an_image_even_if_the_bytes_look_like_text()
     {
        // The extension decides first; `image_mime` never looks at the bytes.
        let blob = ide::git::BaseBlob::Bytes(b"not really a png".to_vec());
        match BaseFile::from_blob("shot.png", blob) {
            BaseFile::Image { mime, base64 } => {
                assert_eq!(mime, "image/png");
                assert!(!base64.is_empty());
            }
            other => panic!("expected Image, got {other:?}"),
        }
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

/// Reads `~/.axiomata/terminal-settings.json` (raw text) or the defaults —
/// the Terminal module's own global preferences file (Checkpoint 5d of
/// `docs/plans/terminal.md`), a sibling of `dashboard.json`, not a section
/// of it. Same translation-only role as `get_dashboard_state` above; a
/// corrupt file is moved to `.bak` and reported in `recovered_backup`, same
/// contract as `axiomata_core::json_state::load`.
#[tauri::command]
pub fn get_terminal_settings() -> Result<LoadedTerminalSettings, String> {
    terminal_settings::load_settings().map_err(|err| err.to_string())
}

/// Validates and atomically writes the Terminal module's global preferences
/// handed in by the frontend. The core only checks "object with numeric
/// `version`", same as `save_dashboard_state`.
#[tauri::command]
pub fn save_terminal_settings(json: String) -> Result<(), String> {
    terminal_settings::save_settings(&json).map_err(|err| err.to_string())
}

/// Reads the editor's preferences (`editor-settings.json`), or the defaults.
/// Same contract as [`get_terminal_settings`].
#[tauri::command]
pub fn get_editor_settings() -> Result<LoadedEditorSettings, String> {
    editor_settings::load_settings().map_err(|err| err.to_string())
}

/// Validates and atomically writes the editor's preferences; the core only
/// checks "object with numeric `version`".
#[tauri::command]
pub fn save_editor_settings(json: String) -> Result<(), String> {
    editor_settings::save_settings(&json).map_err(|err| err.to_string())
}

/// Reads what Vi remembers across restarts (`editor-vi.json`), or an empty
/// default. Same contract as [`get_editor_settings`].
#[tauri::command]
pub fn get_editor_vi_state() -> Result<LoadedJsonState, String> {
    editor_vi::load_vi_state().map_err(|err| err.to_string())
}

/// Validates and atomically writes Vi's remembered state (0600); the core
/// only checks "object with numeric `version`".
#[tauri::command]
pub fn save_editor_vi_state(json: String) -> Result<(), String> {
    editor_vi::save_vi_state(&json).map_err(|err| err.to_string())
}

/// One `*.app` bundle found by [`list_installed_apps`]'s scan.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledApp {
    pub path: String,
    pub name: String,
}

/// [`list_installed_apps`]'s return shape. `truncated` mirrors
/// [`WorkspaceGraph`]'s own field — `true` once the scan hit
/// [`MAX_INSTALLED_APPS`], so the dialog can say so instead of silently
/// showing an incomplete list.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledAppsResult {
    pub apps: Vec<InstalledApp>,
    pub truncated: bool,
}

/// Cap on how many apps [`list_installed_apps`] returns — generous enough to
/// never realistically trigger on a real Mac (a few hundred at most), but
/// bounded all the same.
const MAX_INSTALLED_APPS: usize = 300;

/// Every `*.app` bundle for the App Ring's "+" dialog: the three standard
/// macOS application folders, deduplicated by path, sorted by name.
#[tauri::command]
pub fn list_installed_apps() -> Result<InstalledAppsResult, String> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
    ];
    if let Some(home) = home::home_dir() {
        roots.push(home.join("Applications"));
    }
    // No error path here: a missing/unreadable root (most machines have no
    // `~/Applications` at all) is `scan_apps`'s problem to skip quietly, not
    // this command's to fail on.
    Ok(scan_apps(&roots, MAX_INSTALLED_APPS))
}

/// Scans each of `roots` for `*.app` entries directly inside it (never
/// recursing into a found bundle itself — an `.app`'s own `Contents/…` can
/// contain further `.app`-suffixed helper/plugin bundles that have no
/// business showing up as top-level rows), deduplicated by path and capped
/// at `max`.
///
/// Takes `roots` as a parameter — rather than hard-coding the three real
/// macOS paths — purely so it's unit-testable against a temp-dir fixture;
/// [`list_installed_apps`] is the only caller that supplies the real ones.
/// A root that doesn't exist or can't be read is skipped, not an error: most
/// machines have no `~/Applications` at all, and that must not fail the
/// other two roots' results.
fn scan_apps(roots: &[PathBuf], max: usize) -> InstalledAppsResult {
    let mut apps: Vec<InstalledApp> = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // `.is_dir()` follows symlinks (unlike `DirEntry::file_type()`),
            // so a symlinked `.app` is picked up like any other entry —
            // deliberately no special-casing for it.
            if path.extension().and_then(|e| e.to_str()) != Some("app") || !path.is_dir() {
                continue;
            }
            let path_str = path.to_string_lossy().into_owned();
            if !seen_paths.insert(path_str.clone()) {
                continue;
            }
            // `file_stem` strips only the trailing `.app`, leaving the rest
            // of a macOS bundle name — already human-readable — untouched
            // beyond a trim.
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| path_str.clone());
            apps.push(InstalledApp {
                path: path_str,
                name,
            });
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    let truncated = apps.len() > max;
    apps.truncate(max);
    InstalledAppsResult { apps, truncated }
}

/// The workspace as a graph for the particle view: files (with area, title,
/// size, mtime), wiki/relative links, skills, routines, the CLAUDE.md hub.
/// Walks the workspace and reads Markdown heads; takes the DB lock only for
/// the routine list.
#[tauri::command]
pub fn get_workspace_graph(state: State<'_, CoreState>) -> Result<WorkspaceGraph, String> {
    let db = state.db_lock();
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

/// Deletes a workspace file under the same path guard as
/// `read_workspace_file` (no `..`, no symlink, must be an existing regular
/// file inside `config.workspace_root`). Used by the Second Brain detail
/// panel's "Delete" action.
#[tauri::command]
pub fn delete_workspace_file(state: State<'_, CoreState>, rel: String) -> Result<(), String> {
    workspace::delete_file(&read_config(&state.config), &rel).map_err(|err| err.to_string())
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
    let db = state.db_lock();
    skills::list_runs(&db, limit).map_err(|err| err.to_string())
}

/// Returns one full run (with captured output) by id, or `null` if unknown.
#[tauri::command]
pub fn get_run(state: State<'_, CoreState>, id: i64) -> Result<Option<RunRecord>, String> {
    let db = state.db_lock();
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
    let db = state.db_lock();
    routines::store::list(&db).map_err(|err| err.to_string())
}

/// Creates a routine. `new.target` arrives as `{ "type": "skill" | "prompt",
/// "value": "..." }`. Returns the stored routine (with its computed next fire).
#[tauri::command]
pub fn add_routine(state: State<'_, CoreState>, new: NewRoutine) -> Result<Routine, String> {
    let db = state.db_lock();
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
    let db = state.db_lock();
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
    let db = state.db_lock();
    routines::store::update(&db, id, new).map_err(|err| err.to_string())
}

/// Permanently deletes a routine and its firing history. Returns `false` if
/// there is no such routine.
#[tauri::command]
pub fn delete_routine(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    let db = state.db_lock();
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
    let db = state.db_lock();
    routines::store::list_runs(&db, id, limit).map_err(|err| err.to_string())
}

// ----------------------------------------------------------------- board ---
//
// Thin passthroughs to `axiomata_board::store`, in the same four-line shape as
// the routines commands above: take the state, lock, delegate, stringify. The
// board's own error text is what the frontend shows, so nothing is translated
// here. Card mutations land with CP-K2b; these are what rendering a board and
// managing the board list need.

#[tauri::command]
pub fn list_boards(state: State<'_, CoreState>) -> Result<Vec<board::Board>, String> {
    let db = state.db_lock();
    board::store::list_boards(&db).map_err(|err| err.to_string())
}

/// Returns `None` if there is no such board.
#[tauri::command]
pub fn get_board(state: State<'_, CoreState>, id: i64) -> Result<Option<board::Board>, String> {
    let db = state.db_lock();
    board::store::get_board(&db, id).map_err(|err| err.to_string())
}

/// Creates a board together with its three default columns.
#[tauri::command]
pub fn create_board(state: State<'_, CoreState>, name: String) -> Result<board::Board, String> {
    let config = read_config(&state.config);
    let mut db = state.db_lock();
    let created = board::store::create_board(&mut db, &name).map_err(|err| err.to_string())?;
    board_mirror::after_change(&db, &config, created.id);
    Ok(created)
}

/// Returns `None` if there is no such board.
#[tauri::command]
pub fn rename_board(
    state: State<'_, CoreState>,
    id: i64,
    name: String,
) -> Result<Option<board::Board>, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    let renamed = board::store::rename_board(&db, id, &name).map_err(|err| err.to_string())?;
    if renamed.is_some() {
        board_mirror::after_change(&db, &config, id);
    }
    Ok(renamed)
}

/// Deletes a board with its columns and cards. Returns `false` if there is no
/// such board. The caller is expected to have shown `count_board_cards` first.
#[tauri::command]
pub fn delete_board(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    let config = read_config(&state.config);
    let mut db = state.db_lock();
    let gone = board::store::delete_board(&mut db, id).map_err(|err| err.to_string())?;
    if gone {
        board_mirror::remove(&config, id);
    }
    Ok(gone)
}

#[tauri::command]
pub fn count_board_cards(state: State<'_, CoreState>, board_id: i64) -> Result<i64, String> {
    let db = state.db_lock();
    board::store::count_cards(&db, board_id).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn list_board_columns(
    state: State<'_, CoreState>,
    board_id: i64,
) -> Result<Vec<board::Column>, String> {
    let db = state.db_lock();
    board::store::list_columns(&db, board_id).map_err(|err| err.to_string())
}

/// Archived cards are left out unless `include_archived` is set.
#[tauri::command]
pub fn list_board_cards(
    state: State<'_, CoreState>,
    board_id: i64,
    include_archived: bool,
) -> Result<Vec<board::Card>, String> {
    let db = state.db_lock();
    board::store::list_cards(&db, board_id, include_archived).map_err(|err| err.to_string())
}

// Board mutations (CP-K2b). Same four-line shape; every rule they have to
// respect lives in the store, not here.

#[tauri::command]
pub fn create_card(
    state: State<'_, CoreState>,
    new: board::NewCard,
) -> Result<board::Card, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    let created = board::store::create_card(&db, &new).map_err(|err| err.to_string())?;
    board_mirror::after_change(&db, &config, created.board_id);
    Ok(created)
}

/// Full replace of a card's writable fields — never its signatures, which move
/// only through claim/release/verify. Returns `None` if there is no such card.
#[tauri::command]
pub fn update_card(
    state: State<'_, CoreState>,
    id: i64,
    fields: board::CardFields,
) -> Result<Option<board::Card>, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    let updated = board::store::update_card(&db, id, &fields).map_err(|err| err.to_string())?;
    if let Some(card) = &updated {
        board_mirror::after_change(&db, &config, card.board_id);
    }
    Ok(updated)
}

/// `index` counts the cards the moved one will sit among, excluding itself.
#[tauri::command]
pub fn move_card(
    state: State<'_, CoreState>,
    id: i64,
    column_id: i64,
    index: usize,
) -> Result<bool, String> {
    let config = read_config(&state.config);
    let mut db = state.db_lock();
    // The owner drags the card: handing a card into the review column claims it for them, so an agent can judge it.
    let moved = board::store::move_card_as(&mut db, id, column_id, index, Some(OWNER_ACTOR))
        .map_err(|err| err.to_string())?;
    if moved {
        board_mirror::after_card_change(&db, &config, id);
    }
    Ok(moved)
}

#[tauri::command]
pub fn delete_card(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    // The board has to be read *before* the card is gone with it.
    let board_id = board::store::get_card(&db, id)
        .map_err(|err| err.to_string())?
        .map(|card| card.board_id);
    let gone = board::store::delete_card(&db, id).map_err(|err| err.to_string())?;
    if let Some(board_id) = board_id.filter(|_| gone) {
        board_mirror::after_change(&db, &config, board_id);
    }
    Ok(gone)
}

#[tauri::command]
pub fn set_card_archived(
    state: State<'_, CoreState>,
    id: i64,
    archived: bool,
) -> Result<bool, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    let changed =
        board::store::set_card_archived(&db, id, archived).map_err(|err| err.to_string())?;
    if changed {
        board_mirror::after_card_change(&db, &config, id);
    }
    Ok(changed)
}

// The agent flow on the board (a2a.md, CP-A2): plans, dependencies, history, proposals. The webview acts as the owner;
// agents reach the same functions through the CLI and, from CP-A4, the MCP server.

/// The actor the dashboard acts as. The same spelling the CLI defaults to.
const OWNER_ACTOR: &str = "human:owner";

#[tauri::command]
pub fn list_board_plans(
    state: State<'_, CoreState>,
    board_id: i64,
) -> Result<Vec<board::Plan>, String> {
    let db = state.db_lock();
    board::flow::list_plans(&db, board_id).map_err(|err| err.to_string())
}

#[tauri::command]
pub fn create_board_plan(
    state: State<'_, CoreState>,
    board_id: i64,
    fields: board::PlanFields,
) -> Result<board::Plan, String> {
    let db = state.db_lock();
    board::flow::create_plan(&db, board_id, &fields).map_err(|err| err.to_string())
}

/// What the frontend sends to change a plan. `goal` left out keeps the plan's own: `PlanFields` is a full replace,
/// and a
/// caller that does not know about the goal must not wipe it.
#[derive(serde::Deserialize)]
pub struct PlanUpdate {
    name: String,
    #[serde(default)]
    goal: Option<String>,
    /// Left out keeps the plan's own, like the goal.
    #[serde(default)]
    project_id: Option<i64>,
    #[serde(default)]
    auto_start_max: Option<u32>,
    #[serde(default)]
    max_cost_usd: Option<f64>,
    #[serde(default)]
    max_tokens: Option<u64>,
}

impl PlanUpdate {
    fn into_fields(self, current: &board::Plan) -> board::PlanFields {
        board::PlanFields {
            goal: self.goal.unwrap_or_else(|| current.goal.clone()),
            project_id: self.project_id.or(current.project_id),
            name: self.name,
            auto_start_max: self.auto_start_max,
            max_cost_usd: self.max_cost_usd,
            max_tokens: self.max_tokens,
        }
    }
}

#[tauri::command]
pub fn update_board_plan(
    state: State<'_, CoreState>,
    id: i64,
    fields: PlanUpdate,
) -> Result<Option<board::Plan>, String> {
    let db = state.db_lock();
    let Some(current) = board::flow::get_plan(&db, id).map_err(|err| err.to_string())? else {
        return Ok(None);
    };
    let fields = fields.into_fields(&current);
    board::flow::update_plan(&db, id, &fields).map_err(|err| err.to_string())
}

/// The owner's yes to a plan: its proposals move to Offen, and the planner that proposed them is done — its session goes.
///
/// `run_by_itself` decides, in the same step, whether the plan then runs by itself (A4, CP-A8): `true` sets it to start
/// its cards on its own (in `project_id`, or the one it has), `false` makes it manual — written either way, so a plan the
/// owner unchecked does not run because of an earlier setting. Left out, the plan's settings are not touched. `None` if
/// there is no such draft plan; nothing is changed then.
#[tauri::command]
pub async fn approve_board_plan(
    state: State<'_, CoreState>,
    id: i64,
    run_by_itself: Option<bool>,
    project_id: Option<i64>,
) -> Result<Option<usize>, String> {
    let config = read_config(&state.config);
    let moved = {
        let mut db = state.db_lock();
        let Some(current) = board::flow::get_plan(&db, id).map_err(|err| err.to_string())? else {
            return Ok(None);
        };
        if current.status != board::PlanStatus::Draft {
            return Ok(None);
        }
        if let Some(run) = run_by_itself {
            let fields = board::PlanFields {
                goal: current.goal.clone(),
                project_id: if run {
                    current.project_id.or(project_id)
                } else {
                    current.project_id
                },
                name: current.name.clone(),
                auto_start_max: run.then_some(AUTO_UNBOUNDED),
                max_cost_usd: current.max_cost_usd,
                max_tokens: current.max_tokens,
            };
            if run && fields.project_id.is_none() {
                return Err("a plan that runs by itself needs a project".to_owned());
            }
            board::flow::update_plan(&db, id, &fields).map_err(|err| err.to_string())?;
        }
        let moved =
            board::flow::approve_plan(&mut db, id, OWNER_ACTOR).map_err(|err| err.to_string())?;
        if moved.is_some()
            && let Some(plan) = board::flow::get_plan(&db, id).map_err(|err| err.to_string())?
        {
            board_mirror::after_change(&db, &config, plan.board_id);
        }
        moved
    };
    if moved.is_some() {
        axiomata_core::plan_session::forget_plan_sessions(&state, id).await;
    }
    Ok(moved)
}

/// `auto_start_max` of a plan that runs by itself: no limit of its own — the dependencies and the machine's cap decide.
const AUTO_UNBOUNDED: u32 = 64;

#[tauri::command]
pub async fn close_board_plan(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    let closed = {
        let db = state.db_lock();
        board::flow::close_plan(&db, id).map_err(|err| err.to_string())?
    };
    if closed {
        axiomata_core::plan_session::forget_plan_sessions(&state, id).await;
    }
    Ok(closed)
}

#[tauri::command]
pub async fn delete_board_plan(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    let config = read_config(&state.config);
    let gone = {
        let mut db = state.db_lock();
        let board_id = board::flow::get_plan(&db, id)
            .map_err(|err| err.to_string())?
            .map(|plan| plan.board_id);
        let gone = board::flow::delete_plan(&mut db, id).map_err(|err| err.to_string())?;
        if let Some(board_id) = board_id.filter(|_| gone) {
            board_mirror::after_change(&db, &config, board_id);
        }
        gone
    };
    if gone {
        axiomata_core::plan_session::forget_plan_sessions(&state, id).await;
    }
    Ok(gone)
}

/// The owner starts a planner for a draft plan (CP-A7): a session of the planner role on the engine they picked,
/// reading a read-only checkout of the chosen project. Its harness starts when its pane opens
/// (`prepare_ide_agent`), which sees that the session holds a plan and starts it unattended.
#[tauri::command]
pub async fn start_plan_session(
    state: State<'_, CoreState>,
    plan_id: i64,
    project_id: i64,
    engine_id: Option<String>,
    grill: Option<bool>,
) -> Result<axiomata_core::plan_session::PlanSession, String> {
    axiomata_core::plan_session::start_plan_session(
        &state,
        &axiomata_core::plan_session::PlanStartRequest {
            plan_id,
            project_id,
            engine_id,
            grill: grill.unwrap_or(false),
        },
    )
    .await
    .map_err(|err| err.to_string())
}

/// Every "needs first" edge on a board, as `[card, needs]` pairs.
#[tauri::command]
pub fn list_board_dependencies(
    state: State<'_, CoreState>,
    board_id: i64,
) -> Result<Vec<(i64, i64)>, String> {
    let db = state.db_lock();
    board::flow::list_dependencies(&db, board_id).map_err(|err| err.to_string())
}

/// `false` if the edge existed already; a refusal (cycle, other plan) comes back as the error text.
#[tauri::command]
pub fn add_card_dependency(
    state: State<'_, CoreState>,
    card_id: i64,
    needs: i64,
) -> Result<bool, String> {
    let config = read_config(&state.config);
    let mut db = state.db_lock();
    let added =
        board::flow::add_dependency(&mut db, card_id, needs).map_err(|err| err.to_string())?;
    if added {
        board_mirror::after_card_change(&db, &config, card_id);
    }
    Ok(added)
}

#[tauri::command]
pub fn remove_card_dependency(
    state: State<'_, CoreState>,
    card_id: i64,
    needs: i64,
) -> Result<bool, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    let removed =
        board::flow::remove_dependency(&db, card_id, needs).map_err(|err| err.to_string())?;
    if removed {
        board_mirror::after_card_change(&db, &config, card_id);
    }
    Ok(removed)
}

/// The owner starts a card (A3/A20, A2A CP-A6a): a new session of the card's role on an engine is made for it, in the
/// chosen project, and the card is taken for that session. The harness itself starts when the session's pane opens
/// and calls `prepare_ide_agent`; the pane then sees that the session holds a card and starts it unattended.
#[tauri::command]
pub fn start_card_session(
    state: State<'_, CoreState>,
    card_id: i64,
    project_id: i64,
    engine_id: Option<String>,
) -> Result<axiomata_core::card_session::CardSession, String> {
    let config = read_config(&state.config);
    let session = axiomata_core::card_session::start_card_session(
        &state,
        &axiomata_core::card_session::StartRequest {
            card_id,
            project_id,
            engine_id,
        },
    )
    .map_err(|err| err.to_string())?;
    let db = state.db_lock();
    board_mirror::after_card_change(&db, &config, card_id);
    Ok(session)
}

/// The owner gives a started card back (A23, "Freigeben"): the claim is dropped, the card waits in its open column
/// again and the secret of the session that held it is taken back. `false` if nobody held it.
#[tauri::command]
pub fn release_card(state: State<'_, CoreState>, card_id: i64) -> Result<bool, String> {
    let config = read_config(&state.config);
    let released = axiomata_core::card_session::release_card_session(&state, card_id)
        .map_err(|err| err.to_string())?;
    if released {
        let db = state.db_lock();
        board_mirror::after_card_change(&db, &config, card_id);
    }
    Ok(released)
}

/// The owner starts the review of a reported card by hand, on an engine of their choice — what the studio does on its
/// own as soon as a card is reported, except where it found no engine other than the worker's, or the work changes
/// the files agents read their configuration from and the owner has to accept that (A2A CP-A6b).
#[tauri::command]
pub async fn start_review_session(
    state: State<'_, CoreState>,
    card_id: i64,
    engine_id: Option<String>,
    allow_agent_config: bool,
) -> Result<axiomata_core::card_session::ReviewSession, String> {
    let config = read_config(&state.config);
    let session = axiomata_core::card_session::start_review_session(
        &state,
        &axiomata_core::card_session::ReviewRequest {
            card_id,
            engine_id,
            allow_agent_config,
        },
    )
    .await
    .map_err(|err| err.to_string())?;
    let db = state.db_lock();
    board_mirror::after_card_change(&db, &config, card_id);
    Ok(session)
}

/// The card sessions that are at work right now (workers and reviewers), for the Studio to open their panes on when the
/// page starts — a watcher's event may have come before the page was listening (A2A CP-A6b, CP-A8a).
#[tauri::command]
pub fn open_card_sessions(
    state: State<'_, CoreState>,
) -> Result<Vec<axiomata_core::card_session::OpenCardSession>, String> {
    axiomata_core::card_session::open_card_sessions(&state).map_err(|err| err.to_string())
}

/// The owner takes a reviewed card over (A3, A22): the work lands in the project's main line as one commit, the card is
/// closed and the sessions made for it are cleaned up. A conflict is undone and comes back as the outcome.
#[tauri::command]
pub async fn take_over_card(
    state: State<'_, CoreState>,
    card_id: i64,
    message: Option<String>,
) -> Result<axiomata_core::card_session::CardTakeOver, String> {
    let config = read_config(&state.config);
    let outcome = axiomata_core::card_session::take_over_card(&state, card_id, message.as_deref())
        .await
        .map_err(|err| err.to_string())?;
    let db = state.db_lock();
    board_mirror::after_card_change(&db, &config, card_id);
    Ok(outcome)
}

/// The owner takes a finished plan over (CP-A8b): its line goes into the project's branch, the cards and the plan are
/// closed, the line is removed. A conflict with the branch is undone and comes back as the outcome. Never pushes.
#[tauri::command]
pub async fn take_over_plan(
    state: State<'_, CoreState>,
    id: i64,
) -> Result<axiomata_core::card_session::PlanTakeOver, String> {
    let config = read_config(&state.config);
    let outcome = axiomata_core::card_session::take_over_plan(&state, id)
        .await
        .map_err(|err| err.to_string())?;
    if let axiomata_core::card_session::PlanTakeOver::Done { card_ids, .. } = &outcome {
        let db = state.db_lock();
        for card in card_ids {
            board_mirror::after_card_change(&db, &config, *card);
        }
    }
    Ok(outcome)
}

/// The owner merges a signed-off card into its plan's line by hand — the retry of a card the studio gave up on (CP-A8c).
/// Returns what happened, as the studio's own looks would: the Flow closes the panes of the sessions that are gone.
#[tauri::command]
pub async fn integrate_card(
    state: State<'_, CoreState>,
    card_id: i64,
) -> Result<axiomata_core::card_session::CardIntegration, String> {
    let config = read_config(&state.config);
    let outcome = axiomata_core::card_session::integrate_card_with(&state, card_id, true)
        .await
        .map_err(|err| err.to_string())?;
    let db = state.db_lock();
    board_mirror::after_card_change(&db, &config, card_id);
    Ok(outcome)
}

/// The owner has a card the studio gave up integrating done again (CP-A8c). Returns the sessions that are gone.
#[tauri::command]
pub async fn redo_card(state: State<'_, CoreState>, card_id: i64) -> Result<Vec<i64>, String> {
    axiomata_core::card_session::redo_card(&state, card_id)
        .await
        .map_err(|err| err.to_string())
}

/// The cards of a plan the studio gave up integrating, which wait for the owner (CP-A8c).
#[tauri::command]
pub fn plan_cards_left_for_owner(state: State<'_, CoreState>, id: i64) -> Result<Vec<i64>, String> {
    let db = state.db_lock();
    axiomata_core::card_session::cards_left_for_owner(&db, id).map_err(|err| err.to_string())
}

/// The sharper goal a grilling session proposed for a plan, if one waits for the owner.
#[tauri::command]
pub fn plan_goal_suggestion(
    state: State<'_, CoreState>,
    id: i64,
) -> Result<Option<board::flow::GoalSuggestion>, String> {
    let db = state.db_lock();
    board::flow::goal_suggestion(&db, id).map_err(|err| err.to_string())
}

/// The owner takes the proposed goal over: it becomes the plan's goal and the proposal goes. `at` is the proposal the owner
/// read; one that changed since is refused.
#[tauri::command]
pub fn apply_goal_suggestion(
    state: State<'_, CoreState>,
    id: i64,
    at: String,
) -> Result<Option<board::Plan>, String> {
    let config = read_config(&state.config);
    let mut db = state.db_lock();
    let applied =
        board::flow::apply_goal_suggestion(&mut db, id, &at).map_err(|err| err.to_string())?;
    if let Some(plan) = &applied {
        board_mirror::after_change(&db, &config, plan.board_id);
    }
    Ok(applied)
}

/// The owner discards the proposed goal.
#[tauri::command]
pub fn discard_goal_suggestion(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    let db = state.db_lock();
    board::flow::clear_goal_suggestion(&db, id).map_err(|err| err.to_string())
}

/// The proposals of a plan that a session changed after proposing them, for the planning panel to say so (CP-A9).
#[tauri::command]
pub fn plan_changed_proposals(state: State<'_, CoreState>, id: i64) -> Result<Vec<i64>, String> {
    let db = state.db_lock();
    board::flow::changed_proposals(&db, id).map_err(|err| err.to_string())
}

/// What sessions are doing right now: their last few steps, read from the harness's own record (CP-A9).
#[tauri::command]
pub async fn session_activity(
    state: State<'_, CoreState>,
    agent_ids: Vec<i64>,
) -> Result<Vec<axiomata_core::session_activity::SessionActivity>, String> {
    Ok(axiomata_core::session_activity::read(&state, &agent_ids).await)
}

/// What a plan's sessions spent against its limits, and what the studio spent today against the day's cap (CP-A8c).
#[tauri::command]
pub fn plan_spend(
    state: State<'_, CoreState>,
    id: i64,
) -> Result<axiomata_core::studio_spend::PlanSpend, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    axiomata_core::studio_spend::plan_spend(&db, &config, id).map_err(|err| err.to_string())
}

/// The owner's "go on" for a plan that reached its limit: a fresh allowance on top of what it spent (CP-A8c).
#[tauri::command]
pub fn resume_plan(state: State<'_, CoreState>, id: i64) -> Result<board::Plan, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    axiomata_core::studio_spend::resume_plan(&db, &config, id).map_err(|err| err.to_string())
}

/// The latest `limit` lines of a card's history, oldest first.
#[tauri::command]
pub fn list_card_events(
    state: State<'_, CoreState>,
    card_id: i64,
    limit: usize,
) -> Result<Vec<board::CardEvent>, String> {
    let db = state.db_lock();
    board::flow::list_events(&db, card_id, limit).map_err(|err| err.to_string())
}

/// What the sessions of a card have used and what they may use (A9), read fresh from what their harnesses left behind.
#[tauri::command]
pub async fn card_usage(
    state: State<'_, CoreState>,
    card_id: i64,
) -> Result<Vec<axiomata_core::session_limits::SessionUsage>, String> {
    axiomata_core::session_limits::card_usage(&state, card_id)
        .await
        .map_err(|err| err.to_string())
}

/// The owner writes a line into a card's history.
#[tauri::command]
pub fn add_card_note(
    state: State<'_, CoreState>,
    card_id: i64,
    text: String,
) -> Result<bool, String> {
    let db = state.db_lock();
    let added = board::flow::add_event(&db, card_id, OWNER_ACTOR, board::EventKind::Note, &text)
        .map_err(|err| err.to_string())?;
    Ok(added.is_some())
}

/// The owner's yes to one proposal. `false` if the card is not in a proposal column.
#[tauri::command]
pub fn approve_card_proposal(state: State<'_, CoreState>, card_id: i64) -> Result<bool, String> {
    let config = read_config(&state.config);
    let mut db = state.db_lock();
    let approved = board::flow::approve_proposal(&mut db, card_id, OWNER_ACTOR)
        .map_err(|err| err.to_string())?;
    if approved {
        board_mirror::after_card_change(&db, &config, card_id);
    }
    Ok(approved)
}

/// Calls a card off (`cancel`), marks it failed, or undoes either (`reopen`).
#[tauri::command]
pub fn mark_card(
    state: State<'_, CoreState>,
    card_id: i64,
    mark: String,
    reason: Option<String>,
) -> Result<bool, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    let reason = reason.unwrap_or_default();
    let done = match mark.as_str() {
        "cancel" => board::flow::cancel_card(&db, card_id, OWNER_ACTOR, &reason),
        "fail" => board::flow::mark_failed(&db, card_id, OWNER_ACTOR, &reason),
        "reopen" => board::flow::reopen_card(&db, card_id, OWNER_ACTOR),
        other => {
            return Err(format!(
                "unknown mark {other:?} — expected cancel, fail or reopen"
            ));
        }
    }
    .map_err(|err| err.to_string())?;
    if done {
        board_mirror::after_card_change(&db, &config, card_id);
    }
    Ok(done)
}

#[tauri::command]
pub fn create_board_column(
    state: State<'_, CoreState>,
    board_id: i64,
    new: board::NewColumn,
) -> Result<board::Column, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    let created =
        board::store::create_column(&db, board_id, &new).map_err(|err| err.to_string())?;
    board_mirror::after_change(&db, &config, board_id);
    Ok(created)
}

/// Renames a column and/or re-points it at another status. Re-pointing away
/// from `done` withdraws the sign-off of every card in it — see the store.
#[tauri::command]
pub fn update_board_column(
    state: State<'_, CoreState>,
    id: i64,
    name: String,
    maps_to_status: board::CardStatus,
) -> Result<Option<board::Column>, String> {
    let config = read_config(&state.config);
    let mut db = state.db_lock();
    let updated = board::store::update_column(&mut db, id, &name, maps_to_status)
        .map_err(|err| err.to_string())?;
    if let Some(column) = &updated {
        board_mirror::after_change(&db, &config, column.board_id);
    }
    Ok(updated)
}

/// Returns `false` if the column still holds cards and no destination was
/// given — the caller is expected to ask where they should go.
#[tauri::command]
pub fn delete_board_column(
    state: State<'_, CoreState>,
    id: i64,
    move_cards_to: Option<i64>,
) -> Result<bool, String> {
    let config = read_config(&state.config);
    let mut db = state.db_lock();
    // Same as for a card: resolve the board before the column stops existing.
    let board_id = board::store::get_column(&db, id)
        .map_err(|err| err.to_string())?
        .map(|column| column.board_id);
    let gone =
        board::store::delete_column(&mut db, id, move_cards_to).map_err(|err| err.to_string())?;
    if let Some(board_id) = board_id.filter(|_| gone) {
        board_mirror::after_change(&db, &config, board_id);
    }
    Ok(gone)
}

/// `index` counts the columns the moved one will sit among, excluding itself.
#[tauri::command]
pub fn move_board_column(
    state: State<'_, CoreState>,
    id: i64,
    index: usize,
) -> Result<bool, String> {
    let config = read_config(&state.config);
    let mut db = state.db_lock();
    let moved = board::store::move_column(&mut db, id, index).map_err(|err| err.to_string())?;
    if moved {
        board_mirror::after_column_change(&db, &config, id);
    }
    Ok(moved)
}

// ------------------------------------------------------------------- ide ---
//
// Thin passthroughs to `axiomata_ide::store`, the same shape as the board
// commands above: take the state, lock, delegate, stringify. Two rules the
// store enforces and the frontend depends on: `repo_root` is canonicalised and
// UNIQUE (a clash names the project already sitting there), and removing a
// project removes a row and never a folder.
//
// Every mutating command takes an id plus the one field it changes, never a
// whole `Project` — see the `root_exists` warning on the model for why that
// field must only ever come from the store's own `is_dir`.

#[tauri::command]
pub fn list_ide_projects(state: State<'_, CoreState>) -> Result<Vec<ide::Project>, String> {
    let db = state.db_lock();
    ide::store::list_projects(&db).map_err(|err| err.to_string())
}

/// Returns `None` if there is no such project.
#[tauri::command]
pub fn rename_ide_project(
    state: State<'_, CoreState>,
    id: i64,
    name: String,
) -> Result<Option<ide::Project>, String> {
    let db = state.db_lock();
    ide::store::rename_project(&db, id, &name).map_err(|err| err.to_string())
}

/// Stores the frontend's serialised dock tree. `None` clears it, which is what
/// makes the IDE build its starting layout next time.
#[tauri::command]
pub fn set_ide_project_layout(
    state: State<'_, CoreState>,
    id: i64,
    layout: Option<String>,
) -> Result<bool, String> {
    let db = state.db_lock();
    ide::store::set_layout(&db, id, layout.as_deref()).map_err(|err| err.to_string())
}

/// Marks the project as just opened and returns it — what the project list
/// sorts by. Returns `None` if there is no such project.
#[tauri::command]
pub fn open_ide_project(
    state: State<'_, CoreState>,
    id: i64,
) -> Result<Option<ide::Project>, String> {
    let db = state.db_lock();
    ide::store::touch_opened(&db, id).map_err(|err| err.to_string())?;
    ide::store::get_project(&db, id).map_err(|err| err.to_string())
}

/// Removes the project **row**. Never the folder — the UI says "remove from
/// the list" for that reason.
#[tauri::command]
pub fn delete_ide_project(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    let db = state.db_lock();
    ide::store::delete_project(&db, id).map_err(|err| err.to_string())
}

// Agents (M7.2 CP4). Profiles only: starting one is a PTY session owned by the
// pane showing it, which is `terminal_spawn`'s business, not this layer's.

#[tauri::command]
pub fn list_ide_agents(
    state: State<'_, CoreState>,
    project_id: i64,
) -> Result<Vec<ide::Agent>, String> {
    let db = state.db_lock();
    ide::agent_store::list_agents(&db, project_id).map_err(|err| err.to_string())
}

/// What the Agents panel sends: a name, and the engine and role to run on. The harness, command, model and
/// environment are the engine's — an agent is *made on* an engine, never described from scratch (engines exist only
/// in the settings).
#[derive(serde::Deserialize)]
pub struct AgentSpec {
    name: String,
    engine_id: String,
    role: String,
}

#[tauri::command]
pub fn create_ide_agent_on_engine(
    state: State<'_, CoreState>,
    project_id: i64,
    spec: AgentSpec,
) -> Result<ide::Agent, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    let roles = axiomata_core::roster::roles_for_project(&db, &config, project_id);
    axiomata_core::roster::create_agent_on_engine(
        &db,
        &config,
        &roles,
        project_id,
        &spec.name,
        &spec.engine_id,
        &spec.role,
    )
    .map_err(|err| err.to_string())
}

/// Renames an agent and moves it to another engine and role. `None` if there is no such agent.
#[tauri::command]
pub fn update_ide_agent_on_engine(
    state: State<'_, CoreState>,
    id: i64,
    spec: AgentSpec,
) -> Result<Option<ide::Agent>, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    let Some(agent) = ide::agent_store::get_agent(&db, id).map_err(|err| err.to_string())? else {
        return Ok(None);
    };
    let roles = axiomata_core::roster::roles_for_project(&db, &config, agent.project_id);
    axiomata_core::roster::update_agent_on_engine(
        &db,
        &config,
        &roles,
        id,
        &spec.name,
        &spec.engine_id,
        &spec.role,
    )
    .map_err(|err| err.to_string())
}

// ---- Engines and roles (a2a.md, CP-A1) ----
//
// The webview never names a path: a project's role files are found through the project's id, and a role is
// named by its slug, which the roster refuses to turn into anything but a directory name.

#[tauri::command]
pub fn list_engines(
    state: State<'_, CoreState>,
) -> Result<Vec<axiomata_core::roster::EngineEntry>, String> {
    let config = read_config(&state.config);
    let db = state.db_lock();
    axiomata_core::roster::engine_overview(&db, &config).map_err(|err| err.to_string())
}

/// Adds or replaces an engine (the whole object; the id is the key). Only the engines of the live config change.
#[tauri::command]
pub fn save_engine(
    state: State<'_, CoreState>,
    engine: axiomata_roster::Engine,
) -> Result<(), String> {
    let mut config = read_config(&state.config);
    axiomata_core::roster::save_engine(&mut config, engine).map_err(|err| err.to_string())?;
    state
        .config
        .write()
        .unwrap_or_else(|poison| poison.into_inner())
        .agents
        .engines = config.agents.engines;
    Ok(())
}

/// Removes an engine unless an agent session or a role still uses it.
#[tauri::command]
pub fn delete_engine(state: State<'_, CoreState>, id: String) -> Result<bool, String> {
    let mut config = read_config(&state.config);
    let removed = {
        let db = state.db_lock();
        axiomata_core::roster::delete_engine(&db, &mut config, &id)
            .map_err(|err| err.to_string())?
    };
    if removed {
        state
            .config
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .agents
            .engines = config.agents.engines;
    }
    Ok(removed)
}

#[tauri::command]
pub fn list_roles() -> Result<axiomata_roster::Loaded, String> {
    axiomata_core::roster::list_roles().map_err(|err| err.to_string())
}

/// Saves one of the owner's roles; the engines it names must exist.
#[tauri::command]
pub fn save_role(state: State<'_, CoreState>, role: axiomata_roster::Role) -> Result<(), String> {
    axiomata_core::roster::save_role(&read_config(&state.config), role)
        .map_err(|err| err.to_string())
}

/// Deletes one of the owner's roles unless an agent session plays it.
#[tauri::command]
pub fn delete_role(state: State<'_, CoreState>, name: String) -> Result<bool, String> {
    let db = state.db_lock();
    axiomata_core::roster::delete_role(&db, &name).map_err(|err| err.to_string())
}

fn project_folder(state: &CoreState, project_id: i64) -> Result<PathBuf, String> {
    let db = state.db_lock();
    match ide::store::get_project(&db, project_id).map_err(|err| err.to_string())? {
        Some(project) => Ok(project.repo_root),
        None => Err(format!("no project {project_id}")),
    }
}

/// The roles in force for a project, and what the project itself brings (and whether that is confirmed).
#[tauri::command]
pub fn project_roles(
    state: State<'_, CoreState>,
    project_id: i64,
) -> Result<axiomata_core::roster::ProjectRoles, String> {
    let folder = project_folder(&state, project_id)?;
    axiomata_core::roster::project_roles(&folder, &read_config(&state.config))
        .map_err(|err| err.to_string())
}

/// The owner confirmed the project's role files as shown; accepted only if they still have that content.
#[tauri::command]
pub fn confirm_project_roles(
    state: State<'_, CoreState>,
    project_id: i64,
    hash: String,
) -> Result<(), String> {
    let folder = project_folder(&state, project_id)?;
    axiomata_core::roster::confirm_project_roles(&folder, &hash).map_err(|err| err.to_string())
}

/// Deletes the profile, then its status channel (M7.2 CP6). The row goes
/// first: a channel without a row is harmless, a row whose plan vanished is not.
#[tauri::command]
pub async fn delete_ide_agent(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    // The connection is let go of before anything is awaited (a guard held across an await makes the command
    // unsendable). The worktree is read first: the row is gone afterwards, and the MCP registration is keyed on it.
    let (deleted, opencode_worktree) = {
        let db = state.db_lock();
        let opencode_worktree = ide::agent_store::get_agent(&db, id)
            .map_err(|err| err.to_string())?
            .filter(|agent| agent.harness == ide::model::Harness::Opencode)
            .and_then(|agent| agent.worktree_path);
        let deleted = ide::agent_store::delete_agent(&db, id).map_err(|err| err.to_string())?;
        (deleted, opencode_worktree)
    };
    if deleted {
        if let Some(worktree) = opencode_worktree {
            axiomata_core::agents::opencode::forget_mcp(&worktree).await;
        }
        ide::provision::forget_channel(&axiomata_core::paths::ide_locations().channels, id)
            .map_err(|err| {
                format!("the agent was removed, but its status folder was not: {err}")
            })?;
    }
    Ok(deleted)
}

/// Gives an agent what it needs to run — its own git worktree, a reserved
/// port (M7.2 CP5), a fresh status channel (CP6) and, for Opencode, its
/// session on the shared service (opencode2.md OC2) — and says where and with
/// which command line the harness should start.
///
/// Idempotent, and called on every start rather than only on creation: an
/// agent created before worktrees existed, or one whose directory somebody
/// deleted, is repaired by being started.
#[tauri::command]
pub async fn prepare_ide_agent(
    state: State<'_, CoreState>,
    id: i64,
) -> Result<axiomata_core::ide_start::Started, String> {
    axiomata_core::ide_start::start_agent(&state, id)
        .await
        .map_err(|err| err.to_string())
}

/// Forgets an Opencode agent's session so its next start opens a fresh one
/// ("New session", opencode2.md Q9). The old session stays in Opencode.
#[tauri::command]
pub fn ide_agent_new_session(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    axiomata_core::ide_start::new_session(&state.db, id).map_err(|err| err.to_string())
}

/// The "you have mail" line to type into session `id`'s terminal, or `None` when there is nothing to announce
/// (A2A A8, way 2). `agent_state` is the status the pane already shows; only an idle session qualifies, because typing
/// into a waiting one would answer a permission prompt. Whether the owner is typing in that pane is for the pane to
/// check before it types, and it calls [`ide_mailbox_nudged`] for each line it really typed.
#[tauri::command]
pub fn ide_mailbox_nudge(
    state: State<'_, CoreState>,
    id: i64,
    agent_state: String,
) -> Result<Option<String>, String> {
    let Some(agent_state) = ide::lifecycle::AgentState::parse(&agent_state) else {
        return Ok(None);
    };
    let db = state.db_lock();
    let due = ide::mailbox::nudges(&db, &ide::mailbox::Limits::default(), &[(id, agent_state)])
        .map_err(|err| err.to_string())?;
    Ok(due.into_iter().next().map(|nudge| nudge.line))
}

/// A session's conversation, oldest first: what reached it and what it wrote. Nothing is marked read (CP-A9).
#[tauri::command]
pub fn ide_mailbox_messages(
    state: State<'_, CoreState>,
    id: i64,
) -> Result<Vec<axiomata_core::session_mail::MailLine>, String> {
    let db = state.db_lock();
    axiomata_core::session_mail::conversation(&db, id).map_err(|err| err.to_string())
}

/// How many messages wait unread in each of the sessions' inboxes (CP-A9).
#[tauri::command]
pub fn ide_mailbox_unread(
    state: State<'_, CoreState>,
    ids: Vec<i64>,
) -> Result<std::collections::HashMap<i64, usize>, String> {
    let db = state.db_lock();
    ids.into_iter()
        .map(|id| {
            axiomata_core::session_mail::unread(&db, id)
                .map(|count| (id, count))
                .map_err(|err| err.to_string())
        })
        .collect()
}

/// The owner writes to a session, about its card (CP-A9).
#[tauri::command]
pub fn ide_mailbox_send(
    state: State<'_, CoreState>,
    id: i64,
    message: String,
) -> Result<axiomata_core::session_mail::OwnerSend, String> {
    axiomata_core::session_mail::send_as_owner(&state, id, &message).map_err(|err| err.to_string())
}

/// Notes that the nudge for session `id` was typed, so a message is announced a limited number of times.
#[tauri::command]
pub fn ide_mailbox_nudged(state: State<'_, CoreState>, id: i64) -> Result<(), String> {
    let db = state.db_lock();
    ide::mailbox::record_nudge(&db, id).map_err(|err| err.to_string())
}

/// What every agent of a project is doing and planning (M7.2 CP6/CP6b).
///
/// One call for the whole project, because the IDE polls it every second
/// while it is open. Reads small files only.
///
/// Claude Code agents report through their file channel; Opencode agents
/// through the Opencode service, whose watcher starts on the first call
/// (opencode2.md OC3). Async so that watcher starts inside the runtime.
#[tauri::command]
pub async fn ide_agent_states(
    state: State<'_, CoreState>,
    project_id: i64,
) -> Result<Vec<ide::lifecycle::AgentStatus>, String> {
    axiomata_core::ide_status::ensure_running(std::sync::Arc::clone(&state.db));
    // List, then let go of the connection before the file reads: this runs
    // every second, and the connection is shared by every other command.
    let agents = {
        let db = state.db_lock();
        ide::agent_store::list_agents(&db, project_id).map_err(|err| err.to_string())?
    };
    let mut statuses =
        ide::provision::agent_statuses(&agents, &axiomata_core::paths::ide_locations().channels);
    axiomata_core::ide_status::overlay(&mut statuses, &agents);
    Ok(statuses)
}

/* ------------------------------------------------------------ git (M7.3) ---
 * Every git command below reads where the agent works from the database,
 * lets go of the connection, and only then runs git — on a blocking thread,
 * because a sync Tauri command runs on the main thread and `git diff` on a
 * large repository is not instant. The Diffs tab polls while an agent works,
 * so neither the UI nor the one shared connection may wait on it. */

/// The agent's repository, looked up under a brief database lock.
fn agent_repo_of(
    state: &State<'_, CoreState>,
    id: i64,
) -> Result<ide::provision::AgentRepoState, String> {
    let db = state.db_lock();
    ide::provision::agent_repo(&db, id).map_err(|err| err.to_string())
}

/// The agent's repository, or the sentence saying why it has none.
fn ready_repo_of(state: &State<'_, CoreState>, id: i64) -> Result<ide::git::AgentRepo, String> {
    agent_repo_of(state, id)?
        .ready()
        .map_err(|err| err.to_string())
}

/// Runs a git operation off the main thread.
async fn off_main<T: Send + 'static>(
    work: impl FnOnce() -> ide::Result<T> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|err| format!("git task failed: {err}"))?
        .map_err(|err| err.to_string())
}

/// What the Diffs tab shows: the changes, or why there are none of this
/// agent's own — a shared folder never has any, an agent not started yet
/// will after its first start.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AgentDiffState {
    /// A worktree exists; `changes` is everything the agent has changed (G2).
    Ready { changes: ide::git::AgentChanges },
    /// The project folder is not a git repository; its agents share it.
    SharedFolder,
    /// A repository, but this agent has no worktree yet — start it once.
    NotStarted,
}

/// What an agent has changed since its base (M7.3, G2), or why it has no
/// diff of its own.
#[tauri::command]
pub async fn ide_agent_changes(
    state: State<'_, CoreState>,
    id: i64,
) -> Result<AgentDiffState, String> {
    match agent_repo_of(&state, id)? {
        ide::provision::AgentRepoState::Ready(repo) => off_main(move || repo.changes())
            .await
            .map(|changes| AgentDiffState::Ready { changes }),
        ide::provision::AgentRepoState::SharedFolder => Ok(AgentDiffState::SharedFolder),
        ide::provision::AgentRepoState::NotStarted => Ok(AgentDiffState::NotStarted),
    }
}

/// One file's diff against the agent's base, parsed.
#[tauri::command]
pub async fn ide_agent_file_diff(
    state: State<'_, CoreState>,
    id: i64,
    path: String,
    old_path: Option<String>,
) -> Result<ide::git::FileDiff, String> {
    let repo = ready_repo_of(&state, id)?;
    off_main(move || repo.file_diff(&path, old_path.as_deref())).await
}

/// One file as the agent's base has it — the left side of the diff (M7.3 H2,
/// H8). Text within the diff limit comes back as text, an image as base64 like
/// `file_read_image`, anything else only as its size.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BaseFile {
    /// The base has no such file: the agent added it.
    Absent,
    /// UTF-8 content without a NUL byte, as `axiomata_files::text_from_bytes` decides it.
    Text { text: String },
    /// One of `axiomata_files::image_mime`'s known extensions.
    Image { mime: &'static str, base64: String },
    /// Not text by `axiomata_files::text_from_bytes`, and not an image.
    Binary { size: u64 },
    /// Over the diff/image byte limit passed to `AgentRepo::base_blob`; not read.
    TooLarge { size: u64 },
}

impl BaseFile {
    /// Classifies a base-side blob the same way `file_read`/`file_read_image` would: an image
    /// extension first, then text, otherwise opaque binary.
    fn from_blob(path: &str, blob: ide::git::BaseBlob) -> Self {
        let bytes = match blob {
            ide::git::BaseBlob::Absent => return Self::Absent,
            ide::git::BaseBlob::TooLarge { size } => return Self::TooLarge { size },
            ide::git::BaseBlob::Bytes(bytes) => bytes,
        };
        if let Some(image) = axiomata_files::image_from_bytes(path, &bytes) {
            return Self::Image {
                mime: image.mime,
                base64: image.base64,
            };
        }
        let size = bytes.len() as u64;
        // The same rule the worktree side's `file_read` applies (architecture review, CP8).
        match axiomata_files::text_from_bytes(bytes) {
            Some(text) => Self::Text { text },
            None => Self::Binary { size },
        }
    }
}

/// `path` as the agent's base has it (H2). `path` is the base-side name — a
/// rename's old path.
#[tauri::command]
pub async fn ide_agent_base_file(
    state: State<'_, CoreState>,
    id: i64,
    path: String,
) -> Result<BaseFile, String> {
    let repo = ready_repo_of(&state, id)?;
    let limit = if axiomata_files::image_mime(&path).is_some() {
        axiomata_files::MAX_IMAGE_BYTES
    } else {
        ide::git::MAX_DIFF_BYTES as u64
    };
    off_main(move || {
        repo.base_blob(&path, limit)
            .map(|blob| BaseFile::from_blob(&path, blob))
    })
    .await
}

/// The Mac pasteboard's text, for the editor's Vi mode (`p`, ED3 V3). Runs
/// `pbpaste` off the main thread.
#[tauri::command]
pub async fn clipboard_read() -> Result<String, String> {
    tokio::task::spawn_blocking(axiomata_macos::clipboard::read_text)
        .await
        .map_err(|err| format!("clipboard task failed: {err}"))?
        .map_err(|err| err.to_string())
}

/// Puts `text` on the Mac pasteboard (Vi's `y`, ED3 V3).
#[tauri::command]
pub async fn clipboard_write(text: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || axiomata_macos::clipboard::write_text(&text))
        .await
        .map_err(|err| format!("clipboard task failed: {err}"))?
        .map_err(|err| err.to_string())
}

/// One installed font family, as the font pickers see it (ED5, T10).
#[derive(Debug, Clone, Serialize)]
pub struct InstalledFont {
    family: String,
    /// CSS weights of its upright faces, ascending.
    weights: Vec<u16>,
    monospace: bool,
}

/// The installed families, looked up once per run: CoreText takes a moment
/// over a Mac's thousand-odd faces. A font installed later shows after a restart.
static INSTALLED_FONTS: std::sync::OnceLock<Vec<InstalledFont>> = std::sync::OnceLock::new();

/// Every font family installed on the Mac, with its real weights and whether
/// it is monospaced (ED5, T10, T16) — for the editor's and the terminal's
/// font pickers.
#[tauri::command]
pub async fn installed_fonts() -> Result<Vec<InstalledFont>, String> {
    tokio::task::spawn_blocking(|| {
        INSTALLED_FONTS
            .get_or_init(|| {
                axiomata_macos::fonts::installed_families()
                    .into_iter()
                    .map(|f| InstalledFont {
                        family: f.family,
                        weights: f.weights,
                        monospace: f.monospace,
                    })
                    .collect()
            })
            .clone()
    })
    .await
    .map_err(|err| format!("font lookup failed: {err}"))
}

/// One display as the UI scale sees it (`docs/plans/editor-look.md`, LK0, K9):
/// where it is, in points of the global display space (the webview's
/// `screenX`/`screenY`), and the UI scale its real density asks for.
#[derive(Debug, Clone, Serialize)]
pub struct UiDisplay {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
}

/// The Mac's displays with their UI scale (K9); the page picks the one its
/// window is on. Asked again whenever the window moves to another display.
#[tauri::command]
pub async fn ui_displays() -> Result<Vec<UiDisplay>, String> {
    tokio::task::spawn_blocking(|| {
        axiomata_macos::display::displays()
            .into_iter()
            .map(|d| UiDisplay {
                scale: d.ui_scale(),
                x: d.x,
                y: d.y,
                width: d.width,
                height: d.height,
            })
            .collect()
    })
    .await
    .map_err(|err| format!("display lookup failed: {err}"))
}

/// Puts files back to the agent's base (G13). The UI asks first.
#[tauri::command]
pub async fn ide_agent_discard(
    state: State<'_, CoreState>,
    id: i64,
    paths: Vec<String>,
) -> Result<(), String> {
    let repo = ready_repo_of(&state, id)?;
    off_main(move || repo.discard(&paths)).await
}

/// Puts one hunk back to the agent's base (H6) — the `index`-th of the file's
/// current diff, refused unless it still reads `header`. The UI asks first.
#[tauri::command]
pub async fn ide_agent_discard_hunk(
    state: State<'_, CoreState>,
    id: i64,
    path: String,
    old_path: Option<String>,
    index: usize,
    header: String,
) -> Result<(), String> {
    let repo = ready_repo_of(&state, id)?;
    off_main(move || repo.discard_hunk(&path, old_path.as_deref(), index, &header)).await
}

/// The subject of the agent's latest own commit, for the take-over message (H10).
#[tauri::command]
pub async fn ide_agent_last_subject(
    state: State<'_, CoreState>,
    id: i64,
) -> Result<Option<String>, String> {
    let repo = ready_repo_of(&state, id)?;
    off_main(move || repo.last_subject()).await
}

/// Commits everything the agent left uncommitted (G3); returns the commit.
#[tauri::command]
pub async fn ide_agent_commit(
    state: State<'_, CoreState>,
    id: i64,
    message: String,
) -> Result<String, String> {
    let repo = ready_repo_of(&state, id)?;
    off_main(move || repo.commit_all(&message)).await
}

/// Takes the agent's committed work over into the project folder (G7–G12).
/// `TakeOverTarget::run` refuses while the agent is working or waiting (G12).
#[tauri::command]
pub async fn ide_agent_take_over(
    state: State<'_, CoreState>,
    id: i64,
    mode: ide::git::TakeOverMode,
    message: String,
) -> Result<ide::git::TakeOver, String> {
    let target = {
        let db = state.db_lock();
        ide::provision::TakeOverTarget::read(&db, id).map_err(|err| err.to_string())?
    };
    let roots = axiomata_core::paths::ide_locations().channels;
    off_main(move || target.run(&roots, mode, &message)).await
}

/// Whether an agent's worktree holds work that removing it would throw away.
#[tauri::command]
pub async fn ide_agent_has_changes(state: State<'_, CoreState>, id: i64) -> Result<bool, String> {
    // The path under a brief lock, `git status` without it.
    let path = {
        let db = state.db_lock();
        ide::agent_store::get_agent(&db, id)
            .map_err(|err| err.to_string())?
            .and_then(|agent| agent.worktree_path)
    };
    match path {
        Some(path) if path.is_dir() => {
            off_main(move || ide::worktree::has_uncommitted_changes(&path)).await
        }
        _ => Ok(false),
    }
}

/// Removes an agent's worktree. `force` throws away uncommitted work in it,
/// which is why the caller has to ask first. Read, remove without the
/// connection, record — see `WorktreeToDiscard`.
#[tauri::command]
pub async fn discard_ide_agent_worktree(
    state: State<'_, CoreState>,
    id: i64,
    force: bool,
) -> Result<bool, String> {
    let target = {
        let db = state.db_lock();
        ide::provision::WorktreeToDiscard::read(&db, id).map_err(|err| err.to_string())?
    };
    let Some(target) = target else {
        return Ok(false);
    };
    let (target, removed) = off_main(move || {
        let removed = target.remove(force)?;
        Ok((target, removed))
    })
    .await?;
    if removed {
        axiomata_core::agents::opencode::forget_mcp(target.path()).await;
    }
    let db = state.db_lock();
    target.forget(&db, removed).map_err(|err| err.to_string())?;
    Ok(removed)
}
