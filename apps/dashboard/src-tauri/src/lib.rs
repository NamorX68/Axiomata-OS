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
            commands::sync_memory,
            commands::get_memory_status,
        ])
        .setup(|app| {
            // Initializes `~/.axiomata` (config, database, logs, global
            // skills) and the Second-Brain workspace root on every start.
            // Managed as app state so the command handlers reach the same
            // config/database without re-initializing. `AxiomataCore` locks
            // only its `db` field internally, so no outer `Mutex` is needed.
            let core =
                AxiomataCore::init().expect("failed to initialize the Axiomata-OS core engine");
            // Best-effort: bring the memory router up to date on launch. A
            // failure here (e.g. a hand-mangled CLAUDE.md) must not block start.
            if let Err(err) = axiomata_core::memory::sync(&core.config) {
                eprintln!("startup memory sync failed: {err}");
            }
            // Reactive stale hint for the memory panel. Degrades to the
            // status()-based mtime check if the OS watcher can't start.
            let watcher = axiomata_core::memory::MemoryWatcher::start(&core.config.workspace_root);
            if !watcher.is_active() {
                eprintln!("memory watcher could not start; falling back to poll-based staleness");
            }
            app.manage(core);
            app.manage(watcher);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
