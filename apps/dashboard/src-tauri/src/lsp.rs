//! Tauri glue for the editor's language servers (`docs/plans/editor.md`, ED6).
//!
//! The host lives in `axiomata-files::lsp` and decides which program runs;
//! this file only translates IPC: the page names a root id and a language,
//! never a program or a path. A server's messages reach the page through the
//! `Channel` it passed to [`lsp_start`]; the page's messages go back through
//! [`lsp_send`] as JSON text. Folder roots only — a dialog grant of a single
//! file gets no server (L4), and neither does any `grant:` root, whose folder
//! the owner picked for editing, not for a toolchain to index.

use std::sync::Arc;

use std::path::PathBuf;

use axiomata_files::format::{self, Formatted};
use axiomata_files::lsp::servers::Overrides;
use axiomata_files::lsp::{self, LspHost, Started};
use tauri::State;
use tauri::ipc::Channel;

use crate::commands::CoreState;
use crate::files::{self, FileError};

/// The root id prefix of files outside every root that a server pointed to
/// (`lsp:<handle>`, read-only — `files::file_read`).
pub const FOREIGN_ROOT: &str = "lsp:";

/// The app's one language-server host, and where it and the formatters look
/// for programs and overrides.
pub struct LspState(Arc<LspHost>, Vec<PathBuf>, PathBuf);

impl LspState {
    /// The host, for the file commands that read a server's foreign files.
    pub fn host(&self) -> Arc<LspHost> {
        Arc::clone(&self.0)
    }

    /// A host reading `~/.axiomata/lsp.json` and looking on `PATH` and in the usual install places.
    pub fn new() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let search =
            lsp::servers::search_path(std::env::var_os("PATH").as_deref(), home.as_deref());
        let overrides = lsp::overrides_path(&axiomata_core::paths::axiomata_home());
        Self(
            Arc::new(LspHost::new(overrides.clone(), search.clone())),
            search,
            overrides,
        )
    }
}

/// Formats `text` — the content of `rel` in `root` — with its language's
/// formatter (editor plan L13); Rust picks the program. `None` (and `Missing`)
/// tell the page to ask the language server instead. The override file is
/// read on every call, so an edit to it applies at once.
#[tauri::command]
pub async fn file_format(
    state: State<'_, CoreState>,
    lsp: State<'_, LspState>,
    root: String,
    rel: String,
    language: String,
    text: String,
) -> Result<Formatted, FileError> {
    if text.len() > axiomata_files::format::MAX_OUTPUT_BYTES {
        return Ok(Formatted::None);
    }
    let search = lsp.1.clone();
    let overrides_path = lsp.2.clone();
    files::blocking(&state, move |config, db| {
        let found = files::root(config, db, &root)?;
        // An unreadable override file formats with the table, as the servers start with it.
        let overrides = Overrides::load(&overrides_path).unwrap_or_default();
        format::format(&found, &rel, &language, &text, &overrides, &search)
    })
    .await
}

/// Starts the server for `language` in `root` — or returns the one this
/// `page` already runs there; a reloaded page (a new `page` token) restarts
/// it. Its messages arrive on the channel the page passed first.
#[tauri::command]
pub async fn lsp_start(
    state: State<'_, CoreState>,
    lsp: State<'_, LspState>,
    root: String,
    language: String,
    page: String,
    on_message: Channel<String>,
) -> Result<Started, FileError> {
    if root.starts_with("grant:") {
        return Ok(Started::None);
    }
    let host = Arc::clone(&lsp.0);
    files::blocking(&state, move |config, db| {
        let found = files::root(config, db, &root)?;
        let sink: lsp::Sink = Arc::new(move |message: String| {
            // The page is gone or navigated away; the next start replaces the server.
            let _ = on_message.send(message);
        });
        host.start(&root, &found, &language, &page, sink)
    })
    .await
}

/// Sends one JSON message from `page` to a server it started; only the methods
/// the editor speaks pass (`axiomata_files::lsp::ALLOWED_METHODS`).
#[tauri::command]
pub fn lsp_send(
    lsp: State<'_, LspState>,
    handle: u64,
    page: String,
    message: String,
) -> Result<(), FileError> {
    lsp.0.send(handle, &page, &message).map_err(FileError::from)
}

/// A document was opened on the server (it stays running while any is).
#[tauri::command]
pub fn lsp_opened(lsp: State<'_, LspState>, handle: u64, page: String) {
    lsp.0.opened(handle, &page);
}

/// A document was closed on the server; with none left, its idle time starts.
#[tauri::command]
pub fn lsp_closed(lsp: State<'_, LspState>, handle: u64, page: String) {
    lsp.0.closed(handle, &page);
}
