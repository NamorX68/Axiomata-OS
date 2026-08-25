//! Minimal CLI entry point for exercising the Axiomata-OS core engine
//! end-to-end without the Tauri GUI.

use axiomata_core::{AxiomataCore, paths};

fn main() {
    match AxiomataCore::init() {
        Ok(core) => {
            println!("Axiomata-OS core initialized.");
            println!("  workspace root: {}", core.config.workspace_root.display());
            println!("  config file:    {}", paths::config_path().display());
            println!("  database:       {}", paths::db_path().display());
            println!("  logs directory: {}", paths::logs_dir().display());
            println!("  global skills:  {}", paths::global_skills_dir().display());
        }
        Err(err) => {
            eprintln!("Axiomata-OS failed to initialize: {err}");
            std::process::exit(1);
        }
    }
}
