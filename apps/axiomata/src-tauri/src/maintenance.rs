//! Settings → Storage (`docs/plans/maintenance.md`, MT4): the report, "Clean up now", the rules and the "Delete" of
//! an orphaned worktree. Every command walks the disk or runs git and Glut, so each runs on a blocking thread — the
//! window never waits, and the database is held only inside the core while rows are counted or deleted.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axiomata_core::config::{Config, MaintenanceSettings};
use axiomata_core::maintenance::{self, Report, glut::GlutRow};
use serde::Serialize;
use tauri::State;

use crate::commands::CoreState;

/// What the Storage section shows: Axiomata's report, Glut's row (absent without a Glut that can answer) and the
/// rules in force.
#[derive(Debug, Serialize)]
pub struct StorageView {
    axiomata: Report,
    glut: Option<GlutRow>,
    settings: MaintenanceSettings,
}

/// What "Clean up now" did, and the state after it.
#[derive(Debug, Serialize)]
pub struct CleanOutcome {
    /// Bytes freed: Axiomata's and, when Glut cleaned too, Glut's.
    freed: u64,
    /// Why Glut did not clean, when there is a Glut that should have (its failure line, another format).
    glut_note: Option<String>,
    /// The fresh view after the clean-up.
    view: StorageView,
}

fn settings_of(state: &CoreState) -> MaintenanceSettings {
    state.config_read().maintenance.clone()
}

/// Axiomata's report and Glut's at the same time: Glut walks its own folder, which may take a moment.
fn measure(db: &Mutex<rusqlite::Connection>, settings: MaintenanceSettings) -> StorageView {
    std::thread::scope(|scope| {
        let glut = scope.spawn(maintenance::glut::report);
        let axiomata = maintenance::clean_with(db, &settings, true);
        StorageView {
            axiomata,
            glut: glut.join().ok().flatten(),
            settings,
        }
    })
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|err| format!("the storage task failed: {err}"))
}

/// Sizes, rules, the last clean-up and what a clean-up would free. Deletes nothing.
#[tauri::command]
pub async fn maintenance_report(state: State<'_, CoreState>) -> Result<StorageView, String> {
    let db = Arc::clone(&state.db);
    let settings = settings_of(&state);
    blocking(move || measure(&db, settings)).await
}

/// "Clean up now": Axiomata's clean-up by the saved rules, then Glut's own (`glut clean`) when there is one that can —
/// only after Axiomata's ran, so a refused pass cleans nothing anywhere. The page asked first and showed what would go.
#[tauri::command]
pub async fn maintenance_clean(state: State<'_, CoreState>) -> Result<CleanOutcome, String> {
    let db = Arc::clone(&state.db);
    let settings = settings_of(&state);
    blocking(move || {
        let axiomata = maintenance::clean_with(&db, &settings, false);
        if let Some(error) = &axiomata.error {
            return Err(error.clone());
        }
        let (glut_freed, glut_note) = match maintenance::glut::clean() {
            Some(GlutRow::Known { report }) if !report.dry_run => (report.freeable_bytes, None),
            Some(GlutRow::Known { .. }) => {
                (0, Some("Glut answered with a dry run only".to_owned()))
            }
            Some(GlutRow::Failed { message }) => (0, Some(message)),
            Some(GlutRow::UnknownFormat) => (0, Some("Glut: unknown format".to_owned())),
            None => (0, None),
        };
        Ok(CleanOutcome {
            freed: axiomata.freeable_bytes + glut_freed,
            glut_note,
            view: measure(&db, settings),
        })
    })
    .await?
}

/// Saves the rules (`[maintenance]`): checked first, written into the file config — never by saving the live config,
/// which would undo a workspace change queued for the next start — and then taken over by the live one.
#[tauri::command]
pub async fn maintenance_save_settings(
    state: State<'_, CoreState>,
    settings: MaintenanceSettings,
) -> Result<MaintenanceSettings, String> {
    settings.validate()?;
    let config = Arc::clone(&state.config);
    let to_save = settings.clone();
    blocking(move || -> Result<(), String> {
        let mut on_disk = Config::load().map_err(|err| err.to_string())?;
        on_disk.maintenance = to_save.clone();
        on_disk.save().map_err(|err| err.to_string())?;
        config
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
            .maintenance = to_save;
        Ok(())
    })
    .await??;
    Ok(settings)
}

/// Deletes the orphaned worktree at `path`, changes and all — after the page asked. Only a path the sweep finds right
/// now is accepted, never one the webview made up (`maintenance::remove_orphan_worktree`).
#[tauri::command]
pub async fn maintenance_remove_worktree(
    state: State<'_, CoreState>,
    path: String,
) -> Result<(), String> {
    let db = Arc::clone(&state.db);
    blocking(move || maintenance::remove_orphan_worktree(&db, &PathBuf::from(path))).await?
}
