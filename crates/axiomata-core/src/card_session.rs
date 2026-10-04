//! Starting a card: the owner's click that makes a new session for it (`docs/plans/a2a.md`, A3, A5, A20, CP-A6a).
//!
//! A *session* is an agent row with a worktree of its own and a branch, made for exactly one card from a **role** (what
//! the card names, else `allrounder`) on an **engine** (the role's, a fallback of it, or the one the owner picks now).
//! The app claims the card for the new session itself, atomically and before any process exists (A20): the claim is the
//! session's actor, so the card's `claimed_by` says who works on it, and a failed claim leaves nothing behind.
//!
//! This module only *makes* the session and takes the card. The harness is started when the session's pane — or the
//! CLI — starts it ([`crate::ide_start::start_agent`]), which sees that the session holds a card and starts it
//! unattended, with the card in its MCP server and the start prompt as its first words.

use axiomata_roster::{Engine, Role};
use rusqlite::Connection;
use serde::Serialize;

use crate::AxiomataCore;
use crate::AxiomataError;
use crate::agent_mcp::Capabilities;
use crate::board::{flow, store as board_store};
use crate::config::Config;
use crate::ide::agent_store;
use crate::ide::model::Agent;
use crate::ide::{store as project_store, worktree};
use crate::roster::{self, refusal};
use crate::session::actor_from;

/// The role a card without one is given (the one every install has).
const DEFAULT_ROLE: &str = "allrounder";
/// How many names `allrounder-12`, `allrounder-12-2`, … are tried before giving up.
const MAX_NAME_TRIES: u32 = 50;

type Result<T> = std::result::Result<T, AxiomataError>;

/// What the owner asked for.
#[derive(Debug, Clone)]
pub struct StartRequest {
    pub card_id: i64,
    /// The repository the work happens in — chosen at the start (`board start --project`, the dialog).
    pub project_id: i64,
    /// An engine of the catalog to run on; `None` takes the role's own.
    pub engine_id: Option<String>,
}

/// The session that was made and holds the card now.
#[derive(Debug, Clone, Serialize)]
pub struct CardSession {
    pub agent: Agent,
    pub card_id: i64,
    pub role: String,
    pub engine_id: String,
}

/// The engine a session of `role` runs on: the one asked for, else the role's own, else the first fallback that exists.
///
/// # Errors
///
/// A refusal naming what to do when the choice is empty or the engine cannot carry an unattended session.
fn choose_engine<'a>(
    config: &'a Config,
    role: &Role,
    requested: Option<&str>,
) -> Result<(&'a str, &'a Engine)> {
    let candidates = requested
        .into_iter()
        .map(str::to_owned)
        .chain(role.engine.clone())
        .chain(role.fallback_engines.iter().cloned())
        .collect::<Vec<_>>();
    // A requested engine that does not exist is a mistake to show, not a reason to fall back silently.
    if let Some(asked) = requested
        && !config.agents.engines.contains_key(asked)
    {
        return Err(refusal(
            "engine",
            format!("there is no engine “{asked}”; engines are added in the Studio settings"),
        ));
    }
    let Some((id, engine)) = candidates
        .iter()
        .find_map(|id| config.agents.engines.get_key_value(id.as_str()))
    else {
        return Err(refusal(
            "engine",
            format!(
                "the role “{}” names no engine that exists; pick one when you start the card",
                role.name
            ),
        ));
    };
    if !engine.command.trim().is_empty() {
        return Err(refusal(
            "engine",
            format!(
                "the engine “{id}” runs a command of its own, which Axiomata does not touch; an unattended session \
                 needs the generated command"
            ),
        ));
    }
    Ok((id.as_str(), engine))
}

/// The first of `role-<card>`, `role-<card>-2`, … no session of the project has.
fn free_name(taken: &[Agent], role: &str, card_id: i64) -> Result<String> {
    let base = format!("{role}-{card_id}");
    for attempt in 1..=MAX_NAME_TRIES {
        let name = if attempt == 1 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        if !taken.iter().any(|agent| agent.name == name) {
            return Ok(name);
        }
    }
    Err(refusal(
        "name",
        format!(
            "card #{card_id} has been started {MAX_NAME_TRIES} times; remove its old sessions first"
        ),
    ))
}

/// Makes a session for the card and takes the card for it. See the module documentation.
///
/// # Errors
///
/// A refusal that says why: no such project or card, a project that is no repository (unattended agents must not share
/// the owner's own folder), no `axiomata-cli` for the team tools, an unknown role, a role that does not do work, no
/// usable engine, a card that cannot be started now (not ready, held, a proposal, called off), a card with
/// predecessors (see below). Nothing is left behind then: no session row, no claim.
pub fn start_card_session(core: &AxiomataCore, request: &StartRequest) -> Result<CardSession> {
    let config = core.config_read().clone();
    let mut db = core.db_lock();
    let project = project_store::get_project(&db, request.project_id)?
        .ok_or_else(|| refusal("project", format!("no project {}", request.project_id)))?;
    if !worktree::is_repo(&project.repo_root) {
        return Err(refusal(
            "project",
            "an unattended session needs its own worktree, and this project is not a git repository".to_string(),
        ));
    }
    if crate::agent_entry::cli_path().is_none() {
        return Err(refusal(
            "axiomata-cli",
            "a session working on a card needs the team tools, and no `axiomata-cli` was found next to the app \
             (build the workspace, or set AXIOMATA_CLI)"
                .to_string(),
        ));
    }
    let roles = roster::roles_for_project(&db, &config, request.project_id);
    start_in(&mut db, &config, &roles, request)
}

/// [`start_card_session`] without the checks of the machine (a repository, the CLI), so tests need neither.
fn start_in(
    db: &mut Connection,
    config: &Config,
    roles: &[Role],
    request: &StartRequest,
) -> Result<CardSession> {
    let card = board_store::get_card(db, request.card_id)?
        .ok_or_else(|| refusal("card", format!("no card {}", request.card_id)))?;
    // The first step of a stacked card is the branch of the predecessor (A16); that comes with the next stage, and a
    // session cut from the main branch would work without what the card builds on.
    if !card.depends_on.is_empty() {
        return Err(refusal(
            "card",
            format!(
                "card #{} builds on card #{}; starting on the branch of a predecessor is not there yet",
                card.id, card.depends_on[0]
            ),
        ));
    }
    let role_name = card
        .agent
        .clone()
        .unwrap_or_else(|| DEFAULT_ROLE.to_owned());
    let role = roles
        .iter()
        .find(|role| role.name == role_name)
        .ok_or_else(|| {
            refusal(
                "role",
                format!("there is no role “{role_name}” for this project"),
            )
        })?;
    if !Capabilities::of(role).work {
        return Err(refusal(
            "role",
            format!(
                "the role “{role_name}” does not work cards (it reviews or plans); pick a role of kind implement for \
                 this card"
            ),
        ));
    }
    let (engine_id, _) = choose_engine(config, role, request.engine_id.as_deref())?;
    let name = free_name(
        &agent_store::list_agents(db, request.project_id)?,
        &role_name,
        card.id,
    )?;
    let agent = roster::create_agent_on_engine(
        db,
        config,
        roles,
        request.project_id,
        &name,
        engine_id,
        &role_name,
    )?;
    let Some(actor) = actor_from(Some(&agent.id.to_string()), Some(&agent.name)) else {
        agent_store::delete_agent(db, agent.id)?;
        return Err(refusal("name", format!("“{name}” makes no valid actor")));
    };
    // The claim is the whole check: ready, not held, not a proposal, not called off — the board says which.
    if let Err(err) = flow::start_card(db, card.id, &actor) {
        agent_store::delete_agent(db, agent.id)?;
        return Err(err.into());
    }
    Ok(CardSession {
        agent,
        card_id: card.id,
        role: role_name,
        engine_id: engine_id.to_owned(),
    })
}

/// The owner gives a started card back: the claim is dropped and the card waits in its open column again, and the
/// secret of the session that held it is taken back, so its MCP server cannot be started again (a running one is not
/// stopped from here — the pane is the owner's to close). `false` if nobody held the card.
///
/// # Errors
///
/// A refusal for a card that was signed off, taken over, archived, failed or called off, or that does not exist.
pub fn release_card_session(core: &AxiomataCore, card_id: i64) -> Result<bool> {
    let holder = {
        let mut db = core.db_lock();
        let Some(card) = board_store::get_card(&db, card_id)? else {
            return Err(refusal("card", format!("no card {card_id}")));
        };
        if !flow::release_started(&mut db, card_id, OWNER)? {
            return Ok(false);
        }
        card.claimed_by
    };
    // The holder's actor ends in the session's id (`agent:<name>-<id>`, `session::actor_from`).
    if let Some(id) = holder.as_deref().and_then(session_id_of) {
        axiomata_ide::session_token::revoke(&crate::paths::ide_locations().channels, id)?;
    }
    Ok(true)
}

/// The session id at the end of an actor string, if it is one of a session's.
fn session_id_of(actor: &str) -> Option<i64> {
    let rest = actor.strip_prefix("agent:")?;
    rest.rsplit('-').next()?.parse().ok()
}

/// Who gives a card back from the owner's side.
const OWNER: &str = "human:owner";

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use axiomata_board::{CardFields, NewCard, store};
    use axiomata_roster::{Billing, Harness, Limits, Source, Tier};

    use super::*;
    use crate::db;
    use crate::ide::NewProject;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn role(name: &str, kind: &str, engine: Option<&str>) -> Role {
        Role {
            name: name.into(),
            description: String::new(),
            kind: kind.into(),
            tier: Tier::Medium,
            engine: engine.map(str::to_owned),
            fallback_engines: vec![],
            permissions: vec![],
            limits: Limits::default(),
            creates: vec![],
            instructions: String::new(),
            source: Source::User,
        }
    }

    fn engine(id: &str, command: &str) -> Engine {
        Engine {
            id: id.into(),
            label: id.into(),
            harness: Harness::ClaudeCode,
            command: command.into(),
            model: None,
            env: String::new(),
            billing: Billing::Subscription,
        }
    }

    struct World {
        db: Connection,
        config: Config,
        roles: Vec<Role>,
        project: i64,
        board: i64,
        open: i64,
        proposal: i64,
        dir: PathBuf,
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn world() -> World {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-cardsession-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut db = db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
        let project = project_store::create_project(
            &db,
            NewProject {
                name: "P".into(),
                repo_root: dir.clone(),
            },
        )
        .unwrap()
        .id;
        let board = store::create_board(&mut db, "B").unwrap();
        let columns = store::list_columns(&db, board.id).unwrap();
        let id = |name: &str| columns.iter().find(|c| c.name == name).unwrap().id;
        let mut engines = BTreeMap::new();
        engines.insert("sonnet".to_owned(), engine("sonnet", ""));
        engines.insert("custom".to_owned(), engine("custom", "my-harness --go"));
        let mut config = Config::default();
        config.agents.engines = engines;
        World {
            roles: vec![
                role("allrounder", "implement", None),
                role("builder", "implement", Some("sonnet")),
                role("reviewer", "review", Some("sonnet")),
            ],
            project,
            board: board.id,
            open: id("Offen"),
            proposal: id("Vorschlag"),
            db,
            config,
            dir,
        }
    }

    impl World {
        fn card(&self, column: i64, role: Option<&str>) -> i64 {
            store::create_card(
                &self.db,
                &NewCard {
                    column_id: column,
                    fields: CardFields {
                        title: "work".into(),
                        agent: role.map(str::to_owned),
                        ..CardFields::default()
                    },
                },
            )
            .unwrap()
            .id
        }

        fn start(&mut self, card_id: i64, engine: Option<&str>) -> Result<CardSession> {
            start_in(
                &mut self.db,
                &self.config,
                &self.roles,
                &StartRequest {
                    card_id,
                    project_id: self.project,
                    engine_id: engine.map(str::to_owned),
                },
            )
        }

        fn sessions(&self) -> usize {
            agent_store::list_agents(&self.db, self.project)
                .unwrap()
                .len()
        }
    }

    #[test]
    fn a_card_is_taken_for_a_new_session_of_its_role_on_the_roles_engine() {
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let started = w.start(card, None).unwrap();
        assert_eq!(started.role, "builder");
        assert_eq!(started.engine_id, "sonnet");
        assert_eq!(started.agent.name, format!("builder-{card}"));
        assert_eq!(started.agent.agent_role, "builder");
        assert_eq!(started.agent.engine_id.as_deref(), Some("sonnet"));

        let actor = actor_from(
            Some(&started.agent.id.to_string()),
            Some(&started.agent.name),
        )
        .unwrap();
        let held = store::get_card(&w.db, card).unwrap().unwrap();
        assert_eq!(held.claimed_by.as_deref(), Some(actor.as_str()));
        assert_ne!(held.column_id, w.open, "it moved into work");
    }

    #[test]
    fn a_card_without_a_role_goes_to_the_allrounder_on_the_engine_the_owner_picks() {
        let mut w = world();
        let card = w.card(w.open, None);
        // The allrounder names no engine: without a pick there is nothing to run on, and nothing is made.
        let refused = w.start(card, None).unwrap_err().to_string();
        assert!(refused.contains("pick one"), "{refused}");
        assert_eq!(w.sessions(), 0);
        assert_eq!(
            store::get_card(&w.db, card).unwrap().unwrap().claimed_by,
            None
        );

        let started = w.start(card, Some("sonnet")).unwrap();
        assert_eq!(started.role, "allrounder");
        assert_eq!(started.engine_id, "sonnet");
    }

    #[test]
    fn the_owners_pick_beats_the_roles_engine_and_a_wrong_pick_is_an_error() {
        let mut w = world();
        w.config
            .agents
            .engines
            .insert("opus".into(), engine("opus", ""));
        let card = w.card(w.open, Some("builder"));
        let refused = w.start(card, Some("nope")).unwrap_err().to_string();
        assert!(refused.contains("no engine"), "{refused}");
        assert_eq!(w.sessions(), 0);
        assert_eq!(w.start(card, Some("opus")).unwrap().engine_id, "opus");
    }

    #[test]
    fn an_engine_with_a_command_of_its_own_cannot_carry_an_unattended_session() {
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let refused = w.start(card, Some("custom")).unwrap_err().to_string();
        assert!(refused.contains("command of its own"), "{refused}");
        assert_eq!(w.sessions(), 0);
    }

    #[test]
    fn a_role_that_reviews_or_is_unknown_is_not_started_as_a_worker() {
        let mut w = world();
        let review = w.card(w.open, Some("reviewer"));
        assert!(
            w.start(review, None)
                .unwrap_err()
                .to_string()
                .contains("does not work cards")
        );
        let ghost = w.card(w.open, Some("ghost"));
        assert!(
            w.start(ghost, None)
                .unwrap_err()
                .to_string()
                .contains("no role")
        );
        assert_eq!(w.sessions(), 0);
    }

    #[test]
    fn a_card_that_cannot_be_started_leaves_no_session_behind() {
        let mut w = world();
        let proposal = w.card(w.proposal, Some("builder"));
        let refused = w.start(proposal, None).unwrap_err().to_string();
        assert!(refused.contains("proposal"), "{refused}");
        assert_eq!(w.sessions(), 0, "the session made for it is taken back");

        let card = w.card(w.open, Some("builder"));
        w.start(card, None).unwrap();
        // Taken: a second start would make a second session for the same card, and is refused with the holder named.
        let again = w.start(card, None).unwrap_err().to_string();
        assert!(again.contains("held by"), "{again}");
        assert_eq!(w.sessions(), 1);
    }

    #[test]
    fn a_card_started_again_after_it_was_given_back_gets_a_new_name() {
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let first = w.start(card, None).unwrap();
        let actor = actor_from(Some(&first.agent.id.to_string()), Some(&first.agent.name)).unwrap();
        assert!(flow::release_started(&mut w.db, card, &actor).unwrap());
        let second = w.start(card, None).unwrap();
        assert_eq!(second.agent.name, format!("builder-{card}-2"));
        assert_ne!(first.agent.id, second.agent.id);
    }

    #[test]
    fn a_card_that_builds_on_another_waits_for_the_stacked_start() {
        let mut w = world();
        let base = w.card(w.open, Some("builder"));
        let top = w.card(w.open, Some("builder"));
        let plan = flow::create_plan(
            &w.db,
            w.board,
            &axiomata_board::PlanFields {
                name: "p".into(),
                auto_start_max: None,
                max_cost_usd: None,
                max_tokens: None,
            },
        )
        .unwrap();
        for id in [base, top] {
            let mut card = store::get_card(&w.db, id).unwrap().unwrap();
            card.plan_id = Some(plan.id);
            store::update_card(&w.db, id, &fields_of(&card)).unwrap();
        }
        flow::add_dependency(&mut w.db, top, base).unwrap();
        let refused = w.start(top, None).unwrap_err().to_string();
        assert!(refused.contains("builds on"), "{refused}");
        assert_eq!(w.sessions(), 0);
    }

    fn fields_of(card: &axiomata_board::Card) -> CardFields {
        CardFields {
            title: card.title.clone(),
            body: card.body.clone(),
            labels: card.labels.clone(),
            assignee: card.assignee.clone(),
            due_at: card.due_at,
            plan_id: card.plan_id,
            agent: card.agent.clone(),
            agent_reason: card.agent_reason.clone(),
            tier: card.tier,
            kind: card.kind.clone(),
            acceptance: card.acceptance.clone(),
        }
    }

    #[test]
    fn the_session_id_is_read_from_the_end_of_the_holders_actor() {
        assert_eq!(session_id_of("agent:builder-12-7"), Some(7));
        assert_eq!(session_id_of("agent:agent-3"), Some(3));
        assert_eq!(session_id_of("human:owner"), None);
        assert_eq!(session_id_of("agent:builder"), None);
    }

    #[test]
    fn only_a_card_in_work_counts_as_a_sessions_card() {
        use crate::ide_start::card_of;
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let started = w.start(card, None).unwrap();
        assert_eq!(card_of(&w.db, &started.agent), Some(card));

        let actor = actor_from(
            Some(&started.agent.id.to_string()),
            Some(&started.agent.name),
        )
        .unwrap();
        // Reported: it waits in review and is not work any more, so a restarted pane is not told to do it again.
        flow::report_done(&mut w.db, card, &actor).unwrap();
        assert_eq!(card_of(&w.db, &started.agent), None);

        // A session that holds nothing is not a card session.
        let other = roster::create_agent_on_engine(
            &w.db, &w.config, &w.roles, w.project, "hand", "sonnet", "builder",
        )
        .unwrap();
        assert_eq!(card_of(&w.db, &other), None);
    }

    #[test]
    fn a_fallback_engine_is_used_when_the_roles_own_is_gone() {
        let mut w = world();
        let mut gone = role("builder", "implement", Some("deleted"));
        gone.fallback_engines = vec!["sonnet".into()];
        w.roles = vec![gone];
        let card = w.card(w.open, Some("builder"));
        assert_eq!(w.start(card, None).unwrap().engine_id, "sonnet");
    }
}
