//! Generic "versioned JSON state file under `~/.axiomata/`" read/write
//! machinery — atomic write (0600, `O_EXCL` temp + rename), corrupt-file
//! recovery (moved aside to `.bak`, defaults returned instead of erroring),
//! an oversized-file rejection, and refusing a symlinked state file.
//!
//! Extracted (Checkpoint 5d of `docs/plans/terminal.md`) out of
//! `crate::dashboard`, whose `load_state`/`save_state` used to own this
//! logic outright for `dashboard.json` alone — `terminal-settings.json`
//! (the Terminal module's own global preferences file, `crate::terminal_settings`)
//! needs the exact same contract against a different path and a different
//! default. Each caller supplies only its own path and default JSON; the
//! structural guarantee ("a JSON object with a numeric `version`") and every
//! safety behaviour below live here once. Reuses
//! [`AxiomataError::InvalidDashboardState`] for the error variant — already
//! the general "one of our own local state files is invalid" error before
//! this extraction (`crate::dashboard::load_custom_css` reused it for
//! `theme.css` too), not renamed here to avoid unrelated churn.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AxiomataError;

/// Hard cap on a state file — anything larger is treated as corrupt.
pub const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;

/// What a caller gets back after [`load`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedJsonState {
    /// Raw file text (or the serialised default). Parsed by the frontend,
    /// not here — this module only enforces the structural contract below.
    pub json: String,
    /// Set when the existing file was unreadable and moved to `.bak`.
    pub recovered_backup: Option<PathBuf>,
}

/// Loads the JSON state file at `path`, or `default_json()` if it is
/// missing.
///
/// A file that is not a JSON object with a numeric `version` — or that is
/// oversized — is renamed to `<path>.bak` (replacing any older backup) and
/// the default is returned with `recovered_backup` set. A symlinked state
/// file is refused outright. `default_json` is a closure (not a plain
/// `&str`) purely so a caller whose default is cheap to compute doesn't pay
/// for building it on the common "file already exists" path.
pub fn load(
    path: &Path,
    default_json: impl FnOnce() -> String,
) -> Result<LoadedJsonState, AxiomataError> {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(LoadedJsonState {
            json: default_json(),
            recovered_backup: None,
        });
    };
    if meta.file_type().is_symlink() {
        return Err(AxiomataError::InvalidDashboardState {
            path: path.to_path_buf(),
            reason: "refusing to follow a symlinked state file".to_string(),
        });
    }

    let content = if meta.len() > MAX_STATE_BYTES {
        Err(format!("file exceeds {MAX_STATE_BYTES} bytes"))
    } else {
        fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| validate(&text).map(|()| text))
    };

    match content {
        Ok(json) => Ok(LoadedJsonState {
            json,
            recovered_backup: None,
        }),
        Err(_) => {
            let backup = backup_path(path);
            fs::rename(path, &backup).map_err(|source| AxiomataError::Io {
                path: path.to_path_buf(),
                source,
            })?;
            Ok(LoadedJsonState {
                json: default_json(),
                recovered_backup: Some(backup),
            })
        }
    }
}

/// Validates and atomically writes `json` to `path` (mode 0600), creating
/// its parent directory if needed.
pub fn save(path: &Path, json: &str) -> Result<(), AxiomataError> {
    validate(json).map_err(|reason| AxiomataError::InvalidDashboardState {
        path: path.to_path_buf(),
        reason,
    })?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| AxiomataError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    atomic_write(path, json)
}

/// The structural check both directions share: a JSON object with a
/// non-negative integer `version` field.
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
    use crate::test_support::unique_temp_dir;

    fn default_json() -> String {
        "{\"version\":1}".to_string()
    }

    #[test]
    fn missing_file_yields_the_default_without_a_backup() {
        let dir = unique_temp_dir("json-state-missing");
        let path = dir.join("state.json");
        let loaded = load(&path, default_json).unwrap();
        assert_eq!(loaded.json, default_json());
        assert!(loaded.recovered_backup.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_round_trips_verbatim_at_0600() {
        let dir = unique_temp_dir("json-state-roundtrip");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let text = "{\n  \"version\": 1,\n  \"custom\": {\"kept\": true}\n}\n";
        save(&path, text).unwrap();
        let loaded = load(&path, default_json).unwrap();
        assert_eq!(loaded.json, text);
        assert!(loaded.recovered_backup.is_none());
        assert!(!path.with_extension("json.axiomata-tmp").exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_is_moved_to_bak_and_the_default_returned() {
        let dir = unique_temp_dir("json-state-corrupt");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        fs::write(&path, "{ not json").unwrap();
        let loaded = load(&path, default_json).unwrap();
        assert_eq!(loaded.json, default_json());
        let bak = loaded.recovered_backup.expect("backup path reported");
        assert_eq!(bak, path.with_extension("json.bak"));
        assert_eq!(fs::read_to_string(&bak).unwrap(), "{ not json");
        assert!(!path.exists(), "corrupt file must be moved, not copied");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn object_without_a_numeric_version_counts_as_corrupt() {
        let dir = unique_temp_dir("json-state-no-version");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        fs::write(&path, "{\"version\":\"1\"}").unwrap();
        assert!(
            load(&path, default_json)
                .unwrap()
                .recovered_backup
                .is_some()
        );
        fs::write(&path, "[1,2,3]").unwrap();
        assert!(
            load(&path, default_json)
                .unwrap()
                .recovered_backup
                .is_some()
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_rejects_invalid_state_and_leaves_the_file_untouched() {
        let dir = unique_temp_dir("json-state-save-reject");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        save(&path, "{\"version\":1,\"a\":1}").unwrap();
        let err = save(&path, "{\"a\":1}").unwrap_err();
        assert!(
            matches!(err, AxiomataError::InvalidDashboardState { .. }),
            "{err}"
        );
        let err = save(&path, "nope").unwrap_err();
        assert!(
            matches!(err, AxiomataError::InvalidDashboardState { .. }),
            "{err}"
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\"version\":1,\"a\":1}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_state_file_is_refused() {
        let dir = unique_temp_dir("json-state-symlink");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("elsewhere.json");
        fs::write(&target, "{\"version\":1}").unwrap();
        let path = dir.join("state.json");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let err = load(&path, default_json).unwrap_err();
        assert!(
            matches!(err, AxiomataError::InvalidDashboardState { .. }),
            "{err}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversized_file_is_treated_as_corrupt_and_backed_up() {
        // The size check must fire even for otherwise-structurally-valid
        // JSON — it is checked before `validate()` is ever called, per
        // `load`'s own doc comment ("or that is oversized").
        let dir = unique_temp_dir("json-state-oversized");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let filler = "x".repeat(MAX_STATE_BYTES as usize + 1);
        fs::write(&path, &filler).unwrap();
        let loaded = load(&path, default_json).unwrap();
        assert_eq!(loaded.json, default_json());
        let bak = loaded
            .recovered_backup
            .expect("oversized file must be reported as recovered");
        assert_eq!(bak, path.with_extension("json.bak"));
        assert_eq!(fs::read_to_string(&bak).unwrap(), filler);
        assert!(!path.exists(), "oversized file must be moved, not copied");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_creates_a_missing_parent_directory() {
        // Unlike every other test in this module, deliberately do *not*
        // `create_dir_all` first — `save`'s own doc comment promises it
        // "creating its parent directory if needed", which none of the
        // other round-trip tests actually exercises (they all pre-create
        // the directory via `unique_temp_dir`'s caller convention).
        let dir = unique_temp_dir("json-state-missing-parent");
        let path = dir.join("nested").join("state.json");
        assert!(!dir.exists(), "precondition: nothing pre-created");
        save(&path, "{\"version\":1}").unwrap();
        let loaded = load(&path, default_json).unwrap();
        assert_eq!(loaded.json, "{\"version\":1}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_file_backup_replaces_a_stale_bak() {
        // `load`'s doc comment promises the `.bak` is replaced, not
        // appended to or refused, when an older backup already exists.
        let dir = unique_temp_dir("json-state-stale-bak");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.json");
        let bak = path.with_extension("json.bak");
        fs::write(&bak, "stale backup from a previous corrupt save").unwrap();
        fs::write(&path, "{ still not json").unwrap();
        let loaded = load(&path, default_json).unwrap();
        assert_eq!(loaded.recovered_backup.as_deref(), Some(bak.as_path()));
        assert_eq!(
            fs::read_to_string(&bak).unwrap(),
            "{ still not json",
            "the stale backup must be overwritten by the newly-recovered content"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
