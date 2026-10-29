//! `axiomata-cli app …`: what the app's chat reads about Axiomata itself (`docs/plans/chat-knows-app.md`). Every
//! command here only reads.
//!
//! `app handbook` serves the handbook embedded in this binary (`axiomata_core::handbook`), never the copy in
//! `~/.axiomata/handbook/`, so the text always matches the build that answers — and it needs neither the core nor
//! the database, so it is answered before either starts (a Glut sandbox hides `~/.axiomata`).

use anyhow::{Result, bail};
use axiomata_core::handbook;
use clap::Subcommand;
use serde_json::json;

#[derive(Debug, Subcommand)]
pub enum AppAction {
    /// The handbook: without a name its table of contents, with one that page's Markdown.
    Handbook {
        /// The page's name as the table of contents lists it (`studio`, `kanban`, …).
        name: Option<String>,
        /// The answer as JSON.
        #[arg(long)]
        json: bool,
    },
}

/// The commands that need no core: answered before it starts.
pub fn run_without_core(action: &AppAction) -> Option<Result<()>> {
    match action {
        AppAction::Handbook { name, json } => Some(handbook_cmd(name.as_deref(), *json)),
    }
}

fn handbook_cmd(name: Option<&str>, json: bool) -> Result<()> {
    let Some(name) = name else {
        if json {
            let pages: Vec<_> = handbook::PAGES
                .iter()
                .map(|page| json!({"name": page.name, "title": page.title(), "summary": page.summary()}))
                .collect();
            println!("{}", serde_json::to_string_pretty(&pages)?);
        } else {
            print!("{}", handbook::contents());
        }
        return Ok(());
    };
    let Some(page) = handbook::page(name) else {
        let known: Vec<&str> = handbook::PAGES.iter().map(|page| page.name).collect();
        bail!(
            "the handbook has no page {name:?}; its pages: {}",
            known.join(", ")
        );
    };
    if json {
        let answer = json!({"name": page.name, "title": page.title(), "text": page.text});
        println!("{}", serde_json::to_string_pretty(&answer)?);
    } else {
        print!("{}", page.text);
    }
    Ok(())
}
