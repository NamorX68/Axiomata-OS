//! Keeps the providers' model catalogs and prices fresh (board card #17): one refresh at start, then once a day —
//! the app is rarely quit, so a start-only refresh would let the prices go stale for weeks. No agent turn: two HTTP
//! requests, nothing billed.

use std::time::Duration;

use axiomata_core::AxiomataCore;
use axiomata_core::model_catalog::{self, RefreshOutcome};
use tauri::{AppHandle, Emitter, Manager};

/// The catalog was refreshed; the payload is the [`RefreshOutcome`].
pub const MODEL_CATALOG_UPDATED: &str = "models:catalog-updated";

/// Between two refreshes. Prices on OpenRouter move rarely; a day keeps the meter honest without polling.
const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

/// After a failed fetch (offline at start, Wi-Fi not up yet, Ollama not running): try again soon, not in a day.
const RETRY: Duration = Duration::from_secs(10 * 60);

/// Starts the refreshing task. It lives as long as the app does.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let core = app.state::<AxiomataCore>();
            let outcome = model_catalog::refresh_and_price(&core.config, None).await;
            announce(&app, &outcome);
            let wait = if outcome.any_failed() {
                RETRY
            } else {
                INTERVAL
            };
            tokio::time::sleep(wait).await;
        }
    });
}

/// Logs what changed, tells an open Settings dialog to redraw its lists, and starts the tier research of new models.
pub fn announce(app: &AppHandle, outcome: &RefreshOutcome) {
    if !outcome.repriced.is_empty() {
        tracing::info!(models = ?outcome.repriced, "model prices updated from the provider catalog");
    }
    if let Some(err) = &outcome.price_error {
        tracing::warn!(%err, "could not write the refreshed model prices");
    }
    if let Err(err) = app.emit(MODEL_CATALOG_UPDATED, outcome) {
        tracing::warn!(%err, "could not tell the frontend about the refreshed model catalog");
    }
    // Models that came with this refresh and wait for a tier start their research (agents-model.md, AM2).
    crate::model_research::after_refresh(app, &outcome.catalog);
}
