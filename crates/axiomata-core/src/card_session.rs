//! Starting a card: the owner's click that makes a new session for it (`docs/plans/a2a.md`, A3, A5, A20, CP-A6a).
//!
//! A *session* is an agent row with a worktree of its own and a branch, made for exactly one card from a **role** (what
//! the card names, else `allrounder`) at a **tier** (the card's, else the role's default) on an **engine** of that tier
//! (`docs/plans/agents-model.md`, AM5, Q13, Q15: the owner's default engine of the tier, else the cheapest that can
//! run — or the one the owner picks now). A role's own `engine` no longer decides anything.
//! The app claims the card for the new session itself, atomically and before any process exists (A20): the claim is the
//! session's actor, so the card's `claimed_by` says who works on it, and a failed claim leaves nothing behind.
//!
//! This module only *makes* the session and takes the card. The harness is started when the session's pane — or the
//! CLI — starts it ([`crate::ide_start::start_agent`]), which sees that the session holds a card and starts it
//! unattended, with the card in its MCP server and the start prompt as its first words.

use axiomata_roster::{Engine, Role, Tier};
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;
use serde::Serialize;

use crate::AxiomataCore;
use crate::AxiomataError;
use crate::agent_mcp::Capabilities;
use crate::board::{flow, store as board_store};
use crate::config::Config;
use crate::error::coded;
use crate::ide::agent_store;
use crate::ide::model::Agent;
use crate::ide::{store as project_store, worktree};
use crate::model_tiers::ModelSources;
use crate::roster::{self, no_card, no_engine, no_plan, no_project, no_role, refusal};
use crate::session::actor_from;

/// The role a card without one is given (the one every install has).
const DEFAULT_ROLE: &str = "allrounder";

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

/// The engines of `tier` an unattended session may run on, in the order they are tried ([`crate::engines::engine_for`]:
/// the owner's default engine of the tier, then the cheapest), without those that run a command of their own.
fn tier_engines(
    config: &Config,
    sources: &ModelSources,
    tier: Tier,
) -> Vec<crate::engines::EngineInfo> {
    crate::engines::engine_for(tier, config, sources)
        .into_iter()
        .filter(|info| info.engine.command.trim().is_empty())
        .collect()
}

/// The engine a session at `tier` runs on: the one the owner asked for, else the tier's first that can run
/// (`docs/plans/agents-model.md`, Q13).
///
/// # Errors
///
/// A refusal naming what to do: the asked-for engine does not exist or runs a command of its own, or no engine of the
/// tier can run an unattended session.
pub(crate) fn choose_engine(
    config: &Config,
    tier: Tier,
    requested: Option<&str>,
) -> Result<(String, Engine)> {
    if let Some(asked) = requested {
        let engine = crate::engines::Lookup::new(config)
            .engine(asked)
            .ok_or_else(|| {
                coded(
                    "engine.unknownAddInStudio",
                    &[("id", asked)],
                    format!(
                        "there is no engine “{asked}”; engines are added in the Studio settings"
                    ),
                )
            })?;
        if !engine.command.trim().is_empty() {
            return Err(coded(
                "engine.ownCommand",
                &[("id", asked)],
                format!(
                    "the engine “{asked}” runs a command of its own, which Axiomata does not touch; an unattended \
                     session needs the generated command"
                ),
            ));
        }
        return Ok((engine.id.clone(), engine));
    }
    tier_engines(config, &ModelSources::load(), tier)
        .into_iter()
        .next()
        .map(|info| (info.engine.id.clone(), info.engine))
        .ok_or_else(|| {
            coded(
                "engine.noneForTierUnattended",
                &[("tier", tier.as_str())],
                format!(
                    "no engine of the tier {} can run an unattended session; switch a model of that tier on or set \
                     the tier's default engine in the Studio's Models tab, or pick an engine when you start it",
                    tier.as_str()
                ),
            )
        })
}

/// The tier a session's name says: the card's — unless the owner picked the engine, whose own tier it then runs at.
fn named_tier(config: &Config, engine: &Engine, card_tier: Tier, picked: bool) -> Tier {
    if !picked {
        return card_tier;
    }
    crate::engines::find(config, &ModelSources::load(), &engine.id)
        .and_then(|info| info.rated_tier)
        .unwrap_or(card_tier)
}

/// The name of a new session on `engine_id` (`docs/plans/agents-model.md`, Q8): "Glut Medium Allrounder (Sonnet 5.5)",
/// with ` #2`, ` #3`, … while the project has one of that name. The card's number stays on the card and its tile.
///
/// # Errors
///
/// A refusal for an engine that is gone, or after too many sessions of one name.
pub(crate) fn session_name(
    config: &Config,
    taken: &[Agent],
    engine_id: &str,
    tier: Tier,
    role: &str,
) -> Result<String> {
    let info = crate::engines::find(config, &ModelSources::load(), engine_id)
        .ok_or_else(|| no_engine(engine_id))?;
    let names: Vec<String> = taken.iter().map(|agent| agent.name.clone()).collect();
    crate::agent_start::session_name(&info, tier, role, &names)
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
        .ok_or_else(|| no_project(request.project_id))?;
    if !worktree::is_repo(&project.repo_root) {
        return Err(coded(
            "card.projectNotRepo",
            &[],
            "an unattended session needs its own worktree, and this project is not a git repository",
        ));
    }
    if crate::agent_entry::cli_path().is_none() {
        return Err(coded(
            "card.noCli",
            &[],
            "a session working on a card needs the team tools, and no `axiomata-cli` was found next to the app \
             (build the workspace, or set AXIOMATA_CLI)",
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

/// Starts a board card as a plan (`docs/plans/card-as-plan.md`): a draft plan in the project whose goal is the card,
/// with the card in progress. The project is checked the way a card session's is — the plan's steps will run unattended
/// in worktrees of it. The caller starts the planner and rewrites the board's mirror.
///
/// # Errors
///
/// A refusal that says why: no such project, a project that is no repository, or the board's own refusal (see
/// [`flow::start_card_as_plan`]).
pub fn start_card_plan(
    core: &AxiomataCore,
    card_id: i64,
    project_id: i64,
) -> Result<crate::board::Plan> {
    start_card_plan_in(&mut core.db_lock(), card_id, project_id)
}

fn start_card_plan_in(
    db: &mut Connection,
    card_id: i64,
    project_id: i64,
) -> Result<crate::board::Plan> {
    let project =
        project_store::get_project(db, project_id)?.ok_or_else(|| no_project(project_id))?;
    if !worktree::is_repo(&project.repo_root) {
        return Err(coded(
            "plan.projectNotRepo",
            &[],
            "a plan's steps run unattended in worktrees of their own, and this project is not a git repository",
        ));
    }
    Ok(flow::start_card_as_plan(db, card_id, project_id, OWNER)?)
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
            let project = plan.project_id.unwrap_or_default();
            return Err(coded(
                "card.otherProject",
                &[
                    ("card", &card.id.to_string()),
                    ("plan", &plan.id.to_string()),
                    ("project", &project.to_string()),
                ],
                format!(
                    "card #{} belongs to plan #{}, which runs in project {project}; start it there",
                    card.id, plan.id
                ),
            ));
        }
        let project = project_store::get_project(&db, request.project_id)?
            .ok_or_else(|| no_project(request.project_id))?;
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
    checked_line(
        &core.db,
        &crate::paths::ide_locations().worktrees,
        plan,
        &project.name,
        &project.repo_root,
    )
    .map(|(line, _)| line)
}

/// Finds or makes plan `plan`'s line and checks it is where the studio left it — the one place every use of a line starts
/// from, so a check added here reaches starting a card, integrating one and taking the plan over alike. Returns the line
/// and the commit it was checked at. Records the branch the line was cut from when it is made.
fn checked_line(
    db: &Mutex<Connection>,
    worktrees: &std::path::Path,
    plan: &crate::board::Plan,
    project_name: &str,
    repo_root: &std::path::Path,
) -> Result<(axiomata_ide::plan_line::Line, String)> {
    let path = axiomata_ide::plan_line::line_path(worktrees, project_name, plan.id);
    let line =
        axiomata_ide::plan_line::ensure(repo_root, &path, plan.id, plan.base_branch.as_deref())?;
    if plan.base_branch.is_none() {
        flow::set_plan_base_branch(&lock(db), plan.id, &line.base_branch)?;
    }
    // A line the studio just made is at the base; one it did not make must be where it left it: a branch of that name is
    // a branch like any other, and work put on it by something else is work no reviewer saw.
    let tip = check_line_tip(db, plan, &line)?;
    Ok((line, tip))
}

/// [`start_card_session`] without the checks of the machine (a repository, the CLI), so tests need neither.
fn start_in(
    db: &mut Connection,
    config: &Config,
    roles: &[Role],
    request: &StartRequest,
    line_base: Option<&str>,
) -> Result<CardSession> {
    let card =
        board_store::get_card(db, request.card_id)?.ok_or_else(|| no_card(request.card_id))?;
    // The first step of a stacked card is the branch of the predecessor (A16); that comes with the next stage, and a
    // session cut from the main branch would work without what the card builds on.
    // On the line of its plan, a card that builds on others starts from what they did: it waits until their work is on
    // the line. Anywhere else a card with predecessors cannot be started by the studio.
    match line_base {
        None if !card.depends_on.is_empty() => {
            return Err(coded(
                "card.hasPredecessors",
                &[
                    ("card", &card.id.to_string()),
                    ("dep", &card.depends_on[0].to_string()),
                ],
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
                    return Err(coded(
                        "card.predecessorNotOnLine",
                        &[("card", &card.id.to_string()), ("dep", &dep.to_string())],
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
        .ok_or_else(|| no_role(&role_name))?;
    if !Capabilities::of(role).work {
        return Err(coded(
            "role.doesNotWork",
            &[("name", &role_name)],
            format!(
                "the role “{role_name}” does not work cards (it reviews or plans); pick a role of kind implement for \
                 this card"
            ),
        ));
    }
    let tier = needed_tier(&card, Some(role));
    let (engine_id, engine) = choose_engine(config, tier, request.engine_id.as_deref())?;
    let engine_id = engine_id.as_str();
    let name = session_name(
        config,
        &agent_store::list_agents(db, request.project_id)?,
        engine_id,
        named_tier(config, &engine, tier, request.engine_id.is_some()),
        &role_name,
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
        return Err(coded(
            "agent.invalidActor",
            &[("name", &name)],
            format!("“{name}” makes no valid actor"),
        ));
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

/// The review role a card gets when it names none of kind `review` (one of the seven fixed roles, Q5).
const DEFAULT_REVIEWER: &str = "reviewer";

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

/// The tier a card is worked at (`docs/plans/agents-model.md`, Q15): the card's own tier (the planner sets it by how
/// hard the card is), else the default tier of the role that works it.
fn needed_tier(card: &crate::board::Card, worker_role: Option<&Role>) -> Tier {
    match card.tier {
        Some(crate::board::Tier::Light) => Tier::Light,
        Some(crate::board::Tier::Medium) => Tier::Medium,
        Some(crate::board::Tier::Heavy) => Tier::Heavy,
        None => worker_role.map_or(Tier::Medium, |role| role.tier),
    }
}

/// The role that reviews a card: the one the card names when it is of kind `review`, else `reviewer`, else the first
/// role of kind `review` by name.
fn review_role<'a>(roles: &'a [Role], card: &crate::board::Card) -> Option<&'a Role> {
    let reviews = |role: &&Role| role.kind == KIND_REVIEW;
    card.agent
        .as_deref()
        .and_then(|name| roles.iter().filter(reviews).find(|role| role.name == name))
        .or_else(|| {
            roles
                .iter()
                .filter(reviews)
                .find(|role| role.name == DEFAULT_REVIEWER)
        })
        .or_else(|| {
            roles
                .iter()
                .filter(reviews)
                .min_by(|a, b| a.name.cmp(&b.name))
        })
}

/// The tier the worker of a card worked at: the card's (or its role's default), or higher when its session runs on an
/// engine of a higher tier — after an escalation it does.
fn worker_tier(
    config: &Config,
    sources: &ModelSources,
    card: &crate::board::Card,
    worker_role: Option<&Role>,
    worker_engine: Option<&str>,
) -> Tier {
    let engine_tier = worker_engine
        .and_then(|id| crate::engines::find(config, sources, id))
        .and_then(|info| info.rated_tier);
    needed_tier(card, worker_role).max(engine_tier.unwrap_or(Tier::Light))
}

/// The reviewer of a card, the tier it reviews at and its engine (`docs/plans/agents-model.md`, Q15): the review role
/// ([`review_role`]) at the worker's tier or above, on the first engine of those tiers — the worker's tier first — that
/// is not the worker's: a model marking its own homework agrees with itself (A21). With none, the owner is asked; the
/// worker's engine is never taken, not even when the owner names it.
///
/// # Errors
///
/// No role of kind `review`, an asked-for engine that is the worker's, gone or runs its own command, or no engine of
/// the worker's tier or above but the worker's that can run.
fn choose_reviewer<'a>(
    config: &Config,
    roles: &'a [Role],
    card: &crate::board::Card,
    worker_role: Option<&Role>,
    worker_engine: Option<&str>,
    requested: Option<&str>,
) -> Result<(&'a Role, Tier, String)> {
    let role = review_role(roles, card).ok_or_else(|| {
        coded(
            "review.noRole",
            &[],
            "there is no role of kind `review` for this project; add one in the Studio",
        )
    })?;
    let sources = ModelSources::load();
    let needed = worker_tier(config, &sources, card, worker_role, worker_engine);
    if let Some(asked) = requested {
        if Some(asked) == worker_engine {
            return Err(coded(
                "review.sameEngine",
                &[],
                "the reviewer must run on another engine than the session that worked on the card",
            ));
        }
        let (id, engine) = choose_engine(config, needed, Some(asked))?;
        return Ok((role, named_tier(config, &engine, needed, true), id));
    }
    for tier in Tier::ALL.into_iter().filter(|tier| *tier >= needed) {
        let engines = tier_engines(config, &sources, tier);
        // Never the worker's engine (A21): a model marking its own homework agrees with itself.
        if let Some(info) = engines
            .iter()
            .find(|info| Some(info.engine.id.as_str()) != worker_engine)
        {
            return Ok((role, tier, info.engine.id.clone()));
        }
    }
    Err(coded(
        "review.noEngine",
        &[("tier", needed.as_str())],
        format!(
            "the reviewer needs an engine of the tier {} or above other than the worker's, and none can run; switch \
             another model of that tier on in the Studio's Models tab, or pick one",
            needed.as_str()
        ),
    ))
}

fn refuse_review(reason: &str) -> AxiomataError {
    refusal("review", reason.to_owned())
}

/// The refusal when the session that worked on a card is gone (`card.workerGone`).
fn worker_gone() -> AxiomataError {
    coded(
        "card.workerGone",
        &[],
        "the session that worked on the card is gone",
    )
}

/// The refusal for a review of a card that waits for none (`review.notWaiting`).
fn not_waiting_for_review() -> AxiomataError {
    coded(
        "review.notWaiting",
        &[],
        "the card is not waiting for a review",
    )
}

/// The refusal for a second review of one report (`review.alreadyRunning`).
fn reviewed_already(reviewer: &str) -> AxiomataError {
    coded(
        "review.alreadyRunning",
        &[("name", reviewer)],
        format!("the card is being reviewed already, by {reviewer}"),
    )
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
        .ok_or_else(worker_gone)
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
    /// The tier the reviewer reviews at.
    tier: Tier,
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
    tokio::task::spawn_blocking(move || {
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
    .map_err(|err| refuse_review(&format!("the review task failed: {err}")))?
}

/// [`start_review_session`] without the runtime and with the machine's `axiomata-cli` answered by the caller, so tests
/// need neither.
fn review_blocking(
    db: &Mutex<Connection>,
    config: &Config,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
    roles_of: &dyn Fn(&Connection, i64) -> Vec<Role>,
    request: &ReviewRequest,
    cli_available: bool,
) -> Result<ReviewSession> {
    let plan = plan_review(&lock(db), config, roles_of, request, cli_available)?;
    let snapshot = snapshot_for_review(&plan, request.allow_agent_config)?;
    let (session, retired) = finish_review(&lock(db), config, &plan, &snapshot)?;
    remove_sessions(db, roots, &retired);
    Ok(session)
}

/// Phase one, under the lock: everything that can be refused, asked before anything is committed.
fn plan_review(
    db: &Connection,
    config: &Config,
    roles_of: &dyn Fn(&Connection, i64) -> Vec<Role>,
    request: &ReviewRequest,
    cli_available: bool,
) -> Result<ReviewPlan> {
    let card =
        board_store::get_card(db, request.card_id)?.ok_or_else(|| no_card(request.card_id))?;
    let column = board_store::get_column(db, card.column_id)?;
    if column.and_then(|column| column.stage) != Some(crate::board::ColumnStage::Review)
        || card.state != crate::board::TaskState::InReview
        || card.archived_at.is_some()
    {
        return Err(not_waiting_for_review());
    }
    let worker = card
        .claimed_by
        .as_deref()
        .and_then(session_id_of)
        .and_then(|id| agent_store::get_agent(db, id).ok().flatten())
        .filter(|worker| worker.card_id == Some(card.id) && !worker.card_review)
        .ok_or_else(worker_gone)?;
    let project = project_store::get_project(db, worker.project_id)?
        .ok_or_else(|| no_project(worker.project_id))?;
    if !worktree::is_repo(&project.repo_root) {
        return Err(coded(
            "review.projectNotRepo",
            &[],
            "a reviewer needs a worktree of its own, and this project is not a git repository",
        ));
    }
    if !cli_available {
        return Err(coded(
            "review.noCli",
            &[],
            "a reviewer needs the team tools, and no `axiomata-cli` was found next to the app (build the workspace, or \
             set AXIOMATA_CLI)",
        ));
    }
    let sessions = agent_store::list_agents(db, worker.project_id)?;
    if let Some(existing) = current_reviewer(db, card.id, &sessions)? {
        return Err(reviewed_already(&existing.name));
    }
    let roles = roles_of(db, worker.project_id);
    let worker_role = roles.iter().find(|role| role.name == worker.agent_role);
    let (role, tier, engine_id) = choose_reviewer(
        config,
        &roles,
        &card,
        worker_role,
        worker.engine_id.as_deref(),
        request.engine_id.as_deref(),
    )?;
    let role = role.clone();
    // The name is asked now too, so that a refusal does not come after the snapshot commit.
    session_name(config, &sessions, &engine_id, tier, &role.name)?;
    let repo = crate::ide::provision::agent_repo(db, worker.id)?.ready()?;
    Ok(ReviewPlan {
        card,
        worker,
        repo,
        roles,
        role,
        tier,
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
        let files = touched.join(", ");
        return Err(coded(
            "review.touchesAgentConfig",
            &[("files", &files)],
            format!(
                "the work changes files agents take their configuration from ({files}), and a reviewer started on that \
                 checkout would obey what the worker wrote there. Review it yourself, or start the review by hand and \
                 accept it"
            ),
        ));
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
    let card = board_store::get_card(db, plan.card.id)?.ok_or_else(|| no_card(plan.card.id))?;
    if card.state != crate::board::TaskState::InReview {
        return Err(not_waiting_for_review());
    }
    let sessions = agent_store::list_agents(db, plan.worker.project_id)?;
    if let Some(existing) = current_reviewer(db, card.id, &sessions)? {
        return Err(reviewed_already(&existing.name));
    }
    let name = session_name(
        config,
        &sessions,
        &plan.engine_id,
        plan.tier,
        &plan.role.name,
    )?;
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
    /// The work is in the project's main line, the card is closed and archived, and the sessions made for it are gone.
    /// `cleanup` lists what could not be done — the work is taken over all the same. `already_on_base` says that
    /// nothing was committed: the base branch held the work already, so `commit` names that branch's tip instead of a
    /// commit made here. The card ends all the same, which is the point.
    Done {
        /// Where the work sits on the base branch: the commit this take-over made, or — when `already_on_base` — the
        /// tip that already held it.
        commit: String,
        /// The project the work went into — where the commit is waiting to be pushed.
        project_id: i64,
        /// Whether nothing was committed: the base branch held the work already.
        already_on_base: bool,
        cleanup: Vec<String>,
    },
    /// The squash conflicted and was undone: the project folder is as it was (G9). Nothing else changed.
    Conflict { files: Vec<String> },
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
/// On success `commit` names where the work sits on the base branch — the squash commit this take-over made, or the
/// tip that already held it when `already_on_base` is set — the card is marked taken over (and archived), and the
/// sessions made for the card — the worker and its reviewers — are cleaned up: worktrees removed, the worker's branch
/// deleted, the sessions forgotten with their secrets and Opencode registrations. A conflict is undone and reported.
/// The database is locked only to read and to write, never while git runs.
///
/// When the base branch holds the reviewed state already there is nothing to squash — the owner put it there by hand,
/// or the worker's own commits amount to no change — and the take-over says so instead of refusing (`already_on_base`
/// is set): the card still closes and its sessions still go, which is what taking it over is for. Refusing would
/// leave the card signed off, never taken over, its sessions alive as long as the app runs.
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
    tokio::task::spawn_blocking(move || {
        take_over_blocking(&db, &roots, card_id, message.as_deref())
    })
    .await
    .map_err(|err| refusal("take-over", format!("the take-over task failed: {err}")))?
}

fn take_over_blocking(
    db: &Mutex<Connection>,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
    card_id: i64,
    message: Option<&str>,
) -> Result<CardTakeOver> {
    let plan = plan_take_over(&lock(db), card_id, message)?;
    let (commit, already_on_base) = match squash(&plan, roots)? {
        Squashed::Conflict { files } => return Ok(CardTakeOver::Conflict { files }),
        Squashed::Committed { commit } => (commit, false),
        Squashed::AlreadyThere { commit } => (commit, true),
    };
    // The work is in the main line from here on: nothing below can undo it, so a step that fails is a note.
    let mut cleanup = Vec::new();
    match flow::mark_taken_over(&lock(db), plan.card.id, OWNER) {
        Ok(true) => {}
        Ok(false) => cleanup.push("the card could not be marked as taken over".to_owned()),
        Err(err) => cleanup.push(format!("the card could not be marked as taken over: {err}")),
    }
    let mut removals = begin_removal(&lock(db), &plan.sessions);
    remove_trees(&mut removals, &mut cleanup);
    finish_removal(&lock(db), roots, &removals, &mut cleanup);
    Ok(CardTakeOver::Done {
        commit,
        project_id: plan.card_project,
        already_on_base,
        cleanup,
    })
}

/// Phase one of a take-over, under the lock: the card, the worker and what was reviewed.
fn plan_take_over(db: &Connection, card_id: i64, message: Option<&str>) -> Result<TakeOverPlan> {
    let card = board_store::get_card(db, card_id)?.ok_or_else(|| no_card(card_id))?;
    if card.verified_by.is_none() {
        return Err(coded(
            "card.takeOverNotSignedOff",
            &[],
            "only a card the reviewer signed off can be taken over",
        ));
    }
    if card.taken_over_at.is_some() {
        return Err(coded(
            "card.takenOverAlready",
            &[],
            "the card was taken over already",
        ));
    }
    // A card of a plan that runs by itself goes into the plan's line, and the line is taken over as a whole.
    if card
        .plan_id
        .and_then(|id| flow::get_plan(db, id).ok().flatten())
        .is_some_and(|plan| runs_by_itself(&plan))
    {
        return Err(coded(
            "card.takeOverPlanCard",
            &[],
            "this card belongs to a plan that runs by itself: its work is integrated into the plan's line, and the plan \
             is taken over as a whole",
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
        .ok_or_else(worker_gone)?;
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
        return Err(coded(
            "card.reviewerGone",
            &[],
            "the reviewer that signed the card off is gone, so there is no way to tell that the work is what it saw; send \
             the card back for a new review",
        ));
    }
    match reviewed {
        Some(reviewed) => {
            if repo.is_dirty()? || repo.branch_tip()? != reviewed {
                return Err(coded(
                    "card.changedAfterReview",
                    &[],
                    "the work changed after it was reviewed; send the card back for a new review instead of taking it \
                     over",
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

/// What the squash of a take-over came to (phase two of [`take_over_blocking`]).
enum Squashed {
    /// A new commit on the base branch.
    Committed { commit: String },
    /// The agent's branch contributes nothing the base does not already have — the owner merged it by hand, or the
    /// branch's own commits amount to no change. Nothing was committed; `commit` is the base's tip, which holds the
    /// work. The card still ends here.
    AlreadyThere { commit: String },
    /// A conflict, undone: the base branch is as it was.
    Conflict { files: Vec<String> },
}

/// Phase two of a take-over, no lock: the gate, and the squash.
fn squash(plan: &TakeOverPlan, roots: &axiomata_ide::lifecycle::ChannelRoots) -> Result<Squashed> {
    // The gate is the whole point of this phase: it refuses unless the worker's branch is at exactly the commit the
    // reviewer signed. Which commit that is in the end is the git layer's answer to give — it reports where the work
    // sits, and whether it had to commit it there (`committed`).
    gate_reviewed(&plan.card, &plan.sessions, &plan.repo, &plan.message)?;
    let done = plan.target.run(
        roots,
        axiomata_ide::git::TakeOverMode::Squash,
        &plan.message,
    )?;
    Ok(match done {
        axiomata_ide::git::TakeOver::Done {
            commit,
            committed: true,
        } => Squashed::Committed { commit },
        // Nothing to commit: the work is on the base already. Refusing here — which is what this layer used to do —
        // would strand the card forever: signed off, never taken over, its sessions alive. The card closing is what
        // taking it over is for, so it closes either way (`already_on_base`).
        axiomata_ide::git::TakeOver::Done {
            commit,
            committed: false,
        } => Squashed::AlreadyThere { commit },
        axiomata_ide::git::TakeOver::Conflict { files } => Squashed::Conflict { files },
    })
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
/// stays keeps its row.
fn finish_removal(
    db: &Connection,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
    removals: &[Removal],
    notes: &mut Vec<String>,
) {
    for removal in removals {
        let session = &removal.session;
        if let Err(err) = axiomata_ide::session_token::revoke(roots, session.id) {
            notes.push(format!("the secret of {} stays valid: {err}", session.name));
        }
        if removal.tree_stays {
            continue;
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
}

/// Ends sessions in the three steps above, taking the lock only for the first and the last. Problems are logged: a
/// retired reviewer that lingers is no reason to refuse.
pub(crate) fn remove_sessions(
    db: &Mutex<Connection>,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
    sessions: &[Agent],
) {
    if sessions.is_empty() {
        return;
    }
    let mut notes = Vec::new();
    let mut removals = begin_removal(&lock(db), sessions);
    remove_trees(&mut removals, &mut notes);
    finish_removal(&lock(db), roots, &removals, &mut notes);
    for note in notes {
        tracing::warn!(%note, "retiring a session");
    }
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
    tokio::task::spawn_blocking(move || integrate_blocking(&db, &roots, &worktrees, card_id, owner))
        .await
        .map_err(|err| refusal("integration", format!("the integration task failed: {err}")))?
}

fn integrate_blocking(
    db: &Mutex<Connection>,
    roots: &axiomata_ide::lifecycle::ChannelRoots,
    worktrees: &std::path::Path,
    card_id: i64,
    owner: bool,
) -> Result<CardIntegration> {
    let plan = plan_integration(&lock(db), card_id)?;
    // A card the studio gave up on stays with the owner: looked at again every few seconds it would be put back, redone and
    // paid for again and again.
    if plan.gave_up && !owner {
        return Ok(CardIntegration::LeftForOwner { card_id });
    }
    // The worker is idle once it has reported and been reviewed; one that is still in a turn is left alone until it is.
    let state = axiomata_ide::lifecycle::Channel::for_agent(roots, plan.worker.id)
        .read_status(plan.worker.harness)
        .state;
    if matches!(
        state,
        axiomata_ide::lifecycle::AgentState::Working | axiomata_ide::lifecycle::AgentState::Waiting
    ) {
        return Ok(CardIntegration::Busy { card_id });
    }
    let (line, _) = checked_line(
        db,
        worktrees,
        &plan.plan,
        &plan.project.name,
        &plan.project.repo_root,
    )?;
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
                    return Err(coded(
                        "card.doesNotFitLine",
                        &[],
                        "the card does not fit the line, and could not be put back",
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
    if !keep {
        remove_sessions(db, roots, &plan.sessions);
    }
    for note in notes {
        tracing::warn!(%note, "integrating a card");
    }
    Ok(result)
}

/// Phase one, under the lock: everything that can be refused.
fn plan_integration(db: &Connection, card_id: i64) -> Result<IntegrationPlan> {
    let card = board_store::get_card(db, card_id)?.ok_or_else(|| no_card(card_id))?;
    if card.verified_by.is_none() {
        return Err(coded(
            "card.integrateNotSignedOff",
            &[],
            "only a card the reviewer signed off can be integrated",
        ));
    }
    if card.integrated_at.is_some() || card.taken_over_at.is_some() {
        return Err(coded(
            "card.integratedAlready",
            &[],
            "the card was integrated or taken over already",
        ));
    }
    let plan = card
        .plan_id
        .and_then(|id| flow::get_plan(db, id).ok().flatten())
        .filter(runs_by_itself)
        .ok_or_else(|| {
            coded(
                "card.noRunningPlan",
                &[],
                "the card belongs to no plan that runs by itself",
            )
        })?;
    let worker = card
        .claimed_by
        .as_deref()
        .and_then(session_id_of)
        .and_then(|id| agent_store::get_agent(db, id).ok().flatten())
        .filter(|worker| worker.card_id == Some(card.id) && !worker.card_review)
        .ok_or_else(worker_gone)?;
    let project = project_store::get_project(db, plan.project_id.unwrap_or_default())?
        .ok_or_else(|| coded("plan.projectGone", &[], "the plan's project is gone"))?;
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
    let gave_up = gave_up_on(db, card.id)?;
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

/// The start of the history line that says the owner had a card they were left with done again: from then on the studio
/// looks at the card as at any other, and the next conflict puts it in the owner's hands again at once.
const REDO_PREFIX: &str = "redo requested by the owner";

/// Whether the studio gave up integrating card `card_id` and the owner has not taken it up again since: the studio gave
/// up more often than the owner asked for a redo.
fn gave_up_on(db: &Connection, card_id: i64) -> Result<bool> {
    let count =
        |text: &str| flow::count_events(db, card_id, STUDIO, crate::board::EventKind::Note, text);
    Ok(count(GAVE_UP_PREFIX)? > count(REDO_PREFIX)?)
}

/// The cards of plan `plan_id` the studio gave up integrating and that wait for the owner (the Flow offers them "integrate
/// again" and "do again").
///
/// # Errors
///
/// A database error.
pub fn cards_left_for_owner(db: &Connection, plan_id: i64) -> Result<Vec<i64>> {
    let mut left = Vec::new();
    for board in board_store::list_boards(db)? {
        for card in board_store::list_cards(db, board.id, false)? {
            if card.plan_id == Some(plan_id)
                && card.verified_by.is_some()
                && card.integrated_at.is_none()
                && card.taken_over_at.is_none()
                && card.failed_at.is_none()
                && card.canceled_at.is_none()
                && gave_up_on(db, card.id)?
            {
                left.push(card.id);
            }
        }
    }
    Ok(left)
}

/// The owner has a card the studio gave up integrating done again: it goes back to the top of the open column, its
/// signature and claim are dropped, its sessions are cleaned up, and the plan starts it again on the line as it is now.
/// Returns the sessions that are gone, for the Studio to close their panes.
///
/// # Errors
///
/// A refusal for a card that is not signed off, was integrated or taken over, or that the studio has not given up on (the
/// studio puts such a card back itself).
pub async fn redo_card(core: &AxiomataCore, card_id: i64) -> Result<Vec<i64>> {
    let sessions = {
        let mut db = core.db_lock();
        let card = board_store::get_card(&db, card_id)?.ok_or_else(|| no_card(card_id))?;
        if !gave_up_on(&db, card_id)?
            || card.integrated_at.is_some()
            || card.taken_over_at.is_some()
        {
            return Err(coded(
                "card.notGivenUp",
                &[],
                "only a card the studio gave up integrating can be done again by hand",
            ));
        }
        if !flow::reset_for_rework(
            &mut db,
            card_id,
            OWNER,
            "redo: the owner has the card done again",
        )? {
            return Err(coded(
                "card.couldNotPutBack",
                &[],
                "the card could not be put back to be done again",
            ));
        }
        // A card done anew starts counting its returns from zero. The two steps below are small writes after the reset: if
        // one fails the card is still put back, and the owner's "integrate again" works whatever the history says.
        if let Err(err) = flow::clear_returned_count(&db, card_id) {
            tracing::warn!(%err, "could not reset the returns of a card done again");
        }
        if let Err(err) = flow::add_event(
            &db,
            card_id,
            STUDIO,
            crate::board::EventKind::Note,
            REDO_PREFIX,
        ) {
            tracing::warn!(%err, "could not note that a card was done again");
        }
        let sessions: Vec<Agent> = agent_store::card_sessions(&db)?
            .into_iter()
            .filter(|agent| agent.card_id == Some(card_id))
            .collect();
        crate::board_mirror::after_card_change(&db, &core.config_read().clone(), card_id);
        sessions
    };
    let roots = crate::paths::ide_locations().channels;
    remove_sessions(&core.db, &roots, &sessions);
    Ok(sessions.iter().map(|agent| agent.id).collect())
}

// ------------------------------------------------------------ escalation ---

/// How often a card may be sent back before its role is moved one tier up (A26, `docs/plans/agents-model.md` Q15): the
/// second return is the last one the same tier gets.
pub const ESCALATE_AFTER_RETURNS: u32 = 2;

/// What an escalation came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum CardEscalation {
    /// The same role took the card over a tier higher in the same session: worktree, branch, claim and role stay, the
    /// engine changes, and the next start of the pane runs it. `role` and `from_role` are the same since AM5.
    Done {
        card_id: i64,
        plan_id: Option<i64>,
        project_id: i64,
        agent_id: i64,
        from_role: String,
        role: String,
        engine_id: String,
    },
    /// No higher tier (the card was heavy) or no engine of it that can run: said once, and the owner decides.
    NoStrongerRole { card_id: i64, role: String },
    /// Nothing to do: the card was not sent back often enough, is not in work, or an escalation was tried already.
    NotNeeded { card_id: i64 },
}

/// The tier above `tier`, `None` at heavy: an escalation goes light → medium → heavy and ends there.
fn tier_above(tier: Tier) -> Option<Tier> {
    match tier {
        Tier::Light => Some(Tier::Medium),
        Tier::Medium => Some(Tier::Heavy),
        Tier::Heavy => None,
    }
}

/// Hands a card that was sent back [`ESCALATE_AFTER_RETURNS`] times to the same role one tier higher
/// (`docs/plans/agents-model.md`, Q15) — one try, ever (A26): the worker's session changes engine **in place**, so its
/// worktree, its branch and its claim stay, and the pane's next start runs the new engine on the work that is there.
///
/// At heavy, or with no engine of the next tier that can run, the card stays as it is and the owner decides; that is
/// recorded too, so the same look is not made again.
///
/// # Errors
///
/// A refusal for a card without a worker session of the studio's making, or whose role is gone.
pub async fn escalate_card(core: &AxiomataCore, card_id: i64) -> Result<CardEscalation> {
    let config = core.config_read().clone();
    let db = core.db_lock();
    escalate_in(&db, &config, &roster::roles_for_project, card_id)
}

/// [`escalate_card`] with the roles given; `roles_of` says which roles a project has (the files on disk in
/// production), so tests need no home folder.
fn escalate_in(
    db: &Connection,
    config: &Config,
    roles_of: &dyn Fn(&Connection, &Config, i64) -> Vec<Role>,
    card_id: i64,
) -> Result<CardEscalation> {
    let not_needed = Ok(CardEscalation::NotNeeded { card_id });
    let card = board_store::get_card(db, card_id)?.ok_or_else(|| no_card(card_id))?;
    let in_work = matches!(
        card.state,
        crate::board::TaskState::Working | crate::board::TaskState::InputRequired
    );
    if !in_work
        || card.archived_at.is_some()
        || card.returned_count < ESCALATE_AFTER_RETURNS
        || flow::count_events(db, card_id, STUDIO, crate::board::EventKind::Escalated, "")? > 0
    {
        return not_needed;
    }
    let worker = card
        .claimed_by
        .as_deref()
        .and_then(session_id_of)
        .and_then(|id| agent_store::get_agent(db, id).ok().flatten())
        .filter(|worker| worker.card_id == Some(card_id) && !worker.card_review)
        .ok_or_else(|| {
            coded(
                "card.sessionGone",
                &[],
                "the session that works on the card is gone",
            )
        })?;
    let roles = roles_of(db, config, worker.project_id);
    let current = roles
        .iter()
        .find(|role| role.name == worker.agent_role)
        .ok_or_else(|| no_role(&worker.agent_role))?;
    let sources = ModelSources::load();
    let from = worker_tier(
        config,
        &sources,
        &card,
        Some(current),
        worker.engine_id.as_deref(),
    );
    let next = tier_above(from).and_then(|tier| {
        choose_engine(config, tier, None)
            .ok()
            .map(|(id, _)| (tier, id))
    });
    let Some((tier, engine_id)) = next else {
        let text = format!(
            "sent back {} times; the role “{}” at {} has no higher tier with an engine to run on, so the owner \
             decides: let it go on, take it over, or call it off",
            card.returned_count,
            current.name,
            from.as_str()
        );
        flow::add_event(
            db,
            card_id,
            STUDIO,
            crate::board::EventKind::Escalated,
            &text,
        )?;
        return Ok(CardEscalation::NoStrongerRole {
            card_id,
            role: current.name.clone(),
        });
    };
    let old_engine = worker.engine_id.clone().unwrap_or_default();
    // The name stays: the card's claim is the session's actor, which is made from it.
    roster::update_agent_on_engine(
        db,
        config,
        &roles,
        worker.id,
        &worker.name,
        &engine_id,
        &current.name,
    )?
    .ok_or_else(|| {
        coded(
            "card.sessionGone",
            &[],
            "the session that works on the card is gone",
        )
    })?;
    // The next start is a new conversation on the new engine: it finds the work in the worktree and the reviewers' notes in
    // the card's history, not the old model's context.
    agent_store::set_opencode_session(db, worker.id, None)?;
    let text = format!(
        "sent back {} times: the role “{}” goes from {} on “{old_engine}” to {} on “{engine_id}”; the worktree, the \
         branch and the claim stay",
        card.returned_count,
        current.name,
        from.as_str(),
        tier.as_str()
    );
    flow::add_event(
        db,
        card_id,
        STUDIO,
        crate::board::EventKind::Escalated,
        &text,
    )?;
    crate::board_mirror::after_card_change(db, config, card_id);
    Ok(CardEscalation::Done {
        card_id,
        plan_id: card.plan_id,
        project_id: worker.project_id,
        agent_id: worker.id,
        from_role: current.name.clone(),
        role: current.name.clone(),
        engine_id,
    })
}

/// The line is where the studio last left it. A line that was not made by this call and has no recorded tip is refused
/// too: the branch was there before the studio, and nothing says what is on it.
fn check_line_tip(
    db: &Mutex<Connection>,
    plan: &crate::board::Plan,
    line: &axiomata_ide::plan_line::Line,
) -> Result<String> {
    let tip = axiomata_ide::plan_line::tip(&line.path)?;
    let in_place = match (&plan.line_tip, line.created) {
        (Some(recorded), _) if *recorded == tip => return Ok(tip),
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
        return Err(coded(
            "plan.lineMoved",
            &[("plan", &plan.id.to_string()), ("branch", &line.branch)],
            format!(
                "the line of plan #{} is not where the studio left it: something else moved or made the branch {}; look at \
                 it before anything is merged into it",
                plan.id, line.branch
            ),
        ));
    }
    flow::set_line_tip(&lock(db), plan.id, &tip)?;
    Ok(tip)
}

/// What closing a plan came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanClosed {
    /// `false`: there is no such plan, or it was closed already.
    pub closed: bool,
    /// What of its line could not be removed. The line's work is never lost: a line that holds something the project's
    /// branch lacks stays, and says so here.
    pub cleanup: Vec<String>,
}

/// The owner closes a plan, also one that was approved and never finished: nothing starts from it any more, and its
/// integration line goes — its worktree, then its branch, the way a take-over removes it (never forced). Then the
/// plan can be deleted.
///
/// # Errors
///
/// [`flow::close_plan`]'s refusal while a session still holds a card of the plan; a database error.
pub async fn close_plan(core: &AxiomataCore, plan_id: i64) -> Result<PlanClosed> {
    let db = Arc::clone(&core.db);
    let worktrees = crate::paths::ide_locations().worktrees;
    tokio::task::spawn_blocking(move || close_plan_blocking(&db, &worktrees, plan_id))
        .await
        .map_err(|err| refusal("plan", format!("closing the plan failed: {err}")))?
}

/// [`close_plan`] for a caller that may block (the CLI).
///
/// # Errors
///
/// As [`close_plan`].
pub fn close_plan_blocking(
    db: &Mutex<Connection>,
    worktrees: &std::path::Path,
    plan_id: i64,
) -> Result<PlanClosed> {
    let (closed, plan, project) = {
        let conn = lock(db);
        let closed = flow::close_plan(&conn, plan_id)?;
        let plan = flow::get_plan(&conn, plan_id)?;
        let project = plan
            .as_ref()
            .and_then(|plan| plan.project_id)
            .map(|id| project_store::get_project(&conn, id))
            .transpose()?
            .flatten();
        (closed, plan, project)
    };
    let mut cleanup = Vec::new();
    // A plan without a recorded base branch never made a line: there is nothing on disk to remove.
    if let Some(plan) = plan.filter(|_| closed)
        && let Some(base_branch) = plan.base_branch
    {
        match project {
            Some(project) => {
                let line = axiomata_ide::plan_line::Line {
                    path: axiomata_ide::plan_line::line_path(worktrees, &project.name, plan.id),
                    branch: axiomata_ide::plan_line::line_branch(plan.id),
                    base_branch,
                    created: false,
                };
                if line.path.exists() || worktree::branch_exists(&project.repo_root, &line.branch) {
                    cleanup.extend(axiomata_ide::plan_line::remove(&project.repo_root, &line));
                }
            }
            None => cleanup.push(format!(
                "the project of plan #{} is gone; its line {} was not looked for",
                plan.id,
                axiomata_ide::plan_line::line_branch(plan.id)
            )),
        }
    }
    Ok(PlanClosed { closed, cleanup })
}

/// What taking a plan's line over came to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum PlanTakeOver {
    /// The cards' work is in the project's branch (`commit` is its tip), their cards are closed and archived, the plan is
    /// closed and its line is gone. `cleanup` lists what could not be done — the work is taken over all the same.
    Done {
        commit: String,
        project_id: i64,
        plan_id: i64,
        card_ids: Vec<i64>,
        cleanup: Vec<String>,
    },
    /// The line conflicts with what the project's branch has by now and was undone: nothing changed.
    Conflict { files: Vec<String> },
}

/// The owner's one click on a finished plan: its line goes into the project's branch (the cards' commits as they are
/// while the branch has not moved, else one merge commit), the cards are closed, the plan is closed and the line is
/// removed. Never pushes. A conflict with the branch is undone and comes back as the outcome.
///
/// # Errors
///
/// A refusal for a plan that does not run by itself, is not approved or has a card that is neither on the line nor
/// called off; a line that is not where the studio left it; the git layer's own refusals (the project folder is on
/// another branch, has staged changes, an unsafe config).
pub async fn take_over_plan(core: &AxiomataCore, plan_id: i64) -> Result<PlanTakeOver> {
    let db = Arc::clone(&core.db);
    let worktrees = crate::paths::ide_locations().worktrees;
    tokio::task::spawn_blocking(move || take_over_plan_blocking(&db, &worktrees, plan_id))
        .await
        .map_err(|err| refusal("take-over", format!("the take-over task failed: {err}")))?
}

fn take_over_plan_blocking(
    db: &Mutex<Connection>,
    worktrees: &std::path::Path,
    plan_id: i64,
) -> Result<PlanTakeOver> {
    let (plan, project, integrated) = {
        let conn = lock(db);
        let plan = flow::get_plan(&conn, plan_id)?.ok_or_else(|| no_plan(plan_id))?;
        if !runs_by_itself(&plan) {
            return Err(coded(
                "plan.noLine",
                &[],
                "only a plan that runs by itself has a line to take over",
            ));
        }
        let project_id = plan.project_id.unwrap_or_default();
        let project =
            project_store::get_project(&conn, project_id)?.ok_or_else(|| no_project(project_id))?;
        let cards: Vec<_> = board_store::list_cards(&conn, plan.board_id, false)?
            .into_iter()
            .filter(|card| card.plan_id == Some(plan.id))
            .collect();
        if let Some(open) = cards.iter().find(|card| {
            card.integrated_at.is_none() && card.state != crate::board::TaskState::Canceled
        }) {
            let state = serde_json::to_string(&open.state).unwrap_or_default();
            let state = state.trim_matches('"');
            return Err(coded(
                "plan.cardNotOnLine",
                &[("card", &open.id.to_string()), ("state", state)],
                format!(
                    "card #{} is not on the line yet ({state}); a plan is taken over when all its cards are",
                    open.id
                ),
            ));
        }
        let integrated: Vec<i64> = cards
            .iter()
            .filter(|card| card.integrated_at.is_some())
            .map(|card| card.id)
            .collect();
        if integrated.is_empty() {
            return Err(coded(
                "plan.nothingOnLine",
                &[],
                "no card of the plan is on its line: there is nothing to take over",
            ));
        }
        (plan, project, integrated)
    };
    let (line, checked_tip) =
        checked_line(db, worktrees, &plan, &project.name, &project.repo_root)?;
    let first_line = plan.name.lines().next().unwrap_or_default().trim();
    let message = format!("Plan #{}: {first_line}", plan.id);
    match axiomata_ide::plan_line::take_over(&project.repo_root, &line, &checked_tip, &message)? {
        axiomata_ide::git::TakeOver::Conflict { files } => Ok(PlanTakeOver::Conflict { files }),
        axiomata_ide::git::TakeOver::Done { commit, .. } => {
            // The work is in the project's branch from here on: nothing below can undo it, so a step that fails is a note.
            let mut cleanup = Vec::new();
            for card in &integrated {
                match flow::mark_taken_over(&lock(db), *card, OWNER) {
                    Ok(true) => {}
                    Ok(false) | Err(_) => {
                        cleanup.push(format!("card #{card} could not be marked as taken over"));
                    }
                }
            }
            match flow::close_plan(&lock(db), plan.id) {
                Ok(_) => {}
                Err(err) => cleanup.push(format!("the plan could not be closed: {err}")),
            }
            cleanup.extend(axiomata_ide::plan_line::remove(&project.repo_root, &line));
            Ok(PlanTakeOver::Done {
                commit,
                project_id: project.id,
                plan_id: plan.id,
                card_ids: integrated,
                cleanup,
            })
        }
    }
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
        return Err(coded(
            "card.notMarkedIntegrated",
            &[],
            "the work is on the line, but the card could not be marked as integrated",
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

/// Forgets a session made for a card that never ran — its start failed — with its worktree, secret and channel. The
/// card is the caller's to give back ([`release_card_session`]).
pub async fn discard_session(core: &AxiomataCore, agent_id: i64) -> Result<()> {
    let roots = crate::paths::ide_locations().channels;
    let db = core.db_lock();
    if agent_store::get_agent(&db, agent_id)?.is_none() {
        return Ok(());
    }
    let _ = crate::ide::provision::discard_worktree(&db, agent_id, true);
    let _ = axiomata_ide::session_token::revoke(&roots, agent_id);
    agent_store::delete_agent(&db, agent_id)?;
    let _ = crate::ide::provision::forget_channel(&roots, agent_id);
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
            return Err(no_card(card_id));
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

/// Ends every session the studio made for cards that are going away (A2A CP-A6): the workers and their reviewers.
/// A card's row takes its claim with it; what is left is the session's own — its worktree, branch, secret, channel and
/// Opencode registration. Takes the whole set at once because a board takes every card on it with it. Returns the ids
/// of the sessions that are gone, for the Studio to close their panes. A session that lingers is logged, never a
/// reason to refuse the owner's delete.
pub async fn forget_sessions_of_cards(core: &AxiomataCore, card_ids: &[i64]) -> Vec<i64> {
    if card_ids.is_empty() {
        return Vec::new();
    }
    let db = Arc::clone(&core.db);
    let roots = crate::paths::ide_locations().channels;
    let wanted = card_ids.to_vec();
    let ended = tokio::task::spawn_blocking(move || {
        let sessions = sessions_of_cards(&lock(&db), &wanted);
        let ids: Vec<i64> = sessions.iter().map(|session| session.id).collect();
        remove_sessions(&db, &roots, &sessions);
        ids
    })
    .await;
    let Ok(ids) = ended else {
        tracing::warn!("ending the sessions of cards {card_ids:?} failed");
        return Vec::new();
    };
    ids
}

/// The sessions made for the given cards: a worker and, after a report, its reviewers — one agent row per session.
/// Read in one query across all projects and filtered here, the same way `redo_card` does it; an error is logged, not
/// returned, because `forget_sessions_of_cards` cannot fail the owner's delete either way.
fn sessions_of_cards(db: &Connection, card_ids: &[i64]) -> Vec<Agent> {
    let wanted: std::collections::HashSet<i64> = card_ids.iter().copied().collect();
    let sessions = agent_store::card_sessions(db).unwrap_or_else(|err| {
        tracing::warn!(%err, cards = ?card_ids, "could not read the sessions of cards");
        Vec::new()
    });
    sessions
        .into_iter()
        .filter(|agent| agent.card_id.is_some_and(|id| wanted.contains(&id)))
        .collect()
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
            tier: None,
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
        // Own engines with a tier each (agents-model.md, AM5); the tier's default engine is set for medium and heavy.
        let tiered = |id: &str, tier: Tier| Engine {
            tier: Some(tier),
            ..engine(id, "")
        };
        let mut engines = BTreeMap::new();
        engines.insert("haiku".to_owned(), tiered("haiku", Tier::Light));
        engines.insert("sonnet".to_owned(), tiered("sonnet", Tier::Medium));
        engines.insert("sonnet-2".to_owned(), tiered("sonnet-2", Tier::Medium));
        engines.insert("opus".to_owned(), tiered("opus", Tier::Heavy));
        engines.insert(
            "custom".to_owned(),
            Engine {
                tier: Some(Tier::Medium),
                ..engine("custom", "my-harness --go")
            },
        );
        // `Config::default()` points the workspace at the real `~/Axiomata-Workspace`, and a returned card mirrors
        // its board there.
        let mut config = Config {
            workspace_root: dir.join("workspace"),
            ..Config::default()
        };
        config.agents.engines = engines;
        config
            .agents
            .models
            .default_engines
            .set(Tier::Medium, Some("sonnet".into()));
        config
            .agents
            .models
            .default_engines
            .set(Tier::Heavy, Some("opus".into()));
        // The built-in Anthropic models stay off, so only the engines above are offered.
        for (id, _) in crate::model_catalog::ANTHROPIC_MODELS {
            config.agents.models.choices.insert(
                crate::model_tiers::model_key(crate::config::ProviderId::Anthropic, id),
                crate::model_tiers::ModelChoice {
                    enabled: Some(false),
                    tier: None,
                },
            );
        }
        World {
            roles: vec![
                role("allrounder", "implement", None),
                role("builder", "implement", Some("sonnet")),
                role("reviewer", "review", Some("sonnet")),
            ],
            project,
            board: board.id,
            open: id("Open"),
            proposal: id("Proposal"),
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
                permission_cli: None,
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
            })
        }

        fn sessions(&self) -> usize {
            agent_store::list_agents(&self.db, self.project)
                .unwrap()
                .len()
        }
    }

    #[test]
    fn a_card_is_taken_for_a_new_session_of_its_role_on_its_tiers_engine() {
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let started = w.start(card, None).unwrap();
        assert_eq!(started.role, "builder");
        assert_eq!(started.engine_id, "sonnet");
        // Named like an agent (Q8); the card's number stays on the card.
        assert_eq!(started.agent.name, "Claude Medium Builder (default)");
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
    fn a_card_without_a_role_goes_to_the_allrounder_at_the_cards_tier_and_nowhere_without_an_engine()
     {
        let mut w = world();
        let card = w.card(w.open, None);
        // A light card, and no light engine that can run: nothing to run on, and nothing is made.
        w.config.agents.engines.remove("haiku");
        let mut fields = fields_of(&store::get_card(&w.db, card).unwrap().unwrap());
        fields.tier = Some(crate::board::Tier::Light);
        store::update_card(&w.db, card, &fields).unwrap();
        let refused = w.start(card, None).unwrap_err().to_string();
        assert!(refused.contains("tier light"), "{refused}");
        assert_eq!(w.sessions(), 0);
        assert_eq!(
            store::get_card(&w.db, card).unwrap().unwrap().claimed_by,
            None
        );

        // The owner's pick still wins over the tier.
        let started = w.start(card, Some("sonnet")).unwrap();
        assert_eq!(started.role, "allrounder");
        assert_eq!(started.engine_id, "sonnet");
        // Named by the tier of the engine the owner picked, not the card's.
        assert_eq!(started.agent.name, "Claude Medium Allrounder (default)");
    }

    #[test]
    fn a_card_of_a_plan_that_runs_by_itself_is_started_on_its_tiers_engine_too() {
        let mut w = world();
        let (_, cards, line) = w.line_plan(&["one"]);
        let mut fields = fields_of(&store::get_card(&w.db, cards[0]).unwrap().unwrap());
        fields.tier = Some(crate::board::Tier::Light);
        store::update_card(&w.db, cards[0], &fields).unwrap();
        // The plan's own start, as `plan_run` asks for it: no engine named.
        let started = w.start_on(cards[0], None, Some(&line.branch)).unwrap();
        assert_eq!(started.engine_id, "haiku");
        assert_eq!(started.agent.name, "Claude Light Builder (default)");
    }

    #[test]
    fn a_card_of_a_tier_runs_on_that_tiers_engine_whatever_its_role_names() {
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let mut fields = fields_of(&store::get_card(&w.db, card).unwrap().unwrap());
        fields.tier = Some(crate::board::Tier::Heavy);
        store::update_card(&w.db, card, &fields).unwrap();
        // The role file names sonnet; the card is heavy: the heavy tier's default runs it.
        let started = w.start(card, None).unwrap();
        assert_eq!(started.engine_id, "opus");
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
    fn a_card_is_started_as_a_plan_only_in_a_project_that_is_a_repository() {
        let mut w = world();
        let card = w.card(w.open, None);
        let err = start_card_plan_in(&mut w.db, card, 999).unwrap_err();
        assert!(err.to_string().contains("no project 999"), "{err}");

        let plain = w.dir.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let folder = project_store::create_project(
            &w.db,
            NewProject {
                name: "Folder".into(),
                repo_root: plain,
            },
        )
        .unwrap()
        .id;
        let err = start_card_plan_in(&mut w.db, card, folder).unwrap_err();
        assert!(err.to_string().contains("not a git repository"), "{err}");
        assert!(
            flow::list_plans(&w.db, w.board).unwrap().is_empty(),
            "a refusal makes no plan"
        );

        let plan = start_card_plan_in(&mut w.db, card, w.project).unwrap();
        assert_eq!(plan.origin_card_id, Some(card));
        assert_eq!(plan.project_id, Some(w.project));
        assert!(
            !runs_by_itself(&plan),
            "a draft does not run yet, though it is set to once approved"
        );
        assert_eq!(plan.auto_start_max, Some(flow::AUTO_UNBOUNDED));
        // No reviewer is ever made for the card, whatever column it ends up in — in progress, and in Review.
        assert!(awaiting_in(&w.db).unwrap().is_empty());
        let review = store::list_columns(&w.db, w.board)
            .unwrap()
            .into_iter()
            .find(|c| c.stage == Some(axiomata_board::ColumnStage::Review))
            .unwrap();
        store::move_card(&mut w.db, card, review.id, 0).unwrap();
        assert!(awaiting_in(&w.db).unwrap().is_empty());
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
        assert_eq!(second.agent.name, "Claude Medium Builder (default) #2");
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
    fn a_worker_of_a_plan_in_auto_mode_is_started_with_the_free_rights_read_at_its_start() {
        use crate::ide_start::card_launch;
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
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
        let mut stored = store::get_card(&w.db, card).unwrap().unwrap();
        stored.plan_id = Some(plan.id);
        store::update_card(&w.db, card, &fields_of(&stored)).unwrap();
        let started = w.start(card, None).unwrap();
        assert_eq!(
            card_launch(&w.db, &started.agent),
            Some(crate::agent_entry::CardLaunch::work(card))
        );
        flow::set_plan_auto_rights(&w.db, plan.id, true, "human:owner").unwrap();
        // Switched on, but the plan is still a draft: not yet.
        assert_eq!(
            card_launch(&w.db, &started.agent),
            Some(crate::agent_entry::CardLaunch::work(card))
        );
        flow::approve_plan(&mut w.db, plan.id, "human:owner").unwrap();
        assert_eq!(
            card_launch(&w.db, &started.agent),
            Some(crate::agent_entry::CardLaunch::work(card).with_auto_rights(true))
        );
        // A closed plan's switch is history. Closing is refused while the card is held, so the plan is closed the way a
        // database from before that refusal can have it.
        w.db.execute(
            "UPDATE plans SET status = 'closed' WHERE id = ?1",
            rusqlite::params![plan.id],
        )
        .unwrap();
        assert_ne!(
            card_launch(&w.db, &started.agent),
            Some(crate::agent_entry::CardLaunch::work(card).with_auto_rights(true))
        );
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
        let (card, worker) = w.reported("sonnet");

        // Another engine of the worker's tier (medium), whatever the reviewer's file names.
        let review = w.review(card, None).unwrap();
        assert_eq!(
            (review.role.as_str(), review.engine_id.as_str()),
            ("reviewer", "sonnet-2")
        );
        assert_eq!(review.agent.name, "Claude Medium Reviewer (default)");
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
    fn deleting_a_card_ends_the_sessions_made_for_it() {
        let mut w = world();
        // A worker and, after a report, its reviewer: both sessions carry the card's id and both end with it.
        w.roles.retain(|r| r.name != "reviewer");
        w.roles.push(role("reviewer", "review", Some("opus")));
        let (card, worker) = w.reported("sonnet");
        let reviewer = w.review(card, None).unwrap();
        let worker_row = agent_store::get_agent(&w.db, worker.agent.id)
            .unwrap()
            .unwrap();
        let branch = worker_row.branch.clone().unwrap();
        // Both panes have started, so both have their checkout on disk.
        let worker_cwd = crate::ide::provision::prepare(&w.db, &w.locations(), worker.agent.id)
            .unwrap()
            .cwd;
        let reviewer_cwd = crate::ide::provision::prepare(&w.db, &w.locations(), reviewer.agent.id)
            .unwrap()
            .cwd;
        assert!(worker_cwd.is_dir() && reviewer_cwd.is_dir());
        // A session of another card is not this card's and must survive the delete.
        let other = w.card(w.open, Some("builder"));
        let theirs = w.start(other, Some("sonnet")).unwrap();

        let roots = w.locations().channels;
        let mut gone = w.shared(|db| {
            // The command's order: the card goes first, and only then are its sessions looked up. That the lookup
            // still finds them is the load-bearing fact — `card_id` on a session has no foreign key to the card
            // (`agent_card.sql`), so the card's row takes its claim with it while the sessions stay to be ended.
            assert!(
                store::delete_card(&lock(db), card).unwrap(),
                "the card is gone before its sessions are"
            );
            let sessions = sessions_of_cards(&lock(db), &[card]);
            let ids: Vec<i64> = sessions.iter().map(|session| session.id).collect();
            remove_sessions(db, &roots, &sessions);
            ids
        });
        gone.sort_unstable();

        let mut both = vec![worker.agent.id, reviewer.agent.id];
        both.sort_unstable();
        assert_eq!(gone, both, "the worker and its reviewer are both ended");
        assert!(
            agent_store::get_agent(&w.db, worker.agent.id)
                .unwrap()
                .is_none()
        );
        assert!(
            agent_store::get_agent(&w.db, reviewer.agent.id)
                .unwrap()
                .is_none()
        );
        assert!(!worker_cwd.exists(), "the worker's worktree went with it");
        assert!(
            !reviewer_cwd.exists(),
            "the reviewer's checkout went with it"
        );
        assert!(
            !worktree::branch_exists(&w.repo, &branch),
            "the worker's branch went with it"
        );
        assert!(
            agent_store::get_agent(&w.db, theirs.agent.id)
                .unwrap()
                .is_some(),
            "the other card's session is untouched"
        );
    }

    #[test]
    fn deleting_a_board_ends_the_sessions_of_every_card_on_it() {
        let mut w = world();
        // Two cards, each with a worker of its own: a board takes every card on it with it, and their sessions too.
        let first = w.card(w.open, Some("builder"));
        let one = w.start(first, Some("sonnet")).unwrap();
        let second = w.card(w.open, Some("builder"));
        let two = w.start(second, Some("sonnet")).unwrap();
        // A third card on another board must not be caught by the set.
        let bystander = store::create_board(&mut w.db, "Other").unwrap().id;
        let column = store::list_columns(&w.db, bystander)
            .unwrap()
            .into_iter()
            .find(|column| column.name == "Open")
            .unwrap()
            .id;
        let theirs = w
            .start(w.card(column, Some("builder")), Some("sonnet"))
            .unwrap();

        let (board, roots) = (w.board, w.locations().channels);
        // What the board command does: the ids are read before the board goes, then the set is handed over at once.
        let mut gone = w.shared(|db| {
            let ids: Vec<i64> = store::list_cards(&lock(db), board, true)
                .unwrap()
                .into_iter()
                .map(|card| card.id)
                .collect();
            assert_eq!(
                ids,
                vec![first, second],
                "the board's cards are read before it goes"
            );
            store::delete_board(&mut db.lock().unwrap(), board).unwrap();
            let sessions = sessions_of_cards(&lock(db), &ids);
            let ended: Vec<i64> = sessions.iter().map(|session| session.id).collect();
            remove_sessions(db, &roots, &sessions);
            ended
        });
        gone.sort_unstable();

        let mut both = vec![one.agent.id, two.agent.id];
        both.sort_unstable();
        assert_eq!(gone, both, "every card's session on the board is ended");
        assert!(
            agent_store::get_agent(&w.db, one.agent.id)
                .unwrap()
                .is_none()
        );
        assert!(
            agent_store::get_agent(&w.db, two.agent.id)
                .unwrap()
                .is_none()
        );
        assert!(
            agent_store::get_agent(&w.db, theirs.agent.id)
                .unwrap()
                .is_some(),
            "a session of a card on another board is untouched"
        );
    }

    #[test]
    fn without_an_engine_that_can_review_the_owner_is_asked_and_nothing_is_committed() {
        let mut w = world();
        let (card, worker) = w.reported("sonnet");
        // No engine of the worker's tier or above is left that can run: the studio does not review on its own.
        for id in ["sonnet", "sonnet-2", "opus"] {
            w.config.agents.engines.remove(id);
        }
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
        w.config.agents.engines.insert(
            "opus".into(),
            Engine {
                tier: Some(Tier::Heavy),
                ..engine("opus", "")
            },
        );
        assert_eq!(w.review(card, Some("opus")).unwrap().engine_id, "opus");
    }

    fn reviewer_of(name: &str, tier: Tier, engine: &str) -> Role {
        let mut role = role(name, "review", Some(engine));
        role.tier = tier;
        role
    }

    /// A card of `tier` worked by `builder`, as `choose_reviewer` sees it.
    fn card_at(w: &World, tier: Option<crate::board::Tier>) -> crate::board::Card {
        let id = w.card(w.open, Some("builder"));
        let mut card = store::get_card(&w.db, id).unwrap().unwrap();
        card.tier = tier;
        card
    }

    #[test]
    fn the_reviewer_takes_another_engine_of_the_workers_tier_else_a_tier_up_and_never_the_workers()
    {
        let mut w = world();
        w.roles
            .push(reviewer_of("reviewer-heavy", Tier::Heavy, "opus"));
        let medium = card_at(&w, Some(crate::board::Tier::Medium));
        let pick = |w: &World, card: &crate::board::Card, worker: Option<&str>| {
            let (role, tier, engine) =
                choose_reviewer(&w.config, &w.roles, card, None, worker, None).unwrap();
            (role.name.clone(), tier, engine)
        };
        // The role `reviewer`, whatever its file's tier or engine says; another medium engine than the worker's.
        assert_eq!(
            pick(&w, &medium, Some("sonnet")),
            ("reviewer".to_owned(), Tier::Medium, "sonnet-2".to_owned())
        );
        // The worker's engine is the only one of its tier: the next tier up, never the worker's own (A21).
        w.config.agents.engines.remove("sonnet-2");
        assert_eq!(
            pick(&w, &medium, Some("sonnet")),
            ("reviewer".to_owned(), Tier::Heavy, "opus".to_owned())
        );
        // No engine of the worker's tier can run: the next tier up.
        let light = card_at(&w, Some(crate::board::Tier::Light));
        // The only light engine is the worker's: a tier up.
        assert_eq!(pick(&w, &light, Some("haiku")).1, Tier::Medium);
        w.config.agents.engines.remove("haiku");
        assert_eq!(
            pick(&w, &light, Some("x")),
            ("reviewer".to_owned(), Tier::Medium, "sonnet".to_owned())
        );
        // Never below the worker's tier, and the card may name its own review role.
        let mut heavy = card_at(&w, Some(crate::board::Tier::Heavy));
        // One engine per tier and the worker heavy on opus: nothing but its own engine is left, so the owner is asked.
        assert!(
            choose_reviewer(&w.config, &w.roles, &heavy, None, Some("opus"), None)
                .unwrap_err()
                .to_string()
                .contains("other than the worker's")
        );
        assert_eq!(
            pick(&w, &heavy, Some("sonnet")),
            ("reviewer".to_owned(), Tier::Heavy, "opus".to_owned())
        );
        heavy.agent = Some("reviewer-heavy".into());
        assert_eq!(pick(&w, &heavy, None).0, "reviewer-heavy");
        // Nothing of a high enough tier left: the refusal says what to do.
        w.config.agents.engines.remove("opus");
        assert!(choose_reviewer(&w.config, &w.roles, &heavy, None, None, None).is_err());
        w.roles.retain(|role| role.kind != KIND_REVIEW);
        assert!(choose_reviewer(&w.config, &w.roles, &medium, None, None, None).is_err());
    }

    #[test]
    fn a_worker_runs_at_the_cards_tier_else_its_roles_on_the_tiers_default_else_the_cheapest_that_can_run()
     {
        let mut w = world();
        let sources = ModelSources::default();
        // The tier's default engine first.
        assert_eq!(
            choose_engine(&w.config, Tier::Medium, None).unwrap().0,
            "sonnet"
        );
        // Without a default: the cheapest of the tier (unpriced own engines by id).
        w.config
            .agents
            .models
            .default_engines
            .set(Tier::Medium, None);
        assert_eq!(
            choose_engine(&w.config, Tier::Medium, None).unwrap().0,
            "sonnet"
        );
        let ids: Vec<String> = tier_engines(&w.config, &sources, Tier::Medium)
            .into_iter()
            .map(|info| info.engine.id)
            .collect();
        assert_eq!(ids, ["sonnet", "sonnet-2"]);
        // An engine with a command of its own is no candidate; one asked for by the owner wins over the tier.
        assert!(!ids.contains(&"custom".to_owned()));
        assert_eq!(
            choose_engine(&w.config, Tier::Medium, Some("opus"))
                .unwrap()
                .0,
            "opus"
        );
        assert!(choose_engine(&w.config, Tier::Medium, Some("custom")).is_err());
        // A tier without an engine that can run is refused with what to do.
        w.config.agents.engines.remove("haiku");
        assert!(choose_engine(&w.config, Tier::Light, None).is_err());
    }

    #[test]
    fn a_card_calls_for_its_own_tier_else_the_tier_of_the_role_that_works_it() {
        let w = world();
        let id = w.card(w.open, Some("builder"));
        let mut card = store::get_card(&w.db, id).unwrap().unwrap();
        let heavy_role = {
            let mut role = role("builder", "implement", None);
            role.tier = Tier::Heavy;
            role
        };
        assert_eq!(
            needed_tier(&card, Some(&heavy_role)),
            Tier::Heavy,
            "no tier on the card: the role's"
        );
        assert_eq!(needed_tier(&card, None), Tier::Medium);
        card.tier = Some(crate::board::Tier::Light);
        assert_eq!(
            needed_tier(&card, Some(&heavy_role)),
            Tier::Light,
            "the card's own tier wins"
        );
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
        assert_eq!(
            second.agent.name, "Claude Heavy Reviewer (default) #2",
            "named by the picked engine's tier"
        );
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
    fn a_reviewer_of_a_plan_in_auto_mode_is_started_with_its_rights() {
        use crate::ide_start::card_launch;
        let mut w = world();
        let (card, worker) = w.reported("sonnet");
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
        let mut stored = store::get_card(&w.db, card).unwrap().unwrap();
        stored.plan_id = Some(plan.id);
        store::update_card(&w.db, card, &fields_of(&stored)).unwrap();
        flow::set_plan_auto_rights(&w.db, plan.id, true, "human:owner").unwrap();
        flow::approve_plan(&mut w.db, plan.id, "human:owner").unwrap();

        let review = w.review(card, Some("opus")).unwrap();
        let ready = crate::ide::provision::prepare(&w.db, &w.locations(), review.agent.id).unwrap();
        assert_eq!(
            card_launch(&w.db, &ready.agent),
            Some(
                crate::agent_entry::CardLaunch::review(card, &worker.agent.name, "main")
                    .with_auto_rights(true)
            )
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

        fn take_over(&mut self, card: i64, message: Option<&str>) -> Result<CardTakeOver> {
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
            commit,
            already_on_base,
            cleanup,
            ..
        } = taken
        else {
            panic!("expected the work to be taken over: {:?}", taken);
        };
        assert!(!already_on_base, "the take-over made its own commit");
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
    fn a_card_whose_work_is_already_on_main_still_closes_without_a_new_commit() {
        let mut w = world();
        let (card, worker, review) = w.approved();
        let worker_row = agent_store::get_agent(&w.db, worker.agent.id)
            .unwrap()
            .unwrap();
        let worker_tree = worker_row.worktree_path.clone().unwrap();
        let review_tree = crate::ide::provision::prepare(&w.db, &w.locations(), review.agent.id)
            .unwrap()
            .cwd;
        let branch = worker_row.branch.clone().unwrap();
        // The owner put the reviewed work onto main by hand — the way a card ends up signed off with nothing left to
        // squash. Without this branch of the take-over the card would be refused forever: closed by nobody, its
        // sessions alive for as long as the app runs.
        git(
            &w.repo,
            &["merge", "--quiet", "--no-ff", "-m", "by hand", &branch],
        );
        let main_after = git(&w.repo, &["rev-parse", "main"]);

        let taken = w.take_over(card, None).unwrap();
        let CardTakeOver::Done {
            commit,
            already_on_base,
            cleanup,
            ..
        } = taken
        else {
            panic!("expected the card to be taken over: {:?}", taken);
        };
        assert!(already_on_base, "nothing was committed");
        assert!(cleanup.is_empty(), "{cleanup:?}");
        // The base branch's tip — the commit that holds the work — is reported, and nothing was added to it.
        assert_eq!(commit, main_after);
        assert_eq!(
            git(&w.repo, &["rev-parse", "main"]),
            main_after,
            "nothing was committed"
        );

        // And it still ends the card and its sessions, which is what taking it over is for.
        let closed = store::get_card(&w.db, card).unwrap().unwrap();
        assert!(closed.taken_over_at.is_some() && closed.archived_at.is_some());
        assert!(!worker_tree.exists() && !review_tree.exists());
        assert!(!worktree::branch_exists(&w.repo, &branch));
        assert_eq!(w.sessions(), 0);
    }

    /// The same incident, a different git verb: the owner copied the reviewed work onto main under an id of their
    /// own. The worker's branch is then **ahead** of main, but squashing it brings nothing main does not have — the
    /// case the empty-squash refusal used to strand, and which nothing but the squash itself can see.
    #[test]
    fn work_copied_onto_main_by_hand_also_closes_the_card_without_a_new_commit() {
        let mut w = world();
        let (card, worker, review) = w.approved();
        let worker_row = agent_store::get_agent(&w.db, worker.agent.id)
            .unwrap()
            .unwrap();
        let worker_tree = worker_row.worktree_path.clone().unwrap();
        let review_tree = crate::ide::provision::prepare(&w.db, &w.locations(), review.agent.id)
            .unwrap()
            .cwd;
        let branch = worker_row.branch.clone().unwrap();
        let tip = git(&worker_tree, &["rev-parse", "HEAD"]);
        // `-x` so the copy is a commit of its own: without it, cherry-picking a commit whose parent is already main's
        // tip can produce the very same id (same parent, same tree, same second) and the branch would count as part
        // of main instead of ahead of it.
        git(&w.repo, &["cherry-pick", "--quiet", "-x", &tip]);
        let main_after = git(&w.repo, &["rev-parse", "main"]);
        assert_ne!(main_after, tip, "main got a commit of its own");
        assert_eq!(
            git(
                &w.repo,
                &["rev-list", "--count", &format!("main..{branch}")]
            ),
            "1",
            "the branch is ahead: this is the squash path, not the branch-already-on-main one"
        );

        let taken = w.take_over(card, None).unwrap();
        let CardTakeOver::Done {
            commit,
            already_on_base,
            cleanup,
            ..
        } = taken
        else {
            panic!("expected the card to be taken over: {:?}", taken);
        };
        assert!(already_on_base, "nothing was committed");
        assert!(cleanup.is_empty(), "{cleanup:?}");
        assert_eq!(commit, main_after);
        assert_eq!(
            git(&w.repo, &["rev-parse", "main"]),
            main_after,
            "the squash staged nothing, so no commit was made"
        );

        let closed = store::get_card(&w.db, card).unwrap().unwrap();
        assert!(closed.taken_over_at.is_some() && closed.archived_at.is_some());
        assert!(!worker_tree.exists() && !review_tree.exists());
        assert!(!worktree::branch_exists(&w.repo, &branch));
        assert_eq!(w.sessions(), 0);
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
            matches!(&taken, CardTakeOver::Conflict { files } if files == &["change.txt".to_owned()]),
            "{:?}",
            taken
        );
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
            w.take_over(card, None).unwrap(),
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
        assert!(matches!(taken, CardTakeOver::Done { .. }), "{:?}", taken);
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
            self.shared(|db| integrate_blocking(db, &roots, &worktrees, card, false))
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
            .unwrap();
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

    impl World {
        fn take_over_plan(&mut self, plan: i64) -> Result<PlanTakeOver> {
            let worktrees = self.locations().worktrees;
            self.shared(|db| take_over_plan_blocking(db, &worktrees, plan))
        }

        fn close_plan(&mut self, plan: i64) -> Result<PlanClosed> {
            let worktrees = self.locations().worktrees;
            self.shared(|db| close_plan_blocking(db, &worktrees, plan))
        }
    }

    #[test]
    fn a_plan_given_up_is_closed_with_its_line_and_can_then_be_deleted() {
        let mut w = world();
        let (plan, cards, line) = w.line_plan(&["a"]);
        flow::set_plan_base_branch(&w.db, plan, &line.base_branch).unwrap();
        w.start_on(cards[0], Some("sonnet"), Some(&line.branch))
            .unwrap();

        // A session still holds the card: the plan stays as it is.
        assert!(w.close_plan(plan).is_err());
        assert!(line.path.exists());

        flow::cancel_card(&w.db, cards[0], "human:owner", "given up").unwrap();
        let closed = w.close_plan(plan).unwrap();
        assert!(closed.closed, "{closed:?}");
        assert!(closed.cleanup.is_empty(), "{:?}", closed.cleanup);
        assert!(!line.path.exists());
        assert!(!worktree::branch_exists(&w.repo, &line.branch));
        assert!(flow::delete_plan(&mut w.db, plan).unwrap());
    }

    #[test]
    fn closing_a_plan_keeps_a_line_with_work_the_branch_lacks() {
        let mut w = world();
        let (plan, cards, line) = w.line_plan(&["a"]);
        flow::set_plan_base_branch(&w.db, plan, &line.base_branch).unwrap();
        w.worked_on_the_line(cards[0], &line, "a.txt", "a\n");
        w.integrate(cards[0]).unwrap();

        let closed = w.close_plan(plan).unwrap();
        assert!(closed.closed);
        assert!(
            closed
                .cleanup
                .iter()
                .any(|note| note.contains(&line.branch)),
            "{:?}",
            closed.cleanup
        );
        assert!(worktree::branch_exists(&w.repo, &line.branch));
        assert_eq!(
            git(&w.repo, &["show", &format!("{}:a.txt", line.branch)]),
            "a"
        );
    }

    #[test]
    fn a_finished_plan_is_taken_over_as_the_cards_commits_and_its_cards_plan_and_line_are_closed() {
        let mut w = world();
        let (plan, cards, line) = w.line_plan(&["a", "b"]);
        w.worked_on_the_line(cards[0], &line, "a.txt", "a\n");
        w.worked_on_the_line(cards[1], &line, "b.txt", "b\n");

        // One card is not on the line yet: nothing is taken over.
        w.integrate(cards[0]).unwrap();
        let refused = w.take_over_plan(plan).unwrap_err().to_string();
        assert!(
            refused.contains(&format!("card #{}", cards[1])),
            "{refused}"
        );
        assert_eq!(git(&w.repo, &["rev-list", "--count", "main"]), "1");

        w.integrate(cards[1]).unwrap();
        let taken = w.take_over_plan(plan).unwrap();
        let PlanTakeOver::Done {
            commit,
            card_ids,
            cleanup,
            ..
        } = taken
        else {
            panic!("expected the plan to be taken over: {taken:?}");
        };
        assert!(cleanup.is_empty(), "{cleanup:?}");
        assert_eq!(card_ids, cards);
        assert_eq!(git(&w.repo, &["rev-parse", "main"]), commit);
        assert_eq!(git(&w.repo, &["show", "main:a.txt"]), "a");
        assert_eq!(git(&w.repo, &["show", "main:b.txt"]), "b");
        assert_eq!(git(&w.repo, &["rev-list", "--count", "main"]), "3");

        for card in cards {
            let closed = store::get_card(&w.db, card).unwrap().unwrap();
            assert!(closed.taken_over_at.is_some() && closed.archived_at.is_some());
        }
        assert_eq!(
            flow::get_plan(&w.db, plan).unwrap().unwrap().status,
            crate::board::PlanStatus::Closed
        );
        assert!(!line.path.exists());
        assert!(!worktree::branch_exists(&w.repo, &line.branch));
        // And a closed plan is not taken over again.
        assert!(w.take_over_plan(plan).is_err());
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

    /// Sets how often a card was sent back, as the reviewers' returns would have.
    fn sent_back(w: &World, card: i64, times: u32) {
        w.db.execute(
            "UPDATE cards SET returned_count = ?2 WHERE id = ?1",
            rusqlite::params![card, times],
        )
        .unwrap();
    }

    fn escalate(w: &World, card: i64) -> Result<CardEscalation> {
        let roles = w.roles.clone();
        escalate_in(&w.db, &w.config, &move |_, _, _| roles.clone(), card)
    }

    #[test]
    fn a_card_sent_back_twice_goes_one_tier_up_in_the_same_session_and_only_once() {
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let started = w.start(card, None).unwrap();
        let before = store::get_card(&w.db, card).unwrap().unwrap().claimed_by;

        sent_back(&w, card, 1);
        assert_eq!(
            escalate(&w, card).unwrap(),
            CardEscalation::NotNeeded { card_id: card },
            "one return is the same role's second try"
        );

        sent_back(&w, card, 2);
        let outcome = escalate(&w, card).unwrap();
        assert!(
            matches!(
                &outcome,
                CardEscalation::Done { agent_id, role, engine_id, from_role, .. }
                    if *agent_id == started.agent.id && role == "builder"
                        && engine_id == "opus" && from_role == "builder"
            ),
            "{outcome:?}"
        );
        let agent = agent_store::get_agent(&w.db, started.agent.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            agent.agent_role, "builder",
            "the same role, a tier higher (Q15)"
        );
        assert_eq!(agent.engine_id.as_deref(), Some("opus"));
        assert_eq!(
            agent.name, started.agent.name,
            "the name holds the claim's actor"
        );
        assert_eq!(agent.card_id, Some(card), "the session is still the card's");
        assert_eq!(
            store::get_card(&w.db, card).unwrap().unwrap().claimed_by,
            before,
            "the claim stays with the same session"
        );
        let history = flow::list_events(&w.db, card, 50).unwrap();
        assert!(
            history
                .iter()
                .any(|e| e.kind == crate::board::EventKind::Escalated)
        );

        assert_eq!(
            escalate(&w, card).unwrap(),
            CardEscalation::NotNeeded { card_id: card },
            "one try, ever"
        );
    }

    #[test]
    fn at_heavy_the_owner_is_told_once_and_the_card_stays() {
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let mut fields = fields_of(&store::get_card(&w.db, card).unwrap().unwrap());
        fields.tier = Some(crate::board::Tier::Heavy);
        store::update_card(&w.db, card, &fields).unwrap();
        let started = w.start(card, None).unwrap();
        sent_back(&w, card, 2);
        let outcome = escalate(&w, card).unwrap();
        assert_eq!(
            outcome,
            CardEscalation::NoStrongerRole {
                card_id: card,
                role: "builder".into()
            }
        );
        let agent = agent_store::get_agent(&w.db, started.agent.id)
            .unwrap()
            .unwrap();
        assert_eq!(agent.agent_role, "builder", "nothing changed");
        assert_eq!(
            escalate(&w, card).unwrap(),
            CardEscalation::NotNeeded { card_id: card },
            "the 'no' is remembered too"
        );
    }

    #[test]
    fn a_higher_tier_without_an_engine_that_can_run_is_not_chosen() {
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        w.start(card, None).unwrap();
        // Heavy has only an engine of its own command: nothing an unattended session runs on.
        w.config.agents.engines.remove("opus");
        w.config
            .agents
            .models
            .default_engines
            .set(Tier::Heavy, None);
        w.config.agents.engines.insert(
            "big".into(),
            Engine {
                tier: Some(Tier::Heavy),
                ..engine("big", "my-harness --go")
            },
        );
        sent_back(&w, card, 2);
        assert!(matches!(
            escalate(&w, card).unwrap(),
            CardEscalation::NoStrongerRole { .. }
        ));
    }

    #[test]
    fn a_light_card_escalates_to_medium_then_no_further_than_one_try() {
        let mut w = world();
        let card = w.card(w.open, Some("builder"));
        let mut fields = fields_of(&store::get_card(&w.db, card).unwrap().unwrap());
        fields.tier = Some(crate::board::Tier::Light);
        store::update_card(&w.db, card, &fields).unwrap();
        let started = w.start(card, None).unwrap();
        assert_eq!(started.engine_id, "haiku");
        sent_back(&w, card, 2);
        let outcome = escalate(&w, card).unwrap();
        assert!(
            matches!(&outcome, CardEscalation::Done { engine_id, .. } if engine_id == "sonnet"),
            "{outcome:?}"
        );
        // Its reviewer now reviews at the worker's new tier: medium, on another medium engine.
        let card_now = store::get_card(&w.db, card).unwrap().unwrap();
        w.roles.push(reviewer_of("reviewer", Tier::Light, "haiku"));
        let (_, tier, engine) =
            choose_reviewer(&w.config, &w.roles, &card_now, None, Some("sonnet"), None).unwrap();
        assert_eq!((tier, engine.as_str()), (Tier::Medium, "sonnet-2"));
    }

    #[test]
    fn the_studio_gave_up_on_a_card_until_the_owner_has_it_done_again() {
        let w = world();
        let card = w.card(w.open, Some("builder"));
        assert!(!gave_up_on(&w.db, card).unwrap());
        flow::add_event(
            &w.db,
            card,
            STUDIO,
            crate::board::EventKind::Note,
            &format!("{GAVE_UP_PREFIX}: x"),
        )
        .unwrap();
        assert!(gave_up_on(&w.db, card).unwrap());
        // Anyone's note with the same words is not the studio's.
        flow::add_event(
            &w.db,
            card,
            "agent:other-9",
            crate::board::EventKind::Note,
            REDO_PREFIX,
        )
        .unwrap();
        assert!(gave_up_on(&w.db, card).unwrap());
        flow::add_event(
            &w.db,
            card,
            STUDIO,
            crate::board::EventKind::Note,
            REDO_PREFIX,
        )
        .unwrap();
        assert!(!gave_up_on(&w.db, card).unwrap());
        flow::add_event(
            &w.db,
            card,
            STUDIO,
            crate::board::EventKind::Note,
            &format!("{GAVE_UP_PREFIX}: again"),
        )
        .unwrap();
        assert!(
            gave_up_on(&w.db, card).unwrap(),
            "a second give-up counts again"
        );
    }

    #[test]
    fn only_a_live_signed_off_card_the_studio_gave_up_on_is_left_for_the_owner() {
        let w = world();
        let plan = flow::create_plan(
            &w.db,
            w.board,
            &axiomata_board::PlanFields {
                name: "p".into(),
                goal: String::new(),
                project_id: None,
                auto_start_max: Some(64),
                max_cost_usd: None,
                max_tokens: None,
            },
        )
        .unwrap();
        let mut ids = Vec::new();
        for _ in 0..3 {
            let card = w.card(w.open, Some("builder"));
            let mut fields = store::get_card(&w.db, card).unwrap().unwrap();
            fields.plan_id = Some(plan.id);
            store::update_card(&w.db, card, &fields_of(&fields)).unwrap();
            flow::add_event(
                &w.db,
                card,
                STUDIO,
                crate::board::EventKind::Note,
                &format!("{GAVE_UP_PREFIX}: x"),
            )
            .unwrap();
            w.db.execute(
                "UPDATE cards SET claimed_by = 'agent:w-2', claimed_at = '2026-10-05T09:00:00Z', verified_by = 'agent:r-1', verified_at = '2026-10-05T10:00:00Z' WHERE id = ?1",
                [card],
            )
            .unwrap();
            ids.push(card);
        }
        w.db.execute(
            "UPDATE cards SET taken_over_at = '2026-10-05T11:00:00Z' WHERE id = ?1",
            [ids[1]],
        )
        .unwrap();
        w.db.execute(
            "UPDATE cards SET canceled_at = '2026-10-05T11:00:00Z' WHERE id = ?1",
            [ids[2]],
        )
        .unwrap();
        assert_eq!(cards_left_for_owner(&w.db, plan.id).unwrap(), [ids[0]]);
    }

    #[test]
    fn clearing_the_returns_starts_the_count_from_zero() {
        let w = world();
        let card = w.card(w.open, Some("builder"));
        sent_back(&w, card, 3);
        assert!(flow::clear_returned_count(&w.db, card).unwrap());
        assert_eq!(
            store::get_card(&w.db, card)
                .unwrap()
                .unwrap()
                .returned_count,
            0
        );
        assert!(!flow::clear_returned_count(&w.db, 9999).unwrap());
    }
}
