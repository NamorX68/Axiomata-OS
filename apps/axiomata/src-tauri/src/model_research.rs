//! Starts the research of model tiers (`docs/plans/agents-model.md`, AM2): after every catalog refresh when switched-on
//! models wait for a research proposal (`model_research::claim_due`, at most once a day per model), and on the owner's
//! "propose again" (`research_model_tiers`). Each run is the bundled skill on the ordinary skill path — the spend
//! guard, a recorded run, the skill harness — and one at a time.

use std::sync::atomic::{AtomicBool, Ordering};

use axiomata_core::AxiomataCore;
use axiomata_core::model_catalog::{self, ModelCatalog};
use axiomata_core::{model_research, skills};
use tauri::{AppHandle, Emitter, Manager, State};

/// A research run ended; the Models tab reads the proposals again.
pub const MODEL_TIERS_UPDATED: &str = "models:tiers-updated";

/// Whether a research run is under way in this app. One at a time: two would rate the same models twice.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// After a catalog refresh: starts a research run when models are due, else does nothing (one read of a small file).
pub fn after_refresh(app: &AppHandle, catalog: &ModelCatalog) {
    if RUNNING.load(Ordering::SeqCst) {
        return;
    }
    let config = app.state::<AxiomataCore>().config_read().clone();
    match model_research::claim_due(&config, catalog, chrono::Utc::now()) {
        Ok(keys) if !keys.is_empty() => {
            tracing::info!(models = ?keys, "starting the research of model tiers");
            start(app);
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(%err, "could not look for models whose tier waits for research"),
    }
}

/// Starts one run of the research skill in the background. `false` when one is under way already.
fn start(app: &AppHandle) -> bool {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return false;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let core = app.state::<AxiomataCore>();
        let config = core.config_read().clone();
        let run = skills::execute_and_record_skill(model_research::SKILL, &config, &core.db).await;
        RUNNING.store(false, Ordering::SeqCst);
        match run {
            Ok(record) => {
                tracing::info!(status = ?record.status, "the research of model tiers ended")
            }
            Err(err) => tracing::warn!(%err, "the research of model tiers did not run"),
        }
        if let Err(err) = app.emit(MODEL_TIERS_UPDATED, ()) {
            tracing::warn!(%err, "could not tell the frontend about new tier proposals");
        }
    });
    true
}

/// The owner's "propose again" (AM3): forgets the proposals of `keys` (every switched-on model when `None`) and starts
/// a research run for them. Returns the models asked for; the run reports through [`MODEL_TIERS_UPDATED`].
#[tauri::command]
pub fn research_model_tiers(
    app: AppHandle,
    state: State<'_, AxiomataCore>,
    keys: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    if RUNNING.load(Ordering::SeqCst) {
        return Err(
            "a research of model tiers is under way; ask again when it has ended".to_owned(),
        );
    }
    let config = state.config_read().clone();
    let asked = model_research::request(
        &config,
        &model_catalog::load(),
        keys.as_deref(),
        chrono::Utc::now(),
    )
    .map_err(|err| err.to_string())?;
    if !asked.is_empty() && !start(&app) {
        return Err(
            "a research of model tiers is under way; the models are asked for and wait for the next run".to_owned(),
        );
    }
    Ok(asked)
}
