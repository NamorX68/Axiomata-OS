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

/// Refuses `what` in an agent session: it is one of the owner's gates (a2a.md A7, A17).
pub fn owner_only(what: &str) -> Result<()> {
    owner_only_in(session_actor().as_deref(), what)
}

fn owner_only_in(session: Option<&str>, what: &str) -> Result<()> {
    match session {
        Some(own) => bail!("{what} is for the owner; {own} proposes and the owner decides"),
        None => Ok(()),
    }
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
        auto: Option<u32>,
        /// Go back to starting cards by hand.
        #[arg(long, conflicts_with = "auto")]
        manual: bool,
        #[arg(long)]
        max_cost: Option<f64>,
        #[arg(long)]
        max_tokens: Option<u64>,
    },
    /// Say yes to a plan: its proposals move to Offen and it becomes approved.
    Approve {
        id: i64,
        #[arg(long)]
        actor: Option<String>,
    },
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
            ..
        } if auto.is_some() || max_cost.is_some() || max_tokens.is_some() => {
            owner_only("setting automatic starts or limits on a plan")?
        }
        PlanAction::Edit { .. } => owner_only("changing a plan's settings")?,
        PlanAction::Approve { .. } => owner_only("approving a plan")?,
        PlanAction::Close { .. } => owner_only("closing a plan")?,
        PlanAction::Delete { .. } => owner_only("deleting a plan")?,
        _ => {}
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
            auto,
            max_cost,
            max_tokens,
        } => {
            let plan = flow::create_plan(
                &db,
                board,
                &board::PlanFields {
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
        PlanAction::Close { id } => {
            if !flow::close_plan(&db, id)? {
                bail!("plan #{id} does not exist or is closed already");
            }
            println!("plan #{id} closed");
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
        board::TaskState::InputRequired => "input-required",
        board::TaskState::InReview => "in-review",
        board::TaskState::Done => "done",
        board::TaskState::Verified => "verified",
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
    fn the_owners_gates_are_closed_in_an_agent_session_and_open_in_the_owners_terminal() {
        assert!(owner_only_in(None, "approving").is_ok());
        let refused = owner_only_in(Some("agent:builder-7"), "approving")
            .unwrap_err()
            .to_string();
        assert!(
            refused.contains("is for the owner") && refused.contains("agent:builder-7"),
            "{refused}"
        );
    }
}
