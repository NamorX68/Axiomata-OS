use std::sync::Arc;

use axiomata_core::AxiomataCore;
use axiomata_core::routines::{self, SchedulerHandle};
use tauri::Manager;

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::list_skills,
            commands::list_runs,
            commands::get_run,
            commands::run_skill,
            commands::sync_memory,
            commands::get_memory_status,
            commands::list_routines,
            commands::add_routine,
            commands::set_routine_enabled,
            commands::routine_history,
        ])
        .setup(|app| {
            // Initializes `~/.axiomata` (config, database, logs, global
            // skills) and the Second-Brain workspace root on every start.
            // Managed as app state so the command handlers reach the same
            // config/database without re-initializing. `AxiomataCore` locks
            // only its `db` field internally, so no outer `Mutex` is needed.
            let core =
                AxiomataCore::init().expect("failed to initialize the Axiomata-OS core engine");

            // Best-effort router sync on launch, off the setup thread so a
            // large vault or a slow disk doesn't stall window creation. A
            // failure surfaces as a "stale" badge the user can act on with
            // "Sync now".
            let sync_config = core.config.clone();
            std::thread::spawn(move || {
                if let Err(err) = axiomata_core::memory::sync(&sync_config) {
                    eprintln!("startup memory sync failed: {err}");
                }
            });

            // Start the routine scheduler. `.setup()` is not inside a Tokio
            // runtime, so we hand the loop to `tauri::async_runtime` and keep
            // only the stop handle here; dropping it on app exit ends the loop.
            let (scheduler, stop_rx) = SchedulerHandle::channel();
            tauri::async_runtime::spawn(routines::serve(
                core.config.clone(),
                Arc::clone(&core.db),
                stop_rx,
            ));

            app.manage(core);
            app.manage(scheduler);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
