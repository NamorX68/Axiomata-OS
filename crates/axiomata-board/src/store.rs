//! Every SQL statement the board core issues.
//!
//! Two conventions carried over from `axiomata_core::routines::store`, so the
//! two read the same way:
//!
//! * free functions taking `db: &Connection` — the store owns no connection,
//!   locking is the caller's business;
//! * `create` returns the stored struct, `update` returns `Option<T>` (`None`
//!   = no such id), `delete` returns a `bool` found-flag. "Missing" is never
//!   an error.
//!
//! One convention of its own, and the reason this crate exists in the shape it
//! does: **every mutation is a single statement whose precondition lives in the
//! `WHERE` clause, and success means exactly one row changed.** A Rust-side
//! check followed by an `UPDATE` would be a time-of-check/time-of-use hole the
//! moment a second agent — or the CLI in a second process — does the same
//! thing concurrently.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::model::{
    Board, Card, CardFields, CardStatus, Column, ColumnStage, EventKind, NewCard, NewColumn,
    TaskState, Tier, default_columns,
};
use crate::{BoardError, Result};

const MAX_NAME_LEN: usize = 200;
const MAX_TITLE_LEN: usize = 500;
const MAX_BODY_LEN: usize = 20_000;
const MAX_LABEL_LEN: usize = 60;
const MAX_LABELS: usize = 20;

/// Withdrawing a sign-off. Shared as a fragment because the rule — a
/// verification cannot outlive the card's status being `done` — has three
/// separate *triggers* (the card moves out, the column is re-mapped, the
/// column is deleted into a non-done one) but must be one *write*. Three
/// hand-written copies would be three chances to get the next change to the
/// rule subtly wrong.
const STRIP_VERIFICATION_SET: &str = "verified_by = NULL, verified_at = NULL";

/// Gap below which midpoint insertion stops being representable and the column
/// is renumbered instead. `f64` carries 52 mantissa bits, so repeated midpoints
/// into the *same* gap collapse after roughly fifty inserts — unreachable by
/// hand, entirely reachable by a loop of agents.
const MIN_GAP: f64 = 1e-9;

const BOARD_COLS: &str = "id, name, created_at, updated_at";
const COLUMN_COLS: &str = "id, board_id, name, position, maps_to_status, stage";
const CARD_COLS: &str = "id, board_id, column_id, position, title, body, labels, assignee, \
     claimed_by, claimed_at, verified_by, verified_at, due_at, archived_at, created_at, updated_at, \
     plan_id, agent, agent_reason, tier, kind, acceptance, returned_count, input_required, \
     taken_over_at, failed_at, canceled_at";

/// Most cards one board holds, archived ones included. A runaway agent could otherwise fill a board until every read —
/// and the Markdown mirror rewritten after every change — becomes slow. Far above anything a person keeps.
const MAX_CARDS_PER_BOARD: i64 = 2_000;
const MAX_AGENT_REASON_LEN: usize = 500;
const MAX_ACCEPTANCE_LEN: usize = 20_000;
/// Longest slug accepted for `agent` and `kind` (they name a role and a kind of work).
const MAX_SLUG_LEN: usize = 48;

/// Opens a write transaction on a connection that is only borrowed. `unchecked` because the borrow checker cannot see
/// that nothing else on this connection is mid-transaction; every caller here holds the connection for the call.
pub(crate) fn immediate(db: &Connection) -> Result<rusqlite::Transaction<'_>> {
    Ok(rusqlite::Transaction::new_unchecked(
        db,
        TransactionBehavior::Immediate,
    )?)
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn parse_ts(
    raw: &str,
    table: &'static str,
    id: i64,
    field: &str,
) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| BoardError::CorruptRow {
            table,
            id,
            reason: format!("{field} is not RFC 3339: {err}"),
        })
}

pub(crate) fn parse_opt_ts(
    raw: Option<String>,
    table: &'static str,
    id: i64,
    field: &str,
) -> Result<Option<DateTime<Utc>>> {
    raw.map(|value| parse_ts(&value, table, id, field))
        .transpose()
}

pub(crate) fn check_len(field: &'static str, value: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() {
        return Err(BoardError::Invalid {
            field,
            reason: "must not be empty".to_string(),
        });
    }
    if value.len() > max {
        return Err(BoardError::Invalid {
            field,
            reason: format!("longer than {max} bytes"),
        });
    }
    Ok(())
}

/// Canonicalises an actor string to `kind:name`, lower-cased and trimmed.
///
/// This is not cosmetic. The two-party rule compares actor strings **byte for
/// byte** — in `verify_card`'s `WHERE` clause and again in the `CHECK`
/// constraint. Without canonicalisation, claiming as `agent:one` and verifying
/// as `Agent:One` (or with a stray space) reads as two different actors to both
/// of them, and self-verification slips through the one rule the schema calls
/// load-bearing. Formatting must never decide identity.
pub(crate) fn normalize_actor(actor: &str) -> Result<String> {
    let trimmed = actor.trim().to_ascii_lowercase();
    check_len("actor", &trimmed, MAX_NAME_LEN)?;

    let invalid = |reason: &str| BoardError::Invalid {
        field: "actor",
        reason: reason.to_string(),
    };
    let Some((kind, name)) = trimmed.split_once(':') else {
        return Err(invalid("expected \"human:<name>\" or \"agent:<name>\""));
    };
    if kind != "human" && kind != "agent" {
        return Err(invalid("kind must be \"human\" or \"agent\""));
    }
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(invalid(
            "name must be non-empty and use only letters, digits, '-', '_' or '.'",
        ));
    }
    Ok(trimmed)
}

/// A lower-case slug (letters and digits with `-` or `_` between them), the same shape as a role name.
pub(crate) fn check_slug(field: &'static str, value: &str) -> Result<()> {
    let bytes = value.as_bytes();
    let edge = |b: u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let ok = !bytes.is_empty()
        && bytes.len() <= MAX_SLUG_LEN
        && edge(bytes[0])
        && edge(bytes[bytes.len() - 1])
        && bytes.iter().all(|&b| edge(b) || b == b'-' || b == b'_');
    if !ok {
        return Err(BoardError::Invalid {
            field,
            reason: "use lower-case letters and digits, with - or _ between them".to_string(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------- boards ---

struct RawBoard {
    id: i64,
    name: String,
    created_at: String,
    updated_at: String,
}

impl RawBoard {
    fn into_board(self) -> Result<Board> {
        Ok(Board {
            id: self.id,
            name: self.name,
            created_at: parse_ts(&self.created_at, "boards", self.id, "created_at")?,
            updated_at: parse_ts(&self.updated_at, "boards", self.id, "updated_at")?,
        })
    }
}

fn read_board(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawBoard> {
    Ok(RawBoard {
        id: row.get(0)?,
        name: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
    })
}

/// Creates a board together with its three default columns, in one
/// transaction — a board without columns has no place to put a card, so it is
/// never allowed to exist, not even briefly.
pub fn create_board(db: &mut Connection, name: &str) -> Result<Board> {
    check_len("name", name, MAX_NAME_LEN)?;
    let stamp = now();
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute(
        "INSERT INTO boards (name, created_at, updated_at) VALUES (?1, ?2, ?2)",
        params![name, stamp],
    )?;
    let board_id = tx.last_insert_rowid();
    for (index, column) in default_columns().into_iter().enumerate() {
        tx.execute(
            "INSERT INTO board_columns (board_id, name, position, maps_to_status, stage)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                board_id,
                column.name,
                (index + 1) as f64,
                column.maps_to_status.as_str(),
                column.stage.map(ColumnStage::as_str),
            ],
        )?;
    }
    tx.commit()?;
    get_board(db, board_id)?.ok_or_else(|| BoardError::CorruptRow {
        table: "boards",
        id: board_id,
        reason: "vanished immediately after insert".to_string(),
    })
}

pub fn get_board(db: &Connection, id: i64) -> Result<Option<Board>> {
    let sql = format!("SELECT {BOARD_COLS} FROM boards WHERE id = ?1");
    let raw = db.query_row(&sql, params![id], read_board).optional()?;
    raw.map(RawBoard::into_board).transpose()
}

pub fn list_boards(db: &Connection) -> Result<Vec<Board>> {
    let sql = format!("SELECT {BOARD_COLS} FROM boards ORDER BY name, id");
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map([], read_board)?;
    let mut out = Vec::new();
    for raw in rows {
        out.push(raw?.into_board()?);
    }
    Ok(out)
}

pub fn rename_board(db: &Connection, id: i64, name: &str) -> Result<Option<Board>> {
    check_len("name", name, MAX_NAME_LEN)?;
    let changed = db.execute(
        "UPDATE boards SET name = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, name, now()],
    )?;
    if changed == 0 {
        return Ok(None);
    }
    get_board(db, id)
}

/// Deletes a board with everything on it, in one transaction.
///
/// Cards go first by hand rather than by cascade: `cards` references its column
/// with `ON DELETE RESTRICT` precisely so that a column (or board) delete can
/// never take cards with it unnoticed. The caller is expected to have shown the
/// card count and asked first.
pub fn delete_board(db: &mut Connection, id: i64) -> Result<bool> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute("DELETE FROM cards WHERE board_id = ?1", params![id])?;
    tx.execute("DELETE FROM board_columns WHERE board_id = ?1", params![id])?;
    let changed = tx.execute("DELETE FROM boards WHERE id = ?1", params![id])?;
    tx.commit()?;
    Ok(changed == 1)
}

/// How many cards a board holds — for the confirmation prompt before
/// [`delete_board`].
pub fn count_cards(db: &Connection, board_id: i64) -> Result<i64> {
    Ok(db.query_row(
        "SELECT COUNT(*) FROM cards WHERE board_id = ?1",
        params![board_id],
        |row| row.get(0),
    )?)
}

// --------------------------------------------------------------- columns ---

struct RawColumn {
    id: i64,
    board_id: i64,
    name: String,
    position: f64,
    maps_to_status: String,
    stage: Option<String>,
}

impl RawColumn {
    fn into_column(self) -> Result<Column> {
        let status =
            CardStatus::parse(&self.maps_to_status).ok_or_else(|| BoardError::CorruptRow {
                table: "board_columns",
                id: self.id,
                reason: format!("unknown maps_to_status {:?}", self.maps_to_status),
            })?;
        let stage = self
            .stage
            .as_deref()
            .map(|raw| {
                ColumnStage::parse(raw).ok_or_else(|| BoardError::CorruptRow {
                    table: "board_columns",
                    id: self.id,
                    reason: format!("unknown stage {raw:?}"),
                })
            })
            .transpose()?;
        Ok(Column {
            id: self.id,
            board_id: self.board_id,
            name: self.name,
            position: self.position,
            maps_to_status: status,
            stage,
        })
    }
}

fn read_column(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawColumn> {
    Ok(RawColumn {
        id: row.get(0)?,
        board_id: row.get(1)?,
        name: row.get(2)?,
        position: row.get(3)?,
        maps_to_status: row.get(4)?,
        stage: row.get(5)?,
    })
}

pub fn list_columns(db: &Connection, board_id: i64) -> Result<Vec<Column>> {
    let sql = format!(
        "SELECT {COLUMN_COLS} FROM board_columns WHERE board_id = ?1 ORDER BY position, id"
    );
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map(params![board_id], read_column)?;
    let mut out = Vec::new();
    for raw in rows {
        out.push(raw?.into_column()?);
    }
    Ok(out)
}

pub fn get_column(db: &Connection, id: i64) -> Result<Option<Column>> {
    let sql = format!("SELECT {COLUMN_COLS} FROM board_columns WHERE id = ?1");
    let raw = db.query_row(&sql, params![id], read_column).optional()?;
    raw.map(RawColumn::into_column).transpose()
}

/// A column's role fixes its status: a proposal column is open, a review column is doing.
fn check_stage_pairing(stage: Option<ColumnStage>, status: CardStatus) -> Result<()> {
    if let Some(stage) = stage
        && stage.required_status() != status
    {
        return Err(BoardError::Invalid {
            field: "maps_to_status",
            reason: format!(
                "a {} column must map to {}",
                stage.as_str(),
                stage.required_status().as_str()
            ),
        });
    }
    Ok(())
}

/// Appends a column to the end of the board.
pub fn create_column(db: &Connection, board_id: i64, new: &NewColumn) -> Result<Column> {
    check_len("name", &new.name, MAX_NAME_LEN)?;
    check_stage_pairing(new.stage, new.maps_to_status)?;
    if let Some(stage) = new.stage
        && list_columns(db, board_id)?
            .iter()
            .any(|c| c.stage == Some(stage))
    {
        return Err(BoardError::Invalid {
            field: "stage",
            reason: format!("the board already has a {} column", stage.as_str()),
        });
    }
    let next: f64 = db.query_row(
        "SELECT COALESCE(MAX(position), 0.0) + 1.0 FROM board_columns WHERE board_id = ?1",
        params![board_id],
        |row| row.get(0),
    )?;
    db.execute(
        "INSERT INTO board_columns (board_id, name, position, maps_to_status, stage)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            board_id,
            new.name,
            next,
            new.maps_to_status.as_str(),
            new.stage.map(ColumnStage::as_str)
        ],
    )?;
    let id = db.last_insert_rowid();
    get_column(db, id)?.ok_or_else(|| BoardError::CorruptRow {
        table: "board_columns",
        id,
        reason: "vanished immediately after insert".to_string(),
    })
}

/// Renames a column and/or re-points it at another status.
///
/// The subtle part: a card loses its verification when it stops being done —
/// and that happens in *two* ways, not one. Dragging the card out of a done
/// column is the obvious one ([`move_card`]); re-mapping the column underneath
/// it, without touching the card at all, reaches exactly the same state. Both
/// clear the signature, in the same transaction as the change that caused it.
/// A signature on work that is no longer finished is worse than a lost one.
pub fn update_column(
    db: &mut Connection,
    id: i64,
    name: &str,
    maps_to_status: CardStatus,
) -> Result<Option<Column>> {
    check_len("name", name, MAX_NAME_LEN)?;
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // The role stays; only a status that goes with it is accepted.
    if let Some(existing) = get_column(&tx, id)? {
        check_stage_pairing(existing.stage, maps_to_status)?;
    }
    let changed = tx.execute(
        "UPDATE board_columns SET name = ?2, maps_to_status = ?3 WHERE id = ?1",
        params![id, name, maps_to_status.as_str()],
    )?;
    if changed == 0 {
        tx.commit()?;
        return Ok(None);
    }
    if maps_to_status != CardStatus::Done {
        tx.execute(
            &format!(
                "UPDATE cards SET {STRIP_VERIFICATION_SET}, updated_at = ?2
                 WHERE column_id = ?1 AND verified_by IS NOT NULL"
            ),
            params![id, now()],
        )?;
    }
    tx.commit()?;
    get_column(db, id)
}

/// Deletes a column, moving any cards it holds to `move_cards_to` first.
///
/// Returns `false` if there is no such column, or if it still holds cards and
/// no destination was given — the schema's `RESTRICT` would refuse anyway, and
/// a refusal the caller can act on beats a foreign-key error it cannot.
pub fn delete_column(db: &mut Connection, id: i64, move_cards_to: Option<i64>) -> Result<bool> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // A column with a role is part of the agent flow on every board; take the role away first if it really has to go.
    if get_column(&tx, id)?.is_some_and(|column| column.stage.is_some()) {
        return Err(BoardError::Invalid {
            field: "stage",
            reason: "this column has a role in the agent flow on every board and cannot be deleted"
                .to_string(),
        });
    }
    let held: i64 = tx.query_row(
        "SELECT COUNT(*) FROM cards WHERE column_id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    if held > 0 {
        let Some(target) = move_cards_to else {
            tx.commit()?;
            return Ok(false);
        };
        // The entry rules of the flow columns (claim on review, no work in the proposal column) live in the steps;
        // a bulk move would walk around them.
        if get_column(&tx, target)?.is_some_and(|column| column.stage.is_some()) {
            return Err(BoardError::Invalid {
                field: "move_cards_to",
                reason: "cards cannot be moved in bulk into a column with a role in the agent flow"
                    .to_string(),
            });
        }
        tx.execute(
            "UPDATE cards SET column_id = ?2, updated_at = ?3 WHERE column_id = ?1",
            params![id, target, now()],
        )?;
        // Same rule as update_column: moved out of a done column, the
        // signature goes with it.
        tx.execute(
            &format!(
                "UPDATE cards SET {STRIP_VERIFICATION_SET}, updated_at = ?2
                 WHERE column_id = ?1 AND verified_by IS NOT NULL
                   AND (SELECT maps_to_status FROM board_columns WHERE id = ?1) <> 'done'"
            ),
            params![target, now()],
        )?;
    }
    let changed = tx.execute("DELETE FROM board_columns WHERE id = ?1", params![id])?;
    tx.commit()?;
    Ok(changed == 1)
}

// ----------------------------------------------------------------- cards ---

struct RawCard {
    id: i64,
    board_id: i64,
    column_id: i64,
    position: f64,
    title: String,
    body: String,
    labels: String,
    assignee: Option<String>,
    claimed_by: Option<String>,
    claimed_at: Option<String>,
    verified_by: Option<String>,
    verified_at: Option<String>,
    due_at: Option<String>,
    archived_at: Option<String>,
    created_at: String,
    updated_at: String,
    plan_id: Option<i64>,
    agent: Option<String>,
    agent_reason: Option<String>,
    tier: Option<String>,
    kind: Option<String>,
    acceptance: String,
    returned_count: i64,
    input_required: Option<String>,
    taken_over_at: Option<String>,
    failed_at: Option<String>,
    canceled_at: Option<String>,
}

impl RawCard {
    fn into_card(self) -> Result<Card> {
        let id = self.id;
        let labels: Vec<String> =
            serde_json::from_str(&self.labels).map_err(|err| BoardError::CorruptRow {
                table: "cards",
                id,
                reason: format!("labels is not a JSON string array: {err}"),
            })?;
        let tier = self
            .tier
            .as_deref()
            .map(|raw| {
                Tier::parse(raw).ok_or_else(|| BoardError::CorruptRow {
                    table: "cards",
                    id,
                    reason: format!("unknown tier {raw:?}"),
                })
            })
            .transpose()?;
        Ok(Card {
            id,
            board_id: self.board_id,
            column_id: self.column_id,
            position: self.position,
            title: self.title,
            body: self.body,
            labels,
            assignee: self.assignee,
            claimed_by: self.claimed_by,
            claimed_at: parse_opt_ts(self.claimed_at, "cards", id, "claimed_at")?,
            verified_by: self.verified_by,
            verified_at: parse_opt_ts(self.verified_at, "cards", id, "verified_at")?,
            due_at: parse_opt_ts(self.due_at, "cards", id, "due_at")?,
            archived_at: parse_opt_ts(self.archived_at, "cards", id, "archived_at")?,
            created_at: parse_ts(&self.created_at, "cards", id, "created_at")?,
            updated_at: parse_ts(&self.updated_at, "cards", id, "updated_at")?,
            plan_id: self.plan_id,
            agent: self.agent,
            agent_reason: self.agent_reason,
            tier,
            kind: self.kind,
            acceptance: self.acceptance,
            returned_count: u32::try_from(self.returned_count).unwrap_or(0),
            input_required: self.input_required,
            taken_over_at: parse_opt_ts(self.taken_over_at, "cards", id, "taken_over_at")?,
            failed_at: parse_opt_ts(self.failed_at, "cards", id, "failed_at")?,
            canceled_at: parse_opt_ts(self.canceled_at, "cards", id, "canceled_at")?,
            // Filled in by `decorate`, which knows the columns and the edges.
            depends_on: Vec::new(),
            waiting_on: Vec::new(),
            state: TaskState::default(),
        })
    }
}

fn read_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawCard> {
    Ok(RawCard {
        id: row.get(0)?,
        board_id: row.get(1)?,
        column_id: row.get(2)?,
        position: row.get(3)?,
        title: row.get(4)?,
        body: row.get(5)?,
        labels: row.get(6)?,
        assignee: row.get(7)?,
        claimed_by: row.get(8)?,
        claimed_at: row.get(9)?,
        verified_by: row.get(10)?,
        verified_at: row.get(11)?,
        due_at: row.get(12)?,
        archived_at: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        plan_id: row.get(16)?,
        agent: row.get(17)?,
        agent_reason: row.get(18)?,
        tier: row.get(19)?,
        kind: row.get(20)?,
        acceptance: row.get(21)?,
        returned_count: row.get(22)?,
        input_required: row.get(23)?,
        taken_over_at: row.get(24)?,
        failed_at: row.get(25)?,
        canceled_at: row.get(26)?,
    })
}

/// Fills in what is computed on read: a card's edges and its [`TaskState`].
///
/// One query for the edges of the whole board and one for its columns, however many cards there are — this runs on
/// every render and for every agent that polls the board.
fn decorate(db: &Connection, mut cards: Vec<Card>) -> Result<Vec<Card>> {
    use std::collections::{BTreeSet, HashMap};

    let boards: BTreeSet<i64> = cards.iter().map(|card| card.board_id).collect();
    let mut columns: HashMap<i64, Column> = HashMap::new();
    // card id → (depends-on id, signed off?)
    let mut edges: HashMap<i64, Vec<(i64, bool)>> = HashMap::new();
    for board_id in boards {
        for column in list_columns(db, board_id)? {
            columns.insert(column.id, column);
        }
        // A board's edges for a whole listing, a single card's for `get_card`: an agent that polls one card should not
        // pay for every edge of a large board.
        let only_card = (cards.len() == 1).then(|| cards[0].id);
        let mut stmt = db.prepare(
            "SELECT d.card_id, d.depends_on_id, dep.verified_by IS NOT NULL
             FROM card_deps d
             JOIN cards c ON c.id = d.card_id
             JOIN cards dep ON dep.id = d.depends_on_id
             WHERE c.board_id = ?1 AND (?2 IS NULL OR d.card_id = ?2)
             ORDER BY d.card_id, d.depends_on_id",
        )?;
        let rows = stmt.query_map(params![board_id, only_card], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, bool>(2)?,
            ))
        })?;
        for row in rows {
            let (card_id, dep_id, signed) = row?;
            edges.entry(card_id).or_default().push((dep_id, signed));
        }
    }
    for card in &mut cards {
        if let Some(list) = edges.get(&card.id) {
            card.depends_on = list.iter().map(|(dep, _)| *dep).collect();
            card.waiting_on = list
                .iter()
                .filter(|(_, signed)| !signed)
                .map(|(dep, _)| *dep)
                .collect();
        }
        if let Some(column) = columns.get(&card.column_id) {
            card.state = crate::flow::derive_state(card, column);
        }
    }
    Ok(cards)
}

pub fn get_card(db: &Connection, id: i64) -> Result<Option<Card>> {
    let sql = format!("SELECT {CARD_COLS} FROM cards WHERE id = ?1");
    let raw = db.query_row(&sql, params![id], read_card).optional()?;
    let Some(card) = raw.map(RawCard::into_card).transpose()? else {
        return Ok(None);
    };
    Ok(decorate(db, vec![card])?.pop())
}

/// Every card of a board, ordered the way it is drawn. Archived cards are left
/// out unless asked for — a Done column that grows without bound is the
/// classic way a board stops being looked at.
pub fn list_cards(db: &Connection, board_id: i64, include_archived: bool) -> Result<Vec<Card>> {
    let filter = if include_archived {
        ""
    } else {
        "AND archived_at IS NULL"
    };
    // `position, id` rather than `position` alone: two rows can legitimately
    // share a position after a hand-edit, and the order still has to be stable.
    let sql = format!(
        "SELECT {CARD_COLS} FROM cards WHERE board_id = ?1 {filter} ORDER BY column_id, position, id"
    );
    let mut stmt = db.prepare(&sql)?;
    let rows = stmt.query_map(params![board_id], read_card)?;
    let mut out = Vec::new();
    for raw in rows {
        out.push(raw?.into_card()?);
    }
    decorate(db, out)
}

/// A line of text must stay one line: the title and the labels are written into the Markdown mirror as a single list
/// item, where a line break would forge a heading or a checklist entry in the user's vault.
fn check_one_line(field: &'static str, value: &str) -> Result<()> {
    if value.chars().any(char::is_control) {
        return Err(BoardError::Invalid {
            field,
            reason: "must be a single line without control characters".to_string(),
        });
    }
    Ok(())
}

fn validate_fields(fields: &CardFields) -> Result<()> {
    check_len("title", &fields.title, MAX_TITLE_LEN)?;
    check_one_line("title", &fields.title)?;
    if fields.body.len() > MAX_BODY_LEN {
        return Err(BoardError::Invalid {
            field: "body",
            reason: format!("longer than {MAX_BODY_LEN} bytes"),
        });
    }
    if fields.labels.len() > MAX_LABELS {
        return Err(BoardError::Invalid {
            field: "labels",
            reason: format!("more than {MAX_LABELS} labels"),
        });
    }
    for label in &fields.labels {
        check_len("label", label, MAX_LABEL_LEN)?;
        check_one_line("label", label)?;
    }
    if let Some(agent) = &fields.agent {
        check_slug("agent", agent)?;
    }
    if let Some(kind) = &fields.kind {
        check_slug("kind", kind)?;
    }
    if let Some(reason) = &fields.agent_reason
        && reason.len() > MAX_AGENT_REASON_LEN
    {
        return Err(BoardError::Invalid {
            field: "agent_reason",
            reason: format!("longer than {MAX_AGENT_REASON_LEN} bytes"),
        });
    }
    if fields.acceptance.len() > MAX_ACCEPTANCE_LEN {
        return Err(BoardError::Invalid {
            field: "acceptance",
            reason: format!("longer than {MAX_ACCEPTANCE_LEN} bytes"),
        });
    }
    Ok(())
}

/// A card may only belong to a plan of its own board.
fn check_plan_of_board(db: &Connection, plan_id: Option<i64>, board_id: i64) -> Result<()> {
    let Some(plan_id) = plan_id else {
        return Ok(());
    };
    let plan_board: Option<i64> = db
        .query_row(
            "SELECT board_id FROM plans WHERE id = ?1",
            params![plan_id],
            |row| row.get(0),
        )
        .optional()?;
    match plan_board {
        Some(found) if found == board_id => Ok(()),
        Some(_) => Err(BoardError::Invalid {
            field: "plan_id",
            reason: format!("plan {plan_id} belongs to another board"),
        }),
        None => Err(BoardError::Invalid {
            field: "plan_id",
            reason: format!("no plan {plan_id}"),
        }),
    }
}

/// The assignee is an actor like any other, so it is stored in the same
/// canonical form — otherwise a card could be assigned to an identity that
/// `claim_card` would refuse, and the two halves of "who is on this" would
/// disagree about the same person.
fn canonical_assignee(fields: &CardFields) -> Result<Option<String>> {
    fields.assignee.as_deref().map(normalize_actor).transpose()
}

fn labels_json(labels: &[String]) -> Result<String> {
    serde_json::to_string(labels).map_err(|err| BoardError::Invalid {
        field: "labels",
        reason: err.to_string(),
    })
}

pub fn create_card(db: &Connection, new: &NewCard) -> Result<Card> {
    validate_fields(&new.fields)?;
    let Some(column) = get_column(db, new.column_id)? else {
        return Err(BoardError::Invalid {
            field: "column_id",
            reason: format!("no column {}", new.column_id),
        });
    };
    check_plan_of_board(db, new.fields.plan_id, column.board_id)?;
    if count_cards(db, column.board_id)? >= MAX_CARDS_PER_BOARD {
        return Err(BoardError::Invalid {
            field: "column_id",
            reason: format!(
                "the board already holds {MAX_CARDS_PER_BOARD} cards; archive or delete some first"
            ),
        });
    }
    let position: f64 = db.query_row(
        "SELECT COALESCE(MAX(position), 0.0) + 1.0 FROM cards WHERE column_id = ?1",
        params![new.column_id],
        |row| row.get(0),
    )?;
    let stamp = now();
    db.execute(
        "INSERT INTO cards (board_id, column_id, position, title, body, labels, assignee,
                            due_at, created_at, updated_at,
                            plan_id, agent, agent_reason, tier, kind, acceptance)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        params![
            column.board_id,
            new.column_id,
            position,
            new.fields.title,
            new.fields.body,
            labels_json(&new.fields.labels)?,
            canonical_assignee(&new.fields)?,
            new.fields.due_at.map(|d| d.to_rfc3339()),
            stamp,
            new.fields.plan_id,
            new.fields.agent,
            new.fields.agent_reason,
            new.fields.tier.map(Tier::as_str),
            new.fields.kind,
            new.fields.acceptance,
        ],
    )?;
    let id = db.last_insert_rowid();
    get_card(db, id)?.ok_or_else(|| BoardError::CorruptRow {
        table: "cards",
        id,
        reason: "vanished immediately after insert".to_string(),
    })
}

/// Full replace of the writable fields — never the signatures, which only move
/// through [`claim_card`], [`release_card`] and [`verify_card`].
pub fn update_card(db: &Connection, id: i64, fields: &CardFields) -> Result<Option<Card>> {
    validate_fields(fields)?;
    // One transaction: the plan checks read edges and plans, and an edge added (or a plan deleted) between the check
    // and the write would leave a card whose plan and edges disagree.
    let tx = immediate(db)?;
    let Some(existing) = get_card(&tx, id)? else {
        return Ok(None);
    };
    check_plan_of_board(&tx, fields.plan_id, existing.board_id)?;
    if fields.plan_id != existing.plan_id {
        crate::flow::check_plan_change_keeps_edges(&tx, id)?;
    }
    let changed = tx.execute(
        "UPDATE cards SET title = ?2, body = ?3, labels = ?4, assignee = ?5, due_at = ?6,
                          updated_at = ?7, plan_id = ?8, agent = ?9, agent_reason = ?10,
                          tier = ?11, kind = ?12, acceptance = ?13
         WHERE id = ?1",
        params![
            id,
            fields.title,
            fields.body,
            labels_json(&fields.labels)?,
            canonical_assignee(fields)?,
            fields.due_at.map(|d| d.to_rfc3339()),
            now(),
            fields.plan_id,
            fields.agent,
            fields.agent_reason,
            fields.tier.map(Tier::as_str),
            fields.kind,
            fields.acceptance,
        ],
    )?;
    tx.commit()?;
    if changed == 0 {
        return Ok(None);
    }
    get_card(db, id)
}

pub fn delete_card(db: &Connection, id: i64) -> Result<bool> {
    Ok(db.execute("DELETE FROM cards WHERE id = ?1", params![id])? == 1)
}

/// Archives or un-archives. Archiving is the ordinary way a finished card
/// leaves the board; deleting is for cards that should never have existed.
pub fn set_card_archived(db: &Connection, id: i64, archived: bool) -> Result<bool> {
    let tx = immediate(db)?;
    if archived {
        // An unfinished card that other cards wait for cannot just vanish from the board: they would stay blocked on
        // something nobody sees. Finish it, or take the edge away first.
        let signed: Option<bool> = tx
            .query_row(
                "SELECT verified_by IS NOT NULL FROM cards WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if signed == Some(false) {
            let waiting: i64 = tx.query_row(
                "SELECT COUNT(*) FROM card_deps d JOIN cards c ON c.id = d.card_id
                 WHERE d.depends_on_id = ?1 AND c.archived_at IS NULL",
                params![id],
                |row| row.get(0),
            )?;
            if waiting > 0 {
                return Err(BoardError::Invalid {
                    field: "card_id",
                    reason: format!(
                        "{waiting} other card(s) wait for this one; remove their dependency before archiving it"
                    ),
                });
            }
        }
    }
    let stamp = now();
    let value = archived.then_some(stamp.clone());
    let changed = tx.execute(
        "UPDATE cards SET archived_at = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, value, stamp],
    )?;
    tx.commit()?;
    Ok(changed == 1)
}

// ----------------------------------------------------------- positioning ---

/// Which ordered list a positioning call is about.
///
/// Cards within a column and columns within a board are ordered by exactly the
/// same scheme — a `REAL` position, midpoint insertion, renumber when the gap
/// collapses. Parameterising the two helpers beats keeping a second copy of
/// that arithmetic in step with this one; the float-exhaustion guard was
/// subtle enough the first time.
///
/// Both fields are compile-time constants interpolated into SQL, never caller
/// input — the same arrangement as `CARD_COLS` above.
#[derive(Debug, Clone, Copy)]
struct Ordering {
    table: &'static str,
    parent: &'static str,
}

const CARDS_IN_COLUMN: Ordering = Ordering {
    table: "cards",
    parent: "column_id",
};
const COLUMNS_IN_BOARD: Ordering = Ordering {
    table: "board_columns",
    parent: "board_id",
};

/// Where a card should land when dropped at `index` within `column_id`, and
/// the index actually used.
///
/// `index` counts the cards the moved one would sit **among**, which is why
/// `moving` is excluded from the neighbour list. Counting it would make a move
/// down within its own column land one place short of where the drop indicator
/// promised: the card still occupying slot `index` is the card being dragged.
/// Excluding it also makes the index mean the same thing in both directions
/// and across columns, which is what a caller can reason about.
///
/// An `index` past the end means "append" rather than being an error — that is
/// what a drop below the last card means, and what the CLI's absent `--index`
/// means. Returns the position, the clamped index, and whether the column has
/// to be renumbered first because the gap at that spot is no longer
/// representable (see [`MIN_GAP`]).
fn position_for(
    db: &Connection,
    ord: Ordering,
    parent_id: i64,
    index: usize,
    moving: i64,
) -> Result<(f64, usize, bool)> {
    let sql = format!(
        "SELECT position FROM {} WHERE {} = ?1 AND id <> ?2 ORDER BY position, id",
        ord.table, ord.parent
    );
    let mut stmt = db.prepare(&sql)?;
    let positions: Vec<f64> = stmt
        .query_map(params![parent_id, moving], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;

    let index = index.min(positions.len());
    let before = index.checked_sub(1).and_then(|i| positions.get(i)).copied();
    let after = positions.get(index).copied();

    match (before, after) {
        (None, None) => Ok((1.0, index, false)),
        (None, Some(first)) => Ok((first - 1.0, index, false)),
        (Some(last), None) => Ok((last + 1.0, index, false)),
        (Some(lo), Some(hi)) => {
            let mid = (lo + hi) / 2.0;
            let collapsed = hi - lo < MIN_GAP || mid <= lo || mid >= hi;
            Ok((mid, index, collapsed))
        }
    }
}

/// Renumbers a column to `1.0, 2.0, 3.0…`, restoring room between every pair.
///
/// `exclude` leaves one card out — the one being moved. It is about to be
/// given a position of its own, and counting it here would shift every slot
/// after it by one, so the index that `position_for` resolved (which also
/// excludes it) would no longer point where it meant to.
fn renumber(
    tx: &rusqlite::Transaction<'_>,
    ord: Ordering,
    parent_id: i64,
    exclude: i64,
) -> Result<()> {
    let ids: Vec<i64> = {
        let sql = format!(
            "SELECT id FROM {} WHERE {} = ?1 AND id <> ?2 ORDER BY position, id",
            ord.table, ord.parent
        );
        let mut stmt = tx.prepare(&sql)?;
        let rows = stmt.query_map(params![parent_id, exclude], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    // Prepared once rather than per row: the statement text never varies, and
    // renumbering is the one place here that issues a write per row.
    let sql = format!("UPDATE {} SET position = ?2 WHERE id = ?1", ord.table);
    let mut stmt = tx.prepare(&sql)?;
    for (index, id) in ids.iter().enumerate() {
        stmt.execute(params![id, (index + 1) as f64])?;
    }
    Ok(())
}

/// Moves a card to `index` within `column_id`.
///
/// Also clears the verification if the destination is not a done column — the
/// same rule [`update_column`] applies from the other direction.
pub fn move_card(db: &mut Connection, id: i64, column_id: i64, index: usize) -> Result<bool> {
    move_card_as(db, id, column_id, index, None)
}

/// [`move_card`] on behalf of an actor (A18). Two things only an actor makes possible, both when the card **enters a
/// review column**: an unclaimed card is claimed for the mover ("whoever hands it in has worked on it"), which is what
/// lets somebody else sign it off without loosening the schema's two-party rule, and the move is written into the
/// card's history. Entering review also clears a pending question. Without an actor nothing of this happens.
pub fn move_card_as(
    db: &mut Connection,
    id: i64,
    column_id: i64,
    index: usize,
    actor: Option<&str>,
) -> Result<bool> {
    let actor = actor.map(normalize_actor).transpose()?;
    // An agent moves a card only through its steps (`claim`, `report_done`, `review_verdict`), which check who may do
    // what (a2a.md A18). A free move would be a way around all of them: out of the proposal column without the
    // owner's yes, straight into Done without a review.
    if actor.as_deref().is_some_and(|a| a.starts_with("agent:")) {
        return Err(BoardError::Invalid {
            field: "actor",
            reason: "an agent moves a card only by reporting it done or by judging it".to_string(),
        });
    }
    // The transaction opens *first*, before the target column and the
    // neighbouring positions are read. Reading them beforehand — as this
    // function originally did — is the time-of-check/time-of-use hole this
    // module's own rules forbid: two callers moving into the same column would
    // each compute the same midpoint from the same stale snapshot and write it,
    // leaving two cards sharing a position. It also turns a column deleted in
    // the meantime into a clean refusal instead of a raw foreign-key error.
    // The reads are two indexed lookups, so holding the write lock across them
    // costs microseconds.
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let moved = move_card_in(&tx, id, column_id, index, actor.as_deref())?;
    tx.commit()?;
    Ok(moved)
}

/// [`move_card_as`] inside a transaction the caller already holds — for the flow functions that check their
/// preconditions and move the card as one atomic step. `actor` must already be canonical.
pub(crate) fn move_card_in(
    tx: &rusqlite::Transaction<'_>,
    id: i64,
    column_id: i64,
    index: usize,
    actor: Option<&str>,
) -> Result<bool> {
    let Some(target) = get_column(tx, column_id)? else {
        return Err(BoardError::Invalid {
            field: "column_id",
            reason: format!("no column {column_id}"),
        });
    };
    let (mut position, index, collapsed) = position_for(tx, CARDS_IN_COLUMN, column_id, index, id)?;
    if collapsed {
        renumber(tx, CARDS_IN_COLUMN, column_id, id)?;
        // Against the fresh 1.0/2.0/3.0 spacing, slot `index` sits at
        // `index + 0.5`: before the first card for 0, between neighbours
        // otherwise, past the last one at the end.
        position = index as f64 + 0.5;
    }
    let stamp = now();
    let previous_stage: Option<Option<String>> = tx
        .query_row(
            "SELECT c.stage FROM cards k JOIN board_columns c ON c.id = k.column_id WHERE k.id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    // A card that changes board leaves its plan and its edges behind: both only mean something inside one board.
    let old_board: Option<i64> = tx
        .query_row(
            "SELECT board_id FROM cards WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    if old_board.is_some_and(|board| board != target.board_id) {
        tx.execute(
            "DELETE FROM card_deps WHERE card_id = ?1 OR depends_on_id = ?1",
            params![id],
        )?;
        tx.execute("UPDATE cards SET plan_id = NULL WHERE id = ?1", params![id])?;
    }
    let changed = tx.execute(
        "UPDATE cards SET column_id = ?2, board_id = ?3, position = ?4, updated_at = ?5
         WHERE id = ?1",
        params![id, column_id, target.board_id, position, stamp],
    )?;
    if changed == 1 && target.maps_to_status != CardStatus::Done {
        tx.execute(
            &format!("UPDATE cards SET {STRIP_VERIFICATION_SET} WHERE id = ?1"),
            params![id],
        )?;
    }
    let entered_review = target.stage == Some(ColumnStage::Review)
        && previous_stage.is_some_and(|stage| stage.as_deref() != Some("review"));
    if changed == 1 && entered_review {
        tx.execute(
            "UPDATE cards SET input_required = NULL WHERE id = ?1",
            params![id],
        )?;
        if let Some(actor) = actor {
            tx.execute(
                "UPDATE cards SET claimed_by = ?2, claimed_at = ?3
                 WHERE id = ?1 AND claimed_by IS NULL AND archived_at IS NULL",
                params![id, actor, stamp],
            )?;
            crate::flow::insert_event(tx, id, actor, EventKind::Reported, "")?;
        }
    }
    Ok(changed == 1)
}

/// Moves a column to `index` within its board.
///
/// Same index convention as [`move_card`]: it counts the columns the moved one
/// will sit **among**, excluding itself. Returns `false` if there is no such
/// column.
pub fn move_column(db: &mut Connection, id: i64, index: usize) -> Result<bool> {
    let Some(column) = get_column(db, id)? else {
        return Ok(false);
    };
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (mut position, index, collapsed) =
        position_for(&tx, COLUMNS_IN_BOARD, column.board_id, index, id)?;
    if collapsed {
        renumber(&tx, COLUMNS_IN_BOARD, column.board_id, id)?;
        position = index as f64 + 0.5;
    }
    let changed = tx.execute(
        "UPDATE board_columns SET position = ?2 WHERE id = ?1",
        params![id, position],
    )?;
    tx.commit()?;
    Ok(changed == 1)
}

/// Moves a card to the end of its board's first column mapped to `status`.
///
/// "What does marking a card done mean" is board logic, not caller logic. It
/// lives here so the CLI and the dashboard cannot answer it differently — a
/// board may legitimately have several done columns, and two callers each
/// picking their own would be a difference nobody would notice until it
/// mattered. Returns `Ok(None)` if the board has no column for that status.
pub fn move_to_status(db: &mut Connection, id: i64, status: CardStatus) -> Result<Option<Column>> {
    let Some(card) = get_card(db, id)? else {
        return Ok(None);
    };
    // A column with a role (proposal, review) is never "the" column of a status: marking something open must not
    // put it into the proposal column.
    let Some(target) = list_columns(db, card.board_id)?
        .into_iter()
        .find(|column| column.maps_to_status == status && column.stage.is_none())
    else {
        return Ok(None);
    };
    move_card(db, id, target.id, usize::MAX)?;
    Ok(Some(target))
}

// ------------------------------------------------------------ signatures ---

/// Takes the card, if nobody holds it.
///
/// The compare-and-swap that the whole concurrency story rests on: the
/// precondition `claimed_by IS NULL` sits in the statement, so two actors
/// racing for the same card produce exactly one winner and one `false`. Losing
/// is an ordinary outcome, not an error.
pub fn claim_card(db: &Connection, id: i64, actor: &str) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    let stamp = now();
    // A card in a proposal column has not been approved yet and cannot be taken. `archived_at IS NULL`: an
    // archived card is not live work. Without this an
    // agent enumerating cards with a query that forgot the archive filter could
    // claim something nobody meant to put back in play.
    Ok(db.execute(
        "UPDATE cards SET claimed_by = ?2, claimed_at = ?3, updated_at = ?3
         WHERE id = ?1 AND claimed_by IS NULL AND archived_at IS NULL
           AND NOT EXISTS (SELECT 1 FROM board_columns c
                           WHERE c.id = cards.column_id AND c.stage = 'proposal')",
        params![id, actor, stamp],
    )? == 1)
}

/// Gives the card back — only the holder can.
pub fn release_card(db: &Connection, id: i64, actor: &str) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    let stamp = now();
    let sql = format!(
        "UPDATE cards SET claimed_by = NULL, claimed_at = NULL, {STRIP_VERIFICATION_SET},
                          updated_at = ?3
         WHERE id = ?1 AND claimed_by = ?2"
    );
    Ok(db.execute(&sql, params![id, actor, stamp])? == 1)
}

/// Signs a finished card off.
///
/// Every gate is in the statement, including the two-party rule: the card must
/// be unverified, actually claimed, claimed by **somebody else**, and sitting
/// in a done column. The database's own `CHECK` enforces the two-party rule a
/// second time as a last line of defence — this `WHERE` clause is what turns it
/// into a clean `false` instead of a constraint violation.
pub fn verify_card(db: &Connection, id: i64, actor: &str) -> Result<bool> {
    let actor = normalize_actor(actor)?;
    let stamp = now();
    Ok(db.execute(
        "UPDATE cards SET verified_by = ?2, verified_at = ?3, updated_at = ?3
         WHERE id = ?1
           AND verified_by IS NULL
           AND claimed_by IS NOT NULL
           AND claimed_by <> ?2
           AND archived_at IS NULL
           AND EXISTS (SELECT 1 FROM board_columns c
                       WHERE c.id = cards.column_id AND c.maps_to_status = 'done')",
        params![id, actor, stamp],
    )? == 1)
}

/// Why a [`verify_card`] returned `false`.
///
/// Read *after* the failed attempt, never before it: a check beforehand would
/// be the very race the single-statement rule exists to avoid. Reading
/// afterwards only explains a decision that has already been made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyRefusal {
    NoSuchCard,
    AlreadyVerified,
    NotClaimed,
    SelfVerify,
    NotDone,
    Archived,
}

pub fn explain_verify_refusal(
    db: &Connection,
    id: i64,
    actor: &str,
) -> Result<Option<VerifyRefusal>> {
    // Canonicalised the same way `verify_card` does, so the explanation cannot
    // disagree with the decision it is explaining.
    let actor = normalize_actor(actor)?;
    let Some(card) = get_card(db, id)? else {
        return Ok(Some(VerifyRefusal::NoSuchCard));
    };
    if card.verified_by.is_some() {
        return Ok(Some(VerifyRefusal::AlreadyVerified));
    }
    if card.archived_at.is_some() {
        return Ok(Some(VerifyRefusal::Archived));
    }
    let Some(holder) = card.claimed_by.as_deref() else {
        return Ok(Some(VerifyRefusal::NotClaimed));
    };
    if holder == actor {
        return Ok(Some(VerifyRefusal::SelfVerify));
    }
    let done = get_column(db, card.column_id)?
        .is_some_and(|column| column.maps_to_status == CardStatus::Done);
    if !done {
        return Ok(Some(VerifyRefusal::NotDone));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CardFields;
    use crate::{SCHEMA_SQL_V1, SCHEMA_SQL_V2};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// A private database file per test. Deliberately a file and not
    /// `:memory:`: the concurrency tests need two *connections* to the same
    /// database, which an in-memory one cannot provide.
    fn temp_db_path() -> PathBuf {
        let unique = format!(
            "axiomata-board-test-{}-{}.db",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        std::env::temp_dir().join(unique)
    }

    fn open(path: &PathBuf) -> Connection {
        let conn = Connection::open(path).expect("open");
        conn.pragma_update(None, "foreign_keys", true).expect("fk");
        conn
    }

    fn fresh(path: &PathBuf) -> Connection {
        let conn = open(path);
        conn.execute_batch(SCHEMA_SQL_V1).expect("schema");
        conn.execute_batch(SCHEMA_SQL_V2).expect("flow schema");
        conn
    }

    fn fields(title: &str) -> CardFields {
        CardFields {
            title: title.to_string(),
            body: String::new(),
            labels: Vec::new(),
            assignee: None,
            due_at: None,
            plan_id: None,
            agent: None,
            agent_reason: None,
            tier: None,
            kind: None,
            acceptance: String::new(),
        }
    }

    /// The three plain columns (Offen, In Arbeit, Fertig) of a board — the roles Vorschlag and Review are left
    /// out, so the older tests keep their meaning of "column 0, 1, 2".
    fn plain_columns(db: &Connection, board_id: i64) -> Vec<Column> {
        list_columns(db, board_id)
            .expect("columns")
            .into_iter()
            .filter(|column| column.stage.is_none())
            .collect()
    }

    /// Board with its default columns, plus one card in the first.
    fn seed(db: &mut Connection) -> (Board, Vec<Column>, Card) {
        let board = create_board(db, "Test").expect("create board");
        let columns = plain_columns(db, board.id);
        let card = create_card(
            db,
            &NewCard {
                column_id: columns[0].id,
                fields: fields("Karte"),
            },
        )
        .expect("create card");
        (board, columns, card)
    }

    #[test]
    fn a_new_board_starts_with_the_flow_columns() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let board = create_board(&mut db, "Test").unwrap();
        let columns = list_columns(&db, board.id).unwrap();

        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            ["Vorschlag", "Offen", "In Arbeit", "Review", "Fertig"]
        );
        let statuses: Vec<CardStatus> = columns.iter().map(|c| c.maps_to_status).collect();
        assert_eq!(
            statuses,
            [
                CardStatus::Open,
                CardStatus::Open,
                CardStatus::Doing,
                CardStatus::Doing,
                CardStatus::Done
            ]
        );
        let stages: Vec<Option<ColumnStage>> = columns.iter().map(|c| c.stage).collect();
        assert_eq!(
            stages,
            [
                Some(ColumnStage::Proposal),
                None,
                None,
                Some(ColumnStage::Review),
                None
            ]
        );
        let _ = std::fs::remove_file(&path);
    }

    /// The load-bearing test of the whole crate: two actors reaching for the
    /// same card from two connections produce exactly one winner.
    #[test]
    fn only_one_of_two_concurrent_claims_wins() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, _, card) = seed(&mut db);

        let other = open(&path);
        let first = claim_card(&db, card.id, "agent:one").unwrap();
        let second = claim_card(&other, card.id, "agent:two").unwrap();

        assert!(first, "the first claim should win");
        assert!(!second, "the second claim should lose cleanly, not error");
        let held = get_card(&db, card.id).unwrap().unwrap().claimed_by;
        assert_eq!(held.as_deref(), Some("agent:one"));
        let _ = std::fs::remove_file(&path);
    }

    /// The hole the actor canonicalisation closes: both the `WHERE` clause and
    /// the `CHECK` constraint compare actor strings byte for byte, so without
    /// it "agent:one" and "Agent:One" are two different people and the
    /// two-party rule evaporates. Formatting must never decide identity.
    #[test]
    fn a_differently_spelled_actor_is_still_the_same_actor() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);

        assert!(claim_card(&db, card.id, "agent:one").unwrap());
        assert!(move_card(&mut db, card.id, columns[2].id, 0).unwrap());

        for spelling in ["Agent:One", "  agent:one  ", "AGENT:ONE"] {
            assert!(
                !verify_card(&db, card.id, spelling).unwrap(),
                "{spelling:?} is the claimant wearing a different hat, not a second party"
            );
            assert_eq!(
                explain_verify_refusal(&db, card.id, spelling).unwrap(),
                Some(VerifyRefusal::SelfVerify)
            );
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_actor_without_a_known_kind_is_refused_outright() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, _, card) = seed(&mut db);

        for bad in ["owner", "robot:one", "agent:", "agent:has spaces", ""] {
            assert!(
                matches!(
                    claim_card(&db, card.id, bad),
                    Err(BoardError::Invalid { field: "actor", .. })
                ),
                "{bad:?} should not be accepted as an actor"
            );
        }
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_archived_card_is_not_live_work() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, _, card) = seed(&mut db);

        assert!(set_card_archived(&db, card.id, true).unwrap());
        assert!(
            !claim_card(&db, card.id, "agent:one").unwrap(),
            "an archived card must not be claimable"
        );

        assert!(set_card_archived(&db, card.id, false).unwrap());
        assert!(claim_card(&db, card.id, "agent:one").unwrap());
        let _ = std::fs::remove_file(&path);
    }

    /// "Which column means done" is answered in the store so that two callers
    /// cannot answer it differently.
    #[test]
    fn move_to_status_finds_the_boards_own_done_column() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);

        let landed = move_to_status(&mut db, card.id, CardStatus::Done)
            .unwrap()
            .expect("the seeded board has a done column");
        assert_eq!(landed.id, columns[2].id);
        assert_eq!(
            get_card(&db, card.id).unwrap().unwrap().column_id,
            landed.id
        );

        // A board whose done column is gone reports that, rather than guessing.
        assert!(delete_column(&mut db, columns[2].id, Some(columns[0].id)).unwrap());
        assert!(
            move_to_status(&mut db, card.id, CardStatus::Done)
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn nobody_can_verify_their_own_work() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);
        let done = columns[2].id;

        assert!(claim_card(&db, card.id, "human:owner").unwrap());
        assert!(move_card(&mut db, card.id, done, 0).unwrap());

        assert!(
            !verify_card(&db, card.id, "human:owner").unwrap(),
            "self-verification must be refused"
        );
        assert_eq!(
            explain_verify_refusal(&db, card.id, "human:owner").unwrap(),
            Some(VerifyRefusal::SelfVerify)
        );
        assert!(
            verify_card(&db, card.id, "agent:reviewer").unwrap(),
            "a second party can sign it off"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn verification_needs_a_done_column() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);

        assert!(claim_card(&db, card.id, "agent:one").unwrap());
        // Still sitting in "Offen".
        assert!(!verify_card(&db, card.id, "agent:two").unwrap());
        assert_eq!(
            explain_verify_refusal(&db, card.id, "agent:two").unwrap(),
            Some(VerifyRefusal::NotDone)
        );

        assert!(move_card(&mut db, card.id, columns[2].id, 0).unwrap());
        assert!(verify_card(&db, card.id, "agent:two").unwrap());
        assert_eq!(
            explain_verify_refusal(&db, card.id, "agent:three").unwrap(),
            Some(VerifyRefusal::AlreadyVerified)
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Two routes to the same wrong state — a signature on work that is no
    /// longer finished. Route one: the card leaves the done column.
    #[test]
    fn moving_a_card_out_of_done_clears_its_signature() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);

        assert!(claim_card(&db, card.id, "agent:one").unwrap());
        assert!(move_card(&mut db, card.id, columns[2].id, 0).unwrap());
        assert!(verify_card(&db, card.id, "agent:two").unwrap());

        assert!(move_card(&mut db, card.id, columns[1].id, 0).unwrap());
        let after = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(after.verified_by, None);
        assert_eq!(after.verified_at, None);
        assert_eq!(
            after.claimed_by.as_deref(),
            Some("agent:one"),
            "the claim survives; only the sign-off is withdrawn"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Route two: the card does not move at all — the column stops meaning
    /// "done" underneath it.
    #[test]
    fn remapping_a_column_away_from_done_clears_signatures_in_it() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);
        let done = columns[2].id;

        assert!(claim_card(&db, card.id, "agent:one").unwrap());
        assert!(move_card(&mut db, card.id, done, 0).unwrap());
        assert!(verify_card(&db, card.id, "agent:two").unwrap());

        update_column(&mut db, done, "Doch nicht fertig", CardStatus::Doing)
            .unwrap()
            .expect("column should exist");

        let after = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(
            after.verified_by, None,
            "re-mapping the column must withdraw the sign-off too"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_column_holding_cards_is_only_deleted_with_somewhere_to_put_them() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);

        assert!(
            !delete_column(&mut db, columns[0].id, None).unwrap(),
            "refusing beats a foreign-key error the caller cannot act on"
        );
        assert!(get_card(&db, card.id).unwrap().is_some());

        assert!(delete_column(&mut db, columns[0].id, Some(columns[1].id)).unwrap());
        let moved = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(moved.column_id, columns[1].id);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn deleting_a_board_takes_its_columns_and_cards_with_it() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (board, _, card) = seed(&mut db);

        assert_eq!(count_cards(&db, board.id).unwrap(), 1);
        assert!(delete_board(&mut db, board.id).unwrap());
        assert!(get_card(&db, card.id).unwrap().is_none());
        assert!(list_columns(&db, board.id).unwrap().is_empty());
        assert!(get_board(&db, board.id).unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }

    /// Moving a card *down* inside its own column is where an index that
    /// counts the card itself goes wrong: the slot it is aiming at is still
    /// occupied by the card doing the aiming, so it lands one place short of
    /// where the drop indicator promised.
    #[test]
    fn a_card_moved_down_its_own_column_lands_exactly_where_it_was_dropped() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let board = create_board(&mut db, "Test").unwrap();
        let col = list_columns(&db, board.id).unwrap()[0].id;

        for title in ["a", "b", "c", "d"] {
            create_card(
                &db,
                &NewCard {
                    column_id: col,
                    fields: fields(title),
                },
            )
            .unwrap();
        }
        let titles = |db: &Connection| -> Vec<String> {
            list_cards(db, board.id, false)
                .unwrap()
                .into_iter()
                .map(|c| c.title)
                .collect()
        };
        let a_id = list_cards(&db, board.id, false).unwrap()[0].id;

        // "Drop a between c and d" — slot 2 among the cards that are not `a`.
        assert!(move_card(&mut db, a_id, col, 2).unwrap());
        assert_eq!(titles(&db), ["b", "c", "a", "d"]);
        let _ = std::fs::remove_file(&path);
    }

    /// The same, with the gap collapsed so the rebalance path runs: the
    /// renumbering must leave the moved card out too, or every slot after it
    /// shifts by one and the index stops meaning what it meant.
    #[test]
    fn the_rebalance_path_also_lands_where_it_was_dropped() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let board = create_board(&mut db, "Test").unwrap();
        let col = list_columns(&db, board.id).unwrap()[0].id;

        for title in ["a", "b", "c", "d"] {
            create_card(
                &db,
                &NewCard {
                    column_id: col,
                    fields: fields(title),
                },
            )
            .unwrap();
        }
        // Squeeze b and c together until the midpoint between them collapses.
        db.execute(
            "UPDATE cards SET position = 2.0000000001 WHERE title = 'c'",
            [],
        )
        .unwrap();

        let a_id = list_cards(&db, board.id, false).unwrap()[0].id;
        assert!(move_card(&mut db, a_id, col, 2).unwrap());

        let after: Vec<String> = list_cards(&db, board.id, false)
            .unwrap()
            .into_iter()
            .map(|c| c.title)
            .collect();
        assert_eq!(after, ["b", "c", "a", "d"]);
        let _ = std::fs::remove_file(&path);
    }

    /// Columns are ordered by the same scheme as cards, through the same two
    /// helpers — this is the test that catches the parameterisation going
    /// wrong for one of the two lists.
    #[test]
    fn columns_can_be_reordered_and_land_where_they_were_dropped() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let board = create_board(&mut db, "Test").unwrap();
        let names = |db: &Connection| -> Vec<String> {
            list_columns(db, board.id)
                .unwrap()
                .into_iter()
                .map(|c| c.name)
                .collect()
        };
        let start = ["Vorschlag", "Offen", "In Arbeit", "Review", "Fertig"];
        assert_eq!(names(&db), start);

        let fertig = plain_columns(&db, board.id)[2].id;
        assert!(move_column(&mut db, fertig, 0).unwrap());
        assert_eq!(
            names(&db),
            ["Fertig", "Vorschlag", "Offen", "In Arbeit", "Review"]
        );

        // Moving down its own list: the slot counts the *other* columns.
        assert!(move_column(&mut db, fertig, 4).unwrap());
        assert_eq!(names(&db), start);

        assert!(!move_column(&mut db, 9999, 0).unwrap());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn reordering_columns_leaves_the_cards_in_them_alone() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (board, columns, card) = seed(&mut db);

        assert!(move_column(&mut db, columns[2].id, 0).unwrap());
        let after = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(
            after.column_id, columns[0].id,
            "the card stays in its column"
        );
        assert_eq!(list_cards(&db, board.id, false).unwrap().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn cards_land_where_they_were_dropped() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let board = create_board(&mut db, "Test").unwrap();
        let columns = plain_columns(&db, board.id);
        let col = columns[0].id;

        for title in ["a", "b", "c"] {
            create_card(
                &db,
                &NewCard {
                    column_id: col,
                    fields: fields(title),
                },
            )
            .unwrap();
        }
        let titles = |db: &Connection| -> Vec<String> {
            list_cards(db, board.id, false)
                .unwrap()
                .into_iter()
                .map(|c| c.title)
                .collect()
        };
        assert_eq!(titles(&db), ["a", "b", "c"]);

        // Move "c" to the very front, then "a" into the middle.
        let c_id = list_cards(&db, board.id, false).unwrap()[2].id;
        assert!(move_card(&mut db, c_id, col, 0).unwrap());
        assert_eq!(titles(&db), ["c", "a", "b"]);

        let a_id = list_cards(&db, board.id, false).unwrap()[1].id;
        assert!(move_card(&mut db, a_id, col, 3).unwrap());
        assert_eq!(titles(&db), ["c", "b", "a"]);
        let _ = std::fs::remove_file(&path);
    }

    /// Repeated midpoints into the same gap exhaust f64 after roughly fifty
    /// inserts. A human never gets there; a loop of agents does. The column
    /// must renumber itself rather than silently start losing the order.
    #[test]
    fn repeated_inserts_into_one_gap_renumber_instead_of_collapsing() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let board = create_board(&mut db, "Test").unwrap();
        let col = list_columns(&db, board.id).unwrap()[0].id;

        create_card(
            &db,
            &NewCard {
                column_id: col,
                fields: fields("erste"),
            },
        )
        .unwrap();
        create_card(
            &db,
            &NewCard {
                column_id: col,
                fields: fields("letzte"),
            },
        )
        .unwrap();

        // Always drop into slot 1 — the same gap, eighty times over.
        for _ in 0..80 {
            let card = create_card(
                &db,
                &NewCard {
                    column_id: col,
                    fields: fields("zwischen"),
                },
            )
            .unwrap();
            assert!(move_card(&mut db, card.id, col, 1).unwrap());
        }

        let cards = list_cards(&db, board.id, false).unwrap();
        assert_eq!(cards.len(), 82);
        assert_eq!(cards[0].title, "erste", "the head must not drift");
        assert_eq!(cards[81].title, "letzte", "nor the tail");

        let mut sorted = cards.iter().map(|c| c.position).collect::<Vec<_>>();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            cards.len(),
            "every card must still hold a distinct position"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn archived_cards_stay_out_of_the_way_until_asked_for() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (board, _, card) = seed(&mut db);

        assert!(set_card_archived(&db, card.id, true).unwrap());
        assert!(list_cards(&db, board.id, false).unwrap().is_empty());
        assert_eq!(list_cards(&db, board.id, true).unwrap().len(), 1);

        assert!(set_card_archived(&db, card.id, false).unwrap());
        assert_eq!(list_cards(&db, board.id, false).unwrap().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_card_cannot_be_filed_under_another_boards_column() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);
        let other = create_board(&mut db, "Fremd").unwrap();
        let foreign = list_columns(&db, other.id).unwrap()[0].id;

        // The composite foreign key is what stops this; without it the card
        // would happily point at a column of a different board.
        let result = move_card(&mut db, card.id, foreign, 0);
        assert!(
            result.is_ok(),
            "moving to another board's column is a real move"
        );
        let moved = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(
            moved.board_id, other.id,
            "the card follows the column to its board rather than straddling both"
        );
        assert_eq!(moved.column_id, foreign);
        assert_eq!(
            list_cards(&db, columns[0].board_id, false).unwrap().len(),
            0
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_titles_and_names_are_refused_before_they_reach_the_database() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, _) = seed(&mut db);

        assert!(matches!(
            create_card(
                &db,
                &NewCard {
                    column_id: columns[0].id,
                    fields: fields("   ")
                }
            ),
            Err(BoardError::Invalid { field: "title", .. })
        ));
        assert!(matches!(
            create_board(&mut db, ""),
            Err(BoardError::Invalid { field: "name", .. })
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_hand_edited_label_column_names_the_row_it_broke() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, _, card) = seed(&mut db);

        db.execute(
            "UPDATE cards SET labels = 'not json' WHERE id = ?1",
            params![card.id],
        )
        .unwrap();

        match get_card(&db, card.id) {
            Err(BoardError::CorruptRow { table, id, .. }) => {
                assert_eq!(table, "cards");
                assert_eq!(id, card.id);
            }
            other => panic!("expected a corrupt-row error naming the card, got {other:?}"),
        }
        let _ = std::fs::remove_file(&path);
    }

    // ---------------------------------------------------------- release ---

    #[test]
    fn only_the_holder_can_release_a_card() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, _, card) = seed(&mut db);

        assert!(claim_card(&db, card.id, "agent:one").unwrap());
        assert!(
            !release_card(&db, card.id, "agent:two").unwrap(),
            "a non-holder must not be able to release"
        );
        let still_held = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(still_held.claimed_by.as_deref(), Some("agent:one"));

        assert!(release_card(&db, card.id, "agent:one").unwrap());
        let released = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(released.claimed_by, None);
        assert_eq!(released.claimed_at, None);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn releasing_an_unclaimed_card_fails_cleanly() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, _, card) = seed(&mut db);

        assert!(!release_card(&db, card.id, "agent:one").unwrap());
        let _ = std::fs::remove_file(&path);
    }

    /// A released card cannot keep a sign-off: nobody is accountable for the
    /// claim it was signed off against anymore.
    #[test]
    fn releasing_a_verified_card_withdraws_its_signature_too() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);

        assert!(claim_card(&db, card.id, "agent:one").unwrap());
        assert!(move_card(&mut db, card.id, columns[2].id, 0).unwrap());
        assert!(verify_card(&db, card.id, "agent:two").unwrap());

        assert!(release_card(&db, card.id, "agent:one").unwrap());
        let after = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(after.claimed_by, None);
        assert_eq!(after.claimed_at, None);
        assert_eq!(
            after.verified_by, None,
            "a released card cannot keep a sign-off nobody is accountable for"
        );
        assert_eq!(after.verified_at, None);
        let _ = std::fs::remove_file(&path);
    }

    // ------------------------------------------------------------ boards ---

    #[test]
    fn renaming_a_board_updates_its_name() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let board = create_board(&mut db, "Alt").unwrap();

        let renamed = rename_board(&db, board.id, "Neu").unwrap().unwrap();
        assert_eq!(renamed.name, "Neu");
        assert_eq!(get_board(&db, board.id).unwrap().unwrap().name, "Neu");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn renaming_a_nonexistent_board_returns_none() {
        let path = temp_db_path();
        let db = fresh(&path);
        assert!(rename_board(&db, 999_999, "Egal").unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn list_boards_orders_alphabetically_by_name() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        create_board(&mut db, "Zebra").unwrap();
        create_board(&mut db, "Apfel").unwrap();
        create_board(&mut db, "Mango").unwrap();

        let names: Vec<String> = list_boards(&db)
            .unwrap()
            .into_iter()
            .map(|b| b.name)
            .collect();
        assert_eq!(names, ["Apfel", "Mango", "Zebra"]);
        let _ = std::fs::remove_file(&path);
    }

    // ------------------------------------------------------------- cards ---

    #[test]
    fn updating_a_card_replaces_its_fields_without_touching_signatures() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, _, card) = seed(&mut db);
        assert!(claim_card(&db, card.id, "agent:one").unwrap());

        let new_fields = CardFields {
            title: "Neuer Titel".to_string(),
            body: "Neuer Text".to_string(),
            labels: vec!["dringend".to_string()],
            assignee: Some("human:owner".to_string()),
            due_at: None,
            ..CardFields::default()
        };
        let updated = update_card(&db, card.id, &new_fields).unwrap().unwrap();
        assert_eq!(updated.title, "Neuer Titel");
        assert_eq!(updated.body, "Neuer Text");
        assert_eq!(updated.labels, vec!["dringend".to_string()]);
        assert_eq!(updated.assignee.as_deref(), Some("human:owner"));
        assert_eq!(
            updated.claimed_by.as_deref(),
            Some("agent:one"),
            "update_card must never touch the claim signature"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn updating_a_nonexistent_card_returns_none() {
        let path = temp_db_path();
        let db = fresh(&path);
        assert!(update_card(&db, 999_999, &fields("x")).unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn deleting_a_card_removes_it_and_a_second_delete_reports_absence() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, _, card) = seed(&mut db);

        assert!(delete_card(&db, card.id).unwrap());
        assert!(get_card(&db, card.id).unwrap().is_none());
        assert!(
            !delete_card(&db, card.id).unwrap(),
            "deleting an already-gone card must not error"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn list_cards_groups_by_column_before_ordering_within_it() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let board = create_board(&mut db, "Test").unwrap();
        let columns = plain_columns(&db, board.id);
        let (open, doing, done) = (columns[0].id, columns[1].id, columns[2].id);

        // Deliberately created out of column order, to catch a grouping bug
        // that creation order alone would hide.
        create_card(
            &db,
            &NewCard {
                column_id: done,
                fields: fields("done-1"),
            },
        )
        .unwrap();
        create_card(
            &db,
            &NewCard {
                column_id: open,
                fields: fields("open-1"),
            },
        )
        .unwrap();
        create_card(
            &db,
            &NewCard {
                column_id: doing,
                fields: fields("doing-1"),
            },
        )
        .unwrap();
        create_card(
            &db,
            &NewCard {
                column_id: open,
                fields: fields("open-2"),
            },
        )
        .unwrap();

        let titles: Vec<String> = list_cards(&db, board.id, false)
            .unwrap()
            .into_iter()
            .map(|c| c.title)
            .collect();
        assert_eq!(titles, ["open-1", "open-2", "doing-1", "done-1"]);
        let _ = std::fs::remove_file(&path);
    }

    // ----------------------------------------------------------- columns ---

    #[test]
    fn create_column_appends_after_the_existing_columns() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let board = create_board(&mut db, "Test").unwrap();

        let extra = create_column(
            &db,
            board.id,
            &NewColumn {
                name: "Prüfen".to_string(),
                maps_to_status: CardStatus::Doing,
                stage: None,
            },
        )
        .unwrap();

        let columns = plain_columns(&db, board.id);
        assert_eq!(columns.len(), 4);
        assert_eq!(columns.last().unwrap().id, extra.id);
        assert!(
            extra.position > columns[2].position,
            "a new column must land after every existing one"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn updating_a_nonexistent_column_returns_none() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        assert!(
            update_column(&mut db, 999_999, "Egal", CardStatus::Open)
                .unwrap()
                .is_none()
        );
        let _ = std::fs::remove_file(&path);
    }

    /// The tests above only ever cover a column *leaving* `done`. A rename
    /// that keeps it `done` must take the other branch of `update_column` and
    /// leave any signature alone.
    #[test]
    fn remapping_a_column_that_stays_done_leaves_signatures_intact() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);
        let done = columns[2].id;

        assert!(claim_card(&db, card.id, "agent:one").unwrap());
        assert!(move_card(&mut db, card.id, done, 0).unwrap());
        assert!(verify_card(&db, card.id, "agent:two").unwrap());

        update_column(&mut db, done, "Erledigt", CardStatus::Done)
            .unwrap()
            .expect("column should exist");

        let after = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(
            after.verified_by.as_deref(),
            Some("agent:two"),
            "a rename that keeps the column done must not disturb a signature"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// `delete_column` moving cards out of a done column into *another* done
    /// column: the card is still done in its new home, so the signature must
    /// survive — the branch checks the destination's status, not a blanket
    /// "left its old column" rule.
    #[test]
    fn deleting_a_done_column_into_another_done_column_keeps_the_signature() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (board, columns, card) = seed(&mut db);
        let done = columns[2].id;
        let second_done = create_column(
            &db,
            board.id,
            &NewColumn {
                name: "Auch fertig".to_string(),
                maps_to_status: CardStatus::Done,
                stage: None,
            },
        )
        .unwrap()
        .id;

        assert!(claim_card(&db, card.id, "agent:one").unwrap());
        assert!(move_card(&mut db, card.id, done, 0).unwrap());
        assert!(verify_card(&db, card.id, "agent:two").unwrap());

        assert!(delete_column(&mut db, done, Some(second_done)).unwrap());
        let after = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(after.column_id, second_done);
        assert_eq!(
            after.verified_by.as_deref(),
            Some("agent:two"),
            "the card is still done in its new column, so the sign-off survives"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// Same deletion, but into a non-done column: the sign-off must be
    /// withdrawn, while the underlying claim is left alone.
    #[test]
    fn deleting_a_done_column_into_a_non_done_column_clears_the_signature() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);
        let done = columns[2].id;
        let doing = columns[1].id;

        assert!(claim_card(&db, card.id, "agent:one").unwrap());
        assert!(move_card(&mut db, card.id, done, 0).unwrap());
        assert!(verify_card(&db, card.id, "agent:two").unwrap());

        assert!(delete_column(&mut db, done, Some(doing)).unwrap());
        let after = get_card(&db, card.id).unwrap().unwrap();
        assert_eq!(after.column_id, doing);
        assert_eq!(
            after.verified_by, None,
            "moved into a non-done column, the sign-off must not survive"
        );
        assert_eq!(
            after.claimed_by.as_deref(),
            Some("agent:one"),
            "only the sign-off is withdrawn, not the claim"
        );
        let _ = std::fs::remove_file(&path);
    }

    // --------------------------------------------------- integrity floor ---

    /// `move_card` always keeps `board_id` in step with `column_id`. This
    /// test bypasses it deliberately, to check that the composite
    /// `(column_id, board_id)` foreign key — the actual last line of defence
    /// — refuses the state that a careless caller (or a future bug in
    /// `move_card`) could otherwise produce.
    #[test]
    fn pointing_a_card_at_another_boards_column_without_its_board_id_is_refused() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, _, card) = seed(&mut db);
        let other = create_board(&mut db, "Fremd").unwrap();
        let foreign = list_columns(&db, other.id).unwrap()[0].id;

        let result = db.execute(
            "UPDATE cards SET column_id = ?2 WHERE id = ?1",
            params![card.id, foreign],
        );
        assert!(
            result.is_err(),
            "the composite (column_id, board_id) foreign key should refuse this"
        );
        let _ = std::fs::remove_file(&path);
    }

    /// `verify_card`'s `WHERE` clause is the everyday guard against
    /// self-verification; the schema's `CHECK` is the guard for everything
    /// that does not go through `verify_card` at all — a hand edit, a future
    /// bug elsewhere. This test bypasses the store entirely to make sure that
    /// backstop actually holds.
    #[test]
    fn the_check_constraint_refuses_self_verification_even_bypassing_verify_card() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, card) = seed(&mut db);

        assert!(claim_card(&db, card.id, "human:owner").unwrap());
        assert!(move_card(&mut db, card.id, columns[2].id, 0).unwrap());

        let result = db.execute(
            "UPDATE cards SET verified_by = 'human:owner', verified_at = ?2 WHERE id = ?1",
            params![card.id, now()],
        );
        assert!(
            result.is_err(),
            "the CHECK constraint is the last line of defence against self-verification"
        );
        let _ = std::fs::remove_file(&path);
    }

    // -------------------------------------------------------- validation ---

    #[test]
    fn a_title_at_exactly_the_maximum_length_is_accepted() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, _) = seed(&mut db);
        let title = "x".repeat(MAX_TITLE_LEN);

        let card = create_card(
            &db,
            &NewCard {
                column_id: columns[0].id,
                fields: fields(&title),
            },
        )
        .unwrap();
        assert_eq!(card.title.len(), MAX_TITLE_LEN);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_title_one_byte_over_the_maximum_length_is_refused() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, _) = seed(&mut db);
        let title = "x".repeat(MAX_TITLE_LEN + 1);

        let result = create_card(
            &db,
            &NewCard {
                column_id: columns[0].id,
                fields: fields(&title),
            },
        );
        assert!(matches!(
            result,
            Err(BoardError::Invalid { field: "title", .. })
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_body_over_the_maximum_length_is_refused() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, _) = seed(&mut db);
        let mut over = fields("Karte");
        over.body = "x".repeat(MAX_BODY_LEN + 1);

        let result = create_card(
            &db,
            &NewCard {
                column_id: columns[0].id,
                fields: over,
            },
        );
        assert!(matches!(
            result,
            Err(BoardError::Invalid { field: "body", .. })
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_board_name_over_the_maximum_length_is_refused() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let name = "x".repeat(MAX_NAME_LEN + 1);
        assert!(matches!(
            create_board(&mut db, &name),
            Err(BoardError::Invalid { field: "name", .. })
        ));
        let _ = std::fs::remove_file(&path);
    }

    // --------------------------------------------------------- refusals ---

    #[test]
    fn explain_verify_refusal_distinguishes_no_such_card_from_not_claimed() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, _, card) = seed(&mut db);

        assert_eq!(
            explain_verify_refusal(&db, card.id, "agent:one").unwrap(),
            Some(VerifyRefusal::NotClaimed)
        );
        assert_eq!(
            explain_verify_refusal(&db, 999_999, "agent:one").unwrap(),
            Some(VerifyRefusal::NoSuchCard)
        );
        let _ = std::fs::remove_file(&path);
    }

    /// The crate's own stated convention — "missing is never an error" — for
    /// every mutating entry point at once, against one id that never existed.
    #[test]
    fn mutations_against_a_missing_id_report_absence_instead_of_erroring() {
        let path = temp_db_path();
        let mut db = fresh(&path);
        let (_, columns, _) = seed(&mut db);
        let missing = 999_999;

        assert!(update_card(&db, missing, &fields("x")).unwrap().is_none());
        assert!(!delete_card(&db, missing).unwrap());
        assert!(rename_board(&db, missing, "x").unwrap().is_none());
        assert!(
            update_column(&mut db, missing, "x", CardStatus::Open)
                .unwrap()
                .is_none()
        );
        assert!(!delete_column(&mut db, missing, None).unwrap());
        assert!(!move_card(&mut db, missing, columns[0].id, 0).unwrap());
        assert!(!claim_card(&db, missing, "agent:one").unwrap());
        assert!(!release_card(&db, missing, "agent:one").unwrap());
        assert!(!verify_card(&db, missing, "agent:one").unwrap());
        assert!(!set_card_archived(&db, missing, true).unwrap());
        assert!(get_card(&db, missing).unwrap().is_none());
        assert!(get_board(&db, missing).unwrap().is_none());
        assert!(get_column(&db, missing).unwrap().is_none());
        let _ = std::fs::remove_file(&path);
    }
}
