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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use axiomata_core::config::Config;
use axiomata_core::files::{RootInfo, Roots};
use axiomata_files::{
    self as service, FileMatches, FileWatcher, FilesError, Image, MAX_IMAGE_BYTES, MAX_READ_BYTES,
    MAX_WRITE_BYTES, Root, RootResolver, SearchQuery, SearchSummary, TextFile, Version,
};
use rusqlite::Connection;
use serde::Serialize;
use tauri::ipc::Channel;
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
pub(crate) async fn blocking<T, F>(state: &CoreState, work: F) -> Result<T, FileError>
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

pub(crate) fn root(config: &SharedConfig, db: &SharedDb, id: &str) -> Result<Root, FilesError> {
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

/// Reads a text file (≤ 16 MiB; `large` over 2 MiB, which the editor edits in
/// its light mode) together with its version.
///
/// `lsp:<server>` is the read-only root of files outside every root that a
/// language server pointed to in a definition answer (editor plan L11); `rel`
/// is then the absolute path, and only what that server named is readable.
#[tauri::command]
pub async fn file_read(
    state: State<'_, CoreState>,
    lsp: State<'_, crate::lsp::LspState>,
    root: String,
    rel: String,
) -> Result<TextFile, FileError> {
    if let Some(handle) = root.strip_prefix(crate::lsp::FOREIGN_ROOT) {
        let handle = handle.parse::<u64>().map_err(|_| FileError {
            kind: "UnknownRoot",
            message: format!("unknown file root `{root}`"),
        })?;
        let host = lsp.host();
        return tauri::async_runtime::spawn_blocking(move || {
            host.read_foreign(handle, std::path::Path::new(&rel), MAX_READ_BYTES)
        })
        .await
        .map_err(|err| FileError {
            kind: "Io",
            message: format!("file task failed: {err}"),
        })?
        .map_err(FileError::from);
    }
    blocking(&state, move |config, db| {
        service::read_text(&self::root(config, db, &root)?, &rel, MAX_READ_BYTES)
    })
    .await
}

/// Writes a text file (≤ 16 MiB, `MAX_WRITE_BYTES`) and returns its new version. With
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

/// Every file of a root by path, for quick open (W8): `.gitignore`d,
/// hidden and build folders left out, at most 50 000.
#[tauri::command]
pub async fn file_index(
    state: State<'_, CoreState>,
    root: String,
) -> Result<service::FileIndex, FileError> {
    blocking(&state, move |config, db| {
        service::index_files(&self::root(config, db, &root)?)
    })
    .await
}

/// Files with matches handed to the page at once, at most.
const SEARCH_BATCH_FILES: usize = 32;
/// Longest a found file waits before its batch goes out anyway.
const SEARCH_BATCH_WAIT: Duration = Duration::from_millis(50);

/// Searches reading files at the same time, at most; more wait their turn.
const MAX_RUNNING_SEARCHES: usize = 3;

/// The running project searches (ED5.7, T14), one per search field of the
/// page (`owner`): a new query from the same field stops the one before.
/// At most [`MAX_RUNNING_SEARCHES`] read files at once, so a burst of
/// searches (the webview is not trusted to pace itself) cannot take every
/// blocking thread the other file commands need.
pub struct Searches {
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
    slots: tokio::sync::Semaphore,
}

impl Default for Searches {
    fn default() -> Self {
        Self {
            running: Mutex::default(),
            slots: tokio::sync::Semaphore::new(MAX_RUNNING_SEARCHES),
        }
    }
}

impl Searches {
    /// Registers a search for `owner`, stopping the one it replaces.
    fn start(&self, owner: &str) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut running = self
            .running
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(old) = running.insert(owner.to_owned(), cancel.clone()) {
            old.store(true, Ordering::Relaxed);
        }
        cancel
    }

    /// Forgets `owner`'s search once it ended — unless a newer one took its place.
    fn finish(&self, owner: &str, cancel: &Arc<AtomicBool>) {
        let mut running = self
            .running
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if running.get(owner).is_some_and(|c| Arc::ptr_eq(c, cancel)) {
            running.remove(owner);
        }
    }

    fn cancel(&self, owner: &str) {
        let running = self
            .running
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(cancel) = running.get(owner) {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

/// One message on a search's channel: the next files with matches.
#[derive(Debug, Serialize)]
pub struct SearchBatch {
    files: Vec<FileMatches>,
}

/// Searches the files of `root` (T14): matches stream in batches on
/// `on_batch`, the summary comes back when the search ends. `owner` names
/// the search field asking — its previous search, if still running, stops.
#[tauri::command]
pub async fn file_search(
    state: State<'_, CoreState>,
    searches: State<'_, Searches>,
    root: String,
    query: SearchQuery,
    owner: String,
    on_batch: Channel<SearchBatch>,
) -> Result<SearchSummary, FileError> {
    let cancel = searches.start(&owner);
    let flag = cancel.clone();
    // Waiting for a slot; a newer query from the same field may stop this one meanwhile.
    let Ok(_slot) = searches.slots.acquire().await else {
        return Err(FileError {
            kind: "Io",
            message: "the search service is shutting down".into(),
        });
    };
    if cancel.load(Ordering::Relaxed) {
        searches.finish(&owner, &cancel);
        return Ok(SearchSummary {
            cancelled: true,
            ..SearchSummary::default()
        });
    }
    let result = blocking(&state, move |config, db| {
        let root = self::root(config, db, &root)?;
        let mut batch = Vec::new();
        let mut sent = Instant::now();
        let flush = |batch: &mut Vec<FileMatches>| {
            let files = std::mem::take(batch);
            // The page is gone or navigated away: nobody to stop, the flag is set by the next query.
            let _ = on_batch.send(SearchBatch { files });
        };
        let summary = service::search(&root, &query, &flag, |found| {
            batch.push(found);
            if batch.len() >= SEARCH_BATCH_FILES || sent.elapsed() >= SEARCH_BATCH_WAIT {
                flush(&mut batch);
                sent = Instant::now();
            }
        })?;
        if !batch.is_empty() {
            flush(&mut batch);
        }
        Ok(summary)
    })
    .await;
    searches.finish(&owner, &cancel);
    result
}

/// Stops `owner`'s running search (the field was cleared or closed).
#[tauri::command]
pub fn file_search_cancel(searches: State<'_, Searches>, owner: String) {
    searches.cancel(&owner);
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
    // A language server's file outside every root is read once, never watched (L11).
    if root.starts_with(crate::lsp::FOREIGN_ROOT) {
        return Ok(());
    }
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

// ---------------------------------------------------------------- projects
//
// A project is a folder with a row in the IDE's `projects` table (M7.1) — the
// one registry the editor and the IDE share. The folder always comes from the
// native dialog, driven from here, never as a path the webview typed.

fn ide_error(err: axiomata_core::ide::IdeError) -> FileError {
    FileError {
        kind: "Invalid",
        message: err.to_string(),
    }
}

/// Opens the native folder dialog; `None` if it was cancelled.
async fn pick_folder_path(app: &AppHandle, title: &str) -> Option<PathBuf> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title(title)
        .pick_folder(move |picked| {
            let _ = tx.send(picked.and_then(|p| p.into_path().ok()));
        });
    rx.await.ok().flatten()
}

/// "Open project": the user picks a folder; it becomes (or already is) a
/// project and is marked as just opened. `None` if the dialog was cancelled.
#[tauri::command]
pub async fn project_open(
    app: AppHandle,
    state: State<'_, CoreState>,
) -> Result<Option<axiomata_core::ide::Project>, FileError> {
    let Some(path) = pick_folder_path(&app, "Open project").await else {
        return Ok(None);
    };
    blocking(&state, move |_, db| {
        let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
        axiomata_core::ide::store::open_root(&db, &path)
            .map(Some)
            .map_err(|err| FilesError::Refused {
                path: path.clone(),
                reason: err.to_string(),
            })
    })
    .await
}

/// "New project": the user picks the parent folder, `name` becomes a new
/// folder in it (optionally a git repository), and that folder is opened as a
/// project. `None` if the dialog was cancelled.
#[tauri::command]
pub async fn project_new(
    app: AppHandle,
    state: State<'_, CoreState>,
    name: String,
    git_init: bool,
) -> Result<Option<axiomata_core::ide::Project>, FileError> {
    // Checked before the dialog so a bad name does not cost a pick.
    axiomata_core::ide::newproject::check_folder_name(&name).map_err(ide_error)?;
    let Some(parent) = pick_folder_path(&app, "Where should the new project go?").await else {
        return Ok(None);
    };
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let folder =
            axiomata_core::ide::newproject::create_project_folder(&parent, &name, git_init)
                .map_err(ide_error)?;
        let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
        match axiomata_core::ide::store::open_root(&db, &folder) {
            Ok(project) => Ok(Some(project)),
            Err(err) => {
                // Nothing of ours is in there but what we just made.
                let _ = std::fs::remove_dir_all(&folder);
                Err(ide_error(err))
            }
        }
    })
    .await
    .map_err(|err| FileError {
        kind: "Io",
        message: format!("project task failed: {err}"),
    })?
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
