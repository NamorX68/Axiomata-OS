use axiomata_core::routines::SchedulerHandle;
use tauri::Manager;

mod bootstrap;
mod commands;

/// Initializes `tracing`'s output so `axiomata_core`'s `tracing::info!`/
/// `warn!` calls (the routine scheduler's tick/reconcile summaries, in
/// particular) actually go somewhere — `tracing-subscriber` was a declared
/// workspace dependency that nothing ever called `.init()` on. Defaults to
/// `info`; override with `RUST_LOG` (e.g. `RUST_LOG=debug`).
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Persist the window's size / position / maximized state across
        // restarts (written to `window-state.json` in the OS app-config dir,
        // restored when the window is created).
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::SIZE
                        | tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED,
                )
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_dashboard_state,
            commands::save_dashboard_state,
            commands::read_workspace_file,
            commands::write_workspace_file,
            commands::create_note,
            commands::assistant_send,
            commands::write_module_manifest,
            commands::poll_module_actions,
            commands::complete_module_action,
            commands::load_custom_css,
            commands::get_workspace_graph,
            commands::search_workspace,
            commands::list_skills,
            commands::list_skipped_skills,
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
            // Core init, the startup memory sync, and the routine scheduler —
            // see `bootstrap` for why this is one call instead of `.setup()`
            // inlining all three.
            let services = bootstrap::bootstrap();
            app.manage(services.core);
            app.manage(services.scheduler);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the tauri application");

    app.run(|app_handle, event| {
        // `Drop` alone cannot be relied on to run the scheduler's shutdown:
        // Tauri's default exit path can end the process without unwinding
        // (see the M3 review finding this fixes). Request the stop
        // explicitly here instead, as soon as an exit is requested.
        if let tauri::RunEvent::ExitRequested { .. } = event
            && let Some(scheduler) = app_handle.try_state::<SchedulerHandle>()
        {
            scheduler.shutdown();
        }
    });
}
