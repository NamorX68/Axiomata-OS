//! Loading and saving the Terminal module's global preferences file
//! (`~/.axiomata/terminal-settings.json`) — Checkpoint 5d of
//! `docs/plans/terminal.md`.
//!
//! Deliberately its own file, not a section of `dashboard.json`'s
//! per-instance `canvas.instances[].config` (Checkpoints 5/5b's original
//! design): the owner found that closing/removing a Terminal tile lost
//! every setting on it, because per-instance config lives and dies with
//! the `CanvasInstance` it's attached to. Moving to one shared file fixes
//! that (nothing to lose — the settings aren't attached to any tile) and
//! also means every Terminal tile in the dashboard now shares one
//! preference set rather than each keeping its own; a future standalone
//! Terminal (outside this dashboard entirely) would have something
//! dashboard-shaped state isn't to read from too.
//!
//! Same schema-agnostic contract as `dashboard.json` (see
//! `crate::dashboard`'s own doc comment): the frontend owns the JSON shape
//! (`terminal-settings.svelte`'s fields), this only enforces "object with
//! numeric `version`" and does the atomic, 0600 write —
//! `crate::json_state` owns that shared machinery, used identically by
//! both files.

use crate::error::AxiomataError;
use crate::json_state::{self, LoadedJsonState};
use crate::paths;

/// Current on-disk schema version written by the frontend.
pub const SETTINGS_VERSION: u64 = 1;

/// What the frontend gets on boot. A type alias (not a distinct struct) so
/// this reads identically to `dashboard::LoadedState` at every call site —
/// both are just "the file's raw JSON, plus whether a corrupt copy got
/// moved aside" from `crate::json_state`.
pub type LoadedTerminalSettings = LoadedJsonState;

/// The state written when no file exists yet — just a bare version, no
/// fields at all: `terminal-settings.json` has no required keys the way
/// `dashboard.json` does (no `canvas`/`settings` shape to default), every
/// field is an optional owner preference the frontend adds once set.
pub fn default_settings_json() -> String {
    format!("{{\"version\":{SETTINGS_VERSION}}}")
}

/// Loads `terminal-settings.json`, or the defaults if it is missing. See
/// `crate::json_state::load`'s own doc comment for the full corrupt-file/
/// symlink/oversized-file contract this delegates to.
pub fn load_settings() -> Result<LoadedTerminalSettings, AxiomataError> {
    json_state::load(&paths::terminal_settings_path(), default_settings_json)
}

/// Validates and atomically writes `json` to `terminal-settings.json`
/// (mode 0600). See `crate::json_state::save`'s own doc comment for the
/// full contract.
pub fn save_settings(json: &str) -> Result<(), AxiomataError> {
    json_state::save(&paths::terminal_settings_path(), json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;
    use std::fs;
    use std::path::Path;

    /// Runs `body` with `AXIOMATA_HOME` pointed at a fresh temp dir — same
    /// convention `dashboard.rs`'s own tests use, so `load_settings`/
    /// `save_settings` (which resolve their path via `paths::axiomata_home`
    /// internally, unlike `json_state`'s own tests which take an explicit
    /// path) are exercised through their real, public call path.
    fn with_temp_home(body: impl FnOnce(&Path)) {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-terminal-settings");
        fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }
        body(&home);
        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn missing_file_yields_the_bare_default_without_a_backup() {
        with_temp_home(|_| {
            let loaded = load_settings().unwrap();
            assert_eq!(loaded.json, default_settings_json());
            assert!(loaded.recovered_backup.is_none());
            save_settings(&loaded.json).expect("default must validate");
        });
    }

    #[test]
    fn save_then_load_round_trips_verbatim_at_the_real_path() {
        with_temp_home(|home| {
            let text = "{\n  \"version\": 1,\n  \"fontSizePx\": 14,\n  \"theme\": \"nord\"\n}\n";
            save_settings(text).unwrap();
            let loaded = load_settings().unwrap();
            assert_eq!(loaded.json, text);
            assert_eq!(
                fs::read_to_string(home.join("terminal-settings.json")).unwrap(),
                text
            );
        });
    }

    #[test]
    fn save_rejects_an_object_with_no_numeric_version() {
        with_temp_home(|_| {
            let err = save_settings("{\"fontSizePx\":14}").unwrap_err();
            assert!(
                matches!(err, AxiomataError::InvalidDashboardState { .. }),
                "{err}"
            );
        });
    }

    // The three tests below duplicate coverage `json_state.rs` already has
    // at the generic-logic level, on purpose: they prove the *wiring* — that
    // `load_settings`/`save_settings` resolve to the real
    // `terminal-settings.json` path under `AXIOMATA_HOME` and hit the
    // corrupt/symlink paths through the public API, not just through
    // `json_state::load`/`save` called directly — mirroring the equivalent
    // trio `dashboard.rs`'s own tests keep for `dashboard.json`.

    #[test]
    fn corrupt_file_is_moved_to_bak_and_the_default_returned() {
        with_temp_home(|home| {
            let path = home.join("terminal-settings.json");
            fs::write(&path, "{ not json").unwrap();
            let loaded = load_settings().unwrap();
            assert_eq!(loaded.json, default_settings_json());
            let bak = loaded.recovered_backup.expect("backup path reported");
            assert_eq!(bak, home.join("terminal-settings.json.bak"));
            assert_eq!(fs::read_to_string(&bak).unwrap(), "{ not json");
            assert!(!path.exists(), "corrupt file must be moved, not copied");
        });
    }

    #[test]
    fn object_without_a_numeric_version_counts_as_corrupt_on_load() {
        with_temp_home(|home| {
            fs::write(home.join("terminal-settings.json"), "{\"version\":\"1\"}").unwrap();
            assert!(load_settings().unwrap().recovered_backup.is_some());
            fs::write(home.join("terminal-settings.json"), "[1,2,3]").unwrap();
            assert!(load_settings().unwrap().recovered_backup.is_some());
        });
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_settings_file_is_refused() {
        with_temp_home(|home| {
            let target = home.join("elsewhere.json");
            fs::write(&target, "{\"version\":1}").unwrap();
            std::os::unix::fs::symlink(&target, home.join("terminal-settings.json")).unwrap();
            let err = load_settings().unwrap_err();
            assert!(
                matches!(err, AxiomataError::InvalidDashboardState { .. }),
                "{err}"
            );
        });
    }
}
