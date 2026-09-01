//! Tauri command handlers exposed to the dashboard frontend.
//!
//! All state lives behind a single `Mutex<AxiomataCore>` managed in
//! `lib.rs::run`. The lock is only ever held for synchronous work; `run_skill`
//! deliberately clones the config, releases the lock, runs the agent, and
//! re-takes the lock just to persist — so a `std::sync::MutexGuard` is never
//! held across an `.await`.

use std::sync::Mutex;

use axiomata_core::AxiomataCore;
use axiomata_core::skills::{self, RunRecord, RunSummary, Skill};
use tauri::State;

/// Core engine wrapped for Tauri-managed state.
pub type CoreState = Mutex<AxiomataCore>;

/// Lists every discovered skill (`~/.axiomata/skills/`).
#[tauri::command]
pub fn list_skills() -> Result<Vec<Skill>, String> {
    skills::list_skills().map_err(|err| err.to_string())
}

/// Returns the most recent skill runs as slim summaries, newest first. `limit`
/// is clamped to `skills::MAX_RUN_LIMIT` in the core.
#[tauri::command]
pub fn list_runs(state: State<'_, CoreState>, limit: usize) -> Result<Vec<RunSummary>, String> {
    let core = state.lock().map_err(|err| err.to_string())?;
    skills::list_runs(&core.db, limit).map_err(|err| err.to_string())
}

/// Returns one full run (with captured output) by id, or `null` if unknown.
#[tauri::command]
pub fn get_run(state: State<'_, CoreState>, id: i64) -> Result<Option<RunRecord>, String> {
    let core = state.lock().map_err(|err| err.to_string())?;
    skills::get_run(&core.db, id).map_err(|err| err.to_string())
}

/// Runs a skill by name and returns the persisted run record.
///
/// The run itself (which awaits an agent process or HTTP call) happens with no
/// lock held; the lock is taken only to read the config up front and to write
/// the result at the end.
#[tauri::command]
pub async fn run_skill(state: State<'_, CoreState>, name: String) -> Result<RunRecord, String> {
    let config = {
        let core = state.lock().map_err(|err| err.to_string())?;
        core.config.clone()
    };

    let record = skills::execute_skill(&name, &config)
        .await
        .map_err(|err| err.to_string())?;

    let core = state.lock().map_err(|err| err.to_string())?;
    skills::runlog::record_run(&core.db, record).map_err(|err| err.to_string())
}
