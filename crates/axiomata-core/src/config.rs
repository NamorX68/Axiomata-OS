//! Loading and saving `~/.axiomata/config.toml`.
//!
//! Holds the user-configurable Second-Brain workspace root and agent backend
//! defaults (e.g. the default Ollama model).

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

/// Defaults for the built-in agent backends.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentDefaults {
    /// Ollama model used when a skill or routine doesn't specify one.
    #[serde(default = "default_ollama_model")]
    pub ollama_model: String,
}

impl Default for AgentDefaults {
    fn default() -> Self {
        Self {
            ollama_model: default_ollama_model(),
        }
    }
}

/// Axiomata-OS's own configuration (`~/.axiomata/config.toml`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
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

        fs::write(&path, raw).map_err(|source| AxiomataError::Io { path, source })
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

        let mut custom = Config::default();
        custom.workspace_root = temp_home.join("MyBrain");
        custom.agents.ollama_model = "llama3.2:latest".to_string();
        custom.save().expect("save should succeed");

        let reloaded = Config::load().expect("load should succeed after save");
        assert_eq!(reloaded, custom);

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&temp_home);
    }
}
