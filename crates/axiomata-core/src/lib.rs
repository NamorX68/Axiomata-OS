//! Axiomata-OS core engine.
//!
//! This crate contains the platform-independent "OS" logic: agent backends,
//! skill execution, memory router maintenance, and routine scheduling. It has
//! no dependency on Tauri or macOS-specific APIs, so it can in principle be
//! embedded in a headless binary on another platform later — see the
//! `axiomata-macos` crate for the platform integration boundary.

pub mod agents;
pub mod config;
pub mod db;
pub mod error;
pub mod memory;
pub mod paths;
pub mod routines;
pub mod skills;

pub use error::AxiomataError;

use std::fs;

use config::Config;

/// The initialized Axiomata-OS core engine: loaded config and an open,
/// migrated database connection.
///
/// Construct via [`AxiomataCore::init`], which is idempotent and safe to call
/// on every app start.
pub struct AxiomataCore {
    pub config: Config,
    pub db: rusqlite::Connection,
}

impl AxiomataCore {
    /// Initializes the Axiomata-OS core engine for the current user:
    ///
    /// - loads `~/.axiomata/config.toml`, writing the default config to disk
    ///   on first run so the file exists for the user to inspect/edit,
    /// - creates `~/.axiomata/logs/`, `~/.axiomata/skills/`, and the
    ///   Second-Brain workspace root if any of them don't exist yet,
    /// - opens `~/.axiomata/axiomata.db` and applies pending migrations.
    ///
    /// Safe to call on every app start: every step here is idempotent.
    pub fn init() -> Result<Self, AxiomataError> {
        let config_existed = paths::config_path().exists();
        let config = Config::load()?;
        if !config_existed {
            config.save()?;
        }

        for dir in [
            paths::logs_dir(),
            paths::global_skills_dir(),
            config.workspace_root.clone(),
        ] {
            fs::create_dir_all(&dir).map_err(|source| AxiomataError::Io { path: dir, source })?;
        }

        let db = db::open_and_migrate()?;

        Ok(Self { config, db })
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Shared helpers for tests that need an isolated scratch filesystem
    //! location. Kept in one place so every module's tests behave
    //! consistently instead of re-deriving this logic per module.
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Serializes tests that mutate process-wide environment variables (e.g.
    /// `AXIOMATA_HOME`), since `std::env::set_var` affects the whole process
    /// and `cargo test` runs tests in parallel by default.
    pub static ENV_MUTEX: Mutex<()> = Mutex::new(());

    /// Returns a fresh, unique path under the OS temp directory, suitable as
    /// an isolated scratch home/workspace for a single test run. Callers are
    /// responsible for cleaning it up (`fs::remove_dir_all`) when done.
    pub fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{}-{}", std::process::id(), nanos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentDefaults;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;

    #[test]
    fn init_creates_expected_layout_without_touching_the_real_home() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let temp_home = unique_temp_dir("axiomata-test-init-home");
        let temp_workspace = unique_temp_dir("axiomata-test-init-workspace");

        // SAFETY: serialized by `_guard`, see `paths::tests` for the same
        // reasoning.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &temp_home);
        }

        // Pre-seed a config pointing the workspace root at a scratch dir, so
        // this test never creates `~/Axiomata-Workspace` in the developer's
        // real home directory.
        fs::create_dir_all(&temp_home).unwrap();
        let seed_config = Config {
            workspace_root: temp_workspace.clone(),
            agents: AgentDefaults::default(),
        };
        seed_config.save().expect("seeding config should succeed");

        let core = AxiomataCore::init().expect("init should succeed");

        assert!(paths::config_path().exists());
        assert!(paths::logs_dir().is_dir());
        assert!(paths::global_skills_dir().is_dir());
        assert_eq!(core.config.workspace_root, temp_workspace);
        assert!(temp_workspace.is_dir());
        assert!(paths::db_path().exists());

        // Calling init() again must not fail (idempotent).
        drop(core);
        AxiomataCore::init().expect("second init should also succeed");

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&temp_home);
        let _ = fs::remove_dir_all(&temp_workspace);
    }
}
