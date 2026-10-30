//! Starts the research of model tiers (`docs/plans/agents-model.md`, AM2): after every catalog refresh when switched-on
//! models wait for a research proposal (`model_research::claim_due`, at most once a day per model), and on the owner's
//! "propose again" (`research_model_tiers`). Each run is the bundled skill on the ordinary skill path — the spend
//! guard, a recorded run, the skill harness — and one at a time.

use std::sync::atomic::{AtomicBool, Ordering};

use axiomata_core::AxiomataCore;
use axiomata_core::model_catalog::{self, ModelCatalog};
use axiomata_core::model_research::{self, Marked};
use axiomata_core::skills;
use tauri::{AppHandle, Emitter, Manager, State};

/// A research run ended; the Models tab reads the proposals again.
pub const MODEL_TIERS_UPDATED: &str = "models:tiers-updated";

/// Whether a research run is under way in this app. One at a time: two would rate the same models twice.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Holds [`RUNNING`] for one run and lets go of it when dropped — also when the run's task panics.
struct Running;

impl Running {
    /// `None` when a run is under way already.
    fn claim() -> Option<Self> {
        (!RUNNING.swap(true, Ordering::SeqCst)).then_some(Running)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        RUNNING.store(false, Ordering::SeqCst);
    }
}

/// Whether a research run is under way — the Models tab shows a quiet note while it is.
#[tauri::command]
pub fn model_research_running() -> bool {
    RUNNING.load(Ordering::SeqCst)
}

/// After a catalog refresh: starts a research run when models are due, else does nothing (one read of a small file).
pub fn after_refresh(app: &AppHandle, catalog: &ModelCatalog) {
    if RUNNING.load(Ordering::SeqCst) {
        return;
    }
    let config = app.state::<AxiomataCore>().config_read().clone();
    match model_research::claim_due(&config, catalog, chrono::Utc::now()) {
        Ok(marked) if !marked.is_empty() => {
            tracing::info!(models = ?marked.keys, "starting the research of model tiers");
            if let Err(marked) = start(app, marked) {
                take_back(&marked);
            }
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(%err, "could not look for models whose tier waits for research"),
    }
}

/// Undoes the marks of a run that could not start, so its models are not held back for a day.
fn take_back(marked: &Marked) {
    if let Err(err) = model_research::unmark(marked) {
        tracing::warn!(%err, "could not take back the marks of a research run that did not start");
    }
}

/// Starts one run of the research skill for `marked` in the background; hands `marked` back when one is under way.
fn start(app: &AppHandle, marked: Marked) -> Result<(), Box<Marked>> {
    let Some(running) = Running::claim() else {
        return Err(Box::new(marked));
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _running = running;
        let core = app.state::<AxiomataCore>();
        let config = core.config_read().clone();
        let run = skills::execute_and_record_skill(model_research::SKILL, &config, &core.db).await;
        match run {
            Ok(record) => {
                tracing::info!(status = ?record.status, "the research of model tiers ended")
            }
            Err(err) => tracing::warn!(%err, "the research of model tiers did not run"),
        }
        // What the run left unanswered is no longer asked for; its attempt time keeps it from starting at once.
        if let Err(err) = model_research::finish(&marked) {
            tracing::warn!(%err, "could not close the research of model tiers");
        }
        if let Err(err) = app.emit(MODEL_TIERS_UPDATED, ()) {
            tracing::warn!(%err, "could not tell the frontend about new tier proposals");
        }
    });
    Ok(())
}

/// The owner's "propose again" (AM3): asks for `keys` (every switched-on model when `None`) again and starts a research
/// run for them; their old proposals stay until new ones are recorded. Returns the models asked for; the run reports
/// through [`MODEL_TIERS_UPDATED`].
#[tauri::command]
pub fn research_model_tiers(
    app: AppHandle,
    state: State<'_, AxiomataCore>,
    keys: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    const UNDER_WAY: &str = "a research of model tiers is under way; ask again when it has ended";
    if RUNNING.load(Ordering::SeqCst) {
        return Err(UNDER_WAY.to_owned());
    }
    let config = state.config_read().clone();
    let marked = model_research::request(
        &config,
        &model_catalog::load(),
        keys.as_deref(),
        chrono::Utc::now(),
    )
    .map_err(|err| err.to_string())?;
    let asked = marked.keys.clone();
    if marked.is_empty() {
        return Ok(asked);
    }
    if let Err(marked) = start(&app, marked) {
        take_back(&marked);
        return Err(UNDER_WAY.to_owned());
    }
    Ok(asked)
}
