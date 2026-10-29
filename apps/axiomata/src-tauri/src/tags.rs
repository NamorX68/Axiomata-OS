//! Tags and links (`docs/plans/tags-links.md`): the tag list of Settings → Workspace → Tags, the proposals waiting for
//! the owner, renaming a tag, and pulling the `[[links]]` along when a note is renamed (V3, called by the file app).
//! Every command reads or writes vault files through the core's workspace guard, so each runs on a blocking thread:
//! a vault iCloud is busy with must not freeze the window. The app is the owner here — only these commands answer a
//! proposal or write `TAGS.md`.

use std::sync::Arc;

use axiomata_core::tags::{
    self,
    list::TagEntry,
    proposals::{Answered, TagProposal},
    rename::RenameCount,
};
use tauri::State;

use crate::commands::CoreState;

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, axiomata_core::AxiomataError> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|err| format!("the tag task failed: {err}"))?
        .map_err(|err| err.to_string())
}

/// The vault's tag list (`TAGS.md`, written with the news skills' two tags first if the vault has none).
#[tauri::command]
pub async fn get_tag_list(state: State<'_, CoreState>) -> Result<Vec<TagEntry>, String> {
    let config = state.config_read().clone();
    blocking(move || tags::list::load(&config)).await
}

/// Saves the owner's list as it stands in the settings: checked, sorted, written to `TAGS.md`. A tag left out is off
/// the list; the notes keep it.
#[tauri::command]
pub async fn save_tag_list(
    state: State<'_, CoreState>,
    entries: Vec<TagEntry>,
) -> Result<Vec<TagEntry>, String> {
    let config = state.config_read().clone();
    blocking(move || tags::list::save(&config, entries)).await
}

/// The proposals waiting for the owner, oldest first.
#[tauri::command]
pub async fn tag_proposals(state: State<'_, CoreState>) -> Result<Vec<TagProposal>, String> {
    let db = Arc::clone(&state.db);
    blocking(move || {
        let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
        tags::proposals::open(&db)
    })
    .await
}

/// The owner's answer to a proposal: accepted, the tag goes onto the list (with `description` when given) and onto
/// the notes that wanted it.
#[tauri::command]
pub async fn answer_tag_proposal(
    state: State<'_, CoreState>,
    id: i64,
    accept: bool,
    description: Option<String>,
) -> Result<Answered, String> {
    let db = Arc::clone(&state.db);
    let config = state.config_read().clone();
    blocking(move || tags::proposals::answer(&db, &config, id, accept, description.as_deref()))
        .await
}

/// Renames a tag in every note and on the list; with `dry_run`, only counts what it would change.
#[tauri::command]
pub async fn rename_tag(
    state: State<'_, CoreState>,
    old: String,
    new: String,
    dry_run: bool,
) -> Result<RenameCount, String> {
    let config = state.config_read().clone();
    blocking(move || tags::rename::rename_tag(&config, &old, &new, dry_run)).await
}

/// Rewrites the `[[links]]` to the note `old_rel` in every note so they point to `new_rel` (V3); with `dry_run`, only
/// counts them. Works before or after the file itself was renamed.
#[tauri::command]
pub async fn rename_note_links(
    state: State<'_, CoreState>,
    old_rel: String,
    new_rel: String,
    dry_run: bool,
) -> Result<RenameCount, String> {
    let config = state.config_read().clone();
    blocking(move || tags::rename::rename_note_links(&config, &old_rel, &new_rel, dry_run)).await
}
