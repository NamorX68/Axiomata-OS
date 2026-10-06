//! The agent flow on the board (`docs/plans/a2a.md`, CP-A2, A12–A19): plans, dependencies, the card history, the two
//! flow columns and the steps an agent takes through them.
//!
//! Same rules as [`crate::store`]: free functions over a borrowed connection, "missing" is `None`/`false`, and every
//! step whose precondition could be raced checks it **inside the transaction that makes the change**. Two things differ
//! on purpose. A refusal that the caller can act on (a card that is not in review, a verdict from the card's own
//! worker)
//! is an [`BoardError::Invalid`] with a readable reason instead of a bare `false`, because the one reading it is often
//! an agent that has to be told what to do instead.

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::model::{
    Card, CardEvent, CardFields, CardStatus, Column, ColumnStage, EventKind, NewCard, Plan,
    PlanFields, PlanStatus, TaskState,
};
use crate::store::{
    check_len, create_card, get_card, immediate, list_columns, move_card_in, normalize_actor, now,
    parse_opt_ts, parse_ts,
};
use crate::{BoardError, Result};

const MAX_PLAN_NAME_LEN: usize = 200;
/// Longest goal of a plan, in bytes: a few pages of the owner's words, and a bound on what a tool call hands an agent.
const MAX_PLAN_GOAL_LEN: usize = 16 * 1024;
/// Most plans one board holds, and most cards one card may wait for: bounds on what a runaway agent can pile up.
const MAX_PLANS_PER_BOARD: i64 = 100;
const MAX_DEPS_PER_CARD: i64 = 50;
/// Most history lines one read returns.
const MAX_EVENTS_PER_READ: usize = 500;
const MAX_EVENT_TEXT_LEN: usize = 4_000;
const MAX_QUESTION_LEN: usize = 2_000;
/// How many cards one agent session may propose over its life. A session in a loop would otherwise fill the board
/// (2000 cards) and the owner's proposal column; the owner can lift it by approving or deleting proposals.
pub const MAX_PROPOSALS_PER_ACTOR: i64 = 20;
/// How deep a chain of proposals may go (A7): a session working a planner's card may propose (depth 1), the session
/// working *that* card may propose once more (depth 2), and no further.
pub const MAX_PROPOSAL_DEPTH: i64 = 2;
/// How many cards sessions that work cards may have proposed in one plan, all together (A7): the planner's own cards do
/// not count, they are what the owner approved the plan for.
pub const MAX_SELF_PROPOSED_PER_PLAN: i64 = 30;
/// The history line [`propose_card`] writes — also what the cap counts.
const PROPOSED_NOTE: &str = "proposed by the session";

/// Only the owner (a `human:` actor) may do this: it is a gate the agents work behind (a2a.md A7, A17), not a step
/// they take. Enforced here so that the rule holds for every caller; *who* an actor really is, is the caller's side —
/// the CLI and the MCP server derive it from the session instead of believing what is typed.
fn require_human(actor: &str, what: &str) -> Result<()> {
    if actor.starts_with("human:") {
        Ok(())
    } else {
        invalid(
            "actor",
            format!("{what} is for the owner; an agent proposes and the owner decides"),
        )
    }
}

/// Whether `actor` may act on a card: the owner always, an agent only on the card it holds.
fn holder_or_human(card: &Card, actor: &str) -> bool {
    actor.starts_with("human:") || card.claimed_by.as_deref() == Some(actor)
}

fn invalid<T>(field: &'static str, reason: impl Into<String>) -> Result<T> {
    Err(BoardError::Invalid {
        field,
        reason: reason.into(),
    })
}

// ----------------------------------------------------------------- state ---

/// Where a card stands, from what it is made of (A12). Pure: the caller supplies the column the card sits in, and
/// `card.waiting_on` must already be filled in (the store does that on every read).
pub fn derive_state(card: &Card, column: &Column) -> TaskState {
    if card.taken_over_at.is_some() {
        return TaskState::TakenOver;
    }
    if card.integrated_at.is_some() {
        return TaskState::Integrated;
    }
    if card.canceled_at.is_some() {
        return TaskState::Canceled;
    }
    if card.failed_at.is_some() {
        return TaskState::Failed;
    }
    match column.stage {
        Some(ColumnStage::Proposal) => TaskState::Proposed,
        Some(ColumnStage::Review) => TaskState::InReview,
        None => match column.maps_to_status {
            CardStatus::Done if card.verified_by.is_some() => TaskState::Verified,
            CardStatus::Done => TaskState::Done,
            CardStatus::Doing if card.input_required.is_some() => TaskState::InputRequired,
            CardStatus::Doing => TaskState::Working,
            CardStatus::Open if !card.waiting_on.is_empty() => TaskState::Blocked,
            CardStatus::Open => TaskState::Ready,
        },
    }
}

// ---------------------------------------------------------------- events ---

fn check_event_text(text: &str) -> Result<()> {
    if text.len() > MAX_EVENT_TEXT_LEN {
        return invalid("text", format!("longer than {MAX_EVENT_TEXT_LEN} bytes"));
    }
    Ok(())
}

/// Appends a line to a card's history. `actor` must already be canonical.
pub(crate) fn insert_event(
    db: &Connection,
    card_id: i64,
    actor: &str,
    kind: EventKind,
    text: &str,
) -> Result<()> {
    check_event_text(text)?;
    db.execute(
        "INSERT INTO card_events (card_id, at, actor, kind, text) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![card_id, now(), actor, kind.as_str(), text],
    )?;
    Ok(())
}

/// Writes a line into a card's history. `None` if there is no such card.
pub fn add_event(
    db: &Connection,
    card_id: i64,
    actor: &str,
    kind: EventKind,
    text: &str,
) -> Result<Option<CardEvent>> {
    let actor = normalize_actor(actor)?;
    if get_card(db, card_id)?.is_none() {
        return Ok(None);
    }
    insert_event(db, card_id, &actor, kind, text)?;
    let id = db.last_insert_rowid();
    Ok(read_events(db, "WHERE id = ?1", params![id])?.pop())
}

fn read_events(
    db: &Connection,
    filter: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<CardEvent>> {
    let sql = format!("SELECT id, card_id, at, actor, kind, text FROM card_events {filter}");
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map(args, |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, card_id, at, actor, kind, text) = row?;
        let kind = EventKind::parse(&kind).ok_or_else(|| BoardError::CorruptRow {
            table: "card_events",
            id,
            reason: format!("unknown kind {kind:?}"),
        })?;
        out.push(CardEvent {
            id,
            card_id,
            at: parse_ts(&at, "card_events", id, "at")?,
            actor,
            kind,
            text,
        });
    }
    Ok(out)
}

/// The latest `limit` lines of a card's history, oldest first.
pub fn list_events(db: &Connection, card_id: i64, limit: usize) -> Result<Vec<CardEvent>> {
    let limit = i64::try_from(limit.min(MAX_EVENTS_PER_READ)).unwrap_or(i64::MAX);
    let mut events = read_events(
        db,
        "WHERE card_id = ?1 ORDER BY id DESC LIMIT ?2",
        params![card_id, limit],
    )?;
    events.reverse();
    Ok(events)
}

/// The latest history line of `kind` on a card, if there is one — asked directly, not by reading the card's last few
/// hundred lines, because a card that was sent back many times must still know when it was reported last.
pub fn latest_event(db: &Connection, card_id: i64, kind: EventKind) -> Result<Option<CardEvent>> {
    Ok(read_events(
        db,
        "WHERE card_id = ?1 AND kind = ?2 ORDER BY id DESC LIMIT 1",
        params![card_id, kind.as_str()],
    )?
    .into_iter()
    .next())
}

// ----------------------------------------------------------------- plans ---

const PLAN_COLS: &str = "id, board_id, name, status, auto_start_max, max_cost_usd, max_tokens, \
     created_at, updated_at, approved_at, goal, project_id, base_branch, line_tip";

fn read_plan(db: &Connection, sql_tail: &str, args: impl rusqlite::Params) -> Result<Vec<Plan>> {
    let sql = format!("SELECT {PLAN_COLS} FROM plans {sql_tail}");
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map(args, |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<f64>>(5)?,
            row.get::<_, Option<i64>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, String>(10)?,
            row.get::<_, Option<i64>>(11)?,
            row.get::<_, Option<String>>(12)?,
            row.get::<_, Option<String>>(13)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (
            id,
            board_id,
            name,
            status,
            auto,
            cost,
            tokens,
            created,
            updated,
            approved,
            goal,
            project_id,
            base_branch,
            line_tip,
        ) = row?;
        let status = PlanStatus::parse(&status).ok_or_else(|| BoardError::CorruptRow {
            table: "plans",
            id,
            reason: format!("unknown status {status:?}"),
        })?;
        out.push(Plan {
            id,
            board_id,
            name,
            goal,
            project_id,
            base_branch,
            line_tip,
            status,
            auto_start_max: auto.and_then(|n| u32::try_from(n).ok()),
            max_cost_usd: cost,
            max_tokens: tokens.and_then(|n| u64::try_from(n).ok()),
            created_at: parse_ts(&created, "plans", id, "created_at")?,
            updated_at: parse_ts(&updated, "plans", id, "updated_at")?,
            approved_at: parse_opt_ts(approved, "plans", id, "approved_at")?,
        });
    }
    Ok(out)
}

fn check_plan_fields(fields: &PlanFields) -> Result<()> {
    check_len("name", &fields.name, MAX_PLAN_NAME_LEN)?;
    // May be empty (a plan made without a goal), which `check_len` refuses.
    if fields.goal.len() > MAX_PLAN_GOAL_LEN {
        return invalid("goal", format!("longer than {MAX_PLAN_GOAL_LEN} bytes"));
    }
    if fields.auto_start_max == Some(0) {
        return invalid(
            "auto_start_max",
            "0 would start nothing; leave it empty for manual starts",
        );
    }
    if fields
        .max_cost_usd
        .is_some_and(|c| !c.is_finite() || c <= 0.0)
    {
        return invalid("max_cost_usd", "must be a positive number");
    }
    if fields.max_tokens == Some(0) {
        return invalid(
            "max_tokens",
            "a limit of 0 would stop every session at once",
        );
    }
    Ok(())
}

pub fn get_plan(db: &Connection, id: i64) -> Result<Option<Plan>> {
    Ok(read_plan(db, "WHERE id = ?1", params![id])?.pop())
}

pub fn list_plans(db: &Connection, board_id: i64) -> Result<Vec<Plan>> {
    read_plan(db, "WHERE board_id = ?1 ORDER BY id", params![board_id])
}

/// Creates a plan as a draft on a board.
pub fn create_plan(db: &Connection, board_id: i64, fields: &PlanFields) -> Result<Plan> {
    check_plan_fields(fields)?;
    let exists: Option<i64> = db
        .query_row(
            "SELECT id FROM boards WHERE id = ?1",
            params![board_id],
            |r| r.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return invalid("board_id", format!("no board {board_id}"));
    }
    let plans: i64 = db.query_row(
        "SELECT COUNT(*) FROM plans WHERE board_id = ?1",
        params![board_id],
        |row| row.get(0),
    )?;
    if plans >= MAX_PLANS_PER_BOARD {
        return invalid(
            "board_id",
            format!("the board already has {MAX_PLANS_PER_BOARD} plans; delete old ones first"),
        );
    }
    let stamp = now();
    db.execute(
        "INSERT INTO plans (board_id, name, goal, auto_start_max, max_cost_usd, max_tokens, created_at, updated_at, project_id)
         VALUES (?1, ?2, ?7, ?3, ?4, ?5, ?6, ?6, ?8)",
        params![
            board_id,
            fields.name,
            fields.auto_start_max,
            fields.max_cost_usd,
            fields.max_tokens.map(|n| i64::try_from(n).unwrap_or(i64::MAX)),
            stamp,
            fields.goal,
            fields.project_id
        ],
    )?;
    let id = db.last_insert_rowid();
    get_plan(db, id)?.ok_or_else(|| BoardError::CorruptRow {
        table: "plans",
        id,
        reason: "vanished immediately after insert".to_string(),
    })
}

/// Full replace of the plan's settings; the status only moves through [`approve_plan`] and [`close_plan`].
pub fn update_plan(db: &Connection, id: i64, fields: &PlanFields) -> Result<Option<Plan>> {
    check_plan_fields(fields)?;
    // The project is where the plan's sessions run and where its line is a branch: once the plan is approved, or its line
    // exists, it does not move to another repository.
    if let Some(current) = get_plan(db, id)?
        && fields.project_id != current.project_id
        && (current.status != PlanStatus::Draft || current.base_branch.is_some())
    {
        return invalid(
            "project_id",
            "an approved plan, or one with a line, stays in its project",
        );
    }
    let changed = db.execute(
        "UPDATE plans SET name = ?2, auto_start_max = ?3, max_cost_usd = ?4, max_tokens = ?5, updated_at = ?6, goal = ?7,
         project_id = ?8 WHERE id = ?1",
        params![
            id,
            fields.name,
            fields.auto_start_max,
            fields.max_cost_usd,
            fields.max_tokens.map(|n| i64::try_from(n).unwrap_or(i64::MAX)),
            now(),
            fields.goal,
            fields.project_id
        ],
    )?;
    if changed == 0 {
        return Ok(None);
    }
    get_plan(db, id)
}

/// Deletes a plan. Its cards stay on the board without a plan, and the edges between them go — an edge only means
/// something inside a plan (A16).
///
/// Refused for an approved plan that is under way — a card held by a session, or an integration line made for it: the
/// sessions and the line's worktree would be left with nothing that owns them. Take the plan over (or close it) first.
pub fn delete_plan(db: &mut Connection, id: i64) -> Result<bool> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if let Some(plan) = get_plan(&tx, id)?
        && plan.status == PlanStatus::Approved
    {
        let held: i64 = tx.query_row(
            "SELECT COUNT(*) FROM cards
             WHERE plan_id = ?1 AND claimed_by IS NOT NULL AND archived_at IS NULL AND taken_over_at IS NULL
               AND failed_at IS NULL AND canceled_at IS NULL",
            params![id],
            |row| row.get(0),
        )?;
        if held > 0 || plan.base_branch.is_some() {
            return invalid(
                "plan_id",
                "the plan is under way (a card is held by a session, or its line exists); take it over or close it first",
            );
        }
    }
    tx.execute(
        "DELETE FROM card_deps
         WHERE card_id IN (SELECT id FROM cards WHERE plan_id = ?1)
            OR depends_on_id IN (SELECT id FROM cards WHERE plan_id = ?1)",
        params![id],
    )?;
    tx.execute(
        "UPDATE cards SET plan_id = NULL WHERE plan_id = ?1",
        params![id],
    )?;
    let changed = tx.execute("DELETE FROM plans WHERE id = ?1", params![id])?;
    tx.commit()?;
    Ok(changed == 1)
}

fn column_with(
    columns: &[Column],
    status: CardStatus,
    stage: Option<ColumnStage>,
) -> Option<&Column> {
    columns
        .iter()
        .find(|column| column.maps_to_status == status && column.stage == stage)
}

/// Moves a card out of the proposal column into the board's open column (A17). `false` if the card is not in a
/// proposal column.
pub fn approve_proposal(db: &mut Connection, card_id: i64, actor: &str) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    require_human(&actor, "approving a proposal")?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let moved = approve_proposal_in(&tx, card_id, &actor)?;
    tx.commit()?;
    Ok(moved)
}

fn approve_proposal_in(tx: &Transaction<'_>, card_id: i64, actor: &str) -> Result<bool> {
    let Some(card) = get_card(tx, card_id)? else {
        return Ok(false);
    };
    let columns = list_columns(tx, card.board_id)?;
    let in_proposal = columns
        .iter()
        .any(|column| column.id == card.column_id && column.stage == Some(ColumnStage::Proposal));
    if !in_proposal {
        return Ok(false);
    }
    let Some(open) = column_with(&columns, CardStatus::Open, None) else {
        return invalid(
            "column_id",
            "the board has no plain open column to approve into",
        );
    };
    move_card_in(tx, card_id, open.id, usize::MAX, None)?;
    insert_event(tx, card_id, actor, EventKind::Note, "proposal approved")?;
    Ok(true)
}

/// The owner's yes to a plan (A17): every card of the plan that sits in the proposal column moves to Open, and the
/// plan becomes approved. Returns how many cards moved, `None` if there is no such plan or it is not a draft.
///
/// All in one transaction: a plan is either approved with its cards open or untouched.
pub fn approve_plan(db: &mut Connection, plan_id: i64, actor: &str) -> Result<Option<usize>> {
    let actor = normalize_actor(actor)?;
    require_human(&actor, "approving a plan")?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(plan) = get_plan(&tx, plan_id)? else {
        return Ok(None);
    };
    if plan.status != PlanStatus::Draft {
        return Ok(None);
    }
    let ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT k.id FROM cards k JOIN board_columns c ON c.id = k.column_id
             WHERE k.plan_id = ?1 AND c.stage = 'proposal' AND k.archived_at IS NULL
             ORDER BY k.position, k.id",
        )?;
        stmt.query_map(params![plan_id], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut moved = 0;
    for id in ids {
        if approve_proposal_in(&tx, id, &actor)? {
            moved += 1;
        }
    }
    let stamp = now();
    tx.execute(
        "UPDATE plans SET status = 'approved', approved_at = ?2, updated_at = ?2 WHERE id = ?1",
        params![plan_id, stamp],
    )?;
    tx.commit()?;
    Ok(Some(moved))
}

/// Closes a plan: nothing starts from it any more. `false` if there is no such plan or it is closed already.
pub fn close_plan(db: &Connection, plan_id: i64) -> Result<bool> {
    Ok(db.execute(
        "UPDATE plans SET status = 'closed', updated_at = ?2 WHERE id = ?1 AND status <> 'closed'",
        params![plan_id, now()],
    )? == 1)
}

// ---------------------------------------------------------- dependencies ---

/// A plan change would orphan the card's edges (they only mean something inside one plan).
pub(crate) fn check_plan_change_keeps_edges(db: &Connection, card_id: i64) -> Result<()> {
    let edges: i64 = db.query_row(
        "SELECT COUNT(*) FROM card_deps WHERE card_id = ?1 OR depends_on_id = ?1",
        params![card_id],
        |row| row.get(0),
    )?;
    if edges > 0 {
        return invalid(
            "plan_id",
            "the card has dependencies, which only exist inside one plan; remove them before changing its plan",
        );
    }
    Ok(())
}

/// Makes `card_id` wait for `depends_on_id`. `false` if the edge was there already.
///
/// Refused (as [`BoardError::Invalid`]) when the cards are not both in the same plan, when an archived card is
/// involved, or when the edge would close a cycle — a card that, through any chain, already waits for `card_id`.
/// The check and the insert are one transaction, so two edges added at the same moment cannot together make a cycle.
pub fn add_dependency(db: &mut Connection, card_id: i64, depends_on_id: i64) -> Result<bool> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let added = add_dependency_in(&tx, card_id, depends_on_id)?;
    tx.commit()?;
    Ok(added)
}

/// [`add_dependency`] inside a transaction the caller holds — for [`propose_card`], which makes the card and its edges
/// as one step.
fn add_dependency_in(tx: &Connection, card_id: i64, depends_on_id: i64) -> Result<bool> {
    if card_id == depends_on_id {
        return invalid("depends_on_id", "a card cannot wait for itself");
    }
    let (Some(card), Some(dep)) = (get_card(tx, card_id)?, get_card(tx, depends_on_id)?) else {
        return invalid("depends_on_id", "no such card");
    };
    match (card.plan_id, dep.plan_id) {
        (Some(a), Some(b)) if a == b => {}
        _ => {
            return invalid(
                "depends_on_id",
                "both cards must belong to the same plan; dependencies only exist inside a plan",
            );
        }
    }
    if card.archived_at.is_some() || dep.archived_at.is_some() {
        return invalid(
            "depends_on_id",
            "an archived card cannot take part in a dependency",
        );
    }
    let waiting_for: i64 = tx.query_row(
        "SELECT COUNT(*) FROM card_deps WHERE card_id = ?1",
        params![card_id],
        |row| row.get(0),
    )?;
    if waiting_for >= MAX_DEPS_PER_CARD {
        return invalid(
            "depends_on_id",
            format!("a card can wait for at most {MAX_DEPS_PER_CARD} others"),
        );
    }
    // Does `depends_on_id` already wait for `card_id`, through any chain? Then the new edge closes a cycle.
    let mut stack = vec![depends_on_id];
    let mut seen = std::collections::HashSet::new();
    let mut next_edges = tx.prepare("SELECT depends_on_id FROM card_deps WHERE card_id = ?1")?;
    while let Some(current) = stack.pop() {
        if current == card_id {
            return invalid(
                "depends_on_id",
                format!(
                    "card {depends_on_id} already (indirectly) waits for card {card_id}; that would be a cycle"
                ),
            );
        }
        if !seen.insert(current) {
            continue;
        }
        let next = next_edges
            .query_map(params![current], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        stack.extend(next);
    }
    drop(next_edges);
    let added = tx.execute(
        "INSERT OR IGNORE INTO card_deps (card_id, depends_on_id) VALUES (?1, ?2)",
        params![card_id, depends_on_id],
    )?;
    Ok(added == 1)
}

/// Removes an edge. `false` if there was none.
pub fn remove_dependency(db: &Connection, card_id: i64, depends_on_id: i64) -> Result<bool> {
    Ok(db.execute(
        "DELETE FROM card_deps WHERE card_id = ?1 AND depends_on_id = ?2",
        params![card_id, depends_on_id],
    )? == 1)
}

/// Every edge on a board as `(card, needs)`, for drawing the graph.
pub fn list_dependencies(db: &Connection, board_id: i64) -> Result<Vec<(i64, i64)>> {
    let mut stmt = db.prepare(
        "SELECT d.card_id, d.depends_on_id FROM card_deps d JOIN cards c ON c.id = d.card_id
         WHERE c.board_id = ?1 ORDER BY d.card_id, d.depends_on_id",
    )?;
    let rows = stmt.query_map(params![board_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// ------------------------------------------------------------- the steps ---

/// What a reviewer decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Good: the card is signed off and moves to Done.
    Approve,
    /// Not good: it goes back to In Arbeit, with the reason in its history.
    Return,
}

/// A card that failed, was called off or was taken over takes no more steps.
fn check_not_called_off(card: &Card) -> Result<()> {
    if card.failed_at.is_some() || card.canceled_at.is_some() || card.taken_over_at.is_some() {
        return invalid(
            "card_id",
            "the card has failed, was called off or has been taken over",
        );
    }
    Ok(())
}

/// An agent proposes a card: the card, its "needs first" edges and the history line that says who proposed it are one
/// transaction, so a refused edge leaves no half-made proposal in front of the owner and a retry cannot make a second
/// one. `new` must point at the board's proposal column (the caller picks it, A17). Refused after
/// [`MAX_PROPOSALS_PER_ACTOR`] proposals by the same actor.
pub fn propose_card(
    db: &mut Connection,
    new: &NewCard,
    needs: &[i64],
    actor: &str,
) -> Result<Card> {
    propose_card_from(db, new, needs, actor, None)
}

/// [`propose_card`], where `parent` is the card the proposing session is working on, if it is working one (a planner has
/// none). The proposal lies one level below it ([`MAX_PROPOSAL_DEPTH`]), and a plan takes only so many such proposals
/// ([`MAX_SELF_PROPOSED_PER_PLAN`]): a refusal says which bound was hit, and the session is a model that can read it.
pub fn propose_card_from(
    db: &mut Connection,
    new: &NewCard,
    needs: &[i64],
    actor: &str,
    parent: Option<i64>,
) -> Result<Card> {
    let actor = normalize_actor(actor)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let proposed: i64 = tx.query_row(
        "SELECT COUNT(*) FROM card_events WHERE actor = ?1 AND kind = 'note' AND text = ?2",
        params![actor, PROPOSED_NOTE],
        |row| row.get(0),
    )?;
    if proposed >= MAX_PROPOSALS_PER_ACTOR {
        return invalid(
            "actor",
            format!(
                "you have proposed {MAX_PROPOSALS_PER_ACTOR} cards already; the owner has to approve or \
                    delete some first"
            ),
        );
    }
    let depth = match parent {
        Some(parent) => Some(proposal_depth_in(&tx, parent)? + 1),
        None => None,
    };
    if let Some(depth) = depth {
        if depth > MAX_PROPOSAL_DEPTH {
            return invalid(
                "actor",
                format!(
                    "a card a session proposed may itself lead to proposals only {MAX_PROPOSAL_DEPTH} levels deep; \
                     put what you found in the card's history or a message to the owner instead"
                ),
            );
        }
        if let Some(plan_id) = new.fields.plan_id {
            let so_far: i64 = tx.query_row(
                "SELECT COUNT(*) FROM card_proposal_depth d JOIN cards c ON c.id = d.card_id WHERE c.plan_id = ?1",
                params![plan_id],
                |row| row.get(0),
            )?;
            if so_far >= MAX_SELF_PROPOSED_PER_PLAN {
                return invalid(
                    "actor",
                    format!(
                        "sessions have proposed {MAX_SELF_PROPOSED_PER_PLAN} cards in this plan already; the owner \
                         has to approve or delete some first"
                    ),
                );
            }
        }
    }
    let card = create_card(&tx, new)?;
    for need in needs {
        add_dependency_in(&tx, card.id, *need)?;
    }
    if let Some(depth) = depth {
        tx.execute(
            "INSERT INTO card_proposal_depth (card_id, depth) VALUES (?1, ?2)",
            params![card.id, depth],
        )?;
    }
    insert_event(&tx, card.id, &actor, EventKind::Note, PROPOSED_NOTE)?;
    tx.commit()?;
    // Read again: the dependencies are part of what a card shows.
    get_card(db, card.id)?.ok_or_else(|| BoardError::CorruptRow {
        table: "cards",
        id: card.id,
        reason: "vanished immediately after its proposal".to_string(),
    })
}

/// How deep card `card_id` lies in a chain of proposals by working sessions: 0 for a card nobody proposed from a card.
pub fn proposal_depth(db: &Connection, card_id: i64) -> Result<i64> {
    proposal_depth_in(db, card_id)
}

fn proposal_depth_in(db: &Connection, card_id: i64) -> Result<i64> {
    Ok(db
        .query_row(
            "SELECT depth FROM card_proposal_depth WHERE card_id = ?1",
            params![card_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}

/// The history line [`edit_own_proposal`] writes: the owner reads who changed a proposal and that it was not the owner.
const CHANGED_NOTE: &str = "changed by the session";

/// The proposal `card_id`, if it is one — in the board's proposal column, not archived — **and `actor` is the one who proposed
/// it**: a session mends its own proposals, never another's, and never a card the owner already said yes to.
fn own_proposal(tx: &Connection, card_id: i64, actor: &str) -> Result<Card> {
    let Some(card) = get_card(tx, card_id)? else {
        return invalid("card_id", format!("no card {card_id}"));
    };
    let in_proposal = list_columns(tx, card.board_id)?
        .iter()
        .any(|c| c.id == card.column_id && c.stage == Some(ColumnStage::Proposal));
    if !in_proposal || card.archived_at.is_some() {
        return invalid(
            "card_id",
            "the card is no proposal any more (the owner has approved it); it cannot be changed from here",
        );
    }
    let proposed_by_actor: i64 = tx.query_row(
        "SELECT COUNT(*) FROM card_events WHERE card_id = ?1 AND actor = ?2 AND kind = 'note' AND text = ?3",
        params![card_id, actor, PROPOSED_NOTE],
        |row| row.get(0),
    )?;
    if proposed_by_actor == 0 {
        return invalid("card_id", "you did not propose that card");
    }
    Ok(card)
}

/// A session changes a proposal **it made itself**, while it still waits for the owner's yes: the new `fields` replace the
/// old ones, `needs` (when given) replaces the card's "needs first" edges. The card, its edges and a history line
/// ("changed by the session", by the actor) are one transaction, so a refused edge leaves the proposal as it was.
pub fn edit_own_proposal(
    db: &mut Connection,
    card_id: i64,
    actor: &str,
    fields: &CardFields,
    needs: Option<&[i64]>,
) -> Result<Card> {
    let actor = normalize_actor(actor)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let card = own_proposal(&tx, card_id, &actor)?;
    // The plan and the column stay: a proposal that moved itself to another plan would be a way round the owner's reading.
    let fields = CardFields {
        plan_id: card.plan_id,
        ..fields.clone()
    };
    crate::store::update_card_in(&tx, card_id, &fields)?;
    if let Some(wanted) = needs {
        for have in &card.depends_on {
            if !wanted.contains(have) {
                remove_dependency(&tx, card_id, *have)?;
            }
        }
        for need in wanted {
            if !card.depends_on.contains(need) {
                add_dependency_in(&tx, card_id, *need)?;
            }
        }
    }
    insert_event(&tx, card_id, &actor, EventKind::Note, CHANGED_NOTE)?;
    tx.commit()?;
    get_card(db, card_id)?.ok_or_else(|| BoardError::CorruptRow {
        table: "cards",
        id: card_id,
        reason: "vanished after its change".to_string(),
    })
}

/// A session takes back a proposal **it made itself** that still waits for the owner's yes. The card is deleted, with
/// its edges and history — it was never part of anything.
pub fn withdraw_own_proposal(db: &mut Connection, card_id: i64, actor: &str) -> Result<()> {
    let actor = normalize_actor(actor)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    own_proposal(&tx, card_id, &actor)?;
    crate::store::delete_card(&tx, card_id)?;
    tx.commit()?;
    Ok(())
}

/// The ids of the cards of plan `plan_id` that a session changed after proposing them: for the planning panel to say "by the
/// planner changed", so the owner re-reads them.
pub fn changed_proposals(db: &Connection, plan_id: i64) -> Result<Vec<i64>> {
    let mut stmt = db.prepare(
        "SELECT DISTINCT e.card_id FROM card_events e JOIN cards c ON c.id = e.card_id
         WHERE c.plan_id = ?1 AND e.kind = 'note' AND e.text = ?2 ORDER BY e.card_id",
    )?;
    let ids = stmt
        .query_map(params![plan_id, CHANGED_NOTE], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

/// What [`start_card`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    /// The card was taken and moved into work.
    Started,
    /// The caller held the card in work (or in review) already — the second `claim_task` of a session whose card the
    /// app claimed for it (A20) is a confirmation, not an error.
    AlreadyHeld,
}

/// Takes a ready card and puts it into work: claim and move to the board's plain doing column in one transaction,
/// with a
/// `started` line in the history (A20).
///
/// Refused — with the reason, as [`BoardError::Invalid`] — when there is no such card, it is archived, failed, called
/// off or taken over, it sits in the proposal column (not approved), it is not in an open column, it waits for
/// predecessors, or somebody else holds it. A card the caller holds already is [`Start::AlreadyHeld`] once it is in
/// work; if it was claimed for the caller but still lies in the open column (the app claims it before the session
/// starts), this moves it.
pub fn start_card(db: &mut Connection, card_id: i64, actor: &str) -> Result<Start> {
    let actor = normalize_actor(actor)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(card) = get_card(&tx, card_id)? else {
        return invalid("card_id", format!("no card {card_id}"));
    };
    if card.archived_at.is_some() {
        return invalid("card_id", "the card is archived");
    }
    check_not_called_off(&card)?;
    let columns = list_columns(&tx, card.board_id)?;
    let Some(here) = columns.iter().find(|c| c.id == card.column_id) else {
        return invalid("card_id", "the card lies in no column of its board");
    };
    let held_by_caller = card.claimed_by.as_deref() == Some(actor.as_str());
    if held_by_caller && here.maps_to_status == CardStatus::Doing {
        return Ok(Start::AlreadyHeld);
    }
    if here.stage == Some(ColumnStage::Proposal) {
        return invalid(
            "card_id",
            "the card is a proposal; the owner has not approved it",
        );
    }
    if let Some(holder) = card.claimed_by.as_deref()
        && !held_by_caller
    {
        return invalid("card_id", format!("the card is held by {holder}"));
    }
    if here.maps_to_status != CardStatus::Open {
        return invalid("card_id", "the card is not waiting in an open column");
    }
    if let Some(first) = card.waiting_on.first() {
        return invalid("card_id", format!("the card waits for card #{first}"));
    }
    let Some(doing) = column_with(&columns, CardStatus::Doing, None) else {
        return invalid("column_id", "the board has no column for work in progress");
    };
    // An agent works on one card at a time. The check lives here, after the lock is taken, so two servers of one
    // session cannot both pass it; a card claimed for the caller beforehand (A20) is the caller's one card.
    if actor.starts_with("agent:")
        && let Some(other) = open_claims_in(&tx, &actor)?
            .into_iter()
            .find(|held| *held != card_id)
    {
        return invalid(
            "card_id",
            format!("you already hold card #{other}; finish it before you take another"),
        );
    }
    if !held_by_caller {
        let claimed = tx.execute(
            "UPDATE cards SET claimed_by = ?2, claimed_at = ?3, updated_at = ?3
             WHERE id = ?1 AND claimed_by IS NULL",
            params![card_id, actor, now()],
        )?;
        if claimed != 1 {
            return invalid(
                "card_id",
                "the card was taken by somebody else in the meantime",
            );
        }
    }
    move_card_in(&tx, card_id, doing.id, usize::MAX, None)?;
    insert_event(&tx, card_id, &actor, EventKind::Started, "")?;
    tx.commit()?;
    Ok(Start::Started)
}

/// Gives a started card back (A23 "Freigeben", and the undo of a start whose session could not be made): the claim is
/// dropped and the card goes to the top of the board's plain open column again, with a `released` line in its history.
/// Only the holder or the owner can; a card that was signed off, taken over or archived cannot be given back.
/// `false` if the card is not held by anybody.
///
/// Unlike [`crate::store::release_card`], which only clears the claim, this also moves the card — a card that stayed
/// in "In Arbeit" without a holder would look like work nobody is doing.
pub fn release_started(db: &mut Connection, card_id: i64, actor: &str) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(card) = get_card(&tx, card_id)? else {
        return invalid("card_id", format!("no card {card_id}"));
    };
    if card.claimed_by.is_none() {
        return Ok(false);
    }
    if !holder_or_human(&card, &actor) {
        return invalid(
            "card_id",
            "only the one who holds the card (or the owner) can give it back",
        );
    }
    if card.verified_by.is_some() || card.taken_over_at.is_some() || card.archived_at.is_some() {
        return invalid(
            "card_id",
            "a card that was signed off, taken over or archived cannot be given back",
        );
    }
    // A failed or called-off card keeps its claim; moving it to Open would show a dead card as waiting for work.
    check_not_called_off(&card)?;
    let columns = list_columns(&tx, card.board_id)?;
    let Some(open) = column_with(&columns, CardStatus::Open, None) else {
        return invalid("column_id", "the board has no plain open column");
    };
    tx.execute(
        "UPDATE cards SET claimed_by = NULL, claimed_at = NULL, updated_at = ?2 WHERE id = ?1",
        params![card_id, now()],
    )?;
    move_card_in(&tx, card_id, open.id, 0, None)?;
    insert_event(&tx, card_id, &actor, EventKind::Released, "")?;
    tx.commit()?;
    Ok(true)
}

/// Puts a **signed-off** card back to the top of the plain open column to be done again (CP-A8): its claim and its
/// signature are dropped and `reason` goes into its history as a `released` line. For the studio's own use when a reviewed
/// card does not fit the plan's line any more — what it did was reviewed against a base that has moved, and doing it again
/// on the line as it is now is the honest repair.
///
/// `false` for a card that is not signed off, was integrated or taken over already, or is failed or called off.
pub fn reset_for_rework(
    db: &mut Connection,
    card_id: i64,
    actor: &str,
    reason: &str,
) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    check_event_text(reason)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(card) = get_card(&tx, card_id)? else {
        return invalid("card_id", format!("no card {card_id}"));
    };
    if card.verified_by.is_none()
        || card.integrated_at.is_some()
        || card.taken_over_at.is_some()
        || card.failed_at.is_some()
        || card.canceled_at.is_some()
    {
        return Ok(false);
    }
    let columns = list_columns(&tx, card.board_id)?;
    let Some(open) = column_with(&columns, CardStatus::Open, None) else {
        return invalid("column_id", "the board has no plain open column");
    };
    tx.execute(
        "UPDATE cards SET claimed_by = NULL, claimed_at = NULL, verified_by = NULL, verified_at = NULL,
                input_required = NULL, updated_at = ?2 WHERE id = ?1",
        params![card_id, now()],
    )?;
    move_card_in(&tx, card_id, open.id, 0, None)?;
    insert_event(&tx, card_id, &actor, EventKind::Released, reason)?;
    tx.commit()?;
    Ok(true)
}

/// Counts a card's returns from zero again: the owner had it done anew, so what earlier reviewers sent back is no longer
/// the count the next escalation and the "sent back again" notice are measured against. The history keeps the lines.
/// `false` if there is no such card.
pub fn clear_returned_count(db: &Connection, card_id: i64) -> Result<bool> {
    let changed = db.execute(
        "UPDATE cards SET returned_count = 0, updated_at = ?2 WHERE id = ?1",
        params![card_id, now()],
    )?;
    Ok(changed == 1)
}

/// The cards `actor` holds that are still live work: not archived, not failed, canceled or taken over, not signed off,
/// and not lying in a done column (the owner may drag an unsigned card there, A18 — it is finished work all the same).
/// What "one card at a time" for an agent counts.
pub fn open_claims(db: &Connection, actor: &str) -> Result<Vec<i64>> {
    open_claims_in(db, &normalize_actor(actor)?)
}

/// [`open_claims`] for a canonical actor, usable inside a transaction.
fn open_claims_in(db: &Connection, actor: &str) -> Result<Vec<i64>> {
    let mut stmt = db.prepare(
        "SELECT id FROM cards WHERE claimed_by = ?1 AND archived_at IS NULL AND failed_at IS NULL
           AND canceled_at IS NULL AND taken_over_at IS NULL AND verified_by IS NULL
           AND NOT EXISTS (SELECT 1 FROM board_columns c
                           WHERE c.id = cards.column_id AND c.maps_to_status = 'done')
         ORDER BY id",
    )?;
    let ids = stmt
        .query_map(params![actor], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

/// The working agent reports the card done (A18): it moves to the review column. Refused unless the card is claimed
/// by `actor` and sits in a plain doing column.
pub fn report_done(db: &mut Connection, card_id: i64, actor: &str) -> Result<()> {
    report_done_with_note(db, card_id, actor, None)
}

/// [`report_done`] with a summary for the reviewer, written into the history in the same transaction — a summary that
/// is too long is refused *before* the card moves, so a failed call leaves everything as it was and can be retried.
pub fn report_done_with_note(
    db: &mut Connection,
    card_id: i64,
    actor: &str,
    note: Option<&str>,
) -> Result<()> {
    let actor = normalize_actor(actor)?;
    if let Some(note) = note {
        check_event_text(note)?;
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(card) = get_card(&tx, card_id)? else {
        return invalid("card_id", format!("no card {card_id}"));
    };
    if card.archived_at.is_some() {
        return invalid("card_id", "the card is archived");
    }
    check_not_called_off(&card)?;
    if card.claimed_by.as_deref() != Some(actor.as_str()) {
        return invalid(
            "card_id",
            "only the one who holds the card can report it done",
        );
    }
    let columns = list_columns(&tx, card.board_id)?;
    let working = columns.iter().any(|c| {
        c.id == card.column_id && c.maps_to_status == CardStatus::Doing && c.stage.is_none()
    });
    if !working {
        return invalid(
            "card_id",
            "the card is not in a column for work in progress",
        );
    }
    let Some(review) = column_with(&columns, CardStatus::Doing, Some(ColumnStage::Review)) else {
        return invalid("column_id", "the board has no review column");
    };
    move_card_in(&tx, card_id, review.id, usize::MAX, Some(&actor))?;
    if let Some(note) = note.filter(|n| !n.trim().is_empty()) {
        insert_event(&tx, card_id, &actor, EventKind::Note, note)?;
    }
    tx.commit()?;
    Ok(())
}

/// A reviewer's verdict on a card in the review column (A18). `note` says why when sending it back (required then)
/// and is optional when approving.
///
/// Refused when the card is not in review, or when `actor` is the one who holds it — nobody judges their own work,
/// and the schema's two-party rule would refuse the signature anyway. Approving moves the card to Done and signs it
/// off in the same transaction, so a card is never signed while still counted as in progress.
pub fn review_verdict(
    db: &mut Connection,
    card_id: i64,
    actor: &str,
    verdict: Verdict,
    note: &str,
) -> Result<()> {
    let actor = normalize_actor(actor)?;
    check_event_text(note)?;
    if verdict == Verdict::Return && note.trim().is_empty() {
        return invalid("note", "say why the card goes back");
    }
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(card) = get_card(&tx, card_id)? else {
        return invalid("card_id", format!("no card {card_id}"));
    };
    if card.archived_at.is_some() {
        return invalid("card_id", "the card is archived");
    }
    check_not_called_off(&card)?;
    let columns = list_columns(&tx, card.board_id)?;
    let in_review = columns
        .iter()
        .any(|c| c.id == card.column_id && c.stage == Some(ColumnStage::Review));
    if !in_review {
        return invalid("card_id", "the card is not in the review column");
    }
    let Some(holder) = card.claimed_by.as_deref() else {
        return invalid(
            "card_id",
            "nobody holds the card, so there is no one whose work could be judged",
        );
    };
    if holder == actor {
        return invalid("card_id", "a card cannot be judged by the one who holds it");
    }
    match verdict {
        Verdict::Approve => {
            let Some(done) = column_with(&columns, CardStatus::Done, None) else {
                return invalid("column_id", "the board has no done column");
            };
            move_card_in(&tx, card_id, done.id, usize::MAX, None)?;
            let stamp = now();
            let signed = tx.execute(
                "UPDATE cards SET verified_by = ?2, verified_at = ?3, updated_at = ?3
                 WHERE id = ?1 AND verified_by IS NULL AND claimed_by IS NOT NULL AND claimed_by <> ?2",
                params![card_id, actor, stamp],
            )?;
            if signed != 1 {
                // Dropping the transaction undoes the move too.
                return invalid("card_id", "the card could not be signed off");
            }
            insert_event(&tx, card_id, &actor, EventKind::Approved, note)?;
        }
        Verdict::Return => {
            let Some(doing) = column_with(&columns, CardStatus::Doing, None) else {
                return invalid("column_id", "the board has no column for work in progress");
            };
            move_card_in(&tx, card_id, doing.id, usize::MAX, None)?;
            tx.execute(
                "UPDATE cards SET returned_count = returned_count + 1, updated_at = ?2 WHERE id = ?1",
                params![card_id, now()],
            )?;
            insert_event(&tx, card_id, &actor, EventKind::Returned, note)?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Sets (`Some`) or clears (`None`) the question a working agent waits for an answer to (A12). Clearing a question
/// that was pending records that it was answered; clearing nothing records nothing. `false` if there is no such card.
///
/// Refused on an archived card, and for anyone but the one who holds the card (or the owner).
pub fn set_input_required(
    db: &Connection,
    card_id: i64,
    actor: &str,
    question: Option<&str>,
) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    if let Some(question) = question {
        check_len("question", question, MAX_QUESTION_LEN)?;
    }
    // The change and its history line are one step: a question without its line (or the other way round) would make
    // the history lie.
    let tx = immediate(db)?;
    let Some(card) = get_card(&tx, card_id)? else {
        return Ok(false);
    };
    if card.archived_at.is_some() {
        return invalid("card_id", "the card is archived");
    }
    if !holder_or_human(&card, &actor) {
        return invalid(
            "card_id",
            "only the one who holds the card (or the owner) can change its question",
        );
    }
    if question.is_none() && card.input_required.is_none() {
        return Ok(true);
    }
    tx.execute(
        "UPDATE cards SET input_required = ?2, updated_at = ?3 WHERE id = ?1",
        params![card_id, question, now()],
    )?;
    let kind = if question.is_some() {
        EventKind::InputRequired
    } else {
        EventKind::InputProvided
    };
    insert_event(&tx, card_id, &actor, kind, question.unwrap_or(""))?;
    tx.commit()?;
    Ok(true)
}

fn mark_terminal(
    db: &Connection,
    card_id: i64,
    actor: &str,
    column: &str,
    kind: EventKind,
    reason: &str,
) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    check_event_text(reason)?;
    let tx = immediate(db)?;
    let Some(card) = get_card(&tx, card_id)? else {
        return Ok(false);
    };
    if !holder_or_human(&card, &actor) {
        return invalid(
            "card_id",
            "only the one who holds the card (or the owner) can fail or cancel it",
        );
    }
    if card.verified_by.is_some() || card.taken_over_at.is_some() {
        return invalid("card_id", "a card that was signed off cannot fail any more");
    }
    // `column` is one of two compile-time names below, never caller input.
    let changed = tx.execute(
        &format!(
            "UPDATE cards SET {column} = ?2, updated_at = ?2 WHERE id = ?1 AND {column} IS NULL"
        ),
        params![card_id, now()],
    )?;
    if changed == 1 {
        insert_event(&tx, card_id, &actor, kind, reason)?;
    }
    tx.commit()?;
    Ok(changed == 1)
}

/// The work failed for good (the escalation is used up, or the owner gave up). Blocks cards that wait for it, visibly.
pub fn mark_failed(db: &Connection, card_id: i64, actor: &str, reason: &str) -> Result<bool> {
    mark_terminal(db, card_id, actor, "failed_at", EventKind::Failed, reason)
}

/// The card is called off. Blocks cards that wait for it, visibly.
pub fn cancel_card(db: &Connection, card_id: i64, actor: &str, reason: &str) -> Result<bool> {
    mark_terminal(
        db,
        card_id,
        actor,
        "canceled_at",
        EventKind::Canceled,
        reason,
    )
}

/// Undoes a failure or cancellation so the card can be started again. Owner only. `false` if it had neither.
pub fn reopen_card(db: &Connection, card_id: i64, actor: &str) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    require_human(&actor, "reopening a card")?;
    let tx = immediate(db)?;
    let changed = tx.execute(
        "UPDATE cards SET failed_at = NULL, canceled_at = NULL, updated_at = ?2
         WHERE id = ?1 AND (failed_at IS NOT NULL OR canceled_at IS NOT NULL)",
        params![card_id, now()],
    )?;
    if changed == 1 {
        insert_event(&tx, card_id, &actor, EventKind::Note, "reopened")?;
    }
    tx.commit()?;
    Ok(changed == 1)
}

/// The owner took the finished work over into the main line (A22): the card is marked and archived. Only a signed-off
/// card can be taken over.
pub fn mark_taken_over(db: &Connection, card_id: i64, actor: &str) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    require_human(&actor, "taking work over into the main line")?;
    let tx = immediate(db)?;
    let stamp = now();
    let changed = tx.execute(
        "UPDATE cards SET taken_over_at = ?2, archived_at = COALESCE(archived_at, ?2), updated_at = ?2
         WHERE id = ?1 AND verified_by IS NOT NULL AND taken_over_at IS NULL",
        params![card_id, stamp],
    )?;
    if changed == 1 {
        insert_event(&tx, card_id, &actor, EventKind::TakenOver, "")?;
    }
    tx.commit()?;
    Ok(changed == 1)
}

/// A reviewed card's work was merged into its plan's integration line (CP-A8). Only a signed-off card of a plan can be
/// integrated, and only once; the card stays on the board (it is archived when the plan is taken over).
///
/// Returns `false` for a card that is not signed off, has no plan, or was integrated or taken over already.
pub fn mark_integrated(db: &Connection, card_id: i64, actor: &str) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    let tx = immediate(db)?;
    let changed = tx.execute(
        "UPDATE cards SET integrated_at = ?2, updated_at = ?2
         WHERE id = ?1 AND verified_by IS NOT NULL AND plan_id IS NOT NULL
           AND integrated_at IS NULL AND taken_over_at IS NULL",
        params![card_id, now()],
    )?;
    if changed == 1 {
        insert_event(&tx, card_id, &actor, EventKind::Integrated, "")?;
    }
    tx.commit()?;
    Ok(changed == 1)
}

/// Records the branch a plan's integration line was cut from. Written once, when the line is made; `false` if the plan
/// does not exist or has one already.
pub fn set_plan_base_branch(db: &Connection, plan_id: i64, base_branch: &str) -> Result<bool> {
    check_len("base_branch", base_branch, 255)?;
    let changed = db.execute(
        "UPDATE plans SET base_branch = ?2, updated_at = ?3 WHERE id = ?1 AND base_branch IS NULL",
        params![plan_id, base_branch, now()],
    )?;
    Ok(changed == 1)
}

/// Records the commit the studio last made on a plan's line. `false` if there is no such plan.
pub fn set_line_tip(db: &Connection, plan_id: i64, tip: &str) -> Result<bool> {
    check_len("line_tip", tip, 64)?;
    let changed = db.execute(
        "UPDATE plans SET line_tip = ?2, updated_at = ?3 WHERE id = ?1",
        params![plan_id, tip, now()],
    )?;
    Ok(changed == 1)
}

/// How many lines of a card's history are of `kind`, were written by `actor` and start with `text_prefix`. Counted in the
/// database, not out of the latest few lines: a history that grows must not make an old line forgotten. The actor is
/// part of the question because a note is anyone's to write — a session could write the studio's own words.
pub fn count_events(
    db: &Connection,
    card_id: i64,
    actor: &str,
    kind: EventKind,
    text_prefix: &str,
) -> Result<usize> {
    let escaped = text_prefix
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let n: i64 = db.query_row(
        "SELECT COUNT(*) FROM card_events \
         WHERE card_id = ?1 AND actor = ?2 AND kind = ?3 AND text LIKE ?4 || '%' ESCAPE '\\'",
        params![card_id, actor, kind.as_str(), escaped],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(n).unwrap_or(0))
}

// -------------------------------------------------------- standard columns ---

/// Gives a board the two columns the agent flow needs (A13), without touching anything already there: a column named
/// "Review" that maps to doing gets the review role (so does a "Vorschlag" that maps to open the proposal role),
/// otherwise a new one is inserted — Review right after the last "doing" column, Vorschlag at the left edge. Returns
/// whether anything changed.
pub fn ensure_flow_columns(db: &mut Connection, board_id: i64) -> Result<bool> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut changed = false;
    let columns = list_columns(&tx, board_id)?;
    if columns.is_empty() {
        tx.commit()?;
        return Ok(false);
    }

    if !columns.iter().any(|c| c.stage == Some(ColumnStage::Review)) {
        let existing = columns.iter().find(|c| {
            c.stage.is_none()
                && c.maps_to_status == CardStatus::Doing
                && c.name.trim().eq_ignore_ascii_case("review")
        });
        match existing {
            Some(column) => {
                tx.execute(
                    "UPDATE board_columns SET stage = 'review' WHERE id = ?1",
                    params![column.id],
                )?;
            }
            None => {
                // Right after the last column that maps to doing; failing that, right before the first done column.
                let after = columns
                    .iter()
                    .filter(|c| c.maps_to_status == CardStatus::Doing)
                    .map(|c| c.position)
                    .fold(None, |best: Option<f64>, p| {
                        Some(best.map_or(p, |b| b.max(p)))
                    });
                let before = columns
                    .iter()
                    .filter(|c| c.maps_to_status == CardStatus::Done)
                    .map(|c| c.position)
                    .fold(None, |best: Option<f64>, p| {
                        Some(best.map_or(p, |b| b.min(p)))
                    });
                let max = columns.iter().map(|c| c.position).fold(f64::MIN, f64::max);
                let position = match (after, before) {
                    (Some(a), _) => {
                        let next = columns
                            .iter()
                            .map(|c| c.position)
                            .filter(|p| *p > a)
                            .fold(None, |best: Option<f64>, p| {
                                Some(best.map_or(p, |b| b.min(p)))
                            });
                        next.map_or(a + 1.0, |n| (a + n) / 2.0)
                    }
                    (None, Some(b)) => {
                        let prev = columns
                            .iter()
                            .map(|c| c.position)
                            .filter(|p| *p < b)
                            .fold(None, |best: Option<f64>, p| {
                                Some(best.map_or(p, |x| x.max(p)))
                            });
                        prev.map_or(b - 1.0, |p| (p + b) / 2.0)
                    }
                    (None, None) => max + 1.0,
                };
                tx.execute(
                    "INSERT INTO board_columns (board_id, name, position, maps_to_status, stage)
                     VALUES (?1, 'Review', ?2, 'doing', 'review')",
                    params![board_id, position],
                )?;
            }
        }
        changed = true;
    }

    if !columns
        .iter()
        .any(|c| c.stage == Some(ColumnStage::Proposal))
    {
        let existing = columns.iter().find(|c| {
            c.stage.is_none()
                && c.maps_to_status == CardStatus::Open
                && c.name.trim().eq_ignore_ascii_case("vorschlag")
        });
        match existing {
            Some(column) => {
                tx.execute(
                    "UPDATE board_columns SET stage = 'proposal' WHERE id = ?1",
                    params![column.id],
                )?;
            }
            None => {
                let first = columns.iter().map(|c| c.position).fold(f64::MAX, f64::min);
                tx.execute(
                    "INSERT INTO board_columns (board_id, name, position, maps_to_status, stage)
                     VALUES (?1, 'Vorschlag', ?2, 'open', 'proposal')",
                    params![board_id, first - 1.0],
                )?;
            }
        }
        changed = true;
    }
    tx.commit()?;
    Ok(changed)
}

/// [`ensure_flow_columns`] for every board. Returns how many boards changed. Idempotent; the core runs it at start.
pub fn ensure_flow_columns_all(db: &mut Connection) -> Result<usize> {
    let ids: Vec<i64> = {
        let mut stmt = db.prepare("SELECT id FROM boards ORDER BY id")?;
        stmt.query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mut changed = 0;
    for id in ids {
        if ensure_flow_columns(db, id)? {
            changed += 1;
        }
    }
    Ok(changed)
}

/// The column of a board that plays `stage`, if it has one.
pub fn stage_column(db: &Connection, board_id: i64, stage: ColumnStage) -> Result<Option<Column>> {
    Ok(list_columns(db, board_id)?
        .into_iter()
        .find(|column| column.stage == Some(stage)))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::model::{CardFields, NewCard, NewColumn, Tier};
    use crate::store::{
        claim_card, create_board, create_card, create_column, delete_column, move_card,
        move_to_status, release_card, set_card_archived, update_card, update_column, verify_card,
    };
    use crate::{
        SCHEMA_SQL_V1, SCHEMA_SQL_V2, SCHEMA_SQL_V3, SCHEMA_SQL_V4, SCHEMA_SQL_V5, SCHEMA_SQL_V6,
    };

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_db_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "axiomata-board-flow-{}-{}.db",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn open(path: &PathBuf) -> Connection {
        let conn = Connection::open(path).expect("open");
        conn.pragma_update(None, "foreign_keys", true).expect("fk");
        conn
    }

    struct Fixture {
        db: Connection,
        path: PathBuf,
        board: i64,
        proposal: i64,
        open: i64,
        doing: i64,
        review: i64,
        done: i64,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn fixture() -> Fixture {
        let path = temp_db_path();
        let mut db = open(&path);
        db.execute_batch(SCHEMA_SQL_V1).unwrap();
        db.execute_batch(SCHEMA_SQL_V2).unwrap();
        db.execute_batch(SCHEMA_SQL_V3).unwrap();
        db.execute_batch(SCHEMA_SQL_V4).unwrap();
        db.execute_batch(SCHEMA_SQL_V5).unwrap();
        db.execute_batch(SCHEMA_SQL_V6).unwrap();
        let board = create_board(&mut db, "Flow").unwrap();
        let columns = list_columns(&db, board.id).unwrap();
        let id = |name: &str| columns.iter().find(|c| c.name == name).unwrap().id;
        Fixture {
            board: board.id,
            proposal: id("Vorschlag"),
            open: id("Offen"),
            doing: id("In Arbeit"),
            review: id("Review"),
            done: id("Fertig"),
            db,
            path,
        }
    }

    fn fields(title: &str) -> CardFields {
        CardFields {
            title: title.to_string(),
            ..CardFields::default()
        }
    }

    fn card_in(f: &Fixture, column: i64, title: &str) -> Card {
        create_card(
            &f.db,
            &NewCard {
                column_id: column,
                fields: fields(title),
            },
        )
        .unwrap()
    }

    fn planned_card(f: &Fixture, column: i64, plan: i64, title: &str) -> Card {
        create_card(
            &f.db,
            &NewCard {
                column_id: column,
                fields: CardFields {
                    plan_id: Some(plan),
                    ..fields(title)
                },
            },
        )
        .unwrap()
    }

    fn plan(f: &Fixture) -> Plan {
        create_plan(
            &f.db,
            f.board,
            &PlanFields {
                project_id: None,
                goal: String::new(),
                name: "Plan".into(),
                auto_start_max: None,
                max_cost_usd: None,
                max_tokens: None,
            },
        )
        .unwrap()
    }

    fn state_of(f: &Fixture, card: i64) -> TaskState {
        get_card(&f.db, card).unwrap().unwrap().state
    }

    // ------------------------------------------------------------- start ---

    #[test]
    fn starting_a_card_claims_it_and_moves_it_into_work() {
        let mut f = fixture();
        let card = card_in(&f, f.open, "k");
        assert_eq!(
            start_card(&mut f.db, card.id, "agent:a-1").unwrap(),
            Start::Started
        );
        let after = get_card(&f.db, card.id).unwrap().unwrap();
        assert_eq!(after.claimed_by.as_deref(), Some("agent:a-1"));
        assert_eq!(after.column_id, f.doing);
        assert_eq!(after.state, TaskState::Working);
        let events = list_events(&f.db, card.id, 10).unwrap();
        assert_eq!(events.last().map(|e| e.kind), Some(EventKind::Started));

        // A second call by the holder is a confirmation; anybody else is refused.
        assert_eq!(
            start_card(&mut f.db, card.id, "agent:a-1").unwrap(),
            Start::AlreadyHeld
        );
        let err = start_card(&mut f.db, card.id, "agent:b-2").unwrap_err();
        assert!(
            matches!(
                err,
                BoardError::Invalid {
                    field: "card_id",
                    ..
                }
            ),
            "{err:?}"
        );
    }

    #[test]
    fn a_card_claimed_for_the_caller_but_still_open_is_moved() {
        let mut f = fixture();
        let card = card_in(&f, f.open, "k");
        assert!(claim_card(&f.db, card.id, "agent:a-1").unwrap());
        assert_eq!(
            start_card(&mut f.db, card.id, "agent:a-1").unwrap(),
            Start::Started
        );
        assert_eq!(
            get_card(&f.db, card.id).unwrap().unwrap().column_id,
            f.doing
        );
    }

    #[test]
    fn only_a_ready_open_card_can_be_started() {
        let mut f = fixture();
        let proposal = card_in(&f, f.proposal, "p");
        let review = card_in(&f, f.review, "r");
        let done = card_in(&f, f.done, "d");
        for id in [proposal.id, review.id, done.id, 999] {
            assert!(
                matches!(
                    start_card(&mut f.db, id, "agent:a-1"),
                    Err(BoardError::Invalid { .. })
                ),
                "card {id}"
            );
        }
        // Waiting for a predecessor.
        let plan = plan(&f);
        let first = planned_card(&f, f.open, plan.id, "first");
        let second = planned_card(&f, f.open, plan.id, "second");
        add_dependency(&mut f.db, second.id, first.id).unwrap();
        let err = start_card(&mut f.db, second.id, "agent:a-1").unwrap_err();
        assert!(err.to_string().contains(&format!("#{}", first.id)), "{err}");
        // Archived and called-off cards.
        let archived = card_in(&f, f.open, "a");
        set_card_archived(&f.db, archived.id, true).unwrap();
        assert!(start_card(&mut f.db, archived.id, "agent:a-1").is_err());
        let failed = card_in(&f, f.open, "f");
        assert!(mark_failed(&f.db, failed.id, "human:owner", "no").unwrap());
        assert!(start_card(&mut f.db, failed.id, "agent:a-1").is_err());
    }

    #[test]
    fn two_agents_racing_for_one_card_produce_one_winner() {
        let f = fixture();
        let card = card_in(&f, f.open, "k");
        let path = f.path.clone();
        let workers: Vec<_> = ["agent:a-1", "agent:b-2"]
            .into_iter()
            .map(|actor| {
                let path = path.clone();
                let id = card.id;
                std::thread::spawn(move || {
                    let mut db = open(&path);
                    db.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
                    start_card(&mut db, id, actor).is_ok()
                })
            })
            .collect();
        let winners = workers
            .into_iter()
            .map(|w| w.join().unwrap())
            .filter(|won| *won)
            .count();
        assert_eq!(winners, 1);
    }

    #[test]
    fn open_claims_lists_live_work_only() {
        let mut f = fixture();
        let a = card_in(&f, f.open, "a");
        let b = card_in(&f, f.open, "b");
        start_card(&mut f.db, a.id, "agent:a-1").unwrap();
        // A claim without a start (the CLI's `board claim`) is held work as well.
        assert!(claim_card(&f.db, b.id, "agent:a-1").unwrap());
        assert_eq!(open_claims(&f.db, "agent:a-1").unwrap(), vec![a.id, b.id]);
        assert!(mark_failed(&f.db, b.id, "agent:a-1", "x").unwrap());
        assert_eq!(open_claims(&f.db, "agent:a-1").unwrap(), vec![a.id]);
        assert!(open_claims(&f.db, "agent:b-2").unwrap().is_empty());
    }

    #[test]
    fn an_agent_starts_one_card_at_a_time() {
        let mut f = fixture();
        let a = card_in(&f, f.open, "a");
        let b = card_in(&f, f.open, "b");
        start_card(&mut f.db, a.id, "agent:w-1").unwrap();
        let err = start_card(&mut f.db, b.id, "agent:w-1").unwrap_err();
        assert!(err.to_string().contains(&format!("#{}", a.id)), "{err}");
        // Another agent and the owner are not held to it.
        start_card(&mut f.db, b.id, "agent:x-2").unwrap();
        let c = card_in(&f, f.open, "c");
        start_card(&mut f.db, c.id, "human:owner").unwrap();
        let d = card_in(&f, f.open, "d");
        start_card(&mut f.db, d.id, "human:owner").unwrap();
    }

    #[test]
    fn a_card_dragged_to_done_unsigned_is_no_longer_open_work() {
        let mut f = fixture();
        let a = card_in(&f, f.open, "a");
        start_card(&mut f.db, a.id, "agent:w-1").unwrap();
        assert_eq!(open_claims(&f.db, "agent:w-1").unwrap(), vec![a.id]);
        move_card(&mut f.db, a.id, f.done, 0).unwrap();
        assert!(open_claims(&f.db, "agent:w-1").unwrap().is_empty());
        let b = card_in(&f, f.open, "b");
        start_card(&mut f.db, b.id, "agent:w-1").unwrap();
    }

    #[test]
    fn a_summary_that_is_too_long_leaves_the_card_where_it_was() {
        let mut f = fixture();
        let a = card_in(&f, f.open, "a");
        start_card(&mut f.db, a.id, "agent:w-1").unwrap();
        let long = "x".repeat(MAX_EVENT_TEXT_LEN + 1);
        assert!(report_done_with_note(&mut f.db, a.id, "agent:w-1", Some(&long)).is_err());
        assert_eq!(
            get_card(&f.db, a.id).unwrap().unwrap().column_id,
            f.doing,
            "not moved"
        );

        report_done_with_note(&mut f.db, a.id, "agent:w-1", Some("did it")).unwrap();
        assert_eq!(get_card(&f.db, a.id).unwrap().unwrap().column_id, f.review);
        let events = list_events(&f.db, a.id, 10).unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.kind == EventKind::Note && e.text == "did it")
        );
    }

    #[test]
    fn a_proposal_is_made_whole_or_not_at_all() {
        let mut f = fixture();
        let plan = plan(&f);
        let first = planned_card(&f, f.open, plan.id, "first");
        let stranger = card_in(&f, f.open, "no plan");
        let new = |title: &str| NewCard {
            column_id: f.proposal,
            fields: CardFields {
                plan_id: Some(plan.id),
                ..fields(title)
            },
        };
        let before = crate::store::list_cards(&f.db, f.board, true)
            .unwrap()
            .len();
        // A refused edge leaves nothing behind.
        let err = propose_card(&mut f.db, &new("x"), &[stranger.id], "agent:p-1").unwrap_err();
        assert!(matches!(err, BoardError::Invalid { .. }));
        assert_eq!(
            crate::store::list_cards(&f.db, f.board, true)
                .unwrap()
                .len(),
            before
        );

        let ok = propose_card(&mut f.db, &new("y"), &[first.id], "agent:p-1").unwrap();
        assert_eq!(
            (ok.column_id, ok.depends_on.clone()),
            (f.proposal, vec![first.id])
        );
        assert!(
            list_events(&f.db, ok.id, 5)
                .unwrap()
                .iter()
                .any(|e| e.actor == "agent:p-1")
        );
    }

    #[test]
    fn an_actor_may_propose_only_so_many_cards() {
        let mut f = fixture();
        let new = |n: i64| NewCard {
            column_id: f.proposal,
            fields: fields(&format!("p{n}")),
        };
        for n in 0..MAX_PROPOSALS_PER_ACTOR {
            propose_card(&mut f.db, &new(n), &[], "agent:p-1").unwrap();
        }
        let err = propose_card(&mut f.db, &new(99), &[], "agent:p-1").unwrap_err();
        assert!(err.to_string().contains("proposed"), "{err}");
        propose_card(&mut f.db, &new(100), &[], "agent:q-2").unwrap();
    }

    #[test]
    fn a_proposal_lies_one_level_below_the_card_it_grew_out_of_and_only_so_deep() {
        let mut f = fixture();
        let new = |n: &str| NewCard {
            column_id: f.proposal,
            fields: fields(n),
        };
        let root = card_in(&f, f.proposal, "root");
        assert_eq!(proposal_depth(&f.db, root.id).unwrap(), 0);
        let one =
            propose_card_from(&mut f.db, &new("one"), &[], "agent:a-1", Some(root.id)).unwrap();
        assert_eq!(proposal_depth(&f.db, one.id).unwrap(), 1);
        let two =
            propose_card_from(&mut f.db, &new("two"), &[], "agent:b-2", Some(one.id)).unwrap();
        assert_eq!(proposal_depth(&f.db, two.id).unwrap(), 2);
        let err = propose_card_from(&mut f.db, &new("three"), &[], "agent:c-3", Some(two.id))
            .unwrap_err();
        assert!(err.to_string().contains("levels deep"), "{err}");
        // A planner proposes from no card: depth 0, no row.
        let planned =
            propose_card_from(&mut f.db, &new("planned"), &[], "agent:p-4", None).unwrap();
        assert_eq!(proposal_depth(&f.db, planned.id).unwrap(), 0);
    }

    #[test]
    fn a_plan_takes_only_so_many_proposals_from_working_sessions() {
        let mut f = fixture();
        let plan = create_plan(
            &f.db,
            f.board,
            &PlanFields {
                name: "p".into(),
                goal: String::new(),
                project_id: None,
                auto_start_max: None,
                max_cost_usd: None,
                max_tokens: None,
            },
        )
        .unwrap();
        let root = card_in(&f, f.proposal, "root");
        for n in 0..MAX_SELF_PROPOSED_PER_PLAN {
            let mut fields = fields(&format!("s{n}"));
            fields.plan_id = Some(plan.id);
            let new = NewCard {
                column_id: f.proposal,
                fields,
            };
            // Another actor each time: this is the plan's bound, not the one per session.
            propose_card_from(&mut f.db, &new, &[], &format!("agent:w-{n}"), Some(root.id))
                .unwrap();
        }
        let mut over = fields("over");
        over.plan_id = Some(plan.id);
        let err = propose_card_from(
            &mut f.db,
            &NewCard {
                column_id: f.proposal,
                fields: over.clone(),
            },
            &[],
            "agent:w-999",
            Some(root.id),
        )
        .unwrap_err();
        assert!(err.to_string().contains("in this plan"), "{err}");
        // The planner's own cards are not counted.
        propose_card_from(
            &mut f.db,
            &NewCard {
                column_id: f.proposal,
                fields: over,
            },
            &[],
            "agent:planner-5",
            None,
        )
        .unwrap();
    }

    // ------------------------------------------------------------ states ---

    #[test]
    fn a_cards_state_follows_its_column_and_signatures() {
        let mut f = fixture();
        let card = card_in(&f, f.proposal, "k");
        assert_eq!(state_of(&f, card.id), TaskState::Proposed);

        move_card(&mut f.db, card.id, f.open, 0).unwrap();
        assert_eq!(state_of(&f, card.id), TaskState::Ready);

        assert!(claim_card(&f.db, card.id, "agent:worker").unwrap());
        move_card(&mut f.db, card.id, f.doing, 0).unwrap();
        assert_eq!(state_of(&f, card.id), TaskState::Working);

        set_input_required(&f.db, card.id, "agent:worker", Some("which database?")).unwrap();
        assert_eq!(state_of(&f, card.id), TaskState::InputRequired);
        set_input_required(&f.db, card.id, "human:owner", None).unwrap();
        assert_eq!(state_of(&f, card.id), TaskState::Working);

        report_done(&mut f.db, card.id, "agent:worker").unwrap();
        assert_eq!(state_of(&f, card.id), TaskState::InReview);

        review_verdict(&mut f.db, card.id, "agent:judge", Verdict::Approve, "").unwrap();
        assert_eq!(state_of(&f, card.id), TaskState::Verified);

        assert!(mark_taken_over(&f.db, card.id, "human:owner").unwrap());
        assert_eq!(state_of(&f, card.id), TaskState::TakenOver);
    }

    #[test]
    fn a_done_card_nobody_signed_is_done_not_verified() {
        let mut f = fixture();
        let card = card_in(&f, f.open, "k");
        move_card(&mut f.db, card.id, f.done, 0).unwrap();
        assert_eq!(state_of(&f, card.id), TaskState::Done);
    }

    // ----------------------------------------------------------- columns ---

    #[test]
    fn a_proposal_cannot_be_claimed_until_it_is_approved() {
        let mut f = fixture();
        let card = card_in(&f, f.proposal, "k");
        assert!(!claim_card(&f.db, card.id, "agent:worker").unwrap());
        assert!(approve_proposal(&mut f.db, card.id, "human:owner").unwrap());
        assert!(claim_card(&f.db, card.id, "agent:worker").unwrap());
        assert!(
            !approve_proposal(&mut f.db, card.id, "human:owner").unwrap(),
            "no longer a proposal"
        );
    }

    #[test]
    fn marking_open_never_lands_in_the_proposal_column() {
        let mut f = fixture();
        let card = card_in(&f, f.done, "k");
        let target = move_to_status(&mut f.db, card.id, CardStatus::Open)
            .unwrap()
            .unwrap();
        assert_eq!(target.id, f.open);
    }

    #[test]
    fn the_flow_columns_cannot_be_deleted_and_keep_their_status() {
        let mut f = fixture();
        assert!(matches!(
            delete_column(&mut f.db, f.review, None),
            Err(BoardError::Invalid { field: "stage", .. })
        ));
        assert!(matches!(
            update_column(&mut f.db, f.review, "Review", CardStatus::Done),
            Err(BoardError::Invalid {
                field: "maps_to_status",
                ..
            })
        ));
        assert!(
            update_column(&mut f.db, f.review, "Prüfung", CardStatus::Doing)
                .unwrap()
                .is_some()
        );
        assert!(matches!(
            create_column(
                &f.db,
                f.board,
                &NewColumn {
                    name: "Wrong".into(),
                    maps_to_status: CardStatus::Done,
                    stage: Some(ColumnStage::Review),
                }
            ),
            Err(BoardError::Invalid {
                field: "maps_to_status",
                ..
            })
        ));
    }

    #[test]
    fn an_older_board_gets_a_review_and_a_proposal_column_once_and_nothing_else_changes() {
        let path = temp_db_path();
        let mut db = open(&path);
        db.execute_batch(SCHEMA_SQL_V1).unwrap();
        // A board as it existed before the flow: three plain columns and a card.
        db.execute_batch(
            "INSERT INTO boards (id, name, created_at, updated_at) VALUES (1, 'Alt', 't', 't');
             INSERT INTO board_columns (id, board_id, name, position, maps_to_status) VALUES
               (1, 1, 'Offen', 1.0, 'open'), (2, 1, 'In Arbeit', 2.0, 'doing'), (3, 1, 'Fertig', 3.0, 'done');",
        )
        .unwrap();
        db.execute_batch(SCHEMA_SQL_V2).unwrap();
        db.execute_batch(SCHEMA_SQL_V3).unwrap();
        db.execute_batch(SCHEMA_SQL_V4).unwrap();
        db.execute_batch(SCHEMA_SQL_V5).unwrap();

        assert_eq!(ensure_flow_columns_all(&mut db).unwrap(), 1);
        let names: Vec<String> = list_columns(&db, 1)
            .unwrap()
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(
            names,
            ["Vorschlag", "Offen", "In Arbeit", "Review", "Fertig"]
        );
        assert_eq!(ensure_flow_columns_all(&mut db).unwrap(), 0, "idempotent");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_column_the_owner_already_calls_review_is_adopted_not_duplicated() {
        let path = temp_db_path();
        let mut db = open(&path);
        db.execute_batch(SCHEMA_SQL_V1).unwrap();
        db.execute_batch(
            "INSERT INTO boards (id, name, created_at, updated_at) VALUES (1, 'Alt', 't', 't');
             INSERT INTO board_columns (id, board_id, name, position, maps_to_status) VALUES
               (1, 1, 'Todo', 1.0, 'open'), (2, 1, 'Doing', 2.0, 'doing'), (3, 1, 'review', 3.0, 'doing'),
               (4, 1, 'Done', 4.0, 'done');",
        )
        .unwrap();
        db.execute_batch(SCHEMA_SQL_V2).unwrap();
        db.execute_batch(SCHEMA_SQL_V3).unwrap();
        db.execute_batch(SCHEMA_SQL_V4).unwrap();
        db.execute_batch(SCHEMA_SQL_V5).unwrap();
        ensure_flow_columns(&mut db, 1).unwrap();
        let columns = list_columns(&db, 1).unwrap();
        assert_eq!(
            columns
                .iter()
                .filter(|c| c.stage == Some(ColumnStage::Review))
                .count(),
            1
        );
        assert_eq!(
            columns.iter().find(|c| c.id == 3).unwrap().stage,
            Some(ColumnStage::Review)
        );
        assert_eq!(columns.len(), 5, "only the proposal column was added");
        let _ = std::fs::remove_file(&path);
    }

    // ------------------------------------------------------------ review ---

    #[test]
    fn handing_in_an_unclaimed_card_claims_it_for_the_mover_so_somebody_else_can_judge_it() {
        let mut f = fixture();
        let card = card_in(&f, f.doing, "allein erledigt");
        crate::store::move_card_as(&mut f.db, card.id, f.review, 0, Some("human:owner")).unwrap();

        let held = get_card(&f.db, card.id).unwrap().unwrap();
        assert_eq!(held.claimed_by.as_deref(), Some("human:owner"));
        assert_eq!(held.state, TaskState::InReview);
        assert_eq!(
            list_events(&f.db, card.id, 10).unwrap()[0].kind,
            EventKind::Reported
        );

        review_verdict(&mut f.db, card.id, "agent:reviewer", Verdict::Approve, "ok").unwrap();
        assert_eq!(state_of(&f, card.id), TaskState::Verified);
    }

    #[test]
    fn a_plain_move_into_review_changes_no_claim() {
        let mut f = fixture();
        let card = card_in(&f, f.doing, "k");
        move_card(&mut f.db, card.id, f.review, 0).unwrap();
        assert_eq!(get_card(&f.db, card.id).unwrap().unwrap().claimed_by, None);
    }

    #[test]
    fn only_the_holder_reports_done_and_only_from_work_in_progress() {
        let mut f = fixture();
        let card = card_in(&f, f.open, "k");
        assert!(claim_card(&f.db, card.id, "agent:worker").unwrap());
        // Still in Offen.
        assert!(report_done(&mut f.db, card.id, "agent:worker").is_err());
        move_card(&mut f.db, card.id, f.doing, 0).unwrap();
        // Somebody else.
        assert!(report_done(&mut f.db, card.id, "agent:other").is_err());
        report_done(&mut f.db, card.id, "agent:worker").unwrap();
        // Twice: it is in review now.
        assert!(report_done(&mut f.db, card.id, "agent:worker").is_err());
        assert!(report_done(&mut f.db, 9999, "agent:worker").is_err());
    }

    #[test]
    fn nobody_judges_their_own_work_and_a_return_needs_a_reason() {
        let mut f = fixture();
        let card = card_in(&f, f.doing, "k");
        claim_card(&f.db, card.id, "agent:worker").unwrap();
        report_done(&mut f.db, card.id, "agent:worker").unwrap();

        assert!(review_verdict(&mut f.db, card.id, "agent:worker", Verdict::Approve, "").is_err());
        assert!(review_verdict(&mut f.db, card.id, "agent:judge", Verdict::Return, "  ").is_err());
        assert_eq!(
            state_of(&f, card.id),
            TaskState::InReview,
            "refusals change nothing"
        );
    }

    #[test]
    fn a_returned_card_goes_back_to_work_with_the_reason_in_its_history() {
        let mut f = fixture();
        let card = card_in(&f, f.doing, "k");
        claim_card(&f.db, card.id, "agent:worker").unwrap();
        report_done(&mut f.db, card.id, "agent:worker").unwrap();
        review_verdict(
            &mut f.db,
            card.id,
            "agent:judge",
            Verdict::Return,
            "tests fehlen",
        )
        .unwrap();

        let back = get_card(&f.db, card.id).unwrap().unwrap();
        assert_eq!(back.state, TaskState::Working);
        assert_eq!(back.column_id, f.doing);
        assert_eq!(back.returned_count, 1);
        assert_eq!(
            back.claimed_by.as_deref(),
            Some("agent:worker"),
            "the same worker continues"
        );
        let last = list_events(&f.db, card.id, 10).unwrap().pop().unwrap();
        assert_eq!(
            (last.kind, last.text.as_str(), last.actor.as_str()),
            (EventKind::Returned, "tests fehlen", "agent:judge")
        );

        report_done(&mut f.db, card.id, "agent:worker").unwrap();
        review_verdict(
            &mut f.db,
            card.id,
            "agent:judge",
            Verdict::Return,
            "noch eins",
        )
        .unwrap();
        assert_eq!(get_card(&f.db, card.id).unwrap().unwrap().returned_count, 2);
    }

    #[test]
    fn approving_signs_and_files_the_card_in_one_step_and_a_unclaimed_card_cannot_be_judged() {
        let mut f = fixture();
        let card = card_in(&f, f.doing, "k");
        claim_card(&f.db, card.id, "agent:worker").unwrap();
        report_done(&mut f.db, card.id, "agent:worker").unwrap();
        review_verdict(&mut f.db, card.id, "agent:judge", Verdict::Approve, "gut").unwrap();
        let done = get_card(&f.db, card.id).unwrap().unwrap();
        assert_eq!(
            (done.column_id, done.verified_by.as_deref()),
            (f.done, Some("agent:judge"))
        );

        // A card dragged into review by hand without any claim: nobody's work to judge.
        let orphan = card_in(&f, f.doing, "o");
        move_card(&mut f.db, orphan.id, f.review, 0).unwrap();
        assert!(review_verdict(&mut f.db, orphan.id, "agent:judge", Verdict::Approve, "").is_err());
    }

    #[test]
    fn a_second_verdict_after_the_first_is_refused_even_from_another_connection() {
        let mut f = fixture();
        let card = card_in(&f, f.doing, "k");
        claim_card(&f.db, card.id, "agent:worker").unwrap();
        report_done(&mut f.db, card.id, "agent:worker").unwrap();

        let mut other = open(&f.path);
        let first = review_verdict(&mut f.db, card.id, "agent:one", Verdict::Approve, "");
        let second = review_verdict(&mut other, card.id, "agent:two", Verdict::Approve, "");
        assert!(first.is_ok());
        assert!(second.is_err(), "the card is not in review any more");
        let signed = get_card(&f.db, card.id).unwrap().unwrap();
        assert_eq!(signed.verified_by.as_deref(), Some("agent:one"));
    }

    #[test]
    fn a_plain_verify_still_needs_the_done_column() {
        let mut f = fixture();
        let card = card_in(&f, f.doing, "k");
        claim_card(&f.db, card.id, "agent:worker").unwrap();
        move_card(&mut f.db, card.id, f.review, 0).unwrap();
        assert!(
            !verify_card(&f.db, card.id, "agent:judge").unwrap(),
            "review is not done"
        );
    }

    // ------------------------------------------------------------- plans ---

    #[test]
    fn approving_a_plan_moves_its_proposals_to_open_and_only_once() {
        let mut f = fixture();
        let plan = plan(&f);
        let a = planned_card(&f, f.proposal, plan.id, "a");
        let b = planned_card(&f, f.proposal, plan.id, "b");
        let stranger = card_in(&f, f.proposal, "not in the plan");

        assert_eq!(
            approve_plan(&mut f.db, plan.id, "human:owner").unwrap(),
            Some(2)
        );
        assert_eq!(state_of(&f, a.id), TaskState::Ready);
        assert_eq!(state_of(&f, b.id), TaskState::Ready);
        assert_eq!(state_of(&f, stranger.id), TaskState::Proposed);
        let approved = get_plan(&f.db, plan.id).unwrap().unwrap();
        assert_eq!(approved.status, PlanStatus::Approved);
        assert!(approved.approved_at.is_some());
        assert_eq!(
            approve_plan(&mut f.db, plan.id, "human:owner").unwrap(),
            None
        );
        assert_eq!(approve_plan(&mut f.db, 9999, "human:owner").unwrap(), None);
    }

    #[test]
    fn a_plan_keeps_its_project_and_records_the_branch_its_line_was_cut_from_once() {
        let f = fixture();
        let fields = PlanFields {
            project_id: Some(7),
            goal: String::new(),
            name: "p".into(),
            auto_start_max: None,
            max_cost_usd: None,
            max_tokens: None,
        };
        let made = create_plan(&f.db, f.board, &fields).unwrap();
        assert_eq!(
            (made.project_id, made.base_branch.as_deref()),
            (Some(7), None)
        );

        assert!(set_plan_base_branch(&f.db, made.id, "main").unwrap());
        assert!(
            !set_plan_base_branch(&f.db, made.id, "other").unwrap(),
            "written once"
        );
        let read = get_plan(&f.db, made.id).unwrap().unwrap();
        assert_eq!(read.base_branch.as_deref(), Some("main"));
        assert!(!set_plan_base_branch(&f.db, 9999, "main").unwrap());
        assert!(set_plan_base_branch(&f.db, made.id, &"x".repeat(300)).is_err());
    }

    #[test]
    fn a_signed_off_card_of_a_plan_is_integrated_once_and_then_says_so() {
        let mut f = fixture();
        let plan = plan(&f);
        let worker = "agent:w-1";
        let card = planned_card(&f, f.proposal, plan.id, "a");
        approve_plan(&mut f.db, plan.id, "human:owner").unwrap();
        start_card(&mut f.db, card.id, worker).unwrap();
        report_done(&mut f.db, card.id, worker).unwrap();

        // Not signed off yet: nothing to integrate.
        assert!(!mark_integrated(&f.db, card.id, "human:owner").unwrap());
        review_verdict(&mut f.db, card.id, "agent:r-2", Verdict::Approve, "").unwrap();
        assert!(mark_integrated(&f.db, card.id, "human:owner").unwrap());
        assert!(
            !mark_integrated(&f.db, card.id, "human:owner").unwrap(),
            "only once"
        );
        assert_eq!(state_of(&f, card.id), TaskState::Integrated);
        let events = list_events(&f.db, card.id, 20).unwrap();
        assert!(events.iter().any(|e| e.kind == EventKind::Integrated));

        // A card of no plan has no line to be on.
        let loose = card_in(&f, f.open, "loose");
        assert!(!mark_integrated(&f.db, loose.id, "human:owner").unwrap());
    }

    #[test]
    fn a_signed_off_card_that_does_not_fit_the_line_is_put_back_to_be_done_again() {
        let mut f = fixture();
        let plan = plan(&f);
        let card = planned_card(&f, f.proposal, plan.id, "a");
        approve_plan(&mut f.db, plan.id, "human:owner").unwrap();
        start_card(&mut f.db, card.id, "agent:w-1").unwrap();
        // Not signed off: nothing to put back.
        assert!(!reset_for_rework(&mut f.db, card.id, "agent:studio", "conflict: a.txt").unwrap());
        report_done(&mut f.db, card.id, "agent:w-1").unwrap();
        review_verdict(&mut f.db, card.id, "agent:r-2", Verdict::Approve, "").unwrap();
        assert_eq!(state_of(&f, card.id), TaskState::Verified);

        assert!(reset_for_rework(&mut f.db, card.id, "agent:studio", "conflict: a.txt").unwrap());
        let back = get_card(&f.db, card.id).unwrap().unwrap();
        assert_eq!(back.state, TaskState::Ready);
        assert!(back.claimed_by.is_none() && back.verified_by.is_none());
        let events = list_events(&f.db, card.id, 20).unwrap();
        assert!(
            events
                .iter()
                .any(|e| e.kind == EventKind::Released && e.text == "conflict: a.txt")
        );

        // An integrated card stays where it is.
        start_card(&mut f.db, card.id, "agent:w-3").unwrap();
        report_done(&mut f.db, card.id, "agent:w-3").unwrap();
        review_verdict(&mut f.db, card.id, "agent:r-4", Verdict::Approve, "").unwrap();
        assert!(mark_integrated(&f.db, card.id, "agent:studio").unwrap());
        assert!(!reset_for_rework(&mut f.db, card.id, "agent:studio", "x").unwrap());
    }

    #[test]
    fn the_line_tip_is_kept_and_events_are_counted_in_the_database_whatever_the_history_holds() {
        let f = fixture();
        let made = plan(&f);
        assert_eq!(made.line_tip, None);
        assert!(set_line_tip(&f.db, made.id, &"a".repeat(40)).unwrap());
        assert_eq!(
            get_plan(&f.db, made.id)
                .unwrap()
                .unwrap()
                .line_tip
                .as_deref(),
            Some("a".repeat(40).as_str())
        );
        assert!(!set_line_tip(&f.db, 9999, "x").unwrap());

        let card = card_in(&f, f.open, "c");
        for _ in 0..3 {
            add_event(
                &f.db,
                card.id,
                "agent:studio",
                EventKind::Released,
                "conflict: a.txt",
            )
            .unwrap();
        }
        add_event(
            &f.db,
            card.id,
            "agent:studio",
            EventKind::Released,
            "released",
        )
        .unwrap();
        // Far more than any window of recent lines.
        for _ in 0..300 {
            add_event(
                &f.db,
                card.id,
                "agent:studio",
                EventKind::Note,
                "gave up integrating: x",
            )
            .unwrap();
        }
        assert_eq!(
            count_events(
                &f.db,
                card.id,
                "agent:studio",
                EventKind::Released,
                "conflict:"
            )
            .unwrap(),
            3
        );
        assert_eq!(
            count_events(
                &f.db,
                card.id,
                "agent:studio",
                EventKind::Note,
                "gave up integrating"
            )
            .unwrap(),
            300
        );
        // The studio's words in somebody else's mouth are not the studio's.
        add_event(
            &f.db,
            card.id,
            "agent:builder-1",
            EventKind::Note,
            "gave up integrating: forged",
        )
        .unwrap();
        assert_eq!(
            count_events(
                &f.db,
                card.id,
                "agent:studio",
                EventKind::Note,
                "gave up integrating"
            )
            .unwrap(),
            300
        );
        assert_eq!(
            count_events(&f.db, card.id, "agent:studio", EventKind::Note, "50%_").unwrap(),
            0,
            "a prefix is not a pattern"
        );
    }

    #[test]
    fn the_project_of_a_plan_is_not_changed_once_it_is_approved_or_has_a_line() {
        let mut f = fixture();
        let fields = |project: Option<i64>| PlanFields {
            project_id: project,
            goal: String::new(),
            name: "p".into(),
            auto_start_max: None,
            max_cost_usd: None,
            max_tokens: None,
        };
        let made = create_plan(&f.db, f.board, &fields(None)).unwrap();
        // A draft may still be pointed at its project, and elsewhere.
        assert_eq!(
            update_plan(&f.db, made.id, &fields(Some(1)))
                .unwrap()
                .unwrap()
                .project_id,
            Some(1)
        );
        assert_eq!(
            update_plan(&f.db, made.id, &fields(Some(2)))
                .unwrap()
                .unwrap()
                .project_id,
            Some(2)
        );
        // Not once its line exists.
        set_plan_base_branch(&f.db, made.id, "main").unwrap();
        assert!(update_plan(&f.db, made.id, &fields(Some(3))).is_err());
        assert!(
            update_plan(&f.db, made.id, &fields(Some(2))).is_ok(),
            "the same project is no change"
        );

        // Nor once it is approved.
        let other = create_plan(&f.db, f.board, &fields(Some(1))).unwrap();
        planned_card(&f, f.proposal, other.id, "a");
        approve_plan(&mut f.db, other.id, "human:owner").unwrap();
        assert!(update_plan(&f.db, other.id, &fields(Some(2))).is_err());
        assert!(update_plan(&f.db, other.id, &fields(Some(1))).is_ok());
    }

    #[test]
    fn a_plans_goal_is_kept_replaced_and_bounded() {
        let f = fixture();
        let fields = |goal: &str| PlanFields {
            project_id: None,
            goal: goal.into(),
            name: "p".into(),
            auto_start_max: None,
            max_cost_usd: None,
            max_tokens: None,
        };
        let made = create_plan(&f.db, f.board, &fields("Add a dark mode")).unwrap();
        assert_eq!(made.goal, "Add a dark mode");
        let changed = update_plan(&f.db, made.id, &fields("Add a dark mode, then a light one"))
            .unwrap()
            .unwrap();
        assert_eq!(changed.goal, "Add a dark mode, then a light one");
        // A plan may be made without one.
        assert_eq!(create_plan(&f.db, f.board, &fields("")).unwrap().goal, "");
        let too_long = "x".repeat(MAX_PLAN_GOAL_LEN + 1);
        assert!(create_plan(&f.db, f.board, &fields(&too_long)).is_err());
    }

    #[test]
    fn plan_settings_are_checked_and_a_closed_plan_stays_closed() {
        let f = fixture();
        let bad = |change: fn(&mut PlanFields)| {
            let mut fields = PlanFields {
                project_id: None,
                goal: String::new(),
                name: "p".into(),
                auto_start_max: None,
                max_cost_usd: None,
                max_tokens: None,
            };
            change(&mut fields);
            create_plan(&f.db, f.board, &fields)
        };
        assert!(bad(|p| p.name = " ".into()).is_err());
        assert!(bad(|p| p.auto_start_max = Some(0)).is_err());
        assert!(bad(|p| p.max_cost_usd = Some(-1.0)).is_err());
        assert!(bad(|p| p.max_tokens = Some(0)).is_err());
        let p = plan(&f);
        assert!(close_plan(&f.db, p.id).unwrap());
        assert!(!close_plan(&f.db, p.id).unwrap());
        assert!(
            create_plan(
                &f.db,
                9999,
                &PlanFields {
                    project_id: None,
                    goal: String::new(),
                    name: "x".into(),
                    auto_start_max: None,
                    max_cost_usd: None,
                    max_tokens: None
                }
            )
            .is_err()
        );
    }

    #[test]
    fn a_card_belongs_only_to_a_plan_of_its_own_board() {
        let mut f = fixture();
        let other_board = create_board(&mut f.db, "Anders").unwrap();
        let foreign = create_plan(
            &f.db,
            other_board.id,
            &PlanFields {
                project_id: None,
                goal: String::new(),
                name: "fremd".into(),
                auto_start_max: None,
                max_cost_usd: None,
                max_tokens: None,
            },
        )
        .unwrap();
        let result = create_card(
            &f.db,
            &NewCard {
                column_id: f.open,
                fields: CardFields {
                    plan_id: Some(foreign.id),
                    ..fields("k")
                },
            },
        );
        assert!(matches!(
            result,
            Err(BoardError::Invalid {
                field: "plan_id",
                ..
            })
        ));
    }

    #[test]
    fn deleting_a_plan_keeps_its_cards_and_drops_their_edges() {
        let mut f = fixture();
        let plan = plan(&f);
        let a = planned_card(&f, f.open, plan.id, "a");
        let b = planned_card(&f, f.open, plan.id, "b");
        add_dependency(&mut f.db, b.id, a.id).unwrap();

        assert!(delete_plan(&mut f.db, plan.id).unwrap());
        let b_after = get_card(&f.db, b.id).unwrap().unwrap();
        assert_eq!((b_after.plan_id, b_after.depends_on.len()), (None, 0));
        assert!(get_card(&f.db, a.id).unwrap().is_some());
    }

    #[test]
    fn an_approved_plan_under_way_is_not_deleted_but_a_draft_a_closed_and_an_idle_one_are() {
        let mut f = fixture();
        // A draft: always.
        let draft = plan(&f);
        assert!(delete_plan(&mut f.db, draft.id).unwrap());

        // Approved, a card held by a session: refused, and nothing is touched.
        let busy = plan(&f);
        let card = planned_card(&f, f.open, busy.id, "held");
        approve_plan(&mut f.db, busy.id, "human:owner").unwrap();
        claim_card(&f.db, card.id, "agent:w-1").unwrap();
        let refused = delete_plan(&mut f.db, busy.id).unwrap_err();
        assert!(refused.to_string().contains("under way"), "{refused}");
        assert!(get_plan(&f.db, busy.id).unwrap().is_some());
        assert_eq!(
            get_card(&f.db, card.id).unwrap().unwrap().plan_id,
            Some(busy.id)
        );

        // Approved with a line: refused too; closed afterwards: allowed.
        let lined = plan(&f);
        approve_plan(&mut f.db, lined.id, "human:owner").unwrap();
        set_plan_base_branch(&f.db, lined.id, "main").unwrap();
        assert!(delete_plan(&mut f.db, lined.id).is_err());
        close_plan(&f.db, lined.id).unwrap();
        assert!(delete_plan(&mut f.db, lined.id).unwrap());

        // Approved and idle: allowed.
        let idle = plan(&f);
        approve_plan(&mut f.db, idle.id, "human:owner").unwrap();
        assert!(delete_plan(&mut f.db, idle.id).unwrap());
    }

    #[test]
    fn a_session_mends_its_own_proposal_and_nobody_elses_and_not_after_the_owners_yes() {
        let mut f = fixture();
        let plan = plan(&f);
        let mut new = |title: &str, actor: &str| {
            let mut fields = fields(title);
            fields.plan_id = Some(plan.id);
            propose_card(
                &mut f.db,
                &NewCard {
                    column_id: f.proposal,
                    fields,
                },
                &[],
                actor,
            )
            .unwrap()
        };
        let mine = new("mine", "agent:p-1");
        let other = new("other", "agent:q-2");
        let first = new("first", "agent:p-1");

        let mut changed = fields("mine, better");
        changed.acceptance = "It builds.".to_owned();
        let edited =
            edit_own_proposal(&mut f.db, mine.id, "agent:p-1", &changed, Some(&[first.id]))
                .unwrap();
        assert_eq!(
            (edited.title.as_str(), edited.acceptance.as_str()),
            ("mine, better", "It builds.")
        );
        assert_eq!(edited.depends_on, [first.id]);
        assert_eq!(edited.plan_id, Some(plan.id), "the plan cannot be changed");
        assert_eq!(changed_proposals(&f.db, plan.id).unwrap(), [mine.id]);

        // Another session's proposal is not mine to change or take back.
        assert!(edit_own_proposal(&mut f.db, other.id, "agent:p-1", &changed, None).is_err());
        assert!(withdraw_own_proposal(&mut f.db, other.id, "agent:p-1").is_err());

        // A refused edge leaves the proposal as it was: a card cannot wait for itself.
        let refused =
            edit_own_proposal(&mut f.db, mine.id, "agent:p-1", &changed, Some(&[mine.id]));
        assert!(refused.is_err());
        assert_eq!(
            get_card(&f.db, mine.id).unwrap().unwrap().depends_on,
            [first.id]
        );

        // Needs dropped when an empty list is given, kept when none is.
        let kept = edit_own_proposal(&mut f.db, mine.id, "agent:p-1", &changed, None).unwrap();
        assert_eq!(kept.depends_on, [first.id]);
        let dropped =
            edit_own_proposal(&mut f.db, mine.id, "agent:p-1", &changed, Some(&[])).unwrap();
        assert!(dropped.depends_on.is_empty());

        // After the owner's yes it is a card like any other.
        approve_proposal(&mut f.db, mine.id, "human:owner").unwrap();
        assert!(edit_own_proposal(&mut f.db, mine.id, "agent:p-1", &changed, None).is_err());
        assert!(withdraw_own_proposal(&mut f.db, mine.id, "agent:p-1").is_err());

        // Taking back an open proposal removes the card.
        withdraw_own_proposal(&mut f.db, first.id, "agent:p-1").unwrap();
        assert!(get_card(&f.db, first.id).unwrap().is_none());
    }

    // ------------------------------------------------------ dependencies ---

    #[test]
    fn a_card_waits_until_what_it_needs_is_signed_off() {
        let mut f = fixture();
        let plan = plan(&f);
        let first = planned_card(&f, f.open, plan.id, "first");
        let second = planned_card(&f, f.open, plan.id, "second");
        assert!(add_dependency(&mut f.db, second.id, first.id).unwrap());
        assert!(
            !add_dependency(&mut f.db, second.id, first.id).unwrap(),
            "already there"
        );

        let blocked = get_card(&f.db, second.id).unwrap().unwrap();
        assert_eq!(
            (
                blocked.state,
                blocked.waiting_on.clone(),
                blocked.depends_on.clone()
            ),
            (TaskState::Blocked, vec![first.id], vec![first.id])
        );

        // Done is not enough — it has to be reviewed.
        claim_card(&f.db, first.id, "agent:worker").unwrap();
        move_card(&mut f.db, first.id, f.done, 0).unwrap();
        assert_eq!(state_of(&f, second.id), TaskState::Blocked);

        verify_card(&f.db, first.id, "agent:judge").unwrap();
        let ready = get_card(&f.db, second.id).unwrap().unwrap();
        assert_eq!(
            (ready.state, ready.waiting_on, ready.depends_on),
            (TaskState::Ready, vec![], vec![first.id])
        );
    }

    #[test]
    fn a_failed_or_canceled_predecessor_keeps_its_followers_blocked_until_the_edge_is_removed() {
        let mut f = fixture();
        let plan = plan(&f);
        let first = planned_card(&f, f.open, plan.id, "first");
        let second = planned_card(&f, f.open, plan.id, "second");
        add_dependency(&mut f.db, second.id, first.id).unwrap();

        assert!(mark_failed(&f.db, first.id, "human:owner", "gescheitert").unwrap());
        assert_eq!(state_of(&f, first.id), TaskState::Failed);
        assert_eq!(
            get_card(&f.db, second.id).unwrap().unwrap().waiting_on,
            vec![first.id]
        );
        assert_eq!(state_of(&f, second.id), TaskState::Blocked);

        assert!(remove_dependency(&f.db, second.id, first.id).unwrap());
        assert_eq!(state_of(&f, second.id), TaskState::Ready);
        assert!(!remove_dependency(&f.db, second.id, first.id).unwrap());

        assert!(reopen_card(&f.db, first.id, "human:owner").unwrap());
        assert!(cancel_card(&f.db, first.id, "human:owner", "").unwrap());
        assert_eq!(state_of(&f, first.id), TaskState::Canceled);
        assert!(
            !cancel_card(&f.db, first.id, "human:owner", "").unwrap(),
            "only once"
        );
    }

    #[test]
    fn an_archived_card_that_was_signed_off_still_counts_for_what_waits_on_it() {
        let mut f = fixture();
        let plan = plan(&f);
        let first = planned_card(&f, f.open, plan.id, "first");
        let second = planned_card(&f, f.open, plan.id, "second");
        add_dependency(&mut f.db, second.id, first.id).unwrap();
        claim_card(&f.db, first.id, "agent:worker").unwrap();
        move_card(&mut f.db, first.id, f.done, 0).unwrap();
        verify_card(&f.db, first.id, "agent:judge").unwrap();
        set_card_archived(&f.db, first.id, true).unwrap();
        assert_eq!(state_of(&f, second.id), TaskState::Ready);
    }

    #[test]
    fn edges_are_refused_for_cycles_self_other_plans_and_missing_cards() {
        let mut f = fixture();
        let plan_a = plan(&f);
        let plan_b = plan(&f);
        let a = planned_card(&f, f.open, plan_a.id, "a");
        let b = planned_card(&f, f.open, plan_a.id, "b");
        let c = planned_card(&f, f.open, plan_a.id, "c");
        let elsewhere = planned_card(&f, f.open, plan_b.id, "elsewhere");
        let loose = card_in(&f, f.open, "no plan");

        assert!(add_dependency(&mut f.db, b.id, a.id).unwrap());
        assert!(add_dependency(&mut f.db, c.id, b.id).unwrap());
        for (card, needs) in [
            (a.id, a.id),
            (a.id, c.id), // a -> c -> b -> a
            (b.id, c.id),
            (a.id, elsewhere.id),
            (a.id, loose.id),
            (loose.id, a.id),
            (a.id, 9999),
        ] {
            assert!(
                matches!(
                    add_dependency(&mut f.db, card, needs),
                    Err(BoardError::Invalid { .. })
                ),
                "{card} -> {needs}"
            );
        }
        assert_eq!(
            list_dependencies(&f.db, f.board).unwrap(),
            [(b.id, a.id), (c.id, b.id)]
        );
    }

    #[test]
    fn a_card_with_edges_cannot_change_its_plan() {
        let mut f = fixture();
        let plan_a = plan(&f);
        let plan_b = plan(&f);
        let a = planned_card(&f, f.open, plan_a.id, "a");
        let b = planned_card(&f, f.open, plan_a.id, "b");
        add_dependency(&mut f.db, b.id, a.id).unwrap();
        let moved = CardFields {
            plan_id: Some(plan_b.id),
            ..fields("a")
        };
        assert!(matches!(
            update_card(&f.db, a.id, &moved),
            Err(BoardError::Invalid {
                field: "plan_id",
                ..
            })
        ));
        remove_dependency(&f.db, b.id, a.id).unwrap();
        assert!(update_card(&f.db, a.id, &moved).unwrap().is_some());
    }

    // ------------------------------------------------------- card fields ---

    #[test]
    fn the_agent_fields_round_trip_and_are_checked() {
        let f = fixture();
        let full = CardFields {
            agent: Some("implementer-light".into()),
            agent_reason: Some("klein und klar".into()),
            tier: Some(Tier::Light),
            kind: Some("implement".into()),
            acceptance: "- Tests grün\n- Doku".into(),
            ..fields("mit allem")
        };
        let card = create_card(
            &f.db,
            &NewCard {
                column_id: f.open,
                fields: full,
            },
        )
        .unwrap();
        assert_eq!(card.agent.as_deref(), Some("implementer-light"));
        assert_eq!(
            (card.tier, card.kind.as_deref(), card.acceptance.as_str()),
            (Some(Tier::Light), Some("implement"), "- Tests grün\n- Doku")
        );

        let bad = |change: fn(&mut CardFields)| {
            let mut fields = fields("x");
            change(&mut fields);
            create_card(
                &f.db,
                &NewCard {
                    column_id: f.open,
                    fields,
                },
            )
        };
        assert!(bad(|c| c.agent = Some("Not A Slug".into())).is_err());
        assert!(bad(|c| c.kind = Some("two words".into())).is_err());
        assert!(bad(|c| c.agent_reason = Some("x".repeat(501))).is_err());
        assert!(bad(|c| c.acceptance = "x".repeat(20_001)).is_err());
    }

    // ------------------------------------------------------------ events ---

    #[test]
    fn the_history_keeps_the_latest_lines_in_order_and_refuses_oversized_text() {
        let f = fixture();
        let card = card_in(&f, f.open, "k");
        for n in 0..5 {
            add_event(
                &f.db,
                card.id,
                "human:owner",
                EventKind::Note,
                &format!("eintrag {n}"),
            )
            .unwrap();
        }
        let last_three = list_events(&f.db, card.id, 3).unwrap();
        let texts: Vec<&str> = last_three.iter().map(|e| e.text.as_str()).collect();
        assert_eq!(texts, ["eintrag 2", "eintrag 3", "eintrag 4"]);
        assert!(
            add_event(&f.db, 9999, "human:owner", EventKind::Note, "x")
                .unwrap()
                .is_none()
        );
        assert!(
            add_event(
                &f.db,
                card.id,
                "human:owner",
                EventKind::Note,
                &"x".repeat(4_001)
            )
            .is_err()
        );
        assert!(
            add_event(&f.db, card.id, "nobody", EventKind::Note, "x").is_err(),
            "the actor shape is checked"
        );
    }

    #[test]
    fn deleting_a_card_takes_its_history_and_edges_with_it() {
        let mut f = fixture();
        let plan = plan(&f);
        let a = planned_card(&f, f.open, plan.id, "a");
        let b = planned_card(&f, f.open, plan.id, "b");
        add_dependency(&mut f.db, b.id, a.id).unwrap();
        add_event(&f.db, a.id, "human:owner", EventKind::Note, "x").unwrap();
        assert!(crate::store::delete_card(&f.db, a.id).unwrap());
        assert!(list_events(&f.db, a.id, 10).unwrap().is_empty());
        assert!(
            get_card(&f.db, b.id)
                .unwrap()
                .unwrap()
                .depends_on
                .is_empty()
        );
    }

    #[test]
    fn only_a_signed_card_can_be_taken_over_and_it_leaves_the_board() {
        let mut f = fixture();
        let card = card_in(&f, f.open, "k");
        assert!(
            !mark_taken_over(&f.db, card.id, "human:owner").unwrap(),
            "not signed off"
        );
        claim_card(&f.db, card.id, "agent:worker").unwrap();
        move_card(&mut f.db, card.id, f.done, 0).unwrap();
        verify_card(&f.db, card.id, "agent:judge").unwrap();
        assert!(mark_taken_over(&f.db, card.id, "human:owner").unwrap());
        assert!(
            !mark_taken_over(&f.db, card.id, "human:owner").unwrap(),
            "only once"
        );
        let taken = get_card(&f.db, card.id).unwrap().unwrap();
        assert!(taken.archived_at.is_some() && taken.taken_over_at.is_some());
        assert!(
            crate::store::list_cards(&f.db, f.board, false)
                .unwrap()
                .iter()
                .all(|c| c.id != card.id)
        );
    }

    #[test]
    fn releasing_a_card_does_not_disturb_its_flow_fields() {
        let f = fixture();
        let card = card_in(&f, f.open, "k");
        claim_card(&f.db, card.id, "agent:worker").unwrap();
        set_input_required(&f.db, card.id, "agent:worker", Some("frage")).unwrap();
        release_card(&f.db, card.id, "agent:worker").unwrap();
        assert_eq!(
            get_card(&f.db, card.id)
                .unwrap()
                .unwrap()
                .input_required
                .as_deref(),
            Some("frage")
        );
    }

    // ----------------------------------------------- who may do what (A7, A17, A18) ---

    #[test]
    fn an_agent_cannot_move_a_card_around_the_steps_but_the_owner_can() {
        let mut f = fixture();
        let card = card_in(&f, f.proposal, "k");
        assert!(matches!(
            crate::store::move_card_as(&mut f.db, card.id, f.open, 0, Some("agent:sneaky")),
            Err(BoardError::Invalid { field: "actor", .. })
        ));
        assert_eq!(
            state_of(&f, card.id),
            TaskState::Proposed,
            "the refused move changed nothing"
        );
        assert!(
            crate::store::move_card_as(&mut f.db, card.id, f.open, 0, Some("human:owner")).unwrap()
        );
    }

    #[test]
    fn the_owners_gates_refuse_an_agent_actor() {
        let mut f = fixture();
        let plan = plan(&f);
        let card = planned_card(&f, f.proposal, plan.id, "k");
        assert!(approve_proposal(&mut f.db, card.id, "agent:w").is_err());
        assert!(approve_plan(&mut f.db, plan.id, "agent:w").is_err());
        assert_eq!(state_of(&f, card.id), TaskState::Proposed);

        let signed = card_in(&f, f.open, "s");
        claim_card(&f.db, signed.id, "agent:worker").unwrap();
        move_card(&mut f.db, signed.id, f.done, 0).unwrap();
        verify_card(&f.db, signed.id, "agent:judge").unwrap();
        assert!(mark_taken_over(&f.db, signed.id, "agent:worker").is_err());
        mark_failed(&f.db, card.id, "human:owner", "").unwrap();
        assert!(reopen_card(&f.db, card.id, "agent:worker").is_err());
    }

    #[test]
    fn only_the_holder_or_the_owner_may_touch_a_cards_question_or_fail_it() {
        let f = fixture();
        let card = card_in(&f, f.doing, "k");
        claim_card(&f.db, card.id, "agent:worker").unwrap();

        assert!(set_input_required(&f.db, card.id, "agent:other", Some("frage")).is_err());
        assert!(mark_failed(&f.db, card.id, "agent:other", "").is_err());
        assert!(cancel_card(&f.db, card.id, "agent:other", "").is_err());
        assert!(set_input_required(&f.db, card.id, "agent:worker", Some("frage")).unwrap());
        assert!(set_input_required(&f.db, card.id, "human:owner", None).unwrap());
        assert!(mark_failed(&f.db, card.id, "agent:worker", "gescheitert").unwrap());
    }

    #[test]
    fn clearing_a_question_that_was_not_there_writes_no_history_and_an_archived_card_takes_none() {
        let f = fixture();
        let card = card_in(&f, f.doing, "k");
        assert!(set_input_required(&f.db, card.id, "human:owner", None).unwrap());
        assert!(list_events(&f.db, card.id, 10).unwrap().is_empty());
        assert!(!set_input_required(&f.db, 9999, "human:owner", None).unwrap());
        set_card_archived(&f.db, card.id, true).unwrap();
        assert!(set_input_required(&f.db, card.id, "human:owner", Some("x")).is_err());
    }

    #[test]
    fn a_signed_off_card_cannot_fail_and_a_called_off_card_takes_no_steps() {
        let mut f = fixture();
        let signed = card_in(&f, f.doing, "s");
        claim_card(&f.db, signed.id, "agent:worker").unwrap();
        report_done(&mut f.db, signed.id, "agent:worker").unwrap();
        review_verdict(&mut f.db, signed.id, "agent:judge", Verdict::Approve, "").unwrap();
        assert!(
            mark_failed(&f.db, signed.id, "human:owner", "").is_err(),
            "it would unblock its followers wrongly"
        );

        let called_off = card_in(&f, f.doing, "c");
        claim_card(&f.db, called_off.id, "agent:worker").unwrap();
        cancel_card(&f.db, called_off.id, "agent:worker", "").unwrap();
        assert!(report_done(&mut f.db, called_off.id, "agent:worker").is_err());
        move_card(&mut f.db, called_off.id, f.review, 0).unwrap();
        assert!(
            review_verdict(
                &mut f.db,
                called_off.id,
                "agent:judge",
                Verdict::Approve,
                ""
            )
            .is_err()
        );
    }

    #[test]
    fn the_state_precedence_is_taken_over_then_canceled_then_failed() {
        let mut f = fixture();
        let card = card_in(&f, f.open, "k");
        let column = get_column_for(&f, card.id);
        let mut c = get_card(&f.db, card.id).unwrap().unwrap();
        c.failed_at = Some(chrono::Utc::now());
        assert_eq!(derive_state(&c, &column), TaskState::Failed);
        c.canceled_at = Some(chrono::Utc::now());
        assert_eq!(derive_state(&c, &column), TaskState::Canceled);
        c.taken_over_at = Some(chrono::Utc::now());
        assert_eq!(derive_state(&c, &column), TaskState::TakenOver);
        let _ = &mut f;
    }

    fn get_column_for(f: &Fixture, card_id: i64) -> Column {
        let card = get_card(&f.db, card_id).unwrap().unwrap();
        crate::store::get_column(&f.db, card.column_id)
            .unwrap()
            .unwrap()
    }

    // ----------------------------------------------------- structure and limits ---

    #[test]
    fn a_card_that_changes_board_leaves_its_plan_and_edges_behind() {
        let mut f = fixture();
        let plan = plan(&f);
        let a = planned_card(&f, f.open, plan.id, "a");
        let b = planned_card(&f, f.open, plan.id, "b");
        add_dependency(&mut f.db, b.id, a.id).unwrap();

        let other = create_board(&mut f.db, "Anders").unwrap();
        let other_open = list_columns(&f.db, other.id)
            .unwrap()
            .into_iter()
            .find(|c| c.name == "Offen")
            .unwrap();
        move_card(&mut f.db, a.id, other_open.id, 0).unwrap();

        let moved = get_card(&f.db, a.id).unwrap().unwrap();
        assert_eq!((moved.board_id, moved.plan_id), (other.id, None));
        assert!(
            get_card(&f.db, b.id)
                .unwrap()
                .unwrap()
                .depends_on
                .is_empty()
        );
        assert!(list_dependencies(&f.db, f.board).unwrap().is_empty());
    }

    #[test]
    fn cards_cannot_be_bulk_moved_into_a_flow_column_when_a_column_is_deleted() {
        let mut f = fixture();
        let extra = create_column(
            &f.db,
            f.board,
            &NewColumn {
                name: "Extra".into(),
                maps_to_status: CardStatus::Doing,
                stage: None,
            },
        )
        .unwrap();
        card_in(&f, extra.id, "k");
        assert!(matches!(
            delete_column(&mut f.db, extra.id, Some(f.review)),
            Err(BoardError::Invalid {
                field: "move_cards_to",
                ..
            })
        ));
        assert!(delete_column(&mut f.db, extra.id, Some(f.doing)).unwrap());
    }

    #[test]
    fn an_unfinished_card_that_others_wait_for_cannot_be_archived_but_a_signed_one_can() {
        let mut f = fixture();
        let plan = plan(&f);
        let first = planned_card(&f, f.open, plan.id, "first");
        let second = planned_card(&f, f.open, plan.id, "second");
        add_dependency(&mut f.db, second.id, first.id).unwrap();
        assert!(matches!(
            set_card_archived(&f.db, first.id, true),
            Err(BoardError::Invalid {
                field: "card_id",
                ..
            })
        ));
        // Finished and signed off, it may leave the board (its followers are free).
        claim_card(&f.db, first.id, "agent:worker").unwrap();
        move_card(&mut f.db, first.id, f.done, 0).unwrap();
        verify_card(&f.db, first.id, "agent:judge").unwrap();
        assert!(set_card_archived(&f.db, first.id, true).unwrap());
        assert_eq!(state_of(&f, second.id), TaskState::Ready);
        // The follower itself can always be archived.
        assert!(set_card_archived(&f.db, second.id, true).unwrap());
    }

    #[test]
    fn a_board_has_one_column_per_role_and_a_title_is_one_line() {
        let f = fixture();
        assert!(matches!(
            create_column(
                &f.db,
                f.board,
                &NewColumn {
                    name: "Zweite".into(),
                    maps_to_status: CardStatus::Doing,
                    stage: Some(ColumnStage::Review)
                }
            ),
            Err(BoardError::Invalid { field: "stage", .. })
        ));
        // The index holds even for a writer that skips the check.
        assert!(
            f.db.execute(
                "UPDATE board_columns SET stage = 'review' WHERE id = ?1",
                params![f.doing]
            )
            .is_err()
        );
        for title in ["a\nb", "a\rb", "a\u{1b}[31mb"] {
            let result = create_card(
                &f.db,
                &NewCard {
                    column_id: f.open,
                    fields: fields(title),
                },
            );
            assert!(
                matches!(result, Err(BoardError::Invalid { field: "title", .. })),
                "{title:?}"
            );
        }
    }

    #[test]
    fn a_board_stops_taking_cards_at_the_limit() {
        let f = fixture();
        f.db.execute_batch(&format!(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000)
             INSERT INTO cards (board_id, column_id, position, title, created_at, updated_at)
             SELECT {board}, {col}, i, 'k' || i, 't', 't' FROM n",
            board = f.board,
            col = f.open
        ))
        .unwrap();
        let result = create_card(
            &f.db,
            &NewCard {
                column_id: f.open,
                fields: fields("zu viel"),
            },
        );
        assert!(matches!(
            result,
            Err(BoardError::Invalid {
                field: "column_id",
                ..
            })
        ));
    }

    #[test]
    fn deleting_a_board_takes_its_plans_edges_and_history_with_it() {
        let mut f = fixture();
        let plan = plan(&f);
        let a = planned_card(&f, f.open, plan.id, "a");
        let b = planned_card(&f, f.open, plan.id, "b");
        add_dependency(&mut f.db, b.id, a.id).unwrap();
        add_event(&f.db, a.id, "human:owner", EventKind::Note, "x").unwrap();

        assert!(crate::store::delete_board(&mut f.db, f.board).unwrap());
        for table in [
            "plans",
            "card_deps",
            "card_events",
            "cards",
            "board_columns",
        ] {
            let left: i64 =
                f.db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                    .unwrap();
            assert_eq!(left, 0, "{table}");
        }
    }

    #[test]
    fn a_dependency_limit_and_a_plan_limit_stop_runaway_growth() {
        let mut f = fixture();
        let plan = plan(&f);
        let hub = planned_card(&f, f.open, plan.id, "hub");
        for n in 0..50 {
            let dep = planned_card(&f, f.open, plan.id, &format!("d{n}"));
            add_dependency(&mut f.db, hub.id, dep.id).unwrap();
        }
        let one_more = planned_card(&f, f.open, plan.id, "one more");
        assert!(matches!(
            add_dependency(&mut f.db, hub.id, one_more.id),
            Err(BoardError::Invalid {
                field: "depends_on_id",
                ..
            })
        ));
        for n in 0..99 {
            create_plan(
                &f.db,
                f.board,
                &PlanFields {
                    project_id: None,
                    goal: String::new(),
                    name: format!("p{n}"),
                    auto_start_max: None,
                    max_cost_usd: None,
                    max_tokens: None,
                },
            )
            .unwrap();
        }
        assert!(plan_overflow(&f).is_err());
    }

    fn plan_overflow(f: &Fixture) -> Result<Plan> {
        create_plan(
            &f.db,
            f.board,
            &PlanFields {
                project_id: None,
                goal: String::new(),
                name: "zu viel".into(),
                auto_start_max: None,
                max_cost_usd: None,
                max_tokens: None,
            },
        )
    }

    #[test]
    fn the_latest_line_of_a_kind_is_found_however_long_the_history() {
        let mut f = fixture();
        let card = card_in(&f, f.open, "work");
        assert!(
            latest_event(&f.db, card.id, EventKind::Reported)
                .unwrap()
                .is_none()
        );
        start_card(&mut f.db, card.id, "agent:a-1").unwrap();
        report_done(&mut f.db, card.id, "agent:a-1").unwrap();
        let first = latest_event(&f.db, card.id, EventKind::Reported)
            .unwrap()
            .unwrap();
        // A long history of other lines between the reports does not hide the latest one.
        for n in 0..600 {
            add_event(
                &f.db,
                card.id,
                "human:owner",
                EventKind::Note,
                &format!("n{n}"),
            )
            .unwrap();
        }
        review_verdict(&mut f.db, card.id, "agent:r-2", Verdict::Return, "no").unwrap();
        report_done(&mut f.db, card.id, "agent:a-1").unwrap();
        let second = latest_event(&f.db, card.id, EventKind::Reported)
            .unwrap()
            .unwrap();
        assert!(second.id > first.id);
    }

    #[test]
    fn a_started_card_can_be_given_back_by_its_holder_or_the_owner_and_by_nobody_else() {
        let mut f = fixture();
        let card = card_in(&f, f.open, "work");
        start_card(&mut f.db, card.id, "agent:a-1").unwrap();
        assert!(release_started(&mut f.db, card.id, "agent:b-2").is_err());
        assert!(release_started(&mut f.db, card.id, "agent:a-1").unwrap());

        let back = get_card(&f.db, card.id).unwrap().unwrap();
        assert_eq!(back.claimed_by, None);
        assert_eq!(back.column_id, f.open, "it waits in the open column again");
        let kinds: Vec<_> = list_events(&f.db, card.id, 10)
            .unwrap()
            .iter()
            .map(|e| e.kind)
            .collect();
        assert!(kinds.contains(&EventKind::Released), "{kinds:?}");
        // Not held any more: nothing to give back, and it can be started again.
        assert!(!release_started(&mut f.db, card.id, "agent:a-1").unwrap());
        start_card(&mut f.db, card.id, "agent:b-2").unwrap();
        assert!(release_started(&mut f.db, card.id, "human:owner").unwrap());
    }

    #[test]
    fn a_failed_or_called_off_card_is_not_given_back_into_the_open_column() {
        let mut f = fixture();
        let card = card_in(&f, f.open, "work");
        start_card(&mut f.db, card.id, "agent:a-1").unwrap();
        mark_failed(&f.db, card.id, "agent:a-1", "gave up").unwrap();
        assert!(release_started(&mut f.db, card.id, "human:owner").is_err());
        assert_ne!(get_card(&f.db, card.id).unwrap().unwrap().column_id, f.open);
    }

    #[test]
    fn a_signed_off_card_cannot_be_given_back() {
        let mut f = fixture();
        let card = card_in(&f, f.open, "work");
        start_card(&mut f.db, card.id, "agent:a-1").unwrap();
        report_done(&mut f.db, card.id, "agent:a-1").unwrap();
        review_verdict(&mut f.db, card.id, "agent:r-2", Verdict::Approve, "").unwrap();
        assert!(release_started(&mut f.db, card.id, "human:owner").is_err());
    }
}
