//! Loading and saving `~/.axiomata/config.toml`.
//!
//! Holds the user-configurable Second-Brain workspace root and agent backend
//! defaults (e.g. the default Ollama model).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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

/// Default hard wall-clock limit for a single skill run.
fn default_skill_timeout_secs() -> u64 {
    300
}

/// Defaults for the built-in agent backends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentDefaults {
    /// Ollama model used when a skill or routine doesn't specify one.
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
    /// `config.toml`, so treat a token here as a plaintext secret.
    #[serde(default)]
    pub claude_env: BTreeMap<String, String>,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            ollama_model: default_ollama_model(),
            skill_timeout_secs: default_skill_timeout_secs(),
            claude_env: BTreeMap::new(),
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

        toml::from_str(&raw).map_err(|source| AxiomataError::ConfigParse { path, source })
    }

    /// Writes the config to `~/.axiomata/config.toml`, creating the parent
    /// directory if necessary.
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

        fs::write(&path, raw).map_err(|source| AxiomataError::Io {
            path: path.clone(),
            source,
        })?;

        // May hold a plaintext token in `agents.claude_env`; keep it owner-only
        // on Unix. Best-effort — a permissions failure is not a save failure.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
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
}
