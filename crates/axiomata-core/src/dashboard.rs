//! Loading and saving the dashboard layout file (`~/.axiomata/dashboard.json`).
//!
//! The core is deliberately schema-agnostic here: the frontend owns the JSON
//! shape (layout, settings, per-instance module config) and hand-edits must
//! pass through untouched. The only structural guarantee enforced on both
//! read and write is "a JSON object with a numeric `version`". A file that
//! fails even that is moved aside as `dashboard.json.bak` and replaced by the
//! defaults, so a bad edit never bricks the app. That read/write/recovery
//! machinery itself now lives in `crate::json_state` (Checkpoint 5d of
//! `docs/plans/terminal.md`, extracted once `terminal-settings.json` needed
//! the identical contract against a different path) — this module only
//! supplies `dashboard.json`'s own path and default JSON.

use std::fs;
use std::path::Path;

use crate::error::AxiomataError;
use crate::json_state::{self, LoadedJsonState};
use crate::paths;

/// Current on-disk schema version written by the frontend.
pub const STATE_VERSION: u64 = 1;

/// Largest custom theme file read (it goes into the webview as text).
pub const MAX_CUSTOM_CSS_BYTES: u64 = 64 * 1024;

/// What the frontend gets on boot. A type alias, not a re-export under a new
/// name, so every existing caller (`src-tauri/src/commands.rs`, the
/// frontend's mirrored `LoadedDashboardState` TS type) keeps working
/// unchanged after the `crate::json_state` extraction.
pub type LoadedState = LoadedJsonState;

/// The state written when no file exists yet.
pub fn default_state_json() -> String {
    format!(
        "{{\"version\":{STATE_VERSION},\"settings\":{{\"theme\":\"graphite\",\"customCssPath\":null}},\
         \"canvas\":{{\"instances\":[]}}}}"
    )
}

/// Loads `dashboard.json`, or the defaults if it is missing. See
/// `crate::json_state::load`'s own doc comment for the full corrupt-file/
/// symlink/oversized-file contract this delegates to.
pub fn load_state() -> Result<LoadedState, AxiomataError> {
    json_state::load(&paths::dashboard_state_path(), default_state_json)
}

/// Reads the user's custom theme CSS: `override_path` if given (absolute,
/// `.css`), else `~/.axiomata/theme.css`. `Ok(None)` when the file doesn't
/// exist. A symlink or an oversized file is refused; the CSS itself is *not*
/// validated here — the dashboard's validator decides what gets injected.
pub fn load_custom_css(override_path: Option<&Path>) -> Result<Option<String>, AxiomataError> {
    let path = match override_path {
        Some(p) => {
            if !p.is_absolute() || p.extension().is_none_or(|e| e != "css") {
                return Err(AxiomataError::InvalidDashboardState {
                    path: p.to_path_buf(),
                    reason: "customCssPath must be an absolute path to a .css file".to_string(),
                });
            }
            p.to_path_buf()
        }
        None => paths::custom_theme_path(),
    };
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => return Err(AxiomataError::Io { path, source }),
    };
    if meta.file_type().is_symlink() {
        return Err(AxiomataError::InvalidDashboardState {
            path,
            reason: "refusing to follow a symlinked theme file".to_string(),
        });
    }
    if meta.len() > MAX_CUSTOM_CSS_BYTES {
        return Err(AxiomataError::InvalidDashboardState {
            path,
            reason: format!("theme file exceeds {MAX_CUSTOM_CSS_BYTES} bytes"),
        });
    }
    fs::read_to_string(&path)
        .map(Some)
        .map_err(|source| AxiomataError::Io { path, source })
}

/// Validates and atomically writes `json` to `dashboard.json` (mode 0600).
/// See `crate::json_state::save`'s own doc comment for the full contract.
pub fn save_state(json: &str) -> Result<(), AxiomataError> {
    json_state::save(&paths::dashboard_state_path(), json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;

    /// Runs `body` with `AXIOMATA_HOME` pointed at a fresh temp dir.
    fn with_temp_home(body: impl FnOnce(&Path)) {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-dashboard");
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
    fn missing_file_yields_defaults_without_backup() {
        with_temp_home(|_| {
            let loaded = load_state().unwrap();
            assert_eq!(loaded.json, default_state_json());
            assert!(loaded.recovered_backup.is_none());
            // The default must itself satisfy `save_state`'s own validation
            // (the structural check now lives in `crate::json_state`, not
            // exposed here to call directly) — round-tripping it through a
            // real save proves that rather than just asserting on the
            // string's shape.
            save_state(&loaded.json).expect("default must validate");
        });
    }

    #[test]
    fn save_then_load_round_trips_verbatim() {
        with_temp_home(|home| {
            let text = "{\n  \"version\": 1,\n  \"custom\": {\"kept\": true}\n}\n";
            save_state(text).unwrap();
            let loaded = load_state().unwrap();
            assert_eq!(loaded.json, text);
            assert!(loaded.recovered_backup.is_none());
            assert!(!home.join("dashboard.json.axiomata-tmp").exists());

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = fs::metadata(home.join("dashboard.json"))
                    .unwrap()
                    .permissions()
                    .mode();
                assert_eq!(mode & 0o777, 0o600);
            }
        });
    }

    #[test]
    fn corrupt_file_is_moved_to_bak_and_defaults_returned() {
        with_temp_home(|home| {
            let path = home.join("dashboard.json");
            fs::write(&path, "{ not json").unwrap();
            let loaded = load_state().unwrap();
            assert_eq!(loaded.json, default_state_json());
            let bak = loaded.recovered_backup.expect("backup path reported");
            assert_eq!(bak, home.join("dashboard.json.bak"));
            assert_eq!(fs::read_to_string(&bak).unwrap(), "{ not json");
            assert!(!path.exists(), "corrupt file must be moved, not copied");
        });
    }

    #[test]
    fn object_without_numeric_version_counts_as_corrupt() {
        with_temp_home(|home| {
            fs::write(home.join("dashboard.json"), "{\"version\":\"1\"}").unwrap();
            assert!(load_state().unwrap().recovered_backup.is_some());
            fs::write(home.join("dashboard.json"), "[1,2,3]").unwrap();
            assert!(load_state().unwrap().recovered_backup.is_some());
        });
    }

    #[test]
    fn save_rejects_invalid_state_and_leaves_file_untouched() {
        with_temp_home(|home| {
            save_state("{\"version\":1,\"a\":1}").unwrap();
            let err = save_state("{\"a\":1}").unwrap_err();
            assert!(
                matches!(err, AxiomataError::InvalidDashboardState { .. }),
                "{err}"
            );
            let err = save_state("nope").unwrap_err();
            assert!(
                matches!(err, AxiomataError::InvalidDashboardState { .. }),
                "{err}"
            );
            assert_eq!(
                fs::read_to_string(home.join("dashboard.json")).unwrap(),
                "{\"version\":1,\"a\":1}"
            );
        });
    }

    #[test]
    fn custom_css_absent_then_present_then_override_rules() {
        with_temp_home(|home| {
            assert_eq!(load_custom_css(None).unwrap(), None);
            fs::write(home.join("theme.css"), ":root { --ax-accent: red }").unwrap();
            assert_eq!(
                load_custom_css(None).unwrap().as_deref(),
                Some(":root { --ax-accent: red }")
            );
            let other = home.join("other.css");
            fs::write(&other, "x").unwrap();
            assert_eq!(load_custom_css(Some(&other)).unwrap().as_deref(), Some("x"));
            assert!(load_custom_css(Some(Path::new("relative.css"))).is_err());
            assert!(load_custom_css(Some(&home.join("theme.txt"))).is_err());
            fs::write(
                home.join("big.css"),
                "x".repeat(MAX_CUSTOM_CSS_BYTES as usize + 1),
            )
            .unwrap();
            assert!(load_custom_css(Some(&home.join("big.css"))).is_err());
        });
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_state_file_is_refused() {
        with_temp_home(|home| {
            let target = home.join("elsewhere.json");
            fs::write(&target, "{\"version\":1}").unwrap();
            std::os::unix::fs::symlink(&target, home.join("dashboard.json")).unwrap();
            let err = load_state().unwrap_err();
            assert!(
                matches!(err, AxiomataError::InvalidDashboardState { .. }),
                "{err}"
            );
        });
    }
}
