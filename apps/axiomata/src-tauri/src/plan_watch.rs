//! Runs the plans that run by themselves (`docs/plans/a2a.md`, CP-A8): every few seconds it asks
//! [`axiomata_core::plan_run::PlanRun::tick`] what is to be done — integrate a reviewed card, start a ready one — and
//! tells the frontend what happened through one event, [`PLAN_RUN`], whose payload is the tick's `RunEvent`.
//!
//! The Studio acts on it: a started card's pane is opened in the background (opening it is what starts the harness),
//! the panes of integrated cards are closed, and what needs the owner (a card that could not be started, a plan that
//! is ready to be taken over) is said. Without the app, and without the Studio mounted, nothing starts: a plan runs
//! while it is open.

use std::time::Duration;

use axiomata_core::AxiomataCore;
use axiomata_core::plan_run::PlanRun;
use tauri::{AppHandle, Emitter, Manager};

/// One of the tick's events, as `{ "event": "started" | "integrated" | "blocked" | "ready_to_take_over", … }`.
pub const PLAN_RUN: &str = "plan:run";

/// How often the plans are looked at.
const TICK: Duration = Duration::from_secs(5);

/// Starts the watching task. It lives as long as the app does.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut run = PlanRun::default();
        loop {
            tokio::time::sleep(TICK).await;
            let core = app.state::<AxiomataCore>();
            for event in run.tick(&core).await {
                if let Err(err) = app.emit(PLAN_RUN, &event) {
                    tracing::warn!(%err, "could not tell the frontend what a plan run did");
                }
            }
        }
    });
}
