//! The Tauri shell's composition root.
//!
//! `.setup()` used to inline three independent subsystems — core init, a
//! best-effort startup memory sync, and the routine scheduler — with no
//! shared error handling or lifecycle seam between them. This module pulls
//! that wiring into one place instead.

use std::sync::Arc;

use axiomata_core::AxiomataCore;
use axiomata_core::routines::{self, SchedulerHandle};

/// Everything `.setup()` needs to hand to Tauri as managed state.
pub struct Services {
    pub core: AxiomataCore,
    pub scheduler: SchedulerHandle,
}

/// Initializes the core engine, starts a best-effort startup memory sync on
/// a background thread, and starts the routine scheduler — in that order,
/// each depending only on what came before it.
///
/// # Panics
///
/// Panics if [`AxiomataCore::init`] fails. There is no sensible degraded mode
/// to run the app in without it — config, the database, and the skills
/// directory all come from this step — so this matches `.setup()`'s previous
/// `.expect(...)` behaviour rather than inventing a new failure path here.
pub fn bootstrap() -> Services {
    let core = AxiomataCore::init().expect("failed to initialize the Axiomata-OS core engine");

    // Off the setup thread so a large vault or a slow disk doesn't stall
    // window creation. A failure surfaces as a "stale" badge the user can act
    // on with "Sync now" — it is not fatal to starting the app.
    let sync_config = core.config.clone();
    std::thread::spawn(move || {
        if let Err(err) = axiomata_core::memory::sync(&sync_config) {
            tracing::warn!(%err, "startup memory sync failed");
        }
    });

    // `.setup()` is not itself inside a Tokio runtime, so the loop is handed
    // to `tauri::async_runtime` and only the stop handle is kept here;
    // `lib.rs`'s `RunEvent::ExitRequested` hook uses it to request a stop
    // before the process exits.
    let scheduler = SchedulerHandle::new();
    let stop_rx = scheduler.subscribe();
    tauri::async_runtime::spawn(routines::serve(
        core.config.clone(),
        Arc::clone(&core.db),
        stop_rx,
    ));

    Services { core, scheduler }
}
