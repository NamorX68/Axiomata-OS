//! Loading and saving the editor's preferences file
//! (`~/.axiomata/editor-settings.json`, `docs/plans/editor.md` D6/F12).
//!
//! Same contract as `terminal-settings.json` (`crate::terminal_settings`): the
//! frontend owns the fields (`fileapp/editorSettings.ts` parses and defaults
//! them), this only guarantees "object with numeric `version`" and does the
//! atomic 0600 write through `crate::json_state`. Its own file rather than a
//! corner of `dashboard.json`, so a standalone editor (ED7) has something to
//! read that is not dashboard-shaped.

use crate::error::AxiomataError;
use crate::json_state::{self, LoadedJsonState};
use crate::paths;

/// Current on-disk schema version written by the frontend.
pub const SETTINGS_VERSION: u64 = 1;

/// The file as read, plus whether a corrupt copy was moved aside.
pub type LoadedEditorSettings = LoadedJsonState;

/// What a missing file reads as: every field falls back to the frontend's
/// defaults.
pub fn default_settings_json() -> String {
    format!("{{\"version\":{SETTINGS_VERSION}}}")
}

/// Loads `editor-settings.json`, or the defaults if it is missing. See
/// `json_state::load` for the corrupt-file, symlink and size contract.
pub fn load_settings() -> Result<LoadedEditorSettings, AxiomataError> {
    json_state::load(&paths::editor_settings_path(), default_settings_json)
}

/// Validates and atomically writes `json` to `editor-settings.json` (0600).
pub fn save_settings(json: &str) -> Result<(), AxiomataError> {
    json_state::save(&paths::editor_settings_path(), json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

    #[test]
    fn round_trips_through_json_state_and_defaults_when_missing() {
        let dir = unique_temp_dir("editor-settings");
        let path = dir.join("editor-settings.json");
        let loaded = json_state::load(&path, default_settings_json).unwrap();
        assert_eq!(loaded.json, "{\"version\":1}");

        json_state::save(&path, "{\"version\":1,\"fontSize\":15}").unwrap();
        assert!(
            json_state::load(&path, default_settings_json)
                .unwrap()
                .json
                .contains("\"fontSize\":15")
        );
        assert!(
            json_state::save(&path, "{\"fontSize\":15}").is_err(),
            "a version is required"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    // The tests below exercise `load_settings`/`save_settings` themselves —
    // the public API, resolving `editor-settings.json` via the real
    // `AXIOMATA_HOME`-derived path — rather than `json_state::load`/`save`
    // called directly against a scratch path as above. Same wiring proof
    // `terminal_settings.rs`'s own equivalent trio keeps for its sibling
    // file, since the two modules share the identical contract.

    use crate::test_support::ENV_MUTEX;
    use std::env;
    use std::fs;
    use std::path::Path;

    /// Runs `body` with `AXIOMATA_HOME` pointed at a fresh temp dir — see
    /// `terminal_settings::tests::with_temp_home`, which this mirrors.
    fn with_temp_home(body: impl FnOnce(&Path)) {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-editor-settings");
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
            let text = "{\n  \"version\": 1,\n  \"fontSizePx\": 14,\n  \"vimMode\": true\n}\n";
            save_settings(text).unwrap();
            let loaded = load_settings().unwrap();
            assert_eq!(loaded.json, text);
            assert_eq!(
                fs::read_to_string(home.join("editor-settings.json")).unwrap(),
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

    #[test]
    fn corrupt_file_is_moved_to_bak_and_the_default_returned() {
        with_temp_home(|home| {
            let path = home.join("editor-settings.json");
            fs::write(&path, "{ not json").unwrap();
            let loaded = load_settings().unwrap();
            assert_eq!(loaded.json, default_settings_json());
            let bak = loaded.recovered_backup.expect("backup path reported");
            assert_eq!(bak, home.join("editor-settings.json.bak"));
            assert_eq!(fs::read_to_string(&bak).unwrap(), "{ not json");
            assert!(!path.exists(), "corrupt file must be moved, not copied");
        });
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_settings_file_is_refused() {
        with_temp_home(|home| {
            let target = home.join("elsewhere.json");
            fs::write(&target, "{\"version\":1}").unwrap();
            std::os::unix::fs::symlink(&target, home.join("editor-settings.json")).unwrap();
            let err = load_settings().unwrap_err();
            assert!(
                matches!(err, AxiomataError::InvalidDashboardState { .. }),
                "{err}"
            );
        });
    }
}
