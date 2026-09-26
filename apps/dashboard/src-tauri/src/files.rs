//! Tauri glue for the file service (`docs/plans/editor.md` §ED0, E1/E6/E9):
//! the `file_*` commands the editor will call. The guard lives in the
//! standalone `axiomata-files` crate, the roots in `axiomata_core::files`;
//! this file only translates between IPC and those two, like `terminal.rs`
//! does for `axiomata-terminal`.
//!
//! A file is always named as `{ root, rel }` — never as an absolute path the
//! webview could make up. The one way to reach a new place on disk is
//! [`file_pick`], which drives the native open dialog **from Rust**: the
//! dialog plugin's JavaScript API is deliberately not granted in
//! `capabilities/default.json`, so the webview cannot open a dialog, let
//! alone forge its answer.
//!
//! Watching (E5): [`file_watch`] subscribes the page to one file, and every
//! settled external change arrives as a [`FILES_CHANGED`] event carrying
//! `{ root, rel, kind, version }`. The page recognises its own writes by the
//! version `file_write` returned. Subscriptions die with the page: a reload
//! drops all of them ([`FileWatch::page_reloaded`]).
//!
//! Every command is `async` and does its file I/O on a blocking thread, off
//! the main thread: a 16 MiB read must not stall the window. The database
//! lock is held only while a root id is being turned into a `Root`, never
//! during the I/O itself.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use axiomata_core::config::Config;
use axiomata_core::files::{RootInfo, Roots};
use axiomata_files::{
    self as service, FileWatcher, FilesError, Image, MAX_IMAGE_BYTES, MAX_READ_BYTES,
    MAX_WRITE_BYTES, Root, RootResolver, TextFile, Version,
};
use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::commands::CoreState;

/// A failed file command as the webview sees it (E9): `kind` is
/// [`FilesError::kind`] (`Conflict`, `NotUtf8`, `UnknownRoot`, …) to branch
/// on, `message` is for people.
#[derive(Debug, Serialize)]
pub struct FileError {
    kind: &'static str,
    message: String,
}

impl From<FilesError> for FileError {
    fn from(err: FilesError) -> Self {
        Self {
            kind: err.kind(),
            message: err.to_string(),
        }
    }
}

/// The event every settled change to a watched file is emitted as.
pub const FILES_CHANGED: &str = "files:changed";

/// Emitted the moment [`file_rename`] succeeded: every open copy of that file
/// (a tab, a panel, an IDE pane) follows it, before the watcher, which only
/// sees the old path go, could call it deleted (editor plan W13).
pub const FILES_RENAMED: &str = "files:renamed";

/// Emitted when [`file_delete_tree`] removed something: open files at or under
/// it learn at once, without waiting for the watcher's settle.
pub const FILES_REMOVED: &str = "files:removed";

/// Payload of [`FILES_RENAMED`].
#[derive(Debug, Clone, Serialize)]
pub struct Renamed {
    pub root: String,
    pub from: String,
    pub to: String,
}

/// Payload of [`FILES_REMOVED`].
#[derive(Debug, Clone, Serialize)]
pub struct Removed {
    pub root: String,
    pub rel: String,
}

/// The app's one file watcher, managed by Tauri. `None` if the OS watcher
/// could not be started — the app still runs, `file_watch` then says why.
pub struct FileWatch(Option<Arc<FileWatcher>>);

impl FileWatch {
    /// Starts the watcher, emitting each change as [`FILES_CHANGED`].
    pub fn start(app: &AppHandle) -> Self {
        let app = app.clone();
        let watcher = FileWatcher::new(move |change| {
            if let Err(err) = app.emit(FILES_CHANGED, &change) {
                tracing::warn!(%err, "could not emit a file change");
            }
        });
        match watcher {
            Ok(watcher) => Self(Some(Arc::new(watcher))),
            Err(err) => {
                tracing::warn!(%err, "file watcher unavailable; external changes go unnoticed");
                Self(None)
            }
        }
    }

    /// Drops every subscription: the page that held them is gone.
    pub fn page_reloaded(&self) {
        if let Some(watcher) = &self.0 {
            watcher.unwatch_all();
        }
    }
}

/// What the open dialog handed over.
#[derive(Debug, Serialize)]
pub struct Picked {
    /// The root to use from now on — an existing one if the pick lies inside
    /// it, otherwise a new or reused `grant:<id>`.
    root: String,
    /// The picked file, or folder, within `root` — `""` when the folder is
    /// the root itself.
    rel: String,
    /// Whether a folder was picked.
    folder: bool,
    /// The canonical path, for display only.
    path: PathBuf,
}

type SharedConfig = Arc<RwLock<Config>>;
type SharedDb = Arc<Mutex<Connection>>;

/// Runs `work` on a blocking thread with the shared config and database.
async fn blocking<T, F>(state: &CoreState, work: F) -> Result<T, FileError>
where
    T: Send + 'static,
    F: FnOnce(&SharedConfig, &SharedDb) -> Result<T, FilesError> + Send + 'static,
{
    let (config, db) = (state.config.clone(), state.db.clone());
    tauri::async_runtime::spawn_blocking(move || work(&config, &db))
        .await
        .map_err(|err| FileError {
            kind: "Io",
            message: format!("file task failed: {err}"),
        })?
        .map_err(FileError::from)
}

/// Runs `f` with the roots of this install, holding the DB lock only as long
/// as `f` runs — callers keep `f` to the lookup and do I/O afterwards.
fn with_roots<T>(config: &SharedConfig, db: &SharedDb, f: impl FnOnce(&Roots) -> T) -> T {
    let config = config
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
    f(&Roots::new(&config, &db))
}

fn root(config: &SharedConfig, db: &SharedDb, id: &str) -> Result<Root, FilesError> {
    with_roots(config, db, |roots| roots.root(id))
}

/// Every root that currently resolves, for display.
#[tauri::command]
pub async fn file_roots(state: State<'_, CoreState>) -> Result<Vec<RootInfo>, FileError> {
    blocking(&state, |config, db| {
        with_roots(config, db, |roots| roots.list())
    })
    .await
}

/// Reads a text file (≤ 16 MiB; `large` over 2 MiB, which the editor opens
/// read-only) together with its version.
#[tauri::command]
pub async fn file_read(
    state: State<'_, CoreState>,
    root: String,
    rel: String,
) -> Result<TextFile, FileError> {
    blocking(&state, move |config, db| {
        service::read_text(&self::root(config, db, &root)?, &rel, MAX_READ_BYTES)
    })
    .await
}

/// Writes a text file (≤ 2 MiB) and returns its new version. With
/// `expected`, fails with kind `Conflict` if the file changed since that
/// version was read (E4).
#[tauri::command]
pub async fn file_write(
    state: State<'_, CoreState>,
    root: String,
    rel: String,
    content: String,
    expected: Option<String>,
) -> Result<Version, FileError> {
    blocking(&state, move |config, db| {
        let expected = expected.map(Version::from);
        service::write_text(
            &self::root(config, db, &root)?,
            &rel,
            &content,
            expected.as_ref(),
            MAX_WRITE_BYTES,
        )
    })
    .await
}

/// Deletes a file (for an allowed symlink: the link).
#[tauri::command]
pub async fn file_delete(
    state: State<'_, CoreState>,
    root: String,
    rel: String,
) -> Result<(), FileError> {
    blocking(&state, move |config, db| {
        service::delete(&self::root(config, db, &root)?, &rel)
    })
    .await
}

/// One folder of a root for the file app's tree (`""`: the root itself) —
/// folders first, `.gitignore`d entries marked (editor plan W6).
#[tauri::command]
pub async fn file_list(
    state: State<'_, CoreState>,
    root: String,
    rel: String,
) -> Result<service::Listing, FileError> {
    blocking(&state, move |config, db| {
        service::list_dir(&self::root(config, db, &root)?, &rel)
    })
    .await
}

/// Makes a folder; its parent must exist (W13).
#[tauri::command]
pub async fn file_mkdir(
    state: State<'_, CoreState>,
    root: String,
    rel: String,
) -> Result<(), FileError> {
    blocking(&state, move |config, db| {
        service::make_dir(&self::root(config, db, &root)?, &rel)
    })
    .await
}

/// Renames or moves an entry inside one root; never overwrites (W13). Tells
/// every open copy of the file where it went ([`FILES_RENAMED`]).
#[tauri::command]
pub async fn file_rename(
    app: AppHandle,
    state: State<'_, CoreState>,
    root: String,
    from: String,
    to: String,
) -> Result<(), FileError> {
    let renamed = Renamed {
        root: root.clone(),
        from: from.clone(),
        to: to.clone(),
    };
    blocking(&state, move |config, db| {
        service::rename_entry(&self::root(config, db, &root)?, &from, &to)
    })
    .await?;
    if let Err(err) = app.emit(FILES_RENAMED, &renamed) {
        tracing::warn!(%err, "could not emit a rename");
    }
    Ok(())
}

/// Makes a new, empty text file; never over something already there (W13).
#[tauri::command]
pub async fn file_create(
    state: State<'_, CoreState>,
    root: String,
    rel: String,
) -> Result<Version, FileError> {
    blocking(&state, move |config, db| {
        service::create_text(&self::root(config, db, &root)?, &rel)
    })
    .await
}

/// How many entries a folder holds, for the question before deleting it (W13).
#[tauri::command]
pub async fn file_count(
    state: State<'_, CoreState>,
    root: String,
    rel: String,
) -> Result<usize, FileError> {
    blocking(&state, move |config, db| {
        service::count_tree(&self::root(config, db, &root)?, &rel)
    })
    .await
}

/// Deletes a folder with everything in it, or a file; never the root (W13).
/// The view asks first.
#[tauri::command]
pub async fn file_delete_tree(
    app: AppHandle,
    state: State<'_, CoreState>,
    root: String,
    rel: String,
) -> Result<(), FileError> {
    let removed = Removed {
        root: root.clone(),
        rel: rel.clone(),
    };
    blocking(&state, move |config, db| {
        service::delete_tree(&self::root(config, db, &root)?, &rel)
    })
    .await?;
    if let Err(err) = app.emit(FILES_REMOVED, &removed) {
        tracing::warn!(%err, "could not emit a removal");
    }
    Ok(())
}

/// Reads a raster image (≤ 8 MiB), base64-encoded.
#[tauri::command]
pub async fn file_read_image(
    state: State<'_, CoreState>,
    root: String,
    rel: String,
) -> Result<Image, FileError> {
    blocking(&state, move |config, db| {
        service::read_image(&self::root(config, db, &root)?, &rel, MAX_IMAGE_BYTES)
    })
    .await
}

/// Subscribes the page to external changes of one file (counted: call
/// [`file_unwatch`] once per `file_watch`). The file need not exist yet.
#[tauri::command]
pub async fn file_watch(
    state: State<'_, CoreState>,
    watch: State<'_, FileWatch>,
    root: String,
    rel: String,
) -> Result<(), FileError> {
    let resolved = blocking(&state, {
        let root = root.clone();
        move |config, db| self::root(config, db, &root)
    })
    .await?;
    let watcher = watch.0.clone().ok_or_else(|| FileError {
        kind: "Io",
        message: "the file watcher is not running".into(),
    })?;
    // `watch` reads and hashes the file for its starting version: off the
    // async runtime, like every other file access here.
    tauri::async_runtime::spawn_blocking(move || watcher.watch(&root, &resolved, &rel))
        .await
        .map_err(|err| FileError {
            kind: "Io",
            message: format!("file task failed: {err}"),
        })?
        .map_err(FileError::from)
}

/// Drops one subscription taken with [`file_watch`].
#[tauri::command]
pub fn file_unwatch(watch: State<'_, FileWatch>, root: String, rel: String) {
    if let Some(watcher) = &watch.0 {
        watcher.unwatch(&root, &rel);
    }
}

/// Opens the native open dialog for a file, or with `folder` for a folder,
/// and returns where the pick can be reached — `None` if it was cancelled.
///
/// A picked file inside a registered root (workspace, project, worktree,
/// granted folder) comes back under that root; anything else becomes a
/// grant (E7), reusing an existing one for the same path.
#[tauri::command]
pub async fn file_pick(
    app: AppHandle,
    state: State<'_, CoreState>,
    folder: bool,
) -> Result<Option<Picked>, FileError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let dialog = app.dialog().file();
    let answer = move |picked: Option<tauri_plugin_dialog::FilePath>| {
        let _ = tx.send(picked.and_then(|p| p.into_path().ok()));
    };
    if folder {
        dialog.set_title("Ordner öffnen").pick_folder(answer);
    } else {
        dialog.set_title("Datei öffnen").pick_file(answer);
    }
    let Some(path) = rx.await.ok().flatten() else {
        return Ok(None);
    };
    blocking(&state, move |config, db| {
        // Folders too: a fresh folder grant is `Contained`, so re-granting a
        // folder the strict workspace already covers would loosen its guard.
        let found = with_roots(config, db, |roots| roots.locate(&path));
        let canonical = path.canonicalize().map_err(|source| FilesError::Io {
            path: path.clone(),
            source,
        })?;
        if let Some((root, rel)) = found {
            return Ok(Some(Picked {
                root,
                rel,
                folder,
                path: canonical,
            }));
        }
        let grant = axiomata_core::files::grant_store().grant(&canonical)?;
        // A file grant gives out its one file by name; a folder grant is
        // the folder itself.
        let rel = if folder {
            String::new()
        } else {
            grant
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string()
        };
        Ok(Some(Picked {
            root: format!("grant:{}", grant.id),
            rel,
            folder,
            path: grant.path,
        }))
    })
    .await
}

// ------------------------------------------------------------ editor recovery
//
// Unsaved editor work kept aside (`axiomata_core::editor_recovery`, plan F8).
// Keyed by root id + path as strings only: an entry never grants access to the
// file, it only holds text the editor itself had; restoring it is an ordinary
// save through the guard above.

/// Keeps the unsaved text of `root` + `rel`, based on file version `base`.
#[tauri::command]
pub async fn editor_recovery_save(
    root: String,
    rel: String,
    base: Option<String>,
    content: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        axiomata_core::editor_recovery::save(&root, &rel, base, &content)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

/// The unsaved text kept for `root` + `rel`, if any.
#[tauri::command]
pub async fn editor_recovery_load(
    root: String,
    rel: String,
) -> Result<Option<axiomata_core::editor_recovery::Recovery>, String> {
    tauri::async_runtime::spawn_blocking(move || axiomata_core::editor_recovery::load(&root, &rel))
        .await
        .map_err(|err| err.to_string())
}

/// Forgets the unsaved text kept for `root` + `rel`.
#[tauri::command]
pub async fn editor_recovery_delete(root: String, rel: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        axiomata_core::editor_recovery::delete(&root, &rel)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}
