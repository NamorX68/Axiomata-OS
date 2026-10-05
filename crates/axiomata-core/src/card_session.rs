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
use std::sync::{Arc, Mutex, MutexGuard};

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
pub(crate) fn choose_engine<'a>(
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
                "the role “{}” names no engine that exists; pick one when you start it",
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
    // A card of a plan that runs by itself starts from the plan's line; making it runs git, so it is done before the
    // database is locked.
    let line = line_for_start(core, request)?;
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
    start_in(
        &mut db,
        &config,
        &roles,
        request,
        line.as_ref().map(|line| line.branch.as_str()),
    )
}

/// Whether a plan runs by itself: approved, set to start its cards on its own, and with a project to run them in.
pub fn runs_by_itself(plan: &crate::board::Plan) -> bool {
    plan.status == crate::board::PlanStatus::Approved
        && plan.auto_start_max.is_some()
        && plan.project_id.is_some()
}

/// The integration line of the plan the card belongs to, made if it is not there yet — `None` for a card of no plan or of
/// a plan that does not run by itself. The card must be started in the plan's own project: the line is a branch of
/// that repository.
fn line_for_start(
    core: &AxiomataCore,
    request: &StartRequest,
) -> Result<Option<axiomata_ide::plan_line::Line>> {
    let (plan, project) = {
        let db = core.db_lock();
        let Some(card) = board_store::get_card(&db, request.card_id)? else {
            return Ok(None);
        };
        // A database error is an error: read as "no plan" the card would start off the main branch.
        let Some(plan) = card
            .plan_id
            .map(|id| flow::get_plan(&db, id))
            .transpose()?
            .flatten()
        else {
            return Ok(None);
        };
        if !runs_by_itself(&plan) {
            return Ok(None);
        }
        if plan.project_id != Some(request.project_id) {
            return Err(refusal(
                "project",
                format!(
                    "card #{} belongs to plan #{}, which runs in project {}; start it there",
                    card.id,
                    plan.id,
                    plan.project_id.unwrap_or_default()
                ),
            ));
        }
        let project = project_store::get_project(&db, request.project_id)?
            .ok_or_else(|| refusal("project", format!("no project {}", request.project_id)))?;
        (plan, project)
    };
    make_line(core, &plan, &project).map(Some)
}

/// Makes plan `plan`'s line in `project`'s repository (or finds it) and records the branch it was cut from.
pub(crate) fn make_line(
    core: &AxiomataCore,
    plan: &crate::board::Plan,
    project: &axiomata_ide::Project,
) -> Result<axiomata_ide::plan_line::Line> {
    let path = axiomata_ide::plan_line::line_path(
        &crate::paths::ide_locations().worktrees,
        &project.name,
        plan.id,
    );
    let line = axiomata_ide::plan_line::ensure(
        &project.repo_root,
        &path,
        plan.id,
        plan.base_branch.as_deref(),
    )?;
    if plan.base_branch.is_none() {
        flow::set_plan_base_branch(&core.db_lock(), plan.id, &line.base_branch)?;
    }
    // A line the studio just made is at the base; one it did not make must be where it left it.
    check_line_tip(&core.db, plan, &line)?;
    Ok(line)
}

/// [`start_card_session`] without the checks of the machine (a repository, the CLI), so tests need neither.
fn start_in(
    db: &mut Connection,
    config: &Config,
    roles: &[Role],
    request: &StartRequest,
    line_base: Option<&str>,
) -> Result<CardSession> {
    let card = board_store::get_card(db, request.card_id)?
        .ok_or_else(|| refusal("card", format!("no card {}", request.card_id)))?;
    // The first step of a stacked card is the branch of the predecessor (A16); that comes with the next stage, and a
    // session cut from the main branch would work without what the card builds on.
    // On the line of its plan, a card that builds on others starts from what they did: it waits until their work is on
    // the line. Anywhere else a card with predecessors cannot be started by the studio.
    match line_base {
        None if !card.depends_on.is_empty() => {
            return Err(refusal(
                "card",
                format!(
                    "card #{} builds on card #{}; only a plan that runs by itself starts cards on the work of their \
                     predecessors",
                    card.id, card.depends_on[0]
                ),
            ));
        }
        Some(_) => {
            for dep in &card.depends_on {
                let integrated =
                    board_store::get_card(db, *dep)?.is_some_and(|dep| dep.integrated_at.is_some());
                if !integrated {
                    return Err(refusal(
                        "card",
                        format!(
                            "card #{} builds on card #{dep}, whose work is not on the plan's line yet",
                            card.id
                        ),
                    ));
                }
            }
        }
        None => {}
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
    // The studio started this session for this card; the pane's start reads it from here.
    agent_store::set_card(db, agent.id, Some(card.id), false, None)?;
    // Its branch is cut from the plan's line when it is made, and what it is reviewed against is the line.
    if let Some(base) = line_base {
        agent_store::set_base_branch(db, agent.id, Some(base))?;
    }
    // The claim is the whole check: ready, not held, not a proposal, not called off — the board says which.
    if let Err(err) = flow::start_card(db, card.id, &actor) {
        agent_store::delete_agent(db, agent.id)?;
        return Err(err.into());
    }
    Ok(CardSession {
        agent: agent_store::get_agent(db, agent.id)?.unwrap_or(agent),
        card_id: card.id,
        role: role_name,
        engine_id: engine_id.to_owned(),
    })
}

/// The role kind that judges cards.
const KIND_REVIEW: &str = "review";

/// What the studio (or the owner) asks for when a card is waiting for its review.
#[derive(Debug, Clone)]
pub struct ReviewRequest {
    pub card_id: i64,
    /// An engine of the catalog to review on; `None` takes the reviewer role's own (or a fallback of it).
    pub engine_id: Option<String>,
    /// The owner accepts that the work changes the files agents read their configuration from, so the reviewer is
    /// started on a checkout that holds them. Never set by the studio's own watcher.
    pub allow_agent_config: bool,
}

/// The reviewer session that was made.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewSession {
    pub agent: Agent,
    pub card_id: i64,
    pub role: String,
    pub engine_id: String,
}

/// The reviewer role: the one of kind `review`, and with several the strongest (A21).
fn pick_reviewer(roles: &[Role]) -> Result<&Role> {
    roles
        .iter()
        .filter(|role| role.kind == KIND_REVIEW)
        .max_by_key(|role| role.tier)
        .ok_or_else(|| {
            refuse_review(
                "there is no role of kind `review` for this project; add one in the Studio",
            )
        })
}

fn refuse_review(reason: &str) -> AxiomataError {
    refusal("review", reason.to_owned())
}

/// The engine a reviewer runs on: never the engine of the session that worked on the card (A21) — a model marking its
/// own homework agrees with itself. The one asked for, else the role's own, else the first fallback that exists, has no
/// command of its own and differs.
///
/// # Errors
///
/// A refusal naming what to do: the asked-for engine is the worker's, or nothing usable is left and the owner picks.
fn choose_review_engine<'a>(
    config: &'a Config,
    role: &Role,
    worker_engine: Option<&str>,
    requested: Option<&str>,
) -> Result<&'a str> {
    if let (Some(asked), Some(worker)) = (requested, worker_engine)
        && asked == worker
    {
        return Err(refuse_review(
            "the reviewer must run on another engine than the session that worked on the card",
        ));
    }
    if let Some(asked) = requested {
        let engine = config.agents.engines.get(asked).ok_or_else(|| {
            refusal(
                "engine",
                format!("there is no engine “{asked}”; engines are added in the Studio settings"),
            )
        })?;
        if !engine.command.trim().is_empty() {
            return Err(refuse_review(
                "that engine runs a command of its own, which Axiomata does not touch",
            ));
        }
        return Ok(engine.id.as_str());
    }
    let usable = |id: &&String| {
        Some(id.as_str()) != worker_engine
            && config
                .agents
                .engines
                .get(id.as_str())
                .is_some_and(|engine| engine.command.trim().is_empty())
    };
    role.engine
        .iter()
        .chain(role.fallback_engines.iter())
        .find(usable)
        .and_then(|id| config.agents.engines.get_key_value(id.as_str()))
        .map(|(id, _)| id.as_str())
        .ok_or_else(|| {
            refuse_review(
                "the reviewer needs an engine other than the worker's, and the role names none; pick one",
            )
        })
}

/// The reviewer session that is judging the latest report of a card, if there is one: a reviewer made after the latest
/// `reported` line of the card. One review per report — a card sent back and reported again needs a new one.
pub(crate) fn current_reviewer(
    db: &Connection,
    card_id: i64,
    sessions: &[Agent],
) -> Result<Option<Agent>> {
    let reported_at =
        flow::latest_event(db, card_id, crate::board::EventKind::Reported)?.map(|event| event.at);
    Ok(sessions
        .iter()
        .find(|agent| {
            agent.card_id == Some(card_id)
                && agent.card_review
                && reported_at.is_none_or(|at| agent.created_at >= at)
        })
        .cloned())
}

/// The project of the session that worked on a card — where its reviewer will be made.
fn project_of(db: &Connection, card: &crate::board::Card) -> Result<i64> {
    card.claimed_by
        .as_deref()
        .and_then(session_id_of)
        .and_then(|id| agent_store::get_agent(db, id).ok().flatten())
        .map(|worker| worker.project_id)
        .ok_or_else(|| refuse_review("the session that worked on the card is gone"))
}

/// A card that was reported and has no reviewer yet, for the studio to start one for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AwaitingReview {
    pub card_id: i64,
    /// How often the card was sent back. A new report after a return is a new review, so the pair is what the studio
    /// remembers having tried.
    pub returned_count: u32,
}

/// The cards the studio should start a reviewer for: in review, held by a session **the studio started for that card**
/// (a card the owner dragged into the review column by hand has no worker and is the owner's to review), and with no
/// reviewer for the latest report. One scan of all boards, cheap enough for the watcher's tick.
///
/// # Errors
///
/// A database error.
pub fn cards_awaiting_review(core: &AxiomataCore) -> Result<Vec<AwaitingReview>> {
    awaiting_in(&core.db_lock())
}

fn awaiting_in(db: &Connection) -> Result<Vec<AwaitingReview>> {
    let mut waiting = Vec::new();
    for board in board_store::list_boards(db)? {
        for card in board_store::list_cards(db, board.id, false)? {
            if card.state != crate::board::TaskState::InReview {
                continue;
            }
            let worked_by_a_session_of_ours = card
                .claimed_by
                .as_deref()
                .and_then(session_id_of)
                .and_then(|id| agent_store::get_agent(db, id).ok().flatten())
                .is_some_and(|worker| worker.card_id == Some(card.id) && !worker.card_review);
            if !worked_by_a_session_of_ours {
                continue;
            }
            let sessions = agent_store::list_agents(db, project_of(db, &card)?)?;
            if current_reviewer(db, card.id, &sessions)?.is_some() {
                continue;
            }
            waiting.push(AwaitingReview {
                card_id: card.id,
                returned_count: card.returned_count,
            });
        }
    }
    Ok(waiting)
}

/// Locks the shared connection, recovering a poisoned guard like [`AxiomataCore::db_lock`].
pub(crate) fn lock(db: &Mutex<Connection>) -> MutexGuard<'_, Connection> {
    db.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// What a review needs, read under the lock so that git can run without it: the card, the session that worked on it
/// and where, the role and the engine the reviewer gets.
struct ReviewPlan {
    card: crate::board::Card,
    worker: Agent,
    repo: axiomata_ide::git::AgentRepo,
    roles: Vec<Role>,
    role: Role,
    engine_id: String,
}

/// A card session — worker or reviewer — whose card still wants it, for a pane to be opened on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OpenCardSession {
    pub card_id: i64,
    pub project_id: i64,
    pub agent_id: i64,
}

/// The card sessions that are at work right now, workers and reviewers. The watchers tell the frontend when they make one,
/// but the frontend may not be listening yet (the first tick can come before the page has loaded) or may have been
/// reloaded: asked once when the page starts, this makes sure no session is left without its pane — the pane is what
/// starts it, and a card that is "in work" with no harness holds its place for ever.
///
/// # Errors
///
/// A database error.
pub fn open_card_sessions(core: &AxiomataCore) -> Result<Vec<OpenCardSession>> {
    let db = core.db_lock();
    let mut open = Vec::new();
    for project in project_store::list_projects(&db)? {
        for agent in agent_store::list_agents(&db, project.id)? {
            if let Some(card_id) = agent.card_id
                && crate::ide_start::card_launch(&db, &agent).is_some()
            {
                open.push(OpenCardSession {
                    card_id,
                    project_id: project.id,
                    agent_id: agent.id,
                });
            }
        }
    }
    Ok(open)
}

/// Makes the reviewer session for a card that waits in review (A3, A21) — what the studio does on its own as soon as a
/// card is reported, and the owner can do by hand with an engine of their choice.
///
/// Three things happen: the work is **snapshotted** (everything the worker left uncommitted is committed on its
/// branch — it was told not to; the reviewer's checkout is cut from that commit, so the reviewer sees exactly what the
/// card is about, and a second report after a return gets a fresh snapshot); the **reviewer role and engine** are
/// chosen (another engine than the worker's, or the owner is asked); and a session is made for the reviewer that holds
/// no claim and works in a detached checkout of the snapshot. Its pane starts the harness like a card session's.
/// Reviewers of earlier reports of the card are retired: their sessions, worktrees and secrets go.
///
/// The database is locked only to read and to write — never while git runs: a worktree with a huge tree or a slow
/// disk must not freeze the app.
///
/// # Errors
///
/// A refusal that says why: the card is not waiting in review, the worker session is gone, the project is not a
/// repository, no `axiomata-cli`, no reviewer role, no other engine to run on, a review of this report is already
/// there, or the work changes the files agents read their configuration from (a reviewer started on them would
/// obey what the worker wrote) and the owner did not accept that.
pub async fn start_review_session(
    core: &AxiomataCore,
    request: &ReviewRequest,
) -> Result<ReviewSession> {
    let db = Arc::clone(&core.db);
    let config = core.config_read().clone();
    let request = request.clone();
    let roots = crate::paths::ide_locations().channels;
    let cli_available = crate::agent_entry::cli_path().is_some();
    let (session, retired_locations) = tokio::task::spawn_blocking(move || {
        review_blocking(
            &db,
            &config,
            &roots,
            &|conn, project| roster::roles_for_project(conn, &config, project),
            &request,
            cli_available,
        )
    })
    .await
    .map_err(|err| refuse_review(&format!("the review task failed: {err}")))??;
    for location in &retired_locations {
        crate::agents::opencode::forget_mcp(location).await;
    }
    Ok(session)
}

/// [`start_review_session`] without the runtime and with the machine's `axiomata-cli` answered by the caller, so tests
/// need neither. Returns the session and the Opencode locations whose server registration is to go.
fn review_blocking(
    db: &Mutex<Connection>,
    config: &Config,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
    roles_of: &dyn Fn(&Connection, i64) -> Vec<Role>,
    request: &ReviewRequest,
    cli_available: bool,
) -> Result<(ReviewSession, Vec<std::path::PathBuf>)> {
    let plan = plan_review(&lock(db), config, roles_of, request, cli_available)?;
    let snapshot = snapshot_for_review(&plan, request.allow_agent_config)?;
    let (session, retired) = finish_review(&lock(db), config, &plan, &snapshot)?;
    let locations = remove_sessions(db, roots, &retired);
    Ok((session, locations))
}

/// Phase one, under the lock: everything that can be refused, asked before anything is committed.
fn plan_review(
    db: &Connection,
    config: &Config,
    roles_of: &dyn Fn(&Connection, i64) -> Vec<Role>,
    request: &ReviewRequest,
    cli_available: bool,
) -> Result<ReviewPlan> {
    let card = board_store::get_card(db, request.card_id)?
        .ok_or_else(|| refusal("card", format!("no card {}", request.card_id)))?;
    let column = board_store::get_column(db, card.column_id)?;
    if column.and_then(|column| column.stage) != Some(crate::board::ColumnStage::Review)
        || card.state != crate::board::TaskState::InReview
        || card.archived_at.is_some()
    {
        return Err(refuse_review("the card is not waiting for a review"));
    }
    let worker = card
        .claimed_by
        .as_deref()
        .and_then(session_id_of)
        .and_then(|id| agent_store::get_agent(db, id).ok().flatten())
        .filter(|worker| worker.card_id == Some(card.id) && !worker.card_review)
        .ok_or_else(|| refuse_review("the session that worked on the card is gone"))?;
    let project = project_store::get_project(db, worker.project_id)?
        .ok_or_else(|| refusal("project", format!("no project {}", worker.project_id)))?;
    if !worktree::is_repo(&project.repo_root) {
        return Err(refusal(
            "project",
            "a reviewer needs a worktree of its own, and this project is not a git repository"
                .to_string(),
        ));
    }
    if !cli_available {
        return Err(refuse_review(
            "a reviewer needs the team tools, and no `axiomata-cli` was found next to the app (build the workspace, or \
             set AXIOMATA_CLI)",
        ));
    }
    let sessions = agent_store::list_agents(db, worker.project_id)?;
    if let Some(existing) = current_reviewer(db, card.id, &sessions)? {
        return Err(refuse_review(&format!(
            "the card is being reviewed already, by {}",
            existing.name
        )));
    }
    let roles = roles_of(db, worker.project_id);
    let role = pick_reviewer(&roles)?.clone();
    let engine_id = choose_review_engine(
        config,
        &role,
        worker.engine_id.as_deref(),
        request.engine_id.as_deref(),
    )?
    .to_owned();
    // The name is asked now too, so that a refusal does not come after the snapshot commit.
    free_name(&sessions, &role.name, card.id)?;
    let repo = crate::ide::provision::agent_repo(db, worker.id)?.ready()?;
    Ok(ReviewPlan {
        card,
        worker,
        repo,
        roles,
        role,
        engine_id,
    })
}

/// Phase two, no lock: the snapshot, and the check that it does not carry agent configuration the reviewer would obey.
///
/// The commit is made first and stays on the worker's branch when the review is refused afterwards — it is the
/// worker's own work, committed as the studio would on a take-over, and the next report snapshots again.
fn snapshot_for_review(plan: &ReviewPlan, allow_agent_config: bool) -> Result<String> {
    let first_line = plan.card.title.lines().next().unwrap_or_default().trim();
    let snapshot = plan
        .repo
        .snapshot(&format!("#{} {first_line}", plan.card.id))?;
    let touched = plan.repo.agent_config_changes(&snapshot)?;
    if !touched.is_empty() && !allow_agent_config {
        return Err(refuse_review(&format!(
            "the work changes files agents take their configuration from ({}), and a reviewer started on that checkout \
             would obey what the worker wrote there. Review it yourself, or start the review by hand and accept it",
            touched.join(", ")
        )));
    }
    Ok(snapshot)
}

/// Phase three, under the lock: asked again — the owner may have started the review, or judged the card, in between —
/// then the session is made. Returns it with the reviewers of earlier reports, which are to be retired.
fn finish_review(
    db: &Connection,
    config: &Config,
    plan: &ReviewPlan,
    snapshot: &str,
) -> Result<(ReviewSession, Vec<Agent>)> {
    let card = board_store::get_card(db, plan.card.id)?
        .ok_or_else(|| refusal("card", format!("no card {}", plan.card.id)))?;
    if card.state != crate::board::TaskState::InReview {
        return Err(refuse_review("the card is not waiting for a review"));
    }
    let sessions = agent_store::list_agents(db, plan.worker.project_id)?;
    if let Some(existing) = current_reviewer(db, card.id, &sessions)? {
        return Err(refuse_review(&format!(
            "the card is being reviewed already, by {}",
            existing.name
        )));
    }
    let name = free_name(&sessions, &plan.role.name, card.id)?;
    let agent = roster::create_agent_on_engine(
        db,
        config,
        &plan.roles,
        plan.worker.project_id,
        &name,
        &plan.engine_id,
        &plan.role.name,
    )?;
    if let Err(err) = agent_store::set_card(db, agent.id, Some(card.id), true, Some(snapshot)) {
        agent_store::delete_agent(db, agent.id)?;
        return Err(err.into());
    }
    let retired = sessions
        .into_iter()
        .filter(|earlier| earlier.card_id == Some(card.id) && earlier.card_review)
        .collect();
    let session = ReviewSession {
        agent: agent_store::get_agent(db, agent.id)?.unwrap_or(agent),
        card_id: card.id,
        role: plan.role.name.clone(),
        engine_id: plan.engine_id.clone(),
    };
    Ok((session, retired))
}

/// What taking a card over came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum CardTakeOver {
    /// The work is in the project's main line as one commit, the card is closed and archived, and the sessions made for
    /// it are gone. `cleanup` lists what could not be done — the work is taken over all the same.
    Done {
        commit: String,
        /// The project the work went into — where the commit is waiting to be pushed.
        project_id: i64,
        cleanup: Vec<String>,
    },
    /// The squash conflicted and was undone: the project folder is as it was (G9). Nothing else changed.
    Conflict { files: Vec<String> },
}

/// What a take-over leaves for the async caller: the outcome, and the Opencode locations whose server registration
/// has to go.
#[derive(Debug)]
struct Taken {
    outcome: CardTakeOver,
    opencode_locations: Vec<std::path::PathBuf>,
}

/// What a take-over needs, read under the lock.
struct TakeOverPlan {
    card: crate::board::Card,
    /// The project of the session that worked on the card.
    card_project: i64,
    sessions: Vec<Agent>,
    repo: axiomata_ide::git::AgentRepo,
    target: crate::ide::provision::TakeOverTarget,
    message: String,
}

/// The owner takes a reviewed card over (A3, A22): the second gate, after the reviewer's signature.
///
/// What is taken over is **exactly what was reviewed**: if the worker's branch is not at the commit the signing
/// reviewer was shown — it changed afterwards, or has uncommitted changes, or its worktree was moved off the branch —
/// nothing happens and the card says so, because a signature on other work than the one that ships is no signature. A
/// card the owner signed off without a reviewer has nothing to compare with; its uncommitted work is committed as it
/// stands.
///
/// On success the squash commit is on the base branch, the card is marked taken over (and archived), and the sessions
/// made for the card — the worker and its reviewers — are cleaned up: worktrees removed, the worker's branch deleted,
/// the sessions forgotten with their secrets and Opencode registrations. A conflict is undone and reported. The
/// database is locked only to read and to write, never while git runs.
///
/// The take-over commit runs the project's own hooks, as a take-over always has (the owner's repository, the owner's
/// click). What the worker changed in a tracked hook script reaches them with the work — which is what the review is for.
///
/// # Errors
///
/// A refusal for a card that is not signed off, was taken over already, whose worker session is gone or busy, or whose
/// branch is not the reviewed state; the git layer's own refusals (G8, G10, G12).
pub async fn take_over_card(
    core: &AxiomataCore,
    card_id: i64,
    message: Option<&str>,
) -> Result<CardTakeOver> {
    let db = Arc::clone(&core.db);
    let roots = crate::paths::ide_locations().channels;
    let message = message.map(str::to_owned);
    let taken = tokio::task::spawn_blocking(move || {
        take_over_blocking(&db, &roots, card_id, message.as_deref())
    })
    .await
    .map_err(|err| refusal("take-over", format!("the take-over task failed: {err}")))??;
    for location in &taken.opencode_locations {
        crate::agents::opencode::forget_mcp(location).await;
    }
    Ok(taken.outcome)
}

fn take_over_blocking(
    db: &Mutex<Connection>,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
    card_id: i64,
    message: Option<&str>,
) -> Result<Taken> {
    let plan = plan_take_over(&lock(db), card_id, message)?;
    match squash(&plan, roots)? {
        axiomata_ide::git::TakeOver::Conflict { files } => Ok(Taken {
            outcome: CardTakeOver::Conflict { files },
            opencode_locations: Vec::new(),
        }),
        axiomata_ide::git::TakeOver::Done { commit } => {
            // The work is in the main line from here on: nothing below can undo it, so a step that fails is a note.
            let mut cleanup = Vec::new();
            match flow::mark_taken_over(&lock(db), plan.card.id, OWNER) {
                Ok(true) => {}
                Ok(false) => cleanup.push("the card could not be marked as taken over".to_owned()),
                Err(err) => {
                    cleanup.push(format!("the card could not be marked as taken over: {err}"))
                }
            }
            let mut removals = begin_removal(&lock(db), &plan.sessions);
            remove_trees(&mut removals, &mut cleanup);
            let opencode_locations = finish_removal(&lock(db), roots, &removals, &mut cleanup);
            Ok(Taken {
                outcome: CardTakeOver::Done {
                    commit,
                    project_id: plan.card_project,
                    cleanup,
                },
                opencode_locations,
            })
        }
    }
}

/// Phase one of a take-over, under the lock: the card, the worker and what was reviewed.
fn plan_take_over(db: &Connection, card_id: i64, message: Option<&str>) -> Result<TakeOverPlan> {
    let card = board_store::get_card(db, card_id)?
        .ok_or_else(|| refusal("card", format!("no card {card_id}")))?;
    if card.verified_by.is_none() {
        return Err(refusal(
            "card",
            "only a card the reviewer signed off can be taken over".to_string(),
        ));
    }
    if card.taken_over_at.is_some() {
        return Err(refusal(
            "card",
            "the card was taken over already".to_string(),
        ));
    }
    // A card of a plan that runs by itself goes into the plan's line, and the line is taken over as a whole.
    if card
        .plan_id
        .and_then(|id| flow::get_plan(db, id).ok().flatten())
        .is_some_and(|plan| runs_by_itself(&plan))
    {
        return Err(refusal(
            "card",
            "this card belongs to a plan that runs by itself: its work is integrated into the plan's line, and the plan \
             is taken over as a whole"
                .to_string(),
        ));
    }
    // Only a session the studio started for this card is a worker the studio takes work from: a card claimed by hand
    // through `claim_task` is the owner's to merge.
    let worker = card
        .claimed_by
        .as_deref()
        .and_then(session_id_of)
        .and_then(|id| agent_store::get_agent(db, id).ok().flatten())
        .filter(|worker| worker.card_id == Some(card.id) && !worker.card_review)
        .ok_or_else(|| {
            refusal(
                "card",
                "the session that worked on the card is gone".to_string(),
            )
        })?;
    let first_line = card.title.lines().next().unwrap_or_default().trim();
    let message = message
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map_or_else(|| format!("#{} {first_line}", card.id), str::to_owned);
    let sessions: Vec<Agent> = agent_store::list_agents(db, worker.project_id)?
        .into_iter()
        .filter(|agent| agent.card_id == Some(card.id))
        .collect();
    let repo = crate::ide::provision::agent_repo(db, worker.id)?.ready()?;
    let target = crate::ide::provision::TakeOverTarget::read(db, worker.id)?;
    Ok(TakeOverPlan {
        card_project: worker.project_id,
        card,
        sessions,
        repo,
        target,
        message,
    })
}

/// The gate before work leaves its worker: that what ships is **exactly what was reviewed**. The worktree must sit on the
/// worker's branch, and the branch must be where the signing reviewer was shown it; a card the owner signed off without a
/// reviewer has nothing to compare with, and its uncommitted work is committed as it stands.
fn gate_reviewed(
    card: &crate::board::Card,
    sessions: &[Agent],
    repo: &axiomata_ide::git::AgentRepo,
    message: &str,
) -> Result<String> {
    // Before anything runs in the agent's tree: `status` and `add -A` there run what its config names.
    repo.ensure_safe_config()?;
    // What ships is the branch, so the gate looks at the branch: that the worktree sits on it, and where it is. The
    // worktree's HEAD alone could be somewhere else while the branch was moved.
    repo.ensure_on_own_branch()?;
    // The state that was reviewed: the snapshot of the session that signed the card off — not "the latest reviewer", a
    // reviewer of an earlier report must not be the one whose snapshot is compared. A card the owner signed off has no
    // reviewer session to compare with.
    let reviewed = card
        .verified_by
        .as_deref()
        .and_then(session_id_of)
        .and_then(|id| {
            sessions
                .iter()
                .find(|agent| agent.id == id && agent.card_review)
        })
        .and_then(|reviewer| reviewer.start_ref.clone());
    // A signature of another session whose record is gone has nothing to be compared with, and "nothing to compare" must
    // not read as "the owner signed it off": only a person's own signature goes without a reviewer's snapshot.
    if reviewed.is_none()
        && card
            .verified_by
            .as_deref()
            .is_some_and(|signer| signer.starts_with("agent:"))
    {
        return Err(refusal(
            "card",
            "the reviewer that signed the card off is gone, so there is no way to tell that the work is what it saw; send \
             the card back for a new review"
                .to_string(),
        ));
    }
    match reviewed {
        Some(reviewed) => {
            if repo.is_dirty()? || repo.branch_tip()? != reviewed {
                return Err(refusal(
                    "card",
                    "the work changed after it was reviewed; send the card back for a new review instead of taking it \
                     over"
                        .to_string(),
                ));
            }
            // The commit that was checked, by its id: the worker's harness may still be alive, and a branch name read
            // again after the gate could point at a commit nobody reviewed.
            Ok(reviewed)
        }
        None => {
            if repo.is_dirty()? {
                // `commit_all` answers with the commit it made: no second read of the branch.
                return Ok(repo.commit_all(message)?);
            }
            Ok(repo.branch_tip()?)
        }
    }
}

/// Phase two of a take-over, no lock: the gate, and the squash.
fn squash(
    plan: &TakeOverPlan,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
) -> Result<axiomata_ide::git::TakeOver> {
    gate_reviewed(&plan.card, &plan.sessions, &plan.repo, &plan.message)?;
    Ok(plan.target.run(
        roots,
        axiomata_ide::git::TakeOverMode::Squash,
        &plan.message,
    )?)
}

/// A session that is going away, with the worktree it still has: read under the lock, removed without it, forgotten
/// under it again — `git worktree remove` of a big tree must not hold the database.
struct Removal {
    session: Agent,
    tree: Option<crate::ide::provision::WorktreeToDiscard>,
    /// The worktree could not be removed: the session stays, so that there is still a row to retry from.
    tree_stays: bool,
    repo_root: Option<std::path::PathBuf>,
}

/// Step one of ending sessions, under the lock.
fn begin_removal(db: &Connection, sessions: &[Agent]) -> Vec<Removal> {
    sessions
        .iter()
        .map(|session| Removal {
            tree: crate::ide::provision::WorktreeToDiscard::read(db, session.id)
                .ok()
                .flatten(),
            repo_root: project_store::get_project(db, session.project_id)
                .ok()
                .flatten()
                .map(|project| project.repo_root),
            session: session.clone(),
            tree_stays: false,
        })
        .collect()
}

/// Step two, no lock: the worktrees go, and the branch of every session that had one of its own (a worker; a reviewer
/// has none) once its worktree is gone.
fn remove_trees(removals: &mut [Removal], notes: &mut Vec<String>) {
    for removal in removals {
        if let Some(tree) = &removal.tree
            && let Err(err) = tree.remove(true)
        {
            notes.push(format!(
                "the worktree of {} stays: {err}",
                removal.session.name
            ));
            removal.tree_stays = true;
            continue;
        }
        if let (Some(branch), Some(root)) = (&removal.session.branch, &removal.repo_root)
            && let Err(err) = worktree::delete_branch(root, branch)
        {
            notes.push(format!("the branch {branch} stays: {err}"));
        }
    }
}

/// Step three, under the lock: the sessions are forgotten with their secrets and channels. A session whose worktree
/// stays keeps its row. Returns the Opencode locations whose server registration is to go.
fn finish_removal(
    db: &Connection,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
    removals: &[Removal],
    notes: &mut Vec<String>,
) -> Vec<std::path::PathBuf> {
    let mut opencode_locations = Vec::new();
    for removal in removals {
        let session = &removal.session;
        if let Err(err) = axiomata_ide::session_token::revoke(roots, session.id) {
            notes.push(format!("the secret of {} stays valid: {err}", session.name));
        }
        if removal.tree_stays {
            continue;
        }
        if session.harness == axiomata_roster::Harness::Opencode
            && let Some(path) = &session.worktree_path
        {
            opencode_locations.push(path.clone());
        }
        if let Some(tree) = &removal.tree
            && let Err(err) = tree.forget(db, true)
        {
            notes.push(format!(
                "the record of the worktree of {} stays: {err}",
                session.name
            ));
        }
        match agent_store::delete_agent(db, session.id) {
            Ok(_) => {
                if let Err(err) = crate::ide::provision::forget_channel(roots, session.id) {
                    notes.push(format!(
                        "the status folder of {} stays: {err}",
                        session.name
                    ));
                }
            }
            Err(err) => notes.push(format!("the session {} stays: {err}", session.name)),
        }
    }
    opencode_locations
}

/// Ends sessions in the three steps above, taking the lock only for the first and the last. Returns the Opencode
/// locations whose registration is to go. Problems are logged: a retired reviewer that lingers is no reason to refuse.
pub(crate) fn remove_sessions(
    db: &Mutex<Connection>,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
    sessions: &[Agent],
) -> Vec<std::path::PathBuf> {
    if sessions.is_empty() {
        return Vec::new();
    }
    let mut notes = Vec::new();
    let mut removals = begin_removal(&lock(db), sessions);
    remove_trees(&mut removals, &mut notes);
    let locations = finish_removal(&lock(db), roots, &removals, &mut notes);
    for note in notes {
        tracing::warn!(%note, "retiring a session");
    }
    locations
}

// ------------------------------------------------------------------ the line ---

/// The studio's own actor, for what it does by itself: integrating a reviewed card, putting one back.
pub const STUDIO: &str = "agent:studio";

/// How often a card may be put back because it does not fit the line before it is given up on.
const MAX_LINE_CONFLICTS: usize = 2;

/// What integrating a card into the line of its plan came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum CardIntegration {
    /// The card's work is on the line; its sessions are cleaned up. `agent_ids` are the sessions that are gone, for the
    /// Studio to close their panes.
    Done {
        card_id: i64,
        plan_id: i64,
        project_id: i64,
        /// `None` when the card changed nothing compared to the line.
        commit: Option<String>,
        agent_ids: Vec<i64>,
    },
    /// The work does not fit what the line has by now. The card was put back to be done again on the line as it is, or —
    /// after [`MAX_LINE_CONFLICTS`] such tries — left as it is for the owner.
    Conflict {
        card_id: i64,
        plan_id: i64,
        files: Vec<String>,
        /// The card was not put back: it has failed to fit the line twice and is left for the owner.
        gave_up: bool,
        agent_ids: Vec<i64>,
    },
    /// The worker is in the middle of a turn: nothing was done, the next look tries again.
    Busy { card_id: i64 },
    /// The studio gave up on this card earlier (see `gave_up`): it is left for the owner and only an owner's own request
    /// ([`integrate_card_with`]) tries it again.
    LeftForOwner { card_id: i64 },
}

/// What integrating needs, read under the lock.
struct IntegrationPlan {
    card: crate::board::Card,
    plan: crate::board::Plan,
    project: axiomata_ide::Project,
    worker: Agent,
    sessions: Vec<Agent>,
    repo: axiomata_ide::git::AgentRepo,
    message: String,
    conflicts_so_far: usize,
    /// The studio gave up on this card (a note says so).
    gave_up: bool,
}

/// Merges a card that the reviewer signed off into the line of its plan: one commit, the card marked integrated, its
/// sessions cleaned up. The same gate as a take-over (what was reviewed is what goes in), and the database is locked only
/// to read and to write, never while git runs.
///
/// # Errors
///
/// A refusal for a card that is not signed off, was integrated or taken over already, belongs to no plan that runs by
/// itself, or whose worker session is gone; the git layer's own refusals.
pub async fn integrate_card(core: &AxiomataCore, card_id: i64) -> Result<CardIntegration> {
    integrate_card_with(core, card_id, false).await
}

/// [`integrate_card`], where `owner` says the owner asked for it by hand: a card the studio gave up on is tried again then,
/// and only then — the studio's own looks leave it alone ([`CardIntegration::LeftForOwner`]).
///
/// # Errors
///
/// As [`integrate_card`].
pub async fn integrate_card_with(
    core: &AxiomataCore,
    card_id: i64,
    owner: bool,
) -> Result<CardIntegration> {
    let db = Arc::clone(&core.db);
    let roots = crate::paths::ide_locations().channels;
    let locations = crate::paths::ide_locations();
    let worktrees = locations.worktrees.clone();
    let done = tokio::task::spawn_blocking(move || {
        integrate_blocking(&db, &roots, &worktrees, card_id, owner)
    })
    .await
    .map_err(|err| refusal("integration", format!("the integration task failed: {err}")))??;
    for location in &done.1 {
        crate::agents::opencode::forget_mcp(location).await;
    }
    Ok(done.0)
}

fn integrate_blocking(
    db: &Mutex<Connection>,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
    worktrees: &std::path::Path,
    card_id: i64,
    owner: bool,
) -> Result<(CardIntegration, Vec<std::path::PathBuf>)> {
    let plan = plan_integration(&lock(db), card_id)?;
    // A card the studio gave up on stays with the owner: looked at again every few seconds it would be put back, redone and
    // paid for again and again.
    if plan.gave_up && !owner {
        return Ok((CardIntegration::LeftForOwner { card_id }, Vec::new()));
    }
    // The worker is idle once it has reported and been reviewed; one that is still in a turn is left alone until it is.
    let state = axiomata_ide::lifecycle::Channel::for_agent(roots, plan.worker.id)
        .read_status(plan.worker.harness)
        .state;
    if matches!(
        state,
        axiomata_ide::lifecycle::AgentState::Working | axiomata_ide::lifecycle::AgentState::Waiting
    ) {
        return Ok((CardIntegration::Busy { card_id }, Vec::new()));
    }
    let path = axiomata_ide::plan_line::line_path(worktrees, &plan.project.name, plan.plan.id);
    let line = axiomata_ide::plan_line::ensure(
        &plan.project.repo_root,
        &path,
        plan.plan.id,
        plan.plan.base_branch.as_deref(),
    )?;
    // The line must be where the studio left it: a branch of that name is a branch like any other, and work put on it by
    // something else is work no reviewer saw.
    check_line_tip(db, &plan.plan, &line)?;
    // What was checked is what goes in: the commit the gate saw, not whatever the branch name points at by now.
    let reviewed_commit = gate_reviewed(&plan.card, &plan.sessions, &plan.repo, &plan.message)?;
    let outcome = axiomata_ide::plan_line::integrate(&line, &reviewed_commit, &plan.message)?;
    let agent_ids: Vec<i64> = plan.sessions.iter().map(|agent| agent.id).collect();
    let mut notes = Vec::new();
    let result = match outcome {
        axiomata_ide::plan_line::Integration::Done { commit } => {
            if let Err(err) = flow::set_line_tip(&lock(db), plan.plan.id, &commit) {
                notes.push(format!("the line's tip could not be recorded: {err}"));
            }
            finish_integration(db, &plan, Some(commit), &agent_ids)?
        }
        axiomata_ide::plan_line::Integration::Empty => {
            finish_integration(db, &plan, None, &agent_ids)?
        }
        axiomata_ide::plan_line::Integration::Conflict { files } => {
            let text = format!(
                "conflict: its work does not fit the plan's line any more ({})",
                summary_of(&files)
            );
            // After the second such try the studio stops putting the card back (a card redone for ever is money spent
            // for nothing): it stays as it is, signed off and not on the line, with the reason in its history. A signed-off
            // card cannot be failed, and the owner decides what to do with it.
            let gave_up = plan.conflicts_so_far + 1 >= MAX_LINE_CONFLICTS || plan.gave_up;
            {
                let mut conn = lock(db);
                if gave_up && plan.gave_up {
                    // Said once, when the studio gave up; an owner's retry that conflicts again adds nothing.
                } else if gave_up {
                    flow::add_event(
                        &conn,
                        plan.card.id,
                        STUDIO,
                        crate::board::EventKind::Note,
                        &format!("{GAVE_UP_PREFIX}: {text}"),
                    )?;
                } else if !flow::reset_for_rework(&mut conn, plan.card.id, STUDIO, &text)? {
                    return Err(refusal(
                        "card",
                        "the card does not fit the line, and could not be put back".to_string(),
                    ));
                }
            }
            CardIntegration::Conflict {
                card_id: plan.card.id,
                plan_id: plan.plan.id,
                files,
                gave_up,
                // Its sessions stay when the studio gave up: their panes are what the owner looks at.
                agent_ids: if gave_up {
                    Vec::new()
                } else {
                    agent_ids.clone()
                },
            }
        }
    };
    // Its sessions are of no use any more either way: done, or to be made anew on the line as it is now (a card the
    // studio gave up on keeps them — they are what the owner looks at).
    let keep = matches!(&result, CardIntegration::Conflict { gave_up: true, .. });
    let locations = if keep {
        Vec::new()
    } else {
        remove_sessions(db, roots, &plan.sessions)
    };
    for note in notes {
        tracing::warn!(%note, "integrating a card");
    }
    Ok((result, locations))
}

/// Phase one, under the lock: everything that can be refused.
fn plan_integration(db: &Connection, card_id: i64) -> Result<IntegrationPlan> {
    let card = board_store::get_card(db, card_id)?
        .ok_or_else(|| refusal("card", format!("no card {card_id}")))?;
    if card.verified_by.is_none() {
        return Err(refusal(
            "card",
            "only a card the reviewer signed off can be integrated".to_string(),
        ));
    }
    if card.integrated_at.is_some() || card.taken_over_at.is_some() {
        return Err(refusal(
            "card",
            "the card was integrated or taken over already".to_string(),
        ));
    }
    let plan = card
        .plan_id
        .and_then(|id| flow::get_plan(db, id).ok().flatten())
        .filter(runs_by_itself)
        .ok_or_else(|| {
            refusal(
                "card",
                "the card belongs to no plan that runs by itself".to_string(),
            )
        })?;
    let worker = card
        .claimed_by
        .as_deref()
        .and_then(session_id_of)
        .and_then(|id| agent_store::get_agent(db, id).ok().flatten())
        .filter(|worker| worker.card_id == Some(card.id) && !worker.card_review)
        .ok_or_else(|| {
            refusal(
                "card",
                "the session that worked on the card is gone".to_string(),
            )
        })?;
    let project = project_store::get_project(db, plan.project_id.unwrap_or_default())?
        .ok_or_else(|| refusal("project", "the plan's project is gone".to_string()))?;
    let sessions: Vec<Agent> = agent_store::list_agents(db, worker.project_id)?
        .into_iter()
        .filter(|agent| agent.card_id == Some(card.id))
        .collect();
    let repo = crate::ide::provision::agent_repo(db, worker.id)?.ready()?;
    let first_line = card.title.lines().next().unwrap_or_default().trim();
    let message = format!("#{} {first_line}", card.id);
    // Counted in the database: a long history must not make the first conflict forgotten.
    let conflicts_so_far = flow::count_events(
        db,
        card.id,
        STUDIO,
        crate::board::EventKind::Released,
        "conflict:",
    )?;
    let gave_up = flow::count_events(
        db,
        card.id,
        STUDIO,
        crate::board::EventKind::Note,
        GAVE_UP_PREFIX,
    )? > 0;
    Ok(IntegrationPlan {
        card,
        plan,
        project,
        worker,
        sessions,
        repo,
        message,
        conflicts_so_far,
        gave_up,
    })
}

/// The files of a conflict for a line of history: the first few, and how many more. A history line is bounded.
fn summary_of(files: &[String]) -> String {
    const SHOWN: usize = 8;
    let shown = files
        .iter()
        .take(SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    match files.len().saturating_sub(SHOWN) {
        0 => shown,
        more => format!("{shown}, and {more} more"),
    }
}

/// The start of the history line that says the studio gave up integrating a card.
const GAVE_UP_PREFIX: &str = "gave up integrating";

/// The line is where the studio last left it. A line that was not made by this call and has no recorded tip is refused
/// too: the branch was there before the studio, and nothing says what is on it.
fn check_line_tip(
    db: &Mutex<Connection>,
    plan: &crate::board::Plan,
    line: &axiomata_ide::plan_line::Line,
) -> Result<()> {
    let tip = axiomata_ide::plan_line::tip(&line.path)?;
    let in_place = match (&plan.line_tip, line.created) {
        (Some(recorded), _) if *recorded == tip => return Ok(()),
        (None, true) => true,
        // Made just now, yet a tip was recorded: the branch was deleted in between.
        (Some(_), true) => false,
        // The studio commits first and writes the tip down second: a crash or a failed write in between leaves the line
        // ahead of the record by commits that are the studio's own. Those — and a line nobody put anything on yet — are
        // taken as it is; anything else is not.
        (recorded, false) => axiomata_ide::plan_line::only_studio_commits_since(
            &line.path,
            recorded.as_deref(),
            &line.base_branch,
        )?,
    };
    if !in_place {
        return Err(refusal(
            "plan",
            format!(
                "the line of plan #{} is not where the studio left it: something else moved or made the branch {}; look at \
                 it before anything is merged into it",
                plan.id, line.branch
            ),
        ));
    }
    flow::set_line_tip(&lock(db), plan.id, &tip)?;
    Ok(())
}

/// The work is on the line: the card is marked, and the line's commit is the answer.
///
/// # Errors
///
/// A card that cannot be marked is an error and its sessions are kept: the work is on the line, and the next look finds
/// that out ([`axiomata_ide::plan_line::Integration::Empty`]) and marks the card, whereas a card whose worker is gone
/// could be neither integrated nor marked ever again.
fn finish_integration(
    db: &Mutex<Connection>,
    plan: &IntegrationPlan,
    commit: Option<String>,
    agent_ids: &[i64],
) -> Result<CardIntegration> {
    if !flow::mark_integrated(&lock(db), plan.card.id, STUDIO)? {
        return Err(refusal(
            "card",
            "the work is on the line, but the card could not be marked as integrated".to_string(),
        ));
    }
    Ok(CardIntegration::Done {
        card_id: plan.card.id,
        plan_id: plan.plan.id,
        project_id: plan.project.id,
        commit,
        agent_ids: agent_ids.to_vec(),
    })
}

/// Forgets a session made for a card that never ran — its start failed — with its worktree, secret and channel, and its
/// Opencode registration. The card is the caller's to give back ([`release_card_session`]).
pub async fn discard_session(core: &AxiomataCore, agent_id: i64) -> Result<()> {
    let roots = crate::paths::ide_locations().channels;
    let location = {
        let db = core.db_lock();
        let Some(agent) = agent_store::get_agent(&db, agent_id)? else {
            return Ok(());
        };
        let _ = crate::ide::provision::discard_worktree(&db, agent_id, true);
        let _ = axiomata_ide::session_token::revoke(&roots, agent_id);
        agent_store::delete_agent(&db, agent_id)?;
        let _ = crate::ide::provision::forget_channel(&roots, agent_id);
        agent
            .worktree_path
            .filter(|_| agent.harness == axiomata_roster::Harness::Opencode)
    };
    if let Some(location) = location {
        crate::agents::opencode::forget_mcp(&location).await;
    }
    Ok(())
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
pub(crate) fn session_id_of(actor: &str) -> Option<i64> {
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
        repo: PathBuf,
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
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        for args in [
            &["init", "--initial-branch=main"][..],
            &["config", "user.email", "t@example.com"],
            &["config", "user.name", "T"],
        ] {
            git(&repo, args);
        }
        std::fs::write(repo.join("README.md"), "hi\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "first"]);
        let mut db = db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
        let project = project_store::create_project(
            &db,
            NewProject {
                name: "P".into(),
                repo_root: repo.clone(),
            },
        )
        .unwrap()
        .id;
        let board = store::create_board(&mut db, "B").unwrap();
        let columns = store::list_columns(&db, board.id).unwrap();
        let id = |name: &str| columns.iter().find(|c| c.name == name).unwrap().id;
        let mut engines = BTreeMap::new();
        engines.insert("sonnet".to_owned(), engine("sonnet", ""));
        engines.insert("opus".to_owned(), engine("opus", ""));
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
            repo,
        }
    }

    /// Runs git in `dir`; a failing command fails the test with its own words.
    fn git(dir: &std::path::Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git should run");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
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
            self.start_on(card_id, engine, None)
        }

        fn start_on(
            &mut self,
            card_id: i64,
            engine: Option<&str>,
            line_base: Option<&str>,
        ) -> Result<CardSession> {
            start_in(
                &mut self.db,
                &self.config,
                &self.roles,
                &StartRequest {
                    card_id,
                    project_id: self.project,
                    engine_id: engine.map(str::to_owned),
                },
                line_base,
            )
        }

        fn locations(&self) -> crate::ide::provision::Locations {
            crate::ide::provision::Locations {
                worktrees: self.dir.join("worktrees"),
                channels: axiomata_ide::lifecycle::ChannelRoots {
                    events: self.dir.join("events"),
                    claude_tasks: self.dir.join("tasks"),
                    claude_plans: self.dir.join("plans"),
                },
            }
        }

        /// A card worked on by a session on `engine` that wrote a file in its worktree and reported it done.
        fn reported(&mut self, engine: &str) -> (i64, CardSession) {
            let card = self.card(self.open, Some("builder"));
            let started = self.start(card, Some(engine)).unwrap();
            let ready =
                crate::ide::provision::prepare(&self.db, &self.locations(), started.agent.id)
                    .unwrap();
            std::fs::write(ready.cwd.join("change.txt"), "done\n").unwrap();
            let actor = actor_from(
                Some(&started.agent.id.to_string()),
                Some(&started.agent.name),
            )
            .unwrap();
            flow::report_done(&mut self.db, card, &actor).unwrap();
            (card, started)
        }

        /// Runs `f` on the connection as the shared mutex the production code works with, and gets it back.
        fn shared<R>(&mut self, f: impl FnOnce(&Mutex<Connection>) -> R) -> R {
            let conn = std::mem::replace(&mut self.db, Connection::open_in_memory().unwrap());
            let shared = Mutex::new(conn);
            let out = f(&shared);
            self.db = shared.into_inner().unwrap();
            out
        }

        fn review(&mut self, card_id: i64, engine: Option<&str>) -> Result<ReviewSession> {
            self.review_allowing(card_id, engine, false)
        }

        fn review_allowing(
            &mut self,
            card_id: i64,
            engine: Option<&str>,
            allow_agent_config: bool,
        ) -> Result<ReviewSession> {
            let (config, roles, roots) = (
                self.config.clone(),
                self.roles.clone(),
                self.locations().channels,
            );
            let request = ReviewRequest {
                card_id,
                engine_id: engine.map(str::to_owned),
                allow_agent_config,
            };
            self.shared(|db| {
                review_blocking(db, &config, &roots, &|_, _| roles.clone(), &request, true)
                    .map(|(session, _)| session)
            })
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
                project_id: None,
                goal: String::new(),
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
        use crate::ide_start::card_launch;
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let started = w.start(card, None).unwrap();
        assert_eq!(
            card_launch(&w.db, &started.agent),
            Some(crate::agent_entry::CardLaunch::work(card))
        );

        let actor = actor_from(
            Some(&started.agent.id.to_string()),
            Some(&started.agent.name),
        )
        .unwrap();
        // Reported: it waits in review and is not work any more, so a restarted pane is not told to do it again.
        flow::report_done(&mut w.db, card, &actor).unwrap();
        assert_eq!(card_launch(&w.db, &started.agent), None);

        // A session that holds nothing is not a card session.
        let other = roster::create_agent_on_engine(
            &w.db, &w.config, &w.roles, w.project, "hand", "sonnet", "builder",
        )
        .unwrap();
        assert_eq!(card_launch(&w.db, &other), None);
    }

    #[test]
    fn a_reported_card_gets_a_reviewer_on_another_engine_looking_at_a_snapshot() {
        let mut w = world();
        w.roles.retain(|r| r.name != "reviewer");
        w.roles.push(role("reviewer", "review", Some("opus")));
        let (card, worker) = w.reported("sonnet");

        let review = w.review(card, None).unwrap();
        assert_eq!(
            (review.role.as_str(), review.engine_id.as_str()),
            ("reviewer", "opus")
        );
        assert_eq!(review.agent.name, format!("reviewer-{card}"));
        assert_eq!(review.agent.card_id, Some(card));
        assert!(
            review.agent.card_review,
            "it judges the card, it does not work on it"
        );
        // It holds no claim: the card is still the worker's.
        let held = store::get_card(&w.db, card).unwrap().unwrap();
        let worker_actor =
            actor_from(Some(&worker.agent.id.to_string()), Some(&worker.agent.name)).unwrap();
        assert_eq!(held.claimed_by.as_deref(), Some(worker_actor.as_str()));

        // The work was snapshotted on the worker's branch — it was told not to commit — and that commit is what the
        // reviewer is cut from.
        let snapshot = review.agent.start_ref.clone().unwrap();
        let branch = worker.agent.branch.clone().or_else(|| {
            agent_store::get_agent(&w.db, worker.agent.id)
                .unwrap()
                .unwrap()
                .branch
        });
        assert_eq!(git(&w.repo, &["rev-parse", &branch.unwrap()]), snapshot);
        assert!(git(&w.repo, &["show", "--stat", "--format=%s", &snapshot]).contains("change.txt"));
    }

    #[test]
    fn without_an_engine_other_than_the_workers_the_owner_is_asked_and_nothing_is_committed() {
        let mut w = world();
        let (card, worker) = w.reported("sonnet");
        // The seeded reviewer names the worker's own engine: the studio does not run it there on its own.
        let refused = w.review(card, None).unwrap_err().to_string();
        assert!(refused.contains("pick one"), "{refused}");
        let tip = |w: &World| {
            let branch = agent_store::get_agent(&w.db, worker.agent.id)
                .unwrap()
                .unwrap()
                .branch
                .unwrap();
            git(&w.repo, &["rev-parse", &branch])
        };
        assert_eq!(
            tip(&w),
            git(&w.repo, &["rev-parse", "main"]),
            "no snapshot was made for a review that did not start"
        );
        assert!(
            agent_store::list_agents(&w.db, w.project)
                .unwrap()
                .iter()
                .all(|a| !a.card_review)
        );

        // The owner's pick works.
        assert_eq!(w.review(card, Some("opus")).unwrap().engine_id, "opus");
    }

    #[test]
    fn a_reviewer_on_the_workers_own_engine_is_refused_even_when_asked_for() {
        let mut w = world();
        let (card, _) = w.reported("sonnet");
        let refused = w.review(card, Some("sonnet")).unwrap_err().to_string();
        assert!(refused.contains("another engine"), "{refused}");
        assert!(
            w.review(card, Some("custom"))
                .unwrap_err()
                .to_string()
                .contains("command of its own")
        );
        assert!(
            w.review(card, Some("nope"))
                .unwrap_err()
                .to_string()
                .contains("no engine")
        );
    }

    #[test]
    fn a_report_is_reviewed_once_and_the_next_report_after_a_return_is_reviewed_again() {
        let mut w = world();
        let (card, worker) = w.reported("sonnet");
        let first = w.review(card, Some("opus")).unwrap();
        let again = w.review(card, Some("opus")).unwrap_err().to_string();
        assert!(again.contains("being reviewed already"), "{again}");

        // The reviewer sends it back; the worker fixes it and reports again.
        let reviewer =
            actor_from(Some(&first.agent.id.to_string()), Some(&first.agent.name)).unwrap();
        flow::review_verdict(&mut w.db, card, &reviewer, flow::Verdict::Return, "wrong").unwrap();
        let worker_actor =
            actor_from(Some(&worker.agent.id.to_string()), Some(&worker.agent.name)).unwrap();
        flow::report_done(&mut w.db, card, &worker_actor).unwrap();
        let second = w.review(card, Some("opus")).unwrap();
        assert_ne!(first.agent.id, second.agent.id);
        assert_eq!(second.agent.name, format!("reviewer-{card}-2"));
    }

    #[test]
    fn only_a_card_a_session_of_ours_reported_and_nobody_reviews_yet_is_awaiting_review() {
        let mut w = world();
        let (card, worker) = w.reported("sonnet");
        let expected = AwaitingReview {
            card_id: card,
            returned_count: 0,
        };
        assert_eq!(awaiting_in(&w.db).unwrap(), vec![expected]);

        // A card the owner dragged into the review column has no worker of ours: the owner reviews it.
        let dragged = w.card(w.open, Some("builder"));
        let review_column = store::list_columns(&w.db, w.board)
            .unwrap()
            .into_iter()
            .find(|c| c.stage == Some(crate::board::ColumnStage::Review))
            .unwrap()
            .id;
        store::move_card(&mut w.db, dragged, review_column, 0).unwrap();
        assert_eq!(awaiting_in(&w.db).unwrap(), vec![expected]);

        // Once a reviewer is made for this report it is not awaiting any more; a new report after a return is.
        w.roles.retain(|r| r.name != "reviewer");
        w.roles.push(role("reviewer", "review", Some("opus")));
        let review = w.review(card, None).unwrap();
        assert!(awaiting_in(&w.db).unwrap().is_empty());
        let reviewer =
            actor_from(Some(&review.agent.id.to_string()), Some(&review.agent.name)).unwrap();
        flow::review_verdict(&mut w.db, card, &reviewer, flow::Verdict::Return, "no").unwrap();
        let worker_actor =
            actor_from(Some(&worker.agent.id.to_string()), Some(&worker.agent.name)).unwrap();
        flow::report_done(&mut w.db, card, &worker_actor).unwrap();
        assert_eq!(
            awaiting_in(&w.db).unwrap(),
            vec![AwaitingReview {
                card_id: card,
                returned_count: 1
            }]
        );
    }

    #[test]
    fn a_card_that_is_not_waiting_in_review_is_not_reviewed() {
        let mut w = world();
        let open = w.card(w.open, Some("builder"));
        assert!(
            w.review(open, Some("opus"))
                .unwrap_err()
                .to_string()
                .contains("not waiting")
        );
        let working = w.card(w.open, Some("builder"));
        w.start(working, Some("sonnet")).unwrap();
        assert!(
            w.review(working, Some("opus"))
                .unwrap_err()
                .to_string()
                .contains("not waiting")
        );
        assert!(
            w.review(9999, Some("opus"))
                .unwrap_err()
                .to_string()
                .contains("no card")
        );
        assert_eq!(
            agent_store::list_agents(&w.db, w.project)
                .unwrap()
                .iter()
                .filter(|a| a.card_review)
                .count(),
            0
        );
    }

    #[test]
    fn the_reviewer_checks_out_the_snapshot_and_is_a_reviewer_only_until_the_card_is_signed_off() {
        use crate::ide_start::card_launch;
        let mut w = world();
        let (card, worker) = w.reported("sonnet");
        let review = w.review(card, Some("opus")).unwrap();

        let ready = crate::ide::provision::prepare(&w.db, &w.locations(), review.agent.id).unwrap();
        assert!(
            ready.cwd.join("change.txt").is_file(),
            "it sees exactly what was reported"
        );
        assert_eq!(ready.agent.branch, None);
        assert_eq!(
            card_launch(&w.db, &ready.agent),
            Some(crate::agent_entry::CardLaunch::review(
                card,
                &worker.agent.name,
                "main"
            ))
        );

        let reviewer =
            actor_from(Some(&review.agent.id.to_string()), Some(&review.agent.name)).unwrap();
        flow::review_verdict(&mut w.db, card, &reviewer, flow::Verdict::Approve, "").unwrap();
        assert_eq!(
            card_launch(&w.db, &ready.agent),
            None,
            "a signed-off card needs no reviewer any more"
        );
    }

    impl World {
        /// A reported card, reviewed by a reviewer on `opus` who signed it off.
        fn approved(&mut self) -> (i64, CardSession, ReviewSession) {
            self.roles.retain(|r| r.name != "reviewer");
            self.roles.push(role("reviewer", "review", Some("opus")));
            let (card, worker) = self.reported("sonnet");
            let review = self.review(card, None).unwrap();
            let reviewer =
                actor_from(Some(&review.agent.id.to_string()), Some(&review.agent.name)).unwrap();
            flow::review_verdict(&mut self.db, card, &reviewer, flow::Verdict::Approve, "")
                .unwrap();
            (card, worker, review)
        }

        fn take_over(&mut self, card: i64, message: Option<&str>) -> Result<Taken> {
            let roots = self.locations().channels;
            self.shared(|db| take_over_blocking(db, &roots, card, message))
        }
    }

    #[test]
    fn a_reviewed_card_is_taken_over_as_one_commit_and_its_sessions_are_cleaned_up() {
        let mut w = world();
        let (card, worker, review) = w.approved();
        let worker_row = agent_store::get_agent(&w.db, worker.agent.id)
            .unwrap()
            .unwrap();
        let worker_tree = worker_row.worktree_path.clone().unwrap();
        // The reviewer's pane has started: it has its checkout.
        let review_tree = crate::ide::provision::prepare(&w.db, &w.locations(), review.agent.id)
            .unwrap()
            .cwd;
        assert!(review_tree.is_dir());
        let branch = worker_row.branch.clone().unwrap();
        let main_before = git(&w.repo, &["rev-parse", "main"]);

        let taken = w.take_over(card, None).unwrap();
        let CardTakeOver::Done {
            commit, cleanup, ..
        } = taken.outcome
        else {
            panic!("expected the work to be taken over: {:?}", taken.outcome);
        };
        assert!(cleanup.is_empty(), "{cleanup:?}");

        // One new commit on main, with the card's number and title, holding the work.
        assert_eq!(git(&w.repo, &["rev-parse", "main"]), commit);
        assert_eq!(git(&w.repo, &["rev-parse", "main~1"]), main_before);
        assert_eq!(
            git(&w.repo, &["log", "-1", "--format=%s", "main"]),
            format!("#{card} work")
        );
        assert_eq!(git(&w.repo, &["show", "main:change.txt"]), "done");

        // The card is closed, and the sessions made for it are gone: worktrees, branch, rows.
        let closed = store::get_card(&w.db, card).unwrap().unwrap();
        assert!(closed.taken_over_at.is_some() && closed.archived_at.is_some());
        assert!(!worker_tree.exists() && !review_tree.exists());
        assert!(!worktree::branch_exists(&w.repo, &branch));
        assert_eq!(w.sessions(), 0);
        // And it cannot be taken over twice.
        assert!(
            w.take_over(card, None)
                .unwrap_err()
                .to_string()
                .contains("already")
        );
    }

    #[test]
    fn the_owners_own_commit_message_is_used() {
        let mut w = world();
        let (card, _, _) = w.approved();
        w.take_over(card, Some("  Bigger text in the preview  "))
            .unwrap();
        assert_eq!(
            git(&w.repo, &["log", "-1", "--format=%s", "main"]),
            "Bigger text in the preview"
        );
    }

    #[test]
    fn work_that_changed_after_the_review_is_not_taken_over() {
        let mut w = world();
        let (card, worker, _) = w.approved();
        let tree = agent_store::get_agent(&w.db, worker.agent.id)
            .unwrap()
            .unwrap()
            .worktree_path
            .unwrap();
        let main_before = git(&w.repo, &["rev-parse", "main"]);

        // The worker was still running and wrote more after the signature: that is not what was signed.
        std::fs::write(tree.join("late.txt"), "sneaked in\n").unwrap();
        let refused = w.take_over(card, None).unwrap_err().to_string();
        assert!(
            refused.contains("changed after it was reviewed"),
            "{refused}"
        );

        // Committed instead of left lying around, it is not the reviewed commit either.
        git(&tree, &["add", "-A"]);
        git(&tree, &["commit", "-q", "-m", "late"]);
        assert!(
            w.take_over(card, None)
                .unwrap_err()
                .to_string()
                .contains("changed after")
        );

        assert_eq!(
            git(&w.repo, &["rev-parse", "main"]),
            main_before,
            "nothing reached the main line"
        );
        let card = store::get_card(&w.db, card).unwrap().unwrap();
        assert!(card.taken_over_at.is_none());
        assert_eq!(w.sessions(), 2, "the sessions are still there");
    }

    #[test]
    fn a_card_that_was_not_signed_off_is_not_taken_over() {
        let mut w = world();
        let (card, _) = w.reported("sonnet");
        let refused = w.take_over(card, None).unwrap_err().to_string();
        assert!(refused.contains("signed off"), "{refused}");
        assert!(
            w.take_over(9999, None)
                .unwrap_err()
                .to_string()
                .contains("no card")
        );
    }

    #[test]
    fn a_conflict_is_reported_and_undone_and_nothing_is_cleaned_up() {
        let mut w = world();
        let (card, _, _) = w.approved();
        // The main line got its own version of the same file in the meantime.
        std::fs::write(w.repo.join("change.txt"), "main's own\n").unwrap();
        git(&w.repo, &["add", "change.txt"]);
        git(&w.repo, &["commit", "-q", "-m", "main moved on"]);
        let main_before = git(&w.repo, &["rev-parse", "main"]);

        let taken = w.take_over(card, None).unwrap();
        assert!(
            matches!(&taken.outcome, CardTakeOver::Conflict { files } if files == &["change.txt".to_owned()]),
            "{:?}",
            taken.outcome
        );
        assert!(taken.opencode_locations.is_empty());
        assert_eq!(
            git(&w.repo, &["rev-parse", "main"]),
            main_before,
            "the project folder is as it was"
        );
        assert!(
            store::get_card(&w.db, card)
                .unwrap()
                .unwrap()
                .taken_over_at
                .is_none()
        );
        assert_eq!(w.sessions(), 2);
    }

    /// The actor string of a session.
    fn actor(agent: &Agent) -> String {
        actor_from(Some(&agent.id.to_string()), Some(&agent.name)).unwrap()
    }

    #[test]
    fn a_reviewer_of_an_earlier_report_is_retired_and_cannot_review_again() {
        use crate::ide_start::card_launch;
        let mut w = world();
        let (card, worker) = w.reported("sonnet");
        w.roles.retain(|r| r.name != "reviewer");
        w.roles.push(role("reviewer", "review", Some("opus")));
        let first = w.review(card, None).unwrap();
        let first_tree = crate::ide::provision::prepare(&w.db, &w.locations(), first.agent.id)
            .unwrap()
            .cwd;
        flow::review_verdict(
            &mut w.db,
            card,
            &actor(&first.agent),
            flow::Verdict::Return,
            "no",
        )
        .unwrap();
        flow::report_done(&mut w.db, card, &actor(&worker.agent)).unwrap();

        let second = w.review(card, None).unwrap();
        // The first reviewer's session, worktree and secret are gone when the second is made …
        assert!(
            agent_store::get_agent(&w.db, first.agent.id)
                .unwrap()
                .is_none()
        );
        assert!(!first_tree.exists());
        // … and even a copy of its row that survived would not be a reviewer any more.
        assert_eq!(
            card_launch(&w.db, &first.agent),
            None,
            "an earlier report's reviewer is not the current one"
        );
        assert!(card_launch(&w.db, &second.agent).is_some());
    }

    #[test]
    fn a_worker_whose_card_another_session_holds_now_is_not_a_worker_any_more() {
        use crate::ide_start::card_launch;
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let first = w.start(card, Some("sonnet")).unwrap();
        assert!(card_launch(&w.db, &first.agent).is_some());
        assert!(flow::release_started(&mut w.db, card, &actor(&first.agent)).unwrap());
        let second = w.start(card, Some("sonnet")).unwrap();
        // The old pane, restarted, must not go on working on a card the new session holds.
        assert_eq!(card_launch(&w.db, &first.agent), None);
        assert!(card_launch(&w.db, &second.agent).is_some());
    }

    #[test]
    fn work_that_changes_what_agents_obey_is_not_sent_to_a_reviewer_unless_the_owner_accepts() {
        let mut w = world();
        w.roles.retain(|r| r.name != "reviewer");
        w.roles.push(role("reviewer", "review", Some("opus")));
        let card = w.card(w.open, Some("builder"));
        let worker = w.start(card, Some("sonnet")).unwrap();
        let ready = crate::ide::provision::prepare(&w.db, &w.locations(), worker.agent.id).unwrap();
        std::fs::create_dir_all(ready.cwd.join(".claude")).unwrap();
        std::fs::write(
            ready.cwd.join(".claude/settings.json"),
            "{\"permissions\": {\"allow\": [\"Bash\"]}}\n",
        )
        .unwrap();
        std::fs::write(ready.cwd.join("change.txt"), "done\n").unwrap();
        flow::report_done(&mut w.db, card, &actor(&worker.agent)).unwrap();

        let refused = w.review(card, None).unwrap_err().to_string();
        assert!(refused.contains(".claude/settings.json"), "{refused}");
        assert!(refused.contains("obey what the worker wrote"), "{refused}");
        assert!(
            agent_store::list_agents(&w.db, w.project)
                .unwrap()
                .iter()
                .all(|a| !a.card_review),
            "no reviewer was made"
        );
        // The owner who has read that can start it all the same.
        assert!(w.review_allowing(card, None, true).is_ok());
    }

    #[test]
    fn a_card_claimed_by_hand_is_not_taken_over_by_the_studio() {
        let mut w = world();
        // A session of the owner's own making took the card through `claim_task` and a reviewer signed it off.
        let hand = roster::create_agent_on_engine(
            &w.db, &w.config, &w.roles, w.project, "hand", "sonnet", "builder",
        )
        .unwrap();
        let card = w.card(w.open, Some("builder"));
        flow::start_card(&mut w.db, card, &actor(&hand)).unwrap();
        flow::report_done(&mut w.db, card, &actor(&hand)).unwrap();
        flow::review_verdict(
            &mut w.db,
            card,
            "agent:somebody-9",
            flow::Verdict::Approve,
            "",
        )
        .unwrap();
        let refused = w.take_over(card, None).unwrap_err().to_string();
        assert!(refused.contains("gone"), "{refused}");
    }

    #[test]
    fn a_worker_that_still_works_or_waits_is_not_taken_over() {
        let mut w = world();
        let (card, worker, _) = w.approved();
        let channel =
            axiomata_ide::lifecycle::Channel::for_agent(&w.locations().channels, worker.agent.id);
        std::fs::create_dir_all(channel.dir()).unwrap();
        let now = chrono::Utc::now().timestamp();
        for state in ["working", "waiting"] {
            std::fs::write(channel.dir().join("state"), format!("{state} {now}\n")).unwrap();
            let refused = w.take_over(card, None).unwrap_err().to_string();
            assert!(refused.contains(state), "{refused}");
        }
        std::fs::write(channel.dir().join("state"), format!("idle {now}\n")).unwrap();
        assert!(matches!(
            w.take_over(card, None).unwrap().outcome,
            CardTakeOver::Done { .. }
        ));
    }

    #[test]
    fn a_worktree_moved_off_the_workers_branch_is_not_taken_over() {
        let mut w = world();
        let (card, worker, _) = w.approved();
        let tree = agent_store::get_agent(&w.db, worker.agent.id)
            .unwrap()
            .unwrap()
            .worktree_path
            .unwrap();
        // The branch the reviewer saw is where it was, but the worktree no longer sits on it.
        git(&tree, &["checkout", "--quiet", "--detach"]);
        let refused = w.take_over(card, None).unwrap_err().to_string();
        assert!(refused.contains("not on the agent's branch"), "{refused}");
        assert!(
            store::get_card(&w.db, card)
                .unwrap()
                .unwrap()
                .taken_over_at
                .is_none()
        );
    }

    #[test]
    fn after_a_return_the_card_is_taken_over_against_the_snapshot_of_the_reviewer_who_signed() {
        let mut w = world();
        w.roles.retain(|r| r.name != "reviewer");
        w.roles.push(role("reviewer", "review", Some("opus")));
        let (card, worker) = w.reported("sonnet");
        let first = w.review(card, None).unwrap();
        flow::review_verdict(
            &mut w.db,
            card,
            &actor(&first.agent),
            flow::Verdict::Return,
            "more",
        )
        .unwrap();
        // The worker improves the work and reports again; the second reviewer signs.
        let tree = agent_store::get_agent(&w.db, worker.agent.id)
            .unwrap()
            .unwrap()
            .worktree_path
            .unwrap();
        std::fs::write(tree.join("more.txt"), "more\n").unwrap();
        flow::report_done(&mut w.db, card, &actor(&worker.agent)).unwrap();
        let second = w.review(card, None).unwrap();
        flow::review_verdict(
            &mut w.db,
            card,
            &actor(&second.agent),
            flow::Verdict::Approve,
            "",
        )
        .unwrap();

        let taken = w.take_over(card, None).unwrap();
        assert!(
            matches!(taken.outcome, CardTakeOver::Done { .. }),
            "{:?}",
            taken.outcome
        );
        assert_eq!(git(&w.repo, &["show", "main:more.txt"]), "more");
        assert_eq!(w.sessions(), 0);
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
    // ------------------------------------------------------------- the line ---

    impl World {
        /// An approved plan that runs by itself, in the project of the world, with `titles` as its cards (none depends on
        /// another), and its line made. Returns the plan, the cards and the line.
        fn line_plan(&mut self, titles: &[&str]) -> (i64, Vec<i64>, axiomata_ide::plan_line::Line) {
            let plan = flow::create_plan(
                &self.db,
                self.board,
                &axiomata_board::PlanFields {
                    project_id: Some(self.project),
                    goal: String::new(),
                    name: "P".into(),
                    auto_start_max: Some(64),
                    max_cost_usd: None,
                    max_tokens: None,
                },
            )
            .unwrap()
            .id;
            let cards: Vec<i64> = titles
                .iter()
                .map(|title| {
                    store::create_card(
                        &self.db,
                        &NewCard {
                            column_id: self.proposal,
                            fields: CardFields {
                                title: (*title).into(),
                                agent: Some("builder".into()),
                                plan_id: Some(plan),
                                ..CardFields::default()
                            },
                        },
                    )
                    .unwrap()
                    .id
                })
                .collect();
            flow::approve_plan(&mut self.db, plan, "human:owner").unwrap();
            let line = axiomata_ide::plan_line::ensure(
                &self.repo,
                &axiomata_ide::plan_line::line_path(&self.locations().worktrees, "P", plan),
                plan,
                None,
            )
            .unwrap();
            // The studio records where it left the line when it makes it (`make_line`).
            flow::set_line_tip(
                &self.db,
                plan,
                &axiomata_ide::plan_line::tip(&line.path).unwrap(),
            )
            .unwrap();
            (plan, cards, line)
        }

        /// Starts `card` on the line, makes its worktree, writes `content` into `file` there and reports it done; the card is
        /// then signed off by a reviewer that has no session (so what was reviewed is the work as it stands).
        fn worked_on_the_line(
            &mut self,
            card: i64,
            line: &axiomata_ide::plan_line::Line,
            file: &str,
            content: &str,
        ) -> CardSession {
            let started = self
                .start_on(card, Some("sonnet"), Some(&line.branch))
                .unwrap();
            let ready =
                crate::ide::provision::prepare(&self.db, &self.locations(), started.agent.id)
                    .unwrap();
            std::fs::write(ready.cwd.join(file), content).unwrap();
            let actor = actor_from(
                Some(&started.agent.id.to_string()),
                Some(&started.agent.name),
            )
            .unwrap();
            flow::report_done(&mut self.db, card, &actor).unwrap();
            flow::review_verdict(
                &mut self.db,
                card,
                "human:owner",
                flow::Verdict::Approve,
                "",
            )
            .unwrap();
            started
        }

        fn integrate(&mut self, card: i64) -> Result<CardIntegration> {
            let roots = self.locations().channels;
            let worktrees = self.locations().worktrees;
            self.shared(|db| {
                integrate_blocking(db, &roots, &worktrees, card, false).map(|(outcome, _)| outcome)
            })
        }
    }

    #[test]
    fn a_card_of_a_plan_that_runs_by_itself_starts_from_the_line_and_its_work_is_integrated_into_it()
     {
        let mut w = world();
        let (plan, cards, line) = w.line_plan(&["one"]);
        let started = w.worked_on_the_line(cards[0], &line, "one.txt", "one\n");
        // Its branch was cut from the line, which is also what it is reviewed against.
        assert_eq!(
            started.agent.base_branch.as_deref(),
            Some(line.branch.as_str())
        );
        let tree = agent_store::get_agent(&w.db, started.agent.id)
            .unwrap()
            .unwrap()
            .worktree_path
            .unwrap();
        assert_eq!(
            axiomata_ide::worktree::current_branch(&tree).as_deref(),
            Some(
                axiomata_ide::worktree::branch_name(&started.agent.name, started.agent.id).as_str()
            )
        );

        let outcome = w.integrate(cards[0]).unwrap();
        let CardIntegration::Done {
            commit,
            agent_ids,
            plan_id,
            ..
        } = outcome
        else {
            panic!("{outcome:?}")
        };
        assert_eq!((plan_id, agent_ids), (plan, vec![started.agent.id]));
        let commit = commit.expect("it changed something");
        assert_eq!(git(&line.path, &["rev-parse", "HEAD"]), commit);
        assert_eq!(git(&line.path, &["show", "HEAD:one.txt"]), "one");
        // The card says so, and the main line has not moved.
        let card = store::get_card(&w.db, cards[0]).unwrap().unwrap();
        assert_eq!(card.state, crate::board::TaskState::Integrated);
        assert_eq!(git(&w.repo, &["rev-list", "--count", "main"]), "1");
        // The session is gone with its worktree and branch.
        assert!(
            agent_store::get_agent(&w.db, started.agent.id)
                .unwrap()
                .is_none()
        );
        assert!(!tree.exists());
    }

    #[test]
    fn a_card_that_builds_on_another_is_started_only_once_that_ones_work_is_on_the_line() {
        let mut w = world();
        let (_, cards, line) = w.line_plan(&["first", "second"]);
        flow::add_dependency(&mut w.db, cards[1], cards[0]).unwrap();
        // Signed off is not enough: the work must be on the line.
        w.worked_on_the_line(cards[0], &line, "first.txt", "first\n");
        let early = w
            .start_on(cards[1], Some("sonnet"), Some(&line.branch))
            .unwrap_err()
            .to_string();
        assert!(early.contains("not on the plan's line yet"), "{early}");
        // Outside a plan that runs by itself it is refused all the same.
        assert!(
            w.start(cards[1], Some("sonnet"))
                .unwrap_err()
                .to_string()
                .contains("only a plan that runs by itself")
        );

        w.integrate(cards[0]).unwrap();
        let second = w
            .start_on(cards[1], Some("sonnet"), Some(&line.branch))
            .unwrap();
        let ready = crate::ide::provision::prepare(&w.db, &w.locations(), second.agent.id).unwrap();
        // It starts from what the first card did.
        assert_eq!(
            std::fs::read_to_string(ready.cwd.join("first.txt")).unwrap(),
            "first\n"
        );
    }

    #[test]
    fn two_cards_that_changed_the_same_file_the_second_is_put_back_and_the_next_conflict_gives_it_up()
     {
        let mut w = world();
        let (_, cards, line) = w.line_plan(&["a", "b", "c"]);
        // Two run side by side from the same line.
        let a = w.worked_on_the_line(cards[0], &line, "same.txt", "from a\n");
        let b = w.worked_on_the_line(cards[1], &line, "same.txt", "from b\n");
        assert!(matches!(
            w.integrate(cards[0]).unwrap(),
            CardIntegration::Done { .. }
        ));
        let tip = git(&line.path, &["rev-parse", "HEAD"]);

        let conflict = w.integrate(cards[1]).unwrap();
        let CardIntegration::Conflict {
            files,
            gave_up,
            agent_ids,
            ..
        } = conflict
        else {
            panic!("{conflict:?}")
        };
        assert_eq!((files, gave_up), (vec!["same.txt".to_owned()], false));
        assert_eq!(agent_ids, vec![b.agent.id]);
        assert_eq!(
            git(&line.path, &["rev-parse", "HEAD"]),
            tip,
            "the line is as it was"
        );
        // The card is open again to be done on the line as it is, and its old session is gone.
        let card = store::get_card(&w.db, cards[1]).unwrap().unwrap();
        assert_eq!(card.state, crate::board::TaskState::Ready);
        assert!(agent_store::get_agent(&w.db, b.agent.id).unwrap().is_none());
        assert!(agent_store::get_agent(&w.db, a.agent.id).unwrap().is_none());

        // Done again on the line, and meanwhile a third card changed the same file and went in first: the second try
        // conflicts again and the studio gives up.
        let second_try = w.worked_on_the_line(cards[1], &line, "same.txt", "from b, again\n");
        w.worked_on_the_line(cards[2], &line, "same.txt", "from c\n");
        assert!(matches!(
            w.integrate(cards[2]).unwrap(),
            CardIntegration::Done { .. }
        ));
        let again = w.integrate(cards[1]).unwrap();
        let CardIntegration::Conflict {
            gave_up, agent_ids, ..
        } = again
        else {
            panic!("{again:?}")
        };
        assert!(gave_up);
        // Its session is kept, so nothing is to be closed.
        assert!(agent_ids.is_empty());
        // Left as it is: signed off, not on the line, with the reason in its history — and its session kept to look at.
        let card = store::get_card(&w.db, cards[1]).unwrap().unwrap();
        assert_eq!(card.state, crate::board::TaskState::Verified);
        let events = flow::list_events(&w.db, cards[1], 50).unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.text.starts_with("gave up integrating"))
        );
        assert!(
            agent_store::get_agent(&w.db, second_try.agent.id)
                .unwrap()
                .is_some()
        );

        // The studio's own looks leave it alone from now on, and say so without touching anything.
        let events_before = flow::list_events(&w.db, cards[1], 50).unwrap().len();
        let roots = w.locations().channels;
        let worktrees = w.locations().worktrees;
        let left = w
            .shared(|db| integrate_blocking(db, &roots, &worktrees, cards[1], false))
            .unwrap()
            .0;
        assert_eq!(left, CardIntegration::LeftForOwner { card_id: cards[1] });
        assert_eq!(
            flow::list_events(&w.db, cards[1], 50).unwrap().len(),
            events_before
        );
        assert_eq!(
            store::get_card(&w.db, cards[1]).unwrap().unwrap().state,
            crate::board::TaskState::Verified
        );
    }

    #[test]
    fn a_card_that_is_not_signed_off_or_whose_work_changed_after_the_review_is_not_integrated() {
        let mut w = world();
        let (_, cards, line) = w.line_plan(&["a"]);
        w.start_on(cards[0], Some("sonnet"), Some(&line.branch))
            .unwrap();
        let refused = w.integrate(cards[0]).unwrap_err().to_string();
        assert!(refused.contains("signed off"), "{refused}");
    }

    impl World {
        /// [`World::worked_on_the_line`], but a reviewer *session* signs the card off, so the gate has a snapshot to hold the
        /// work against.
        fn reviewed_on_the_line(
            &mut self,
            card: i64,
            line: &axiomata_ide::plan_line::Line,
        ) -> (CardSession, ReviewSession) {
            self.roles.retain(|r| r.name != "reviewer");
            self.roles.push(role("reviewer", "review", Some("opus")));
            let started = self
                .start_on(card, Some("sonnet"), Some(&line.branch))
                .unwrap();
            let ready =
                crate::ide::provision::prepare(&self.db, &self.locations(), started.agent.id)
                    .unwrap();
            std::fs::write(ready.cwd.join("a.txt"), "a\n").unwrap();
            let worker = actor_from(
                Some(&started.agent.id.to_string()),
                Some(&started.agent.name),
            )
            .unwrap();
            flow::report_done(&mut self.db, card, &worker).unwrap();
            let review = self.review(card, None).unwrap();
            let reviewer =
                actor_from(Some(&review.agent.id.to_string()), Some(&review.agent.name)).unwrap();
            flow::review_verdict(&mut self.db, card, &reviewer, flow::Verdict::Approve, "")
                .unwrap();
            (started, review)
        }
    }

    #[test]
    fn work_that_changed_after_the_review_is_not_integrated() {
        let mut w = world();
        let (plan, cards, line) = w.line_plan(&["a"]);
        let (started, _) = w.reviewed_on_the_line(cards[0], &line);
        let tree = agent_store::get_agent(&w.db, started.agent.id)
            .unwrap()
            .unwrap()
            .worktree_path
            .unwrap();
        let line_before = axiomata_ide::plan_line::tip(&line.path).unwrap();

        std::fs::write(tree.join("late.txt"), "sneaked in\n").unwrap();
        let refused = w.integrate(cards[0]).unwrap_err().to_string();
        assert!(
            refused.contains("changed after it was reviewed"),
            "{refused}"
        );

        assert_eq!(
            axiomata_ide::plan_line::tip(&line.path).unwrap(),
            line_before,
            "nothing reached the line"
        );
        let card = store::get_card(&w.db, cards[0]).unwrap().unwrap();
        assert!(card.integrated_at.is_none());
        assert_eq!(card.plan_id, Some(plan));
        assert_eq!(w.sessions(), 2, "the sessions are still there");
    }

    #[test]
    fn a_signature_of_a_reviewer_session_that_is_gone_is_not_enough_to_integrate() {
        let mut w = world();
        let (_, cards, line) = w.line_plan(&["a"]);
        let (_, review) = w.reviewed_on_the_line(cards[0], &line);
        // Nothing is left to say what the reviewer saw: that must not read as "the owner signed it off".
        assert!(agent_store::delete_agent(&w.db, review.agent.id).unwrap());

        let refused = w.integrate(cards[0]).unwrap_err().to_string();
        assert!(refused.contains("is gone"), "{refused}");
        assert!(
            store::get_card(&w.db, cards[0])
                .unwrap()
                .unwrap()
                .integrated_at
                .is_none()
        );
    }

    #[test]
    fn a_card_that_changed_nothing_is_integrated_without_a_commit() {
        let mut w = world();
        let (_, cards, line) = w.line_plan(&["a"]);
        let started = w
            .start_on(cards[0], Some("sonnet"), Some(&line.branch))
            .unwrap();
        crate::ide::provision::prepare(&w.db, &w.locations(), started.agent.id).unwrap();
        let worker = actor_from(
            Some(&started.agent.id.to_string()),
            Some(&started.agent.name),
        )
        .unwrap();
        flow::report_done(&mut w.db, cards[0], &worker).unwrap();
        flow::review_verdict(
            &mut w.db,
            cards[0],
            "human:owner",
            flow::Verdict::Approve,
            "",
        )
        .unwrap();
        let line_before = axiomata_ide::plan_line::tip(&line.path).unwrap();

        let outcome = w.integrate(cards[0]).unwrap();
        let CardIntegration::Done { commit, .. } = outcome else {
            panic!("expected the card to be integrated: {outcome:?}");
        };
        assert_eq!(commit, None);
        assert_eq!(
            axiomata_ide::plan_line::tip(&line.path).unwrap(),
            line_before
        );
        let card = store::get_card(&w.db, cards[0]).unwrap().unwrap();
        assert!(card.integrated_at.is_some());
        assert_eq!(w.sessions(), 0);
    }

    #[test]
    fn a_line_that_is_ahead_of_its_record_by_the_studios_own_commit_is_taken_up_again() {
        let mut w = world();
        let (plan, cards, line) = w.line_plan(&["a", "b"]);
        let start = axiomata_ide::plan_line::tip(&line.path).unwrap();
        w.worked_on_the_line(cards[0], &line, "a.txt", "a\n");
        assert!(matches!(
            w.integrate(cards[0]).unwrap(),
            CardIntegration::Done { .. }
        ));
        // The record is lost, as after a crash between the commit and the write.
        flow::set_line_tip(&w.db, plan, &start).unwrap();

        w.worked_on_the_line(cards[1], &line, "b.txt", "b\n");
        assert!(matches!(
            w.integrate(cards[1]).unwrap(),
            CardIntegration::Done { .. }
        ));
        let recorded = flow::get_plan(&w.db, plan).unwrap().unwrap().line_tip;
        assert_eq!(
            recorded,
            Some(axiomata_ide::plan_line::tip(&line.path).unwrap())
        );
    }

    #[test]
    fn a_repository_that_names_a_filter_is_not_staged_in_and_nothing_is_integrated() {
        let mut w = world();
        let (_, cards, line) = w.line_plan(&["a"]);
        w.worked_on_the_line(cards[0], &line, "a.txt", "a\n");
        git(
            &w.repo,
            &["config", "filter.evil.clean", "touch /tmp/never"],
        );
        let line_before = axiomata_ide::plan_line::tip(&line.path).unwrap();

        let refused = w.integrate(cards[0]).unwrap_err().to_string();
        assert!(refused.contains("filter.evil.clean"), "{refused}");
        assert_eq!(
            axiomata_ide::plan_line::tip(&line.path).unwrap(),
            line_before
        );
    }

    #[test]
    fn the_cards_of_a_plan_that_runs_by_itself_are_not_taken_over_one_by_one() {
        let mut w = world();
        let (_, cards, line) = w.line_plan(&["a"]);
        w.worked_on_the_line(cards[0], &line, "a.txt", "a\n");
        let refused = plan_take_over(&w.db, cards[0], None)
            .map(|_| ())
            .unwrap_err()
            .to_string();
        assert!(refused.contains("taken over as a whole"), "{refused}");
    }
}
