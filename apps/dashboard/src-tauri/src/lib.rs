use axiomata_core::AxiomataCore;
use tauri::Manager;

mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            commands::list_skills,
            commands::list_runs,
            commands::get_run,
            commands::run_skill,
        ])
        .setup(|app| {
            // Initializes `~/.axiomata` (config, database, logs, global
            // skills) and the Second-Brain workspace root on every start.
            // Managed as app state so the command handlers reach the same
            // config/database without re-initializing. `AxiomataCore` locks
            // only its `db` field internally, so no outer `Mutex` is needed.
            let core =
                AxiomataCore::init().expect("failed to initialize the Axiomata-OS core engine");
            app.manage(core);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
