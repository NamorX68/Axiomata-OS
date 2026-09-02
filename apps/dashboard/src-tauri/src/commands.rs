//! Tauri command handlers exposed to the dashboard frontend.
//!
//! The managed state is the `AxiomataCore` itself: `config` is read directly
//! (no lock — it is never mutated at runtime), and only `core.db` sits behind a
//! `Mutex`. `run_skill` calls `execute_and_record_skill`, which runs the agent
//! with no lock held and takes the database lock only to write the result — so
//! a `MutexGuard` is never held across an `.await`.

use axiomata_core::AxiomataCore;
use axiomata_core::memory::{self, MemoryStatus, SyncReport};
use axiomata_core::skills::{self, RunRecord, RunSummary, Skill};
use tauri::State;

/// The Tauri-managed core engine.
pub type CoreState = AxiomataCore;

/// Lists every discovered skill (`~/.axiomata/skills/`).
#[tauri::command]
pub fn list_skills() -> Result<Vec<Skill>, String> {
    skills::list_skills().map_err(|err| err.to_string())
}

/// Returns the most recent skill runs as slim summaries, newest first. `limit`
/// is clamped to `skills::MAX_RUN_LIMIT` in the core.
#[tauri::command]
pub fn list_runs(state: State<'_, CoreState>, limit: usize) -> Result<Vec<RunSummary>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    skills::list_runs(&db, limit).map_err(|err| err.to_string())
}

/// Returns one full run (with captured output) by id, or `null` if unknown.
#[tauri::command]
pub fn get_run(state: State<'_, CoreState>, id: i64) -> Result<Option<RunRecord>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    skills::get_run(&db, id).map_err(|err| err.to_string())
}

/// Runs a skill by name and returns the persisted run record.
#[tauri::command]
pub async fn run_skill(state: State<'_, CoreState>, name: String) -> Result<RunRecord, String> {
    skills::execute_and_record_skill(&name, &state.config, &state.db)
        .await
        .map_err(|err| err.to_string())
}

/// Regenerates the workspace router `CLAUDE.md` blocks. Reads no database.
#[tauri::command]
pub fn sync_memory(state: State<'_, CoreState>) -> Result<SyncReport, String> {
    memory::sync(&state.config).map_err(|err| err.to_string())
}

/// Reports whether the memory router is stale — a plain walk-and-compare.
#[tauri::command]
pub fn get_memory_status(state: State<'_, CoreState>) -> Result<MemoryStatus, String> {
    memory::status(&state.config).map_err(|err| err.to_string())
}
