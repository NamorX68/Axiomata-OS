//! `axiomata-cli app …`: what the app's chat reads about Axiomata itself (`docs/plans/chat-knows-app.md`, CH1/CH3).
//! Every command here only reads: the handbook, and the live state through `axiomata_core::app_state`, as text or with
//! `--json`. The settings view never shows a key or a token, only whether one is set (Q7).
//!
//! `app handbook` serves the handbook embedded in this binary (`axiomata_core::handbook`), never the copy in
//! `~/.axiomata/handbook/`, so the text always matches the build that answers — and it needs neither the core nor
//! the database, so it is answered before either starts (a Glut sandbox hides `~/.axiomata`).

use anyhow::{Result, bail};
use axiomata_core::app_state::{self, Request, Topic};
use axiomata_core::{AxiomataCore, handbook};
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
    /// The Studio: projects, sessions (engine, role, card, state) and who waits for the owner.
    Studio(Json),
    /// Spend today and this month per provider, the Studio's spend and the limits.
    Spend(Json),
    /// The newest skill and routine runs.
    Runs {
        /// How many (at most 100).
        #[arg(long, default_value_t = app_state::DEFAULT_RUNS)]
        limit: usize,
        #[command(flatten)]
        json: Json,
    },
    /// The routines and when they fire next.
    Routines(Json),
    /// The boards with their columns and card counts; with `--board` that board's cards by title.
    Board {
        /// A board's id, to list its cards.
        #[arg(long)]
        board: Option<i64>,
        #[command(flatten)]
        json: Json,
    },
    /// Axiomata's storage: sizes, the clean-up rules and what a clean-up would free (deletes nothing).
    Storage(Json),
    /// The settings that say how the app behaves; keys and tokens only as set / not set.
    Settings(Json),
    /// The tag list, the proposals waiting and the vault run's step.
    Tags(Json),
}

/// `--json` for every live view.
#[derive(Debug, Clone, Copy, clap::Args)]
pub struct Json {
    /// The answer as JSON.
    #[arg(long)]
    json: bool,
}

/// The commands that need no core: answered before it starts.
pub fn run_without_core(action: &AppAction) -> Option<Result<()>> {
    match action {
        AppAction::Handbook { name, json } => Some(handbook_cmd(name.as_deref(), *json)),
        _ => None,
    }
}

/// The live views, which read the database and the config.
pub fn run(core: &AxiomataCore, action: AppAction) -> Result<()> {
    refuse_in_agent_session(axiomata_core::session::session_actor().as_deref())?;
    let (request, json) = match action {
        AppAction::Handbook { .. } => unreachable!("answered before the core starts"),
        AppAction::Studio(json) => (Request::new(Topic::Studio), json),
        AppAction::Spend(json) => (Request::new(Topic::Spend), json),
        AppAction::Runs { limit, json } => (
            Request {
                limit,
                ..Request::new(Topic::Runs)
            },
            json,
        ),
        AppAction::Routines(json) => (Request::new(Topic::Routines), json),
        AppAction::Board { board, json } => (
            Request {
                board,
                ..Request::new(Topic::Board)
            },
            json,
        ),
        AppAction::Storage(json) => (Request::new(Topic::Storage), json),
        AppAction::Settings(json) => (Request::new(Topic::Settings), json),
        AppAction::Tags(json) => (Request::new(Topic::Tags), json),
    };
    let value = app_state::read_core(core, request)?;
    if json.json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        print!("{}", app_state::render(request.topic, &value));
    }
    Ok(())
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

/// The live views are for the app's chat (Q3), not for a Studio session: a card's agent learns what it needs from its
/// card and the team tools, and the owner's spend, settings and other sessions' questions are none of its business.
/// Not a sandbox — a session runs as the owner's user — but no door is opened for it here. The handbook stays open: it
/// is the build's own text.
fn refuse_in_agent_session(session: Option<&str>) -> Result<()> {
    if let Some(own) = session {
        bail!(
            "the app's live views are for the owner's chat; {own} reads its card and the team tools instead"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_session_gets_no_live_view() {
        assert!(refuse_in_agent_session(Some("agent:worker")).is_err());
        assert!(refuse_in_agent_session(None).is_ok());
    }
}
