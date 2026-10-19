//! The agent flow on the board, as commands (`docs/plans/a2a.md`, CP-A2): plans, dependencies, the card history and
//! the steps an agent takes through the columns. Thin on purpose — the rules live in `axiomata_board::flow`, and the
//! same functions become the MCP tools in CP-A4.

use anyhow::{Context, Result, bail};
use axiomata_core::board::{self, flow};
use axiomata_core::{AxiomataCore, board_mirror};
use clap::{Args, Subcommand};

use axiomata_core::session::session_actor;

use crate::read_config;

/// Who a command acts as. In an agent session it is that agent, and a different `--actor` is refused; in the owner's
/// own terminal it is whatever was passed, `human:owner` by default.
pub fn resolve_actor(given: Option<String>) -> Result<String> {
    actor_in(session_actor(), given)
}

fn actor_in(session: Option<String>, given: Option<String>) -> Result<String> {
    match session {
        Some(own) => {
            if let Some(typed) = given {
                let typed = typed.trim().to_ascii_lowercase();
                if typed != own {
                    bail!(
                        "this shell belongs to the agent session {own}; it cannot act as {typed}"
                    );
                }
            }
            Ok(own)
        }
        None => Ok(given.unwrap_or_else(|| "human:owner".to_string())),
    }
}

/// Which kind of app-harness run this command comes from ([`axiomata_core::agent_entry::HARNESS_RUN_ENV`]), if any.
/// Its words may come from untrusted text and no agent session stands behind it: it is not the owner either. On Glut
/// and under a skill's declared rules the mark holds; on Opencode's unattended shell a model could drop it, so there
/// it is a second line, not a boundary (glut-round, security audits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarnessRun {
    /// A skill or routine run: nobody watches it.
    Skill,
    /// A chat turn: the owner asked for it.
    Chat,
}

pub fn harness_run_kind() -> Option<HarnessRun> {
    let value = std::env::var_os(axiomata_core::agent_entry::HARNESS_RUN_ENV)?;
    // Any value but "chat" is the stricter one.
    Some(if value == axiomata_core::agent_entry::HARNESS_RUN_CHAT {
        HarnessRun::Chat
    } else {
        HarnessRun::Skill
    })
}

/// Whether an app harness runs this command at all.
pub fn harness_run() -> bool {
    harness_run_kind().is_some()
}

/// Refuses touching card `id` from an app harness's run unless it is a plain card: in a plain column, in no plan, for
/// no role, taken by nobody, with no acceptance text — a flow card's text becomes an agent's prompt, and its column
/// moves are the owner's yes or a reviewer's word (glut-round, fourth audit).
pub fn plain_card_in_harness_run(db: &rusqlite::Connection, id: i64) -> Result<()> {
    if harness_run() {
        plain_card(db, id)
    } else {
        Ok(())
    }
}

fn plain_card(db: &rusqlite::Connection, id: i64) -> Result<()> {
    let card = board::store::get_card(db, id)?.with_context(|| format!("no card with id {id}"))?;
    let stage = board::store::get_column(db, card.column_id)?.and_then(|c| c.stage);
    let flow_card = card.plan_id.is_some()
        || card.agent.is_some()
        || card.claimed_by.is_some()
        || !card.acceptance.trim().is_empty();
    if flow_card || stage.is_some() {
        bail!(
            "card #{id} is part of the agent flow; a skill or chat turn of the app leaves it to the owner"
        );
    }
    Ok(())
}

/// Refuses moving a card from an app harness's run into a flow column or a done one: approving, reviewing and finishing
/// are the owner's and the reviewers' steps.
pub fn plain_target_in_harness_run(db: &rusqlite::Connection, column: i64) -> Result<()> {
    if harness_run() {
        plain_target(db, column)
    } else {
        Ok(())
    }
}

fn plain_target(db: &rusqlite::Connection, column: i64) -> Result<()> {
    let target = board::store::get_column(db, column)?
        .with_context(|| format!("no column with id {column}"))?;
    if target.stage.is_some() || target.maps_to_status == board::CardStatus::Done {
        bail!("a skill or chat turn of the app moves cards between plain open columns only");
    }
    Ok(())
}

/// Refuses `what` in an agent session and in an app harness's run: it is one of the owner's gates (a2a.md A7, A17).
pub fn owner_only(what: &str) -> Result<()> {
    owner_only_in(session_actor().as_deref(), harness_run(), what)
}

fn owner_only_in(session: Option<&str>, harness_run: bool, what: &str) -> Result<()> {
    match session {
        Some(own) => bail!("{what} is for the owner; {own} proposes and the owner decides"),
        None if harness_run => {
            bail!("{what} is for the owner; a skill or chat turn of the app does not decide it")
        }
        None => Ok(()),
    }
}

/// Refuses an agent's step (taking, reporting, judging a card) in an app harness's run: no session takes it there.
pub fn no_agent_step_in_a_harness_run(what: &str) -> Result<()> {
    not_in_harness_run(harness_run(), what)
}

fn not_in_harness_run(harness_run: bool, what: &str) -> Result<()> {
    if harness_run {
        bail!(
            "{what} is an agent session's step; a skill or chat turn of the app does not take it"
        );
    }
    Ok(())
}

/// What a card an app harness's run adds, edits or moves may be (glut-round W8, security audit): a plain card in a
/// plain column. Its words may come from untrusted text (a `ToDo.md`, a page), so it joins or leaves no plan, names no
/// role and sets no flow field — a card in a self-running plan becomes an agent's prompt — and it goes into neither the
/// proposal nor the review column.
pub fn plain_card_only(
    stage: Option<board::ColumnStage>,
    flow: &FlowFlags,
    leaves_plan: bool,
) -> Result<()> {
    if !flow.is_empty() || leaves_plan {
        bail!("a skill's card gets no plan, role or flow field; the owner sets those on the board");
    }
    if stage.is_some() {
        bail!(
            "a skill puts cards into a plain column, never into the proposal or the review column"
        );
    }
    Ok(())
}

/// Refuses `what` in an agent session: for an agent it goes through the MCP server, which knows the session by a
/// secret. This shell only knows it by `AXIOMATA_AGENT_ID`, which every child process of the harness inherits and any
/// of them can change — so a step that depends on *who* takes it (taking a card, reporting it, judging it, proposing
/// one) is not taken here (`docs/plans/a2a.md`, A39/CP-A5). The owner's own terminal is not affected.
pub fn mcp_only(what: &str) -> Result<()> {
    mcp_only_in(session_actor().as_deref(), what)
}

fn mcp_only_in(session: Option<&str>, what: &str) -> Result<()> {
    match session {
        Some(own) => bail!(
            "{what} is not taken from the shell of {own}: use the `{}` MCP tools of this session (and if it has none, \
             ask the owner)",
            axiomata_core::agent_entry::SERVER_NAME
        ),
        None => Ok(()),
    }
}

/// The agent fields of a card, shared by `board add` and `board edit`.
#[derive(Debug, Default, Args)]
pub struct FlowFlags {
    /// The kind of work: a lower-case word such as implement, review, test, doc.
    #[arg(long)]
    pub kind: Option<String>,
    /// How strong an agent the card needs: light, medium or heavy.
    #[arg(long)]
    pub tier: Option<String>,
    /// The role this card is meant for (a name under ~/.axiomata/agents).
    #[arg(long)]
    pub agent: Option<String>,
    /// Why that role.
    #[arg(long)]
    pub agent_reason: Option<String>,
    /// Acceptance criteria, Markdown.
    #[arg(long)]
    pub acceptance: Option<String>,
    /// The plan the card belongs to.
    #[arg(long)]
    pub plan: Option<i64>,
}

impl FlowFlags {
    /// Whether no flow field was passed.
    pub fn is_empty(&self) -> bool {
        self.kind.is_none()
            && self.tier.is_none()
            && self.agent.is_none()
            && self.agent_reason.is_none()
            && self.acceptance.is_none()
            && self.plan.is_none()
    }

    /// Lays what was passed over `fields`; whatever was not passed keeps its value.
    pub fn apply(&self, fields: &mut board::CardFields) -> Result<()> {
        if let Some(kind) = &self.kind {
            fields.kind = Some(kind.clone());
        }
        if let Some(raw) = &self.tier {
            fields.tier = Some(board::Tier::parse(raw).with_context(|| {
                format!("unknown tier {raw:?} — expected light, medium or heavy")
            })?);
        }
        if let Some(agent) = &self.agent {
            fields.agent = Some(agent.clone());
        }
        if let Some(reason) = &self.agent_reason {
            fields.agent_reason = Some(reason.clone());
        }
        if let Some(acceptance) = &self.acceptance {
            fields.acceptance = acceptance.clone();
        }
        if let Some(plan) = self.plan {
            fields.plan_id = Some(plan);
        }
        Ok(())
    }
}

#[derive(Debug, Subcommand)]
pub enum PlanAction {
    /// List a board's plans.
    List { board: i64 },
    /// Create a plan (a draft) on a board.
    New {
        board: i64,
        name: String,
        /// What the plan is for; the planner reads it.
        #[arg(long, default_value = "")]
        goal: String,
        /// The project (repository) the plan's sessions run in.
        #[arg(long)]
        project: Option<i64>,
        /// Start ready cards by themselves, up to this many at once. Omitted = start by hand.
        #[arg(long)]
        auto: Option<u32>,
        /// Cost limit for the whole plan, USD.
        #[arg(long)]
        max_cost: Option<f64>,
        /// Token limit for the whole plan.
        #[arg(long)]
        max_tokens: Option<u64>,
    },
    /// Change a plan's settings. A flag left out keeps its value.
    Edit {
        id: i64,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        project: Option<i64>,
        #[arg(long)]
        auto: Option<u32>,
        /// Go back to starting cards by hand.
        #[arg(long, conflicts_with = "auto")]
        manual: bool,
        #[arg(long)]
        max_cost: Option<f64>,
        #[arg(long)]
        max_tokens: Option<u64>,
    },
    /// Start a planner for a draft plan: a session that reads the project (read-only) and proposes cards. The owner's
    /// step; a session needs an engine, since the planner role names none.
    Start {
        id: i64,
        /// The project (repository) the planner reads.
        #[arg(long)]
        project: i64,
        /// An engine of the catalog (`ide engines list`).
        #[arg(long)]
        engine: Option<String>,
        /// Start a session that grills the plan's goal (interviews you, then proposes a sharper goal) instead of a
        /// planner.
        #[arg(long)]
        grill: bool,
    },
    /// The sharper goal a grilling session proposed for a plan. Without a flag it is shown; `--accept` makes it the
    /// plan's goal, `--discard` drops it. The owner's step: the goal is the owner's own words.
    Goal {
        id: i64,
        /// Take the proposal over as the plan's goal.
        #[arg(long, conflicts_with = "discard")]
        accept: bool,
        /// Drop the proposal.
        #[arg(long)]
        discard: bool,
    },
    /// Start a board card as a plan (docs/plans/card-as-plan.md): a draft plan in the project whose goal is the card,
    /// which runs by itself once approved; the card moves to In Progress and then follows its plan across the board.
    /// Start a planner for it next (`board plan start <plan> --project <p>`). The owner's step.
    FromCard {
        card: i64,
        /// The project (repository) the plan's steps run in.
        #[arg(long)]
        project: i64,
    },
    /// Say yes to a plan: its proposals move to Offen and it becomes approved.
    Approve {
        id: i64,
        #[arg(long)]
        actor: Option<String>,
    },
    /// What a plan's sessions spent, against its limits, and what the studio spent today against the day's cap.
    Spend { id: i64 },
    /// Go on after a plan reached its limit: a fresh allowance on top of what it spent. The owner's step.
    Resume { id: i64 },
    /// Close a plan: nothing starts from it any more.
    Close { id: i64 },
    /// Delete a plan. Its cards stay, without a plan.
    Delete { id: i64 },
}

#[derive(Debug, Subcommand)]
pub enum DepAction {
    /// Make a card wait for another one (both in the same plan, no cycles).
    Add { card: i64, needs: i64 },
    /// Remove such an edge.
    Remove { card: i64, needs: i64 },
    /// Every edge on a board.
    List { board: i64 },
}

fn plan_line(plan: &board::Plan) -> String {
    let auto = plan
        .auto_start_max
        .map_or("by hand".to_string(), |n| format!("auto, up to {n}"));
    format!(
        "#{:<4} {}  [{}] {auto}",
        plan.id,
        plan.name,
        plan.status.as_str()
    )
}

pub fn plan_cmd(core: &AxiomataCore, action: PlanAction) -> Result<()> {
    // An agent may draft a plan; what spends money or opens the gate is the owner's (A7, A9, A17).
    match &action {
        PlanAction::New {
            auto,
            max_cost,
            max_tokens,
            project,
            ..
        } if auto.is_some() || max_cost.is_some() || max_tokens.is_some() || project.is_some() => {
            // The project is where the plan's sessions will run: choosing it is choosing where an agent may work.
            owner_only("setting a plan's project, automatic starts or limits")?
        }
        // The goal is what a planner is told to do: an agent that could write it would be writing the owner's brief.
        PlanAction::New { goal, .. } if !goal.is_empty() => owner_only("writing a plan's goal")?,
        PlanAction::Edit { .. } => owner_only("changing a plan's settings")?,
        PlanAction::Start { .. } => owner_only("starting a planner")?,
        PlanAction::FromCard { .. } => owner_only("starting a card as a plan")?,
        PlanAction::Approve { .. } => owner_only("approving a plan")?,
        PlanAction::Resume { .. } => owner_only("giving a plan a new allowance")?,
        PlanAction::Goal { .. } => owner_only("reading or taking over a proposed goal")?,
        PlanAction::Close { .. } => owner_only("closing a plan")?,
        PlanAction::Delete { .. } => owner_only("deleting a plan")?,
        _ => {}
    }
    // Closing takes the database lock itself, between the git steps that remove the plan's line.
    if let PlanAction::Close { id } = action {
        return close_plan(core, id);
    }
    let mut db = core.db_lock();
    match action {
        PlanAction::List { board } => {
            let plans = flow::list_plans(&db, board)?;
            if plans.is_empty() {
                println!("no plans on board #{board}");
            }
            for plan in plans {
                println!("{}", plan_line(&plan));
            }
        }
        PlanAction::New {
            board,
            name,
            goal,
            project,
            auto,
            max_cost,
            max_tokens,
        } => {
            let plan = flow::create_plan(
                &db,
                board,
                &board::PlanFields {
                    project_id: project,
                    goal,
                    name,
                    auto_start_max: auto,
                    max_cost_usd: max_cost,
                    max_tokens,
                },
            )?;
            println!("created plan {}", plan_line(&plan));
        }
        PlanAction::Edit {
            id,
            name,
            goal,
            project,
            auto,
            manual,
            max_cost,
            max_tokens,
        } => {
            let Some(plan) = flow::get_plan(&db, id)? else {
                bail!("no plan with id {id}");
            };
            let updated = flow::update_plan(
                &db,
                id,
                &board::PlanFields {
                    project_id: project.or(plan.project_id),
                    goal: goal.unwrap_or(plan.goal),
                    name: name.unwrap_or(plan.name),
                    auto_start_max: if manual {
                        None
                    } else {
                        auto.or(plan.auto_start_max)
                    },
                    max_cost_usd: max_cost.or(plan.max_cost_usd),
                    max_tokens: max_tokens.or(plan.max_tokens),
                },
            )?
            .with_context(|| format!("no plan with id {id}"))?;
            println!("updated plan {}", plan_line(&updated));
        }
        PlanAction::Spend { id } => {
            let config = crate::read_config(core);
            let spend = axiomata_core::studio_spend::plan_spend(&db, &config, id)?;
            println!(
                "plan #{id}: {} tokens of {}, ${:.2} of ${:.2}",
                spend.spent.tokens,
                spend.limits.max_tokens,
                spend.spent.cost_usd,
                spend.limits.max_cost_usd
            );
            if let Some(why) = &spend.plan_over {
                println!("  held back: {why} (`board plan resume {id}` goes on)");
            }
            match spend.day_cap_usd {
                Some(cap) => println!("today: ${:.2} of the day's ${cap:.2}", spend.today.cost_usd),
                None => println!("today: ${:.2} (no daily cap)", spend.today.cost_usd),
            }
            if let Some(why) = &spend.day_over {
                println!("  held back for today: {why}");
            }
        }
        PlanAction::Goal {
            id,
            accept,
            discard,
        } => {
            if accept {
                // Taking over from the command line is taking over what is there now: the proposal is read in the same
                // breath.
                let Some(read) = flow::goal_suggestion(&db, id)? else {
                    println!("plan #{id} has no proposed goal");
                    return Ok(());
                };
                match flow::apply_goal_suggestion(&mut db, id, &read.at)? {
                    Some(plan) => println!(
                        "plan #{id}: the proposed goal is now the goal\n\n{}",
                        plan.goal
                    ),
                    None => println!("plan #{id} has no proposed goal"),
                }
            } else if discard {
                let gone = flow::clear_goal_suggestion(&db, id)?;
                println!(
                    "{}",
                    if gone {
                        "proposed goal discarded"
                    } else {
                        "there was none"
                    }
                );
            } else {
                match flow::goal_suggestion(&db, id)? {
                    Some(proposed) => println!("proposed {}:\n\n{}", proposed.at, proposed.goal),
                    None => println!("plan #{id} has no proposed goal"),
                }
            }
        }
        PlanAction::Resume { id } => {
            let config = crate::read_config(core);
            let plan = axiomata_core::studio_spend::resume_plan(&db, &config, id)?;
            println!(
                "plan #{id} goes on: limits now ${:.2} and {} tokens",
                plan.max_cost_usd.unwrap_or_default(),
                plan.max_tokens.unwrap_or_default()
            );
        }
        PlanAction::Start { .. } | PlanAction::FromCard { .. } => {
            bail!("starting a planner is an asynchronous step and is handled before this point")
        }
        PlanAction::Approve { id, actor } => {
            let actor = resolve_actor(actor)?;
            let Some(moved) = flow::approve_plan(&mut db, id, &actor)? else {
                bail!("plan #{id} does not exist or is not a draft");
            };
            if let Some(plan) = flow::get_plan(&db, id)? {
                board_mirror::after_change(&db, &read_config(core), plan.board_id);
            }
            println!("plan #{id} approved — {moved} card(s) moved to Offen");
        }
        PlanAction::Close { .. } => {
            unreachable!("closing a plan is handled before the lock is taken")
        }
        PlanAction::Delete { id } => {
            let board_id = flow::get_plan(&db, id)?.map(|plan| plan.board_id);
            if !flow::delete_plan(&mut db, id)? {
                bail!("no plan with id {id}");
            }
            if let Some(board_id) = board_id {
                board_mirror::after_change(&db, &read_config(core), board_id);
            }
            println!("plan #{id} deleted (its cards stay on the board)");
        }
    }
    Ok(())
}

/// Closes plan `id` and removes its line (`card_session::close_plan`).
fn close_plan(core: &AxiomataCore, id: i64) -> Result<()> {
    let worktrees = axiomata_core::paths::ide_locations().worktrees;
    let outcome = axiomata_core::card_session::close_plan_blocking(&core.db, &worktrees, id)?;
    if !outcome.closed {
        bail!("plan #{id} does not exist or is closed already");
    }
    let db = core.db_lock();
    // The card the plan was made for left it (to Open or Done).
    if let Some(plan) = flow::get_plan(&db, id)?
        && plan.origin_card_id.is_some()
    {
        board_mirror::after_change(&db, &read_config(core), plan.board_id);
    }
    println!("plan #{id} closed");
    for note in outcome.cleanup {
        println!("  note: {note}");
    }
    Ok(())
}

pub fn dep_cmd(core: &AxiomataCore, action: DepAction) -> Result<()> {
    let mut db = core.db_lock();
    match action {
        DepAction::Add { card, needs } => {
            if flow::add_dependency(&mut db, card, needs)? {
                board_mirror::after_card_change(&db, &read_config(core), card);
                println!("card #{card} now waits for card #{needs}");
            } else {
                println!("card #{card} already waits for card #{needs}");
            }
        }
        DepAction::Remove { card, needs } => {
            if !flow::remove_dependency(&db, card, needs)? {
                bail!("card #{card} does not wait for card #{needs}");
            }
            board_mirror::after_card_change(&db, &read_config(core), card);
            println!("card #{card} no longer waits for card #{needs}");
        }
        DepAction::List { board } => {
            let edges = flow::list_dependencies(&db, board)?;
            if edges.is_empty() {
                println!("no dependencies on board #{board}");
            }
            for (card, needs) in edges {
                println!("#{card} needs #{needs}");
            }
        }
    }
    Ok(())
}

/// What `board report|verdict|…` do, by name — the agent's steps through the columns.
pub fn report(core: &AxiomataCore, id: i64, actor: &str) -> Result<()> {
    let mut db = core.db_lock();
    flow::report_done(&mut db, id, actor)?;
    board_mirror::after_card_change(&db, &read_config(core), id);
    println!("card #{id} reported done — it waits in the review column");
    Ok(())
}

pub fn verdict(core: &AxiomataCore, id: i64, actor: &str, approve: bool, note: &str) -> Result<()> {
    let mut db = core.db_lock();
    let verdict = if approve {
        flow::Verdict::Approve
    } else {
        flow::Verdict::Return
    };
    flow::review_verdict(&mut db, id, actor, verdict, note)?;
    board_mirror::after_card_change(&db, &read_config(core), id);
    println!(
        "card #{id} {}",
        if approve { "approved" } else { "sent back" }
    );
    Ok(())
}

pub fn events(core: &AxiomataCore, id: i64, limit: usize) -> Result<()> {
    let db = core.db_lock();
    if board::store::get_card(&db, id)?.is_none() {
        bail!("no card with id {id}");
    }
    let events = flow::list_events(&db, id, limit)?;
    if events.is_empty() {
        println!("no history yet");
    }
    for event in events {
        println!(
            "{}  {:<14} {:<18} {}",
            event.at.format("%Y-%m-%d %H:%M"),
            event.kind.as_str(),
            event.actor,
            event.text.replace('\n', " ")
        );
    }
    Ok(())
}

pub fn note(core: &AxiomataCore, id: i64, actor: &str, text: &str) -> Result<()> {
    let db = core.db_lock();
    if flow::add_event(&db, id, actor, board::EventKind::Note, text)?.is_none() {
        bail!("no card with id {id}");
    }
    board_mirror::after_card_change(&db, &read_config(core), id);
    println!("noted on card #{id}");
    Ok(())
}

pub fn input(core: &AxiomataCore, id: i64, actor: &str, ask: Option<&str>) -> Result<()> {
    let db = core.db_lock();
    if !flow::set_input_required(&db, id, actor, ask)? {
        bail!("no card with id {id}");
    }
    board_mirror::after_card_change(&db, &read_config(core), id);
    println!(
        "{}",
        if ask.is_some() {
            format!("card #{id} now waits for an answer")
        } else {
            format!("card #{id}: question cleared")
        }
    );
    Ok(())
}

/// Which terminal mark `board fail|cancel|reopen|taken-over` sets.
pub enum Mark {
    Fail,
    Cancel,
    Reopen,
    TakeOver,
}

pub fn mark(core: &AxiomataCore, id: i64, actor: &str, what: Mark, reason: &str) -> Result<()> {
    let db = core.db_lock();
    let (done, message) = match what {
        Mark::Fail => (flow::mark_failed(&db, id, actor, reason)?, "marked failed"),
        Mark::Cancel => (flow::cancel_card(&db, id, actor, reason)?, "canceled"),
        Mark::Reopen => (flow::reopen_card(&db, id, actor)?, "reopened"),
        Mark::TakeOver => (
            flow::mark_taken_over(&db, id, actor)?,
            "taken over and archived",
        ),
    };
    if !done {
        bail!(
            "card #{id} cannot be changed that way (no such card, already in that state, or not signed off)"
        );
    }
    board_mirror::after_card_change(&db, &read_config(core), id);
    println!("card #{id} {message}");
    Ok(())
}

pub fn approve_proposal(core: &AxiomataCore, id: i64, actor: &str) -> Result<()> {
    let mut db = core.db_lock();
    if !flow::approve_proposal(&mut db, id, actor)? {
        bail!("card #{id} is not a proposal");
    }
    board_mirror::after_card_change(&db, &read_config(core), id);
    println!("proposal #{id} approved — it is open now");
    Ok(())
}

/// One extra line for a card under `board list`: the parts of the flow that are not obvious from its column.
pub fn flow_marks(card: &board::Card) -> String {
    let mut marks = String::new();
    if !matches!(
        card.state,
        board::TaskState::Ready | board::TaskState::Working
    ) {
        marks.push_str(&format!("  state:{}", state_name(card.state)));
    }
    if let Some(plan) = card.origin_of {
        marks.push_str(&format!("  goal-of:#{plan}"));
    }
    if let Some(plan) = card.plan_id {
        marks.push_str(&format!("  plan:#{plan}"));
    }
    if let Some(agent) = &card.agent {
        marks.push_str(&format!("  role:{agent}"));
    }
    if !card.waiting_on.is_empty() {
        let ids: Vec<String> = card.waiting_on.iter().map(|id| format!("#{id}")).collect();
        marks.push_str(&format!("  waits:{}", ids.join(",")));
    }
    marks
}

fn state_name(state: board::TaskState) -> &'static str {
    match state {
        board::TaskState::Proposed => "proposed",
        board::TaskState::Blocked => "blocked",
        board::TaskState::Ready => "ready",
        board::TaskState::Working => "working",
        board::TaskState::Planned => "planned",
        board::TaskState::InputRequired => "input-required",
        board::TaskState::InReview => "in-review",
        board::TaskState::Done => "done",
        board::TaskState::Verified => "verified",
        board::TaskState::Integrated => "integrated",
        board::TaskState::TakenOver => "taken-over",
        board::TaskState::Failed => "failed",
        board::TaskState::Canceled => "canceled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owners_terminal_acts_as_the_owner_unless_told_otherwise() {
        assert_eq!(actor_in(None, None).unwrap(), "human:owner");
        assert_eq!(
            actor_in(None, Some("agent:test".into())).unwrap(),
            "agent:test"
        );
    }

    #[test]
    fn an_agent_session_always_acts_as_itself() {
        let own = Some("agent:builder-7".to_string());
        assert_eq!(actor_in(own.clone(), None).unwrap(), "agent:builder-7");
        assert_eq!(
            actor_in(own.clone(), Some(" Agent:Builder-7 ".into())).unwrap(),
            "agent:builder-7"
        );
        for lie in ["human:owner", "agent:reviewer-2", "agent:builder-8"] {
            assert!(actor_in(own.clone(), Some(lie.into())).is_err(), "{lie}");
        }
    }

    #[test]
    fn the_steps_that_depend_on_who_takes_them_are_for_the_mcp_server_in_an_agent_session() {
        assert!(mcp_only_in(None, "claiming a card").is_ok());
        let refused = mcp_only_in(Some("agent:builder-7"), "claiming a card")
            .unwrap_err()
            .to_string();
        assert!(refused.contains("claiming a card"), "{refused}");
        assert!(refused.contains("MCP tools"), "{refused}");
    }

    #[test]
    fn the_owners_gates_and_the_agents_steps_are_closed_in_a_harness_run() {
        let refused = owner_only_in(None, true, "approving")
            .unwrap_err()
            .to_string();
        assert!(refused.contains("skill or chat turn"), "{refused}");
        assert!(not_in_harness_run(true, "claiming a card").is_err());
        assert!(not_in_harness_run(false, "claiming a card").is_ok());
    }

    /// Fourth audit: a harness run edits or moves only a plain card, and only into a plain open column — no
    /// approving a proposal by moving it, no finishing a card past its review.
    #[test]
    fn a_harness_run_touches_only_plain_cards_and_moves_them_between_plain_open_columns() {
        let dir = std::env::temp_dir().join(format!("axiomata-cli-plain-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut db = axiomata_core::db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
        let created = board::store::create_board(&mut db, "B").unwrap();
        let columns = board::store::list_columns(&db, created.id).unwrap();
        let column =
            |pick: &dyn Fn(&board::Column) -> bool| columns.iter().find(|c| pick(c)).unwrap().id;
        let open = column(&|c| c.stage.is_none() && c.maps_to_status == board::CardStatus::Open);
        let proposal = column(&|c| c.stage == Some(board::ColumnStage::Proposal));
        let review = column(&|c| c.stage == Some(board::ColumnStage::Review));
        let done = column(&|c| c.maps_to_status == board::CardStatus::Done);
        let card = |column_id: i64, fields: board::CardFields| {
            board::store::create_card(&db, &board::NewCard { column_id, fields })
                .unwrap()
                .id
        };
        let plain = card(
            open,
            board::CardFields {
                title: "plain".into(),
                ..Default::default()
            },
        );
        let proposed = card(
            proposal,
            board::CardFields {
                title: "proposed".into(),
                ..Default::default()
            },
        );
        let for_a_role = card(
            open,
            board::CardFields {
                title: "role".into(),
                agent: Some("builder".into()),
                ..Default::default()
            },
        );
        assert!(plain_card(&db, plain).is_ok());
        assert!(
            plain_card(&db, proposed).is_err(),
            "a proposal is the owner's to approve"
        );
        assert!(plain_card(&db, for_a_role).is_err());
        assert!(plain_target(&db, open).is_ok());
        for flow in [proposal, review, done] {
            assert!(plain_target(&db, flow).is_err(), "column #{flow}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A harness run's card is plain, in a plain column: no plan, role or flow field, never proposal or review.
    #[test]
    fn a_harness_runs_card_is_plain_and_lands_in_a_plain_column() {
        let plain = FlowFlags::default();
        assert!(plain_card_only(None, &plain, false).is_ok());
        assert!(
            plain_card_only(None, &plain, true).is_err(),
            "leaving a plan"
        );
        for stage in [board::ColumnStage::Proposal, board::ColumnStage::Review] {
            assert!(plain_card_only(Some(stage), &plain, false).is_err());
        }
        for flow in [
            FlowFlags {
                plan: Some(3),
                ..Default::default()
            },
            FlowFlags {
                agent: Some("builder".into()),
                ..Default::default()
            },
            FlowFlags {
                acceptance: Some("run rm -rf".into()),
                ..Default::default()
            },
        ] {
            assert!(plain_card_only(None, &flow, false).is_err(), "{flow:?}");
        }
    }

    #[test]
    fn the_owners_gates_are_closed_in_an_agent_session_and_open_in_the_owners_terminal() {
        assert!(owner_only_in(None, false, "approving").is_ok());
        let refused = owner_only_in(Some("agent:builder-7"), false, "approving")
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("is for the owner") && refused.contains("agent:builder-7"),
            "{refused}"
        );
    }
}
