//! Resolution of Axiomata-OS's own runtime data directory.
//!
//! Axiomata-OS keeps its own data (config, database, logs, global skills) under
//! `~/.axiomata`, deliberately separate from the user's chosen Second-Brain
//! workspace. See `crate::config` for the workspace root setting.

use std::env;
use std::path::PathBuf;

/// Name of the environment variable that overrides the default `~/.axiomata`
/// location. Used by tests, and by anyone who wants to run an isolated
/// instance side by side with their normal one.
pub const AXIOMATA_HOME_ENV: &str = "AXIOMATA_HOME";

/// Returns Axiomata-OS's own runtime data directory.
///
/// Resolves to `$AXIOMATA_HOME` if that environment variable is set, otherwise
/// to `~/.axiomata`. Deliberately a visible dotfolder in the user's home
/// directory (mirroring `~/.claude`) rather than a hidden OS-convention path
/// such as `~/Library/Application Support/...`, since Axiomata-OS is meant to
/// be inspected by hand.
///
/// # Panics
///
/// Panics if `$AXIOMATA_HOME` is unset and the OS cannot report a home
/// directory for the current user — an environment Axiomata-OS cannot
/// meaningfully run in.
pub fn axiomata_home() -> PathBuf {
    if let Some(override_path) = env::var_os(AXIOMATA_HOME_ENV) {
        return PathBuf::from(override_path);
    }
    home::home_dir()
        .expect("could not determine the current user's home directory")
        .join(".axiomata")
}

/// Path to the app-level config file (`~/.axiomata/config.toml`).
pub fn config_path() -> PathBuf {
    axiomata_home().join("config.toml")
}

/// Path to the SQLite database (`~/.axiomata/axiomata.db`).
pub fn db_path() -> PathBuf {
    axiomata_home().join("axiomata.db")
}

/// Directory for JSONL run/routine logs (`~/.axiomata/logs/`).
pub fn logs_dir() -> PathBuf {
    axiomata_home().join("logs")
}

/// Path to the JSONL skill-run log (`~/.axiomata/logs/runs.log`), a
/// human-tailable mirror of the `runs` database table.
pub fn runs_log_path() -> PathBuf {
    logs_dir().join("runs.log")
}

/// Path to the memory-router sync marker (`~/.axiomata/memory-last-sync.json`),
/// a small `{ "<canonical workspace path>": "<rfc3339>" }` map recording when
/// each workspace was last synced. Kept in app-data, not in the workspace, so
/// the router never adds a file to the user's vault for its own bookkeeping.
pub fn memory_last_sync_path() -> PathBuf {
    axiomata_home().join("memory-last-sync.json")
}

/// Directory for global, app-managed skills (`~/.axiomata/skills/`).
pub fn global_skills_dir() -> PathBuf {
    axiomata_home().join("skills")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axiomata_home_respects_override_and_derives_paths() {
        let _guard = crate::test_support::ENV_MUTEX.lock().unwrap();

        // SAFETY: `env::set_var`/`remove_var` are `unsafe` as of edition 2024
        // because mutating process-wide env vars is not thread-safe in
        // general. `_guard` above serializes every test in this crate that
        // touches `AXIOMATA_HOME`, so there is no actual race here.
        unsafe {
            env::set_var(AXIOMATA_HOME_ENV, "/tmp/axiomata-test-home");
        }

        let expected_home = PathBuf::from("/tmp/axiomata-test-home");
        assert_eq!(axiomata_home(), expected_home);
        assert_eq!(config_path(), expected_home.join("config.toml"));
        assert_eq!(db_path(), expected_home.join("axiomata.db"));
        assert_eq!(logs_dir(), expected_home.join("logs"));
        assert_eq!(runs_log_path(), expected_home.join("logs").join("runs.log"));
        assert_eq!(global_skills_dir(), expected_home.join("skills"));

        unsafe {
            env::remove_var(AXIOMATA_HOME_ENV);
        }
    }
}
