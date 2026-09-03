//! Loading and saving the dashboard layout file (`~/.axiomata/dashboard.json`).
//!
//! The core is deliberately schema-agnostic here: the frontend owns the JSON
//! shape (layout, settings, per-instance module config) and hand-edits must
//! pass through untouched. The only structural guarantee enforced on both
//! read and write is "a JSON object with a numeric `version`". A file that
//! fails even that is moved aside as `dashboard.json.bak` and replaced by the
//! defaults, so a bad edit never bricks the app.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AxiomataError;
use crate::paths;

/// Current on-disk schema version written by the frontend.
pub const STATE_VERSION: u64 = 1;

/// Largest custom theme file read (it goes into the webview as text).
pub const MAX_CUSTOM_CSS_BYTES: u64 = 64 * 1024;

/// Hard cap on the state file — anything larger is treated as corrupt.
pub const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;

/// What the frontend gets on boot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedState {
    /// Raw file text (or the serialised default). Parsed by the frontend.
    pub json: String,
    /// Set when the existing file was unreadable and moved to `.bak`.
    pub recovered_backup: Option<PathBuf>,
}

/// The state written when no file exists yet.
pub fn default_state_json() -> String {
    format!(
        "{{\"version\":{STATE_VERSION},\"settings\":{{\"theme\":\"graphite\",\"customCssPath\":null}},\
         \"canvas\":{{\"instances\":[]}}}}"
    )
}

/// Loads `dashboard.json`, or the defaults if it is missing.
///
/// A file that is not a JSON object with a numeric `version` — or that is
/// oversized — is renamed to `dashboard.json.bak` (replacing any older backup)
/// and the defaults are returned with `recovered_backup` set. A symlinked
/// state file is refused outright.
pub fn load_state() -> Result<LoadedState, AxiomataError> {
    let path = paths::dashboard_state_path();
    let Ok(meta) = fs::symlink_metadata(&path) else {
        return Ok(LoadedState {
            json: default_state_json(),
            recovered_backup: None,
        });
    };
    if meta.file_type().is_symlink() {
        return Err(AxiomataError::InvalidDashboardState {
            path,
            reason: "refusing to follow a symlinked state file".to_string(),
        });
    }

    let content = if meta.len() > MAX_STATE_BYTES {
        Err(format!("file exceeds {MAX_STATE_BYTES} bytes"))
    } else {
        fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| validate(&text).map(|()| text))
    };

    match content {
        Ok(json) => Ok(LoadedState {
            json,
            recovered_backup: None,
        }),
        Err(_) => {
            let backup = backup_path(&path);
            fs::rename(&path, &backup).map_err(|source| AxiomataError::Io {
                path: path.clone(),
                source,
            })?;
            Ok(LoadedState {
                json: default_state_json(),
                recovered_backup: Some(backup),
            })
        }
    }
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
pub fn save_state(json: &str) -> Result<(), AxiomataError> {
    let path = paths::dashboard_state_path();
    validate(json).map_err(|reason| AxiomataError::InvalidDashboardState {
        path: path.clone(),
        reason,
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AxiomataError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    atomic_write(&path, json)
}

/// The structural check both directions share.
fn validate(text: &str) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let obj = value
        .as_object()
        .ok_or_else(|| "top level must be a JSON object".to_string())?;
    match obj.get("version") {
        Some(v) if v.is_u64() => Ok(()),
        Some(_) => Err("`version` must be a non-negative integer".to_string()),
        None => Err("missing `version`".to_string()),
    }
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

/// Temp file beside the target (created `O_EXCL`, so a planted symlink is
/// never followed), then `rename`, then best-effort `0600`.
fn atomic_write(path: &Path, content: &str) -> Result<(), AxiomataError> {
    let tmp = path.with_extension("json.axiomata-tmp");
    let _ = fs::remove_file(&tmp); // a stale leftover from a crashed save
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| f.write_all(content.as_bytes()))
        .map_err(|source| AxiomataError::Io {
            path: tmp.clone(),
            source,
        })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    }
    fs::rename(&tmp, path).map_err(|source| {
        let _ = fs::remove_file(&tmp);
        AxiomataError::Io {
            path: path.to_path_buf(),
            source,
        }
    })
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
            validate(&loaded.json).expect("default must validate");
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
