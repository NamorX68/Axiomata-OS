//! Loading and saving what Vi remembers across restarts
//! (`~/.axiomata/editor-vi.json`, `docs/plans/editor.md` ED3 V4): the named
//! registers (and with them macros), the file marks `A`–`Z`, the search and
//! command-line histories and the last search.
//!
//! Same contract as `editor-settings.json` (`crate::editor_settings`): the
//! frontend owns the fields (`fileapp/viPersist.ts` validates them on the way
//! in), this only guarantees "object with numeric `version`" and does the
//! atomic 0600 write through `crate::json_state` — 0600 matters here, since a
//! yanked line may be anything the owner had in a file.

use crate::error::AxiomataError;
use crate::json_state::{self, LoadedJsonState};
use crate::paths;

/// Current on-disk schema version written by the frontend.
pub const VI_STATE_VERSION: u64 = 1;

/// What a missing file reads as: nothing remembered yet.
pub fn default_vi_state_json() -> String {
    format!("{{\"version\":{VI_STATE_VERSION}}}")
}

/// Loads `editor-vi.json`, or the empty default if it is missing. See
/// `json_state::load` for the corrupt-file, symlink and size contract.
pub fn load_vi_state() -> Result<LoadedJsonState, AxiomataError> {
    json_state::load(&paths::editor_vi_path(), default_vi_state_json)
}

/// Validates and atomically writes `json` to `editor-vi.json` (0600).
pub fn save_vi_state(json: &str) -> Result<(), AxiomataError> {
    json_state::save(&paths::editor_vi_path(), json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;
    use std::fs;
    use std::path::Path;

    /// Runs `body` with `AXIOMATA_HOME` pointed at a fresh temp dir, like
    /// `editor_settings::tests::with_temp_home`.
    fn with_temp_home(body: impl FnOnce(&Path)) {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-editor-vi");
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
    fn missing_file_yields_the_empty_default() {
        with_temp_home(|_| {
            let loaded = load_vi_state().unwrap();
            assert_eq!(loaded.json, default_vi_state_json());
            assert!(loaded.recovered_backup.is_none());
        });
    }

    #[test]
    fn save_then_load_round_trips_at_the_real_path_with_owner_only_permissions() {
        with_temp_home(|home| {
            let text = "{\"version\":1,\"registers\":{\"a\":{\"text\":\"x\",\"kind\":\"char\"}}}";
            save_vi_state(text).unwrap();
            assert_eq!(load_vi_state().unwrap().json, text);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = fs::metadata(home.join("editor-vi.json"))
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600);
            }
        });
    }

    #[test]
    fn save_rejects_an_object_without_a_version() {
        with_temp_home(|_| {
            assert!(save_vi_state("{\"registers\":{}}").is_err());
        });
    }

    #[test]
    fn corrupt_file_is_moved_to_bak_and_the_default_returned() {
        // Mirrors `editor_settings::tests::corrupt_file_is_moved_to_bak_and_the_default_returned`.
        with_temp_home(|home| {
            let path = home.join("editor-vi.json");
            fs::write(&path, "{ not json").unwrap();
            let loaded = load_vi_state().unwrap();
            assert_eq!(loaded.json, default_vi_state_json());
            let bak = loaded.recovered_backup.expect("backup path reported");
            assert_eq!(bak, home.join("editor-vi.json.bak"));
            assert_eq!(fs::read_to_string(&bak).unwrap(), "{ not json");
            assert!(!path.exists(), "corrupt file must be moved, not copied");
        });
    }
}
