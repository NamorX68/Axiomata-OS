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
    vault_run,
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
    let db = Arc::clone(&state.db);
    blocking(move || {
        let saved = tags::list::save(&config, entries)?;
        settle(&db, &saved);
        Ok(saved)
    })
    .await
}

/// Closes the proposals of tags the owner has put on the list himself; a failure only leaves the badge lit.
fn settle(db: &std::sync::Mutex<rusqlite::Connection>, list: &[TagEntry]) {
    let tags: Vec<String> = list.iter().map(|entry| entry.tag.clone()).collect();
    let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Err(err) = tags::proposals::settle_listed(&db, &tags) {
        tracing::warn!("tag proposals not settled: {err}");
    }
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

/// Renames a tag in every note and on the list (merging into a listed tag of the new name); with `dry_run`, only
/// counts what it would change.
#[tauri::command]
pub async fn rename_tag(
    state: State<'_, CoreState>,
    old: String,
    new: String,
    dry_run: bool,
) -> Result<RenameCount, String> {
    let config = state.config_read().clone();
    let db = Arc::clone(&state.db);
    blocking(move || {
        let renamed = tags::rename::rename_tag(&config, &old, &new, None, dry_run)?;
        if !dry_run {
            settle(&db, &tags::list::load(&config)?);
        }
        Ok(renamed.count())
    })
    .await
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

/// What the vault run's section shows (`docs/plans/tags-links.md` Q9, TL6): the step under way, the estimate of each
/// step before it is started, and the preview of the notes step.
#[derive(Debug, serde::Serialize)]
pub struct VaultTagging {
    phase: Option<vault_run::Phase>,
    list_estimate: vault_run::Estimate,
    notes_estimate: vault_run::Estimate,
    plan: Vec<vault_run::PlannedNote>,
}

/// The vault run as it stands.
#[tauri::command]
pub async fn vault_tagging_status(state: State<'_, CoreState>) -> Result<VaultTagging, String> {
    let db = Arc::clone(&state.db);
    let config = state.config_read().clone();
    blocking(move || {
        let notes = vault_run::taggable_notes(&config)?;
        let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
        let plan = vault_run::plan(&db)?;
        // The notes step goes on where a plan stopped: its estimate counts only the notes still without one.
        let planned: std::collections::HashSet<String> =
            plan.iter().map(|entry| entry.note.to_lowercase()).collect();
        let left: Vec<vault_run::NoteSize> = notes
            .iter()
            .filter(|note| !planned.contains(&note.path.to_lowercase()))
            .cloned()
            .collect();
        Ok(VaultTagging {
            phase: vault_run::phase(&db)?,
            list_estimate: vault_run::estimate(&config, vault_run::Phase::List, &notes),
            notes_estimate: vault_run::estimate(&config, vault_run::Phase::Notes, &left),
            plan,
        })
    })
    .await
}

/// Starts a step of the vault run — the owner confirmed its estimate — and waits for the skill: `list` proposes the
/// tag list, `notes` plans every note into the preview. The step is over when the run ends, however it ended; the plan
/// stays for the preview.
#[tauri::command]
pub async fn vault_tagging_start(
    state: State<'_, CoreState>,
    phase: String,
) -> Result<axiomata_core::skills::RunRecord, String> {
    let phase = vault_run::Phase::parse(&phase).map_err(|err| err.to_string())?;
    let config = state.config_read().clone();
    {
        let db = state.db_lock();
        if vault_run::phase(&db)
            .map_err(|err| err.to_string())?
            .is_some()
        {
            return Err("a vault run is under way already".to_owned());
        }
        vault_run::set_phase(&db, Some(phase)).map_err(|err| err.to_string())?;
    }
    let run =
        axiomata_core::skills::execute_and_record_skill(vault_run::SKILL, &config, &state.db).await;
    let _ = vault_run::set_phase(&state.db_lock(), None);
    run.map_err(|err| err.to_string())
}

/// Writes the preview into the notes — the owner's step — and clears it.
#[tauri::command]
pub async fn vault_tagging_apply(
    state: State<'_, CoreState>,
) -> Result<vault_run::Applied, String> {
    let db = Arc::clone(&state.db);
    let config = state.config_read().clone();
    blocking(move || vault_run::apply(&config, &db)).await
}

/// Throws the preview away.
#[tauri::command]
pub async fn vault_tagging_discard(state: State<'_, CoreState>) -> Result<(), String> {
    let db = Arc::clone(&state.db);
    blocking(move || {
        let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
        vault_run::discard(&db)
    })
    .await
}

/// Ends a step left marked as under way — the app closed during a run — and keeps the preview.
#[tauri::command]
pub async fn vault_tagging_end(state: State<'_, CoreState>) -> Result<(), String> {
    let db = Arc::clone(&state.db);
    blocking(move || {
        let db = db.lock().unwrap_or_else(|poison| poison.into_inner());
        vault_run::set_phase(&db, None)
    })
    .await
}
