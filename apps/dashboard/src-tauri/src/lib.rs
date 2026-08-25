use std::sync::Mutex;

use axiomata_core::AxiomataCore;
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .setup(|app| {
            // Initializes `~/.axiomata` (config, database, logs, global
            // skills) and the Second-Brain workspace root on every start.
            // Managed as app state so future Tauri commands (M1+) can reach
            // the same config/database without re-initializing.
            let core =
                AxiomataCore::init().expect("failed to initialize the Axiomata-OS core engine");
            app.manage(Mutex::new(core));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
