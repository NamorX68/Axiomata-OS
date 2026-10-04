//! Every SQL statement of the mailbox.
//!
//! Same conventions as [`crate::agent_store`]: free functions over a borrowed connection, "missing" is an `Option`,
//! never an error. A function that writes **several rows that must stand or fall together** takes `&mut Connection` and
//! runs in one `IMMEDIATE` transaction — the MCP server (CP-A4) will be a second process writing to the same file, and
//! "count this sender's messages, then insert" is exactly the check-then-act that two processes would race on.
//!
//! Nothing here looks at the file system or at a running process. Which sessions are alive is runtime state that the
//! caller passes in (`live`), the same way [`crate::lifecycle`] keeps the status out of the database.

use chrono::{DateTime, Duration, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::model::{
    Inbox, InboxEntry, Limits, Message, MessageKind, MessageStatus, NewMessage, Part, Recipient,
    Refusal, SendResult, Sender, check_parts,
};
use crate::lifecycle::AgentState;
use crate::{IdeError, Result};

/// How long the messages about a finished card stay, counted from the day it was taken over or canceled (A29).
pub const CARD_RETENTION_DAYS: i64 = 14;
/// How long a message that belongs to no card stays (A29).
pub const LOOSE_RETENTION_DAYS: i64 = 30;
/// The most entries one `read_inbox` call returns. An inbox that deep is a sign something is wrong, and the rest is
/// still there for the next call.
pub const MAX_READ: usize = 50;
/// The most senders a nudge line spells out; the line is typed into a terminal, so it stays short.
const MAX_NUDGE_SENDERS: usize = 3;

const MESSAGE_COLS: &str = "m.id, m.created_at, m.sender, m.to_addr, m.card_id, m.kind, m.parts, m.in_reply_to, \
                            m.chain, m.status";

fn stamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339()
}

fn parse_ts(raw: &str, table: &'static str, id: i64, field: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| IdeError::CorruptRow {
            table,
            id,
            reason: format!("{field} is not RFC 3339: {err}"),
        })
}

/// A message row with its enums, parts and timestamp still as text.
struct RawMessage {
    id: i64,
    created_at: String,
    sender: String,
    to_addr: String,
    card_id: Option<i64>,
    kind: String,
    parts: String,
    in_reply_to: Option<i64>,
    chain: i64,
    status: String,
}

fn row_to_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawMessage> {
    Ok(RawMessage {
        id: row.get(0)?,
        created_at: row.get(1)?,
        sender: row.get(2)?,
        to_addr: row.get(3)?,
        card_id: row.get(4)?,
        kind: row.get(5)?,
        parts: row.get(6)?,
        in_reply_to: row.get(7)?,
        chain: row.get(8)?,
        status: row.get(9)?,
    })
}

impl RawMessage {
    fn into_message(self) -> Result<Message> {
        let corrupt = |reason: String| IdeError::CorruptRow {
            table: "mail_messages",
            id: self.id,
            reason,
        };
        Ok(Message {
            created_at: parse_ts(&self.created_at, "mail_messages", self.id, "created_at")?,
            sender: Sender::parse(&self.sender)
                .ok_or_else(|| corrupt(format!("unknown sender {:?}", self.sender)))?,
            to: Recipient::parse(&self.to_addr)
                .ok_or_else(|| corrupt(format!("unknown recipient {:?}", self.to_addr)))?,
            kind: MessageKind::parse(&self.kind)
                .ok_or_else(|| corrupt(format!("unknown kind {:?}", self.kind)))?,
            parts: serde_json::from_str(&self.parts)
                .map_err(|err| corrupt(format!("parts are not valid: {err}")))?,
            chain: u32::try_from(self.chain)
                .map_err(|_| corrupt(format!("chain {} out of range", self.chain)))?,
            status: MessageStatus::parse(&self.status)
                .ok_or_else(|| corrupt(format!("unknown status {:?}", self.status)))?,
            id: self.id,
            card_id: self.card_id,
            in_reply_to: self.in_reply_to,
        })
    }
}

/// Everything an `INSERT` into `mail_messages` needs, so the helper does not take eleven arguments.
struct Draft<'a> {
    sender: &'a Sender,
    to: &'a Recipient,
    card_id: Option<i64>,
    kind: MessageKind,
    parts: &'a [Part],
    in_reply_to: Option<i64>,
    chain: u32,
    status: MessageStatus,
}

fn insert_message(db: &Connection, draft: &Draft<'_>) -> Result<Message> {
    let parts = serde_json::to_string(draft.parts).map_err(|err| IdeError::Invalid {
        field: "parts",
        reason: err.to_string(),
    })?;
    db.execute(
        "INSERT INTO mail_messages (created_at, sender, to_addr, card_id, kind, parts, in_reply_to, chain, status) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            stamp(Utc::now()),
            draft.sender.as_text(),
            draft.to.as_text(),
            draft.card_id,
            draft.kind.as_str(),
            parts,
            draft.in_reply_to,
            draft.chain,
            draft.status.as_str(),
        ],
    )?;
    let id = db.last_insert_rowid();
    // Read back instead of assembling by hand: what the caller gets is exactly what a later read returns.
    get_message(db, id)?.ok_or(IdeError::CorruptRow {
        table: "mail_messages",
        id,
        reason: "row vanished right after its insert".to_owned(),
    })
}

fn deliver(db: &Connection, message_id: i64, inbox: &Inbox) -> Result<()> {
    db.execute(
        "INSERT INTO mail_deliveries (message_id, inbox) VALUES (?1, ?2)",
        params![message_id, inbox.as_text()],
    )?;
    Ok(())
}

/// One message by id.
///
/// # Errors
///
/// [`IdeError::Database`], or [`IdeError::CorruptRow`] for a row this crate could not have written.
pub fn get_message(db: &Connection, id: i64) -> Result<Option<Message>> {
    let raw = db
        .query_row(
            &format!("SELECT {MESSAGE_COLS} FROM mail_messages m WHERE m.id = ?1"),
            params![id],
            row_to_raw,
        )
        .optional()?;
    raw.map(RawMessage::into_message).transpose()
}

/// Where a recipient's message can go.
enum Resolved {
    Inboxes(Vec<Inbox>),
    Nobody(String),
}

/// Turns an address into concrete, live inboxes. `live` lists the sessions whose harness is running; the sender is
/// left out of a role's fan-out so a reviewer writing to "the reviewers" does not mail itself.
///
/// **A session reaches only sessions of its own project.** A role such as `reviewer` exists in every project, and a
/// message carries text and file paths of one repository; without the fence it would also land at the reviewer of an
/// unrelated project. The owner is not fenced: addressing a role or a session across projects is a deliberate act.
fn resolve(db: &Connection, to: &Recipient, sender: &Sender, live: &[i64]) -> Result<Resolved> {
    let project_of = |id: i64| -> Result<Option<i64>> {
        Ok(db
            .query_row(
                "SELECT project_id FROM ide_agents WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?)
    };
    let fence: Option<i64> = match sender {
        Sender::Session(me) => match project_of(*me)? {
            Some(project) => Some(project),
            None => {
                return Ok(Resolved::Nobody(
                    "the sending session is not known".to_owned(),
                ));
            }
        },
        Sender::Owner | Sender::System => None,
    };
    match to {
        Recipient::Owner => Ok(Resolved::Inboxes(vec![Inbox::Owner])),
        Recipient::Session(id) => Ok(match project_of(*id)? {
            // The same words for "no such session" and "a session of another project": the sender learns nothing
            // about projects it is not part of.
            None => Resolved::Nobody(format!("there is no session {id}")),
            Some(project) if fence.is_some_and(|f| f != project) => {
                Resolved::Nobody(format!("there is no session {id}"))
            }
            Some(_) if !live.contains(id) => Resolved::Nobody(format!("session {id} has ended")),
            Some(_) => Resolved::Inboxes(vec![Inbox::Session(*id)]),
        }),
        Recipient::Role(name) => {
            let mut stmt = db.prepare(
                "SELECT id FROM ide_agents WHERE agent_role = ?1 AND (?2 IS NULL OR project_id = ?2) ORDER BY id",
            )?;
            let ids = stmt
                .query_map(params![name, fence], |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let own = match sender {
                Sender::Session(id) => Some(*id),
                _ => None,
            };
            let inboxes: Vec<Inbox> = ids
                .into_iter()
                .filter(|id| live.contains(id) && Some(*id) != own)
                .map(Inbox::Session)
                .collect();
            Ok(if inboxes.is_empty() {
                Resolved::Nobody(format!("no live session plays the role {name}"))
            } else {
                Resolved::Inboxes(inboxes)
            })
        }
    }
}

/// Writes a system notice and puts it into `inboxes`. Notices are never answered ([`MessageKind::Notice`]), and they
/// do not count against anyone's message limit because their sender is the studio.
fn post_notice(
    db: &Connection,
    about: &Message,
    to: &Recipient,
    inboxes: &[Inbox],
    text: String,
) -> Result<Message> {
    let notice = insert_message(
        db,
        &Draft {
            sender: &Sender::System,
            to,
            card_id: about.card_id,
            kind: MessageKind::Notice,
            parts: &[Part::Text { text }],
            in_reply_to: Some(about.id),
            chain: 0,
            status: MessageStatus::Delivered,
        },
    )?;
    for inbox in inboxes {
        deliver(db, notice.id, inbox)?;
    }
    Ok(notice)
}

/// Tells a session that its message did not arrive, and the owner with it (A29). An owner who wrote the message
/// needs no notice about their own message, and the studio's notices are never answered.
fn notify_undeliverable(db: &Connection, message: &Message, reason: &str) -> Result<()> {
    let Sender::Session(id) = message.sender else {
        return Ok(());
    };
    post_notice(
        db,
        message,
        &Recipient::Session(id),
        &[Inbox::Session(id), Inbox::Owner],
        format!(
            "Message #{} from {} to {} could not be delivered: {reason}.",
            message.id, message.sender, message.to
        ),
    )?;
    Ok(())
}

/// Sends a message.
///
/// What happens, in order:
///
/// 1. The parts are checked ([`check_parts`]) and `sender` must be a session or the owner — the studio's own notices
///    never come through here.
/// 2. A session's message to somebody who wrote to it about the same card and is still unanswered counts as the reply
///    (`infer_parent`), named or not. A reply (`in_reply_to`) must answer a message the sender **received**, and not
///    an ack or a notice (A8). Its chain
///    counter is the parent's plus one; a fresh message, or anything the owner writes, starts a chain at 1 (the owner
///    stepping in is what the chain limit asks for).
/// 3. A session may send at most [`Limits::max_per_sender_and_card`] messages about one card.
/// 4. The recipient is resolved to live inboxes. Nobody to deliver to → the message is kept as
///    [`MessageStatus::Undeliverable`] and a notice goes to the sender and the owner (A29). A chain past
///    [`Limits::max_chain`] → the message is kept as [`MessageStatus::Held`], only the owner's inbox gets it, and the
///    sender gets a notice; [`release_held`] lets it go on.
///
/// # Errors
///
/// [`IdeError::Invalid`] for bad parts, a system sender, or a reply to a message the sender never received;
/// [`IdeError::Database`] otherwise. A limit, a self-send or a reply to an ack is a [`SendResult::Refused`], not an
/// error.
pub fn send(
    db: &mut Connection,
    limits: &Limits,
    sender: &Sender,
    mut new: NewMessage,
    live: &[i64],
) -> Result<SendResult> {
    check_parts(&new.parts)?;
    if matches!(sender, Sender::System) {
        return Err(IdeError::Invalid {
            field: "sender",
            reason: "the studio's notices are written by the mailbox itself".to_owned(),
        });
    }
    // A notice is the studio's voice; a session that could write one would look like the studio in the owner's inbox.
    if new.kind == MessageKind::Notice {
        return Err(IdeError::Invalid {
            field: "kind",
            reason: "notices are written by the mailbox itself".to_owned(),
        });
    }
    if let (Sender::Session(me), Recipient::Session(to)) = (sender, &new.to)
        && me == to
    {
        return Ok(SendResult::Refused {
            refusal: Refusal::ToSelf,
        });
    }

    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;

    // A session that writes to somebody who wrote to it, about the same card, and has not answered yet *is* answering,
    // whether or not it says so. Without this the chain counter would guard only agents that politely name the message
    // they reply to: writing "fresh" messages would restart the count at 1 for ever.
    if new.in_reply_to.is_none()
        && let Sender::Session(me) = sender
    {
        new.in_reply_to = infer_parent(&tx, *me, &new.to, new.card_id)?;
    }

    let mut chain = 1;
    if let Some(parent_id) = new.in_reply_to {
        let parent = get_message(&tx, parent_id)?.ok_or_else(|| IdeError::Invalid {
            field: "in_reply_to",
            reason: format!("there is no message {parent_id}"),
        })?;
        let received = match sender.inbox() {
            Some(inbox) => tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM mail_deliveries WHERE message_id = ?1 AND inbox = ?2)",
                params![parent_id, inbox.as_text()],
                |r| r.get::<_, bool>(0),
            )?,
            None => false,
        };
        if !received {
            return Err(IdeError::Invalid {
                field: "in_reply_to",
                reason: format!("message {parent_id} was not sent to you"),
            });
        }
        if !parent.kind.expects_reply() {
            return Ok(SendResult::Refused {
                refusal: Refusal::NoReplyExpected,
            });
        }
        if !matches!(sender, Sender::Owner) {
            chain = parent.chain + 1;
        }
    }

    if matches!(sender, Sender::Session(_)) {
        let sent: i64 = tx.query_row(
            // `IS` rather than `=`: a message without a card counts against the "no card" bucket.
            "SELECT COUNT(*) FROM mail_messages WHERE sender = ?1 AND card_id IS ?2",
            params![sender.as_text(), new.card_id],
            |r| r.get(0),
        )?;
        let sent_in_all: i64 = tx.query_row(
            "SELECT COUNT(*) FROM mail_messages WHERE sender = ?1",
            params![sender.as_text()],
            |r| r.get(0),
        )?;
        if sent_in_all >= i64::from(limits.max_per_sender_total) {
            return Ok(SendResult::Refused {
                refusal: Refusal::SenderTotalLimit {
                    limit: limits.max_per_sender_total,
                },
            });
        }
        if sent >= i64::from(limits.max_per_sender_and_card) {
            return Ok(SendResult::Refused {
                refusal: Refusal::SenderLimit {
                    limit: limits.max_per_sender_and_card,
                },
            });
        }
    }

    let result = place(&tx, limits, sender, &new, chain, live)?;
    tx.commit()?;
    Ok(result)
}

/// The newest message that `me` received from the addressed party about `card`, is an ordinary message (not an ack or a
/// notice), was delivered, and that `me` has not answered yet. An answer that was *held* at the chain limit is not an
/// answer: counting it would let the next message start a fresh chain right after the owner was asked.
fn infer_parent(
    db: &Connection,
    me: i64,
    to: &Recipient,
    card_id: Option<i64>,
) -> Result<Option<i64>> {
    let from = match to {
        Recipient::Session(_) => "m.sender = ?3",
        Recipient::Role(_) => {
            "m.sender IN (SELECT 'session:' || id FROM ide_agents WHERE agent_role = ?3)"
        }
        Recipient::Owner => "m.sender = ?3",
    };
    let party = match to {
        Recipient::Session(id) => format!("session:{id}"),
        Recipient::Role(name) => name.clone(),
        Recipient::Owner => "owner".to_owned(),
    };
    Ok(db
        .query_row(
            &format!(
                "SELECT m.id FROM mail_deliveries d JOIN mail_messages m ON m.id = d.message_id \
                 WHERE d.inbox = ?1 AND m.kind = 'message' AND m.status = 'delivered' AND m.card_id IS ?2 \
                   AND {from} \
                   AND NOT EXISTS (SELECT 1 FROM mail_messages r \
                                   WHERE r.in_reply_to = m.id AND r.sender = ?4 AND r.status = 'delivered') \
                 ORDER BY m.id DESC LIMIT 1"
            ),
            params![Inbox::Session(me).as_text(), card_id, party, Sender::Session(me).as_text()],
            |r| r.get(0),
        )
        .optional()?)
}

/// Stores a checked message and routes it: delivered, held at the chain limit, or undeliverable.
fn place(
    tx: &Connection,
    limits: &Limits,
    sender: &Sender,
    new: &NewMessage,
    chain: u32,
    live: &[i64],
) -> Result<SendResult> {
    let draft = |status| Draft {
        sender,
        to: &new.to,
        card_id: new.card_id,
        kind: new.kind,
        parts: &new.parts,
        in_reply_to: new.in_reply_to,
        chain,
        status,
    };

    // The chain limit only guards the way *into* an agent: a message to the owner ends a chain, so it is never held.
    if !matches!(new.to, Recipient::Owner) && chain > limits.max_chain {
        let message = insert_message(tx, &draft(MessageStatus::Held))?;
        deliver(tx, message.id, &Inbox::Owner)?;
        if let Sender::Session(id) = sender {
            post_notice(
                tx,
                &message,
                &Recipient::Session(*id),
                &[Inbox::Session(*id)],
                format!(
                    "Message #{} was not delivered: the conversation reached {} replies in a row. The owner has been \
                     asked and decides whether it goes on.",
                    message.id, limits.max_chain
                ),
            )?;
        }
        return Ok(SendResult::Held { message });
    }

    match resolve(tx, &new.to, sender, live)? {
        Resolved::Inboxes(inboxes) => {
            let message = insert_message(tx, &draft(MessageStatus::Delivered))?;
            for inbox in &inboxes {
                deliver(tx, message.id, inbox)?;
            }
            Ok(SendResult::Delivered { message, inboxes })
        }
        Resolved::Nobody(reason) => {
            let message = insert_message(tx, &draft(MessageStatus::Undeliverable))?;
            notify_undeliverable(tx, &message, &reason)?;
            Ok(SendResult::Undeliverable { message, reason })
        }
    }
}

/// The owner lets a held message go on (A8: after the chain limit the owner decides).
///
/// The count restarts at 1 — the owner stepping in is a new beginning, exactly as when the owner writes a message.
/// `None` when there is no such message or it is not held.
///
/// # Errors
///
/// [`IdeError::Database`] or [`IdeError::CorruptRow`].
pub fn release_held(
    db: &mut Connection,
    message_id: i64,
    live: &[i64],
) -> Result<Option<SendResult>> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let Some(message) = get_message(&tx, message_id)? else {
        return Ok(None);
    };
    if message.status != MessageStatus::Held {
        return Ok(None);
    }
    let result = match resolve(&tx, &message.to, &message.sender, live)? {
        Resolved::Inboxes(inboxes) => {
            tx.execute(
                "UPDATE mail_messages SET status = 'delivered', chain = 1 WHERE id = ?1",
                params![message_id],
            )?;
            for inbox in &inboxes {
                deliver(&tx, message_id, inbox)?;
            }
            let message = get_message(&tx, message_id)?.ok_or(IdeError::CorruptRow {
                table: "mail_messages",
                id: message_id,
                reason: "row vanished inside its own transaction".to_owned(),
            })?;
            SendResult::Delivered { message, inboxes }
        }
        Resolved::Nobody(reason) => {
            tx.execute(
                "UPDATE mail_messages SET status = 'undeliverable' WHERE id = ?1",
                params![message_id],
            )?;
            let message = get_message(&tx, message_id)?.ok_or(IdeError::CorruptRow {
                table: "mail_messages",
                id: message_id,
                reason: "row vanished inside its own transaction".to_owned(),
            })?;
            notify_undeliverable(&tx, &message, &reason)?;
            SendResult::Undeliverable { message, reason }
        }
    };
    tx.commit()?;
    Ok(Some(result))
}

/// Reads an inbox, oldest first, at most [`MAX_READ`] entries (or `limit`, whichever is smaller).
///
/// With `unread_only` it is the **oldest** unread entries, so a backlog is worked off in order; without it, the
/// **newest** entries (still returned oldest first), which is what a history view wants.
///
/// With `mark_read` the returned entries are marked read in the same call — the way an agent's `read_inbox` works —
/// and come back with `read_at` set. Every row is converted *before* anything is marked, so a damaged row fails the
/// call without having swallowed the healthy messages beside it. The marking is per delivery id, so a message that
/// arrives between the read and the mark is untouched and shows up next time.
///
/// # Errors
///
/// [`IdeError::Database`] or [`IdeError::CorruptRow`].
pub fn read_inbox(
    db: &Connection,
    inbox: &Inbox,
    unread_only: bool,
    limit: usize,
    mark_read: bool,
) -> Result<Vec<InboxEntry>> {
    let limit = limit.clamp(1, MAX_READ);
    let order = if unread_only { "ASC" } else { "DESC" };
    let mut stmt = db.prepare(&format!(
        "SELECT d.id, d.read_at, {MESSAGE_COLS} FROM mail_deliveries d \
         JOIN mail_messages m ON m.id = d.message_id \
         WHERE d.inbox = ?1 AND (?2 = 0 OR d.read_at IS NULL) \
         ORDER BY d.id {order} LIMIT ?3"
    ))?;
    let rows = stmt
        .query_map(params![inbox.as_text(), unread_only, limit as i64], |row| {
            let delivery_id: i64 = row.get(0)?;
            let read_at: Option<String> = row.get(1)?;
            // `row_to_raw` reads columns 0.. so the message columns are shifted by the two delivery columns.
            let raw = RawMessage {
                id: row.get(2)?,
                created_at: row.get(3)?,
                sender: row.get(4)?,
                to_addr: row.get(5)?,
                card_id: row.get(6)?,
                kind: row.get(7)?,
                parts: row.get(8)?,
                in_reply_to: row.get(9)?,
                chain: row.get(10)?,
                status: row.get(11)?,
            };
            Ok((delivery_id, read_at, raw))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut entries = Vec::with_capacity(rows.len());
    for (delivery_id, read_at, raw) in rows {
        let read_at = match read_at {
            Some(text) => Some(parse_ts(&text, "mail_deliveries", delivery_id, "read_at")?),
            None => None,
        };
        entries.push(InboxEntry {
            delivery_id,
            message: raw.into_message()?,
            read_at,
        });
    }
    if !unread_only {
        entries.reverse();
    }

    if mark_read {
        let marked_at = Utc::now();
        let mut mark = db
            .prepare("UPDATE mail_deliveries SET read_at = ?1 WHERE id = ?2 AND read_at IS NULL")?;
        for entry in entries.iter_mut().filter(|e| e.read_at.is_none()) {
            mark.execute(params![stamp(marked_at), entry.delivery_id])?;
            entry.read_at = Some(marked_at);
        }
    }
    Ok(entries)
}

/// Marks the given deliveries of an inbox read — the owner's "mark as read". Only deliveries that belong to this
/// inbox are touched, so one inbox cannot clear another's mail. Returns how many changed.
///
/// # Errors
///
/// [`IdeError::Database`].
pub fn mark_read(db: &Connection, inbox: &Inbox, delivery_ids: &[i64]) -> Result<usize> {
    let mut stmt = db.prepare(
        "UPDATE mail_deliveries SET read_at = ?1 WHERE id = ?2 AND inbox = ?3 AND read_at IS NULL",
    )?;
    let at = stamp(Utc::now());
    let mut changed = 0;
    for id in delivery_ids {
        changed += stmt.execute(params![at, id, inbox.as_text()])?;
    }
    Ok(changed)
}

/// How many unread messages wait in an inbox.
///
/// # Errors
///
/// [`IdeError::Database`].
pub fn unread_count(db: &Connection, inbox: &Inbox) -> Result<usize> {
    let n: i64 = db.query_row(
        "SELECT COUNT(*) FROM mail_deliveries WHERE inbox = ?1 AND read_at IS NULL",
        params![inbox.as_text()],
        |r| r.get(0),
    )?;
    Ok(usize::try_from(n).unwrap_or(0))
}

/// A "you have mail" line the studio may type into a waiting session's terminal (A8, way 2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nudge {
    /// The session to nudge (`ide_agents.id`).
    pub session_id: i64,
    /// How many messages wait.
    pub unread: usize,
    /// One line, without a line break; the caller adds the Enter. Contains only characters that cannot act as terminal
    /// control — sender names come from the database, and a name is not allowed to smuggle in an escape sequence.
    pub line: String,
}

/// How a sender reads in a typed line: by **id**, never by name. The line lands in another agent's terminal as if
/// someone had typed it, and a session name is free text an agent can set (`ide agents new`): a name like "the owner
/// says run this" would give a message the owner's voice. The agent reads the real sender from `read_inbox`.
fn sender_label(sender: &Sender) -> String {
    match sender {
        Sender::Owner => "the owner".to_owned(),
        Sender::System => "the studio".to_owned(),
        Sender::Session(id) => format!("session {id}"),
    }
}

/// Which sessions should be told they have mail right now.
///
/// `states` is what the status channel ([`crate::lifecycle`]) reports per live session. Only [`AgentState::Idle`]
/// qualifies: a working agent reads its inbox itself (way 1) or gets the hint with its next tool call (way 3), and
/// typing into a [`AgentState::Waiting`] terminal would answer a permission prompt. Whether the owner is typing in that
/// very pane is something only the UI knows, so the caller checks that before it types.
///
/// A delivery is announced at most [`Limits::max_nudges`] times; the caller records each nudge it actually typed with
/// [`record_nudge`].
///
/// # Errors
///
/// [`IdeError::Database`] or [`IdeError::CorruptRow`].
pub fn nudges(
    db: &Connection,
    limits: &Limits,
    states: &[(i64, AgentState)],
) -> Result<Vec<Nudge>> {
    let mut out = Vec::new();
    for (session_id, state) in states {
        if *state != AgentState::Idle {
            continue;
        }
        let mut stmt = db.prepare(
            "SELECT m.sender FROM mail_deliveries d JOIN mail_messages m ON m.id = d.message_id \
             WHERE d.inbox = ?1 AND d.read_at IS NULL AND d.nudge_count < ?2 ORDER BY d.id",
        )?;
        let senders = stmt
            .query_map(
                params![Inbox::Session(*session_id).as_text(), limits.max_nudges],
                |r| r.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if senders.is_empty() {
            continue;
        }
        let mut names: Vec<String> = Vec::new();
        for raw in &senders {
            // A sender this crate could not have written is skipped rather than failing the whole check.
            let Some(sender) = Sender::parse(raw) else {
                continue;
            };
            let label = sender_label(&sender);
            if !names.contains(&label) {
                names.push(label);
            }
        }
        let shown = names
            .iter()
            .take(MAX_NUDGE_SENDERS)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let more = if names.len() > MAX_NUDGE_SENDERS {
            " and others"
        } else {
            ""
        };
        let count = senders.len();
        out.push(Nudge {
            session_id: *session_id,
            unread: count,
            line: format!(
                "You have {count} new message{} from {shown}{more} in your Axiomata inbox. Call read_inbox to read \
                 {}.",
                if count == 1 { "" } else { "s" },
                if count == 1 { "it" } else { "them" },
            ),
        });
    }
    Ok(out)
}

/// Notes that a nudge was typed for everything unread in a session's inbox.
///
/// # Errors
///
/// [`IdeError::Database`].
pub fn record_nudge(db: &Connection, session_id: i64) -> Result<()> {
    db.execute(
        "UPDATE mail_deliveries SET nudge_count = nudge_count + 1 WHERE inbox = ?1 AND read_at IS NULL",
        params![Inbox::Session(session_id).as_text()],
    )?;
    Ok(())
}

/// A card that was taken over or canceled, with the moment it was — what retention counts from (A29). The board knows
/// this, the mailbox does not, so the caller supplies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosedCard {
    pub card_id: i64,
    pub closed_at: DateTime<Utc>,
}

/// Deletes what has outlived its retention (A29): messages about a closed card [`CARD_RETENTION_DAYS`] after it closed,
/// messages without a card [`LOOSE_RETENTION_DAYS`] after they were written. Returns how many messages went.
///
/// Meant to run once at start. A message whose card is *not* in `closed` stays however old it is — a card still in
/// progress is still in need of its history. That includes a card that was **deleted** from the board: the board
/// cannot say "closed" about a row that is gone, so the caller should list deleted cards as closed (with the moment
/// they were deleted, or, when that is unknown, any old date) — otherwise their mail waits for ever.
///
/// # Errors
///
/// [`IdeError::Database`].
pub fn purge(db: &mut Connection, now: DateTime<Utc>, closed: &[ClosedCard]) -> Result<usize> {
    let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut doomed: Vec<i64> = Vec::new();
    {
        let mut by_card = tx.prepare("SELECT id FROM mail_messages WHERE card_id = ?1")?;
        for card in closed {
            if card.closed_at + Duration::days(CARD_RETENTION_DAYS) < now {
                let ids = by_card
                    .query_map(params![card.card_id], |r| r.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                doomed.extend(ids);
            }
        }
        let loose_cutoff = stamp(now - Duration::days(LOOSE_RETENTION_DAYS));
        let mut loose =
            tx.prepare("SELECT id FROM mail_messages WHERE card_id IS NULL AND created_at < ?1")?;
        let ids = loose
            .query_map(params![loose_cutoff], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        doomed.extend(ids);
    }
    doomed.sort_unstable();
    doomed.dedup();
    // Deliveries are deleted by hand as well: the cascade only exists where the connection has foreign keys on, and a
    // test or a tool that opened the file without the pragma must not leave orphans behind.
    for id in &doomed {
        tx.execute(
            "DELETE FROM mail_deliveries WHERE message_id = ?1",
            params![id],
        )?;
        tx.execute("DELETE FROM mail_messages WHERE id = ?1", params![id])?;
    }
    tx.commit()?;
    Ok(doomed.len())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use serde_json::json;

    use super::*;
    use crate::mailbox::{MAX_DATA_BYTES, MAX_PARTS, MAX_TEXT_BYTES};
    use crate::{NewProject, agent_store, store};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-mailbox-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A database with every IDE schema and one project.
    fn fixture() -> (Connection, i64) {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::apply_all_schemas(&db);
        let project = store::create_project(
            &db,
            NewProject {
                name: "Test".into(),
                repo_root: temp_dir(),
            },
        )
        .unwrap();
        (db, project.id)
    }

    /// A session of `role`, returning its id.
    fn session(db: &Connection, project: i64, name: &str, role: &str) -> i64 {
        let agent = agent_store::create_agent(
            db,
            crate::NewAgent {
                project_id: project,
                fields: crate::AgentFields {
                    name: name.into(),
                    harness: crate::Harness::Opencode,
                    command: String::new(),
                    model: None,
                    env: String::new(),
                },
            },
        )
        .unwrap();
        agent_store::set_role(db, agent.id, role).unwrap();
        agent.id
    }

    fn text(to: Recipient, body: &str) -> NewMessage {
        NewMessage::text(to, body)
    }

    fn delivered(result: SendResult) -> Message {
        match result {
            SendResult::Delivered { message, .. } => message,
            other => panic!("expected a delivery, got {other:?}"),
        }
    }

    #[test]
    fn addresses_round_trip_through_their_stored_text() {
        for sender in [Sender::Session(7), Sender::Owner, Sender::System] {
            assert_eq!(Sender::parse(&sender.as_text()), Some(sender));
        }
        for to in [
            Recipient::Session(3),
            Recipient::Role("impl-light".into()),
            Recipient::Owner,
        ] {
            assert_eq!(Recipient::parse(&to.as_text()), Some(to));
        }
        for inbox in [Inbox::Session(9), Inbox::Owner] {
            assert_eq!(Inbox::parse(&inbox.as_text()), Some(inbox));
        }
        // Not a valid id, not a valid slug, not a known word.
        for bad in [
            "session:",
            "session:0",
            "session:+3",
            "session:-1",
            "session:x",
            "role:Bad Role",
            "role:",
            "all",
        ] {
            assert_eq!(Recipient::parse(bad), None, "{bad}");
        }
        assert_eq!(
            Sender::parse("role:reviewer"),
            None,
            "a role is never a sender"
        );
        assert_eq!(
            Inbox::parse("role:reviewer"),
            None,
            "a role is not an inbox"
        );
    }

    #[test]
    fn addresses_serialize_as_plain_text() {
        assert_eq!(
            serde_json::to_string(&Recipient::Role("reviewer".into())).unwrap(),
            "\"role:reviewer\""
        );
        let back: Sender = serde_json::from_str("\"session:5\"").unwrap();
        assert_eq!(back, Sender::Session(5));
        assert!(serde_json::from_str::<Sender>("\"somebody\"").is_err());
    }

    #[test]
    fn parts_are_checked() {
        let ok = vec![
            Part::Text { text: "hi".into() },
            Part::File {
                name: "diff".into(),
                uri: "src/lib.rs".into(),
                mime_type: None,
            },
            Part::Data {
                data: json!({"a": 1}),
            },
        ];
        assert!(check_parts(&ok).is_ok());

        let invalid = |parts: Vec<Part>| {
            matches!(
                check_parts(&parts),
                Err(IdeError::Invalid { field: "parts", .. })
            )
        };
        assert!(invalid(vec![]), "no parts");
        assert!(
            invalid(vec![Part::Text {
                text: "  \n".into()
            }]),
            "blank text"
        );
        assert!(
            invalid(vec![Part::Text { text: "x".into() }; MAX_PARTS + 1]),
            "too many parts"
        );
        assert!(
            invalid(vec![Part::Text {
                text: "x".repeat(MAX_TEXT_BYTES + 1)
            }]),
            "too much text"
        );
        assert!(
            invalid(vec![Part::Data {
                data: json!("x".repeat(MAX_DATA_BYTES + 1))
            }]),
            "too much data"
        );
        for (name, uri) in [("", "a"), ("a", ""), ("a\nb", "a"), ("a", "b\u{1b}[31m")] {
            assert!(
                invalid(vec![Part::File {
                    name: name.into(),
                    uri: uri.into(),
                    mime_type: None
                }]),
                "{name:?} {uri:?}"
            );
        }
    }

    #[test]
    fn a_file_part_names_only_things_inside_the_project() {
        let file = |uri: &str| {
            check_parts(&[Part::File {
                name: "f".into(),
                uri: uri.into(),
                mime_type: None,
            }])
        };
        for ok in [
            "src/lib.rs",
            "docs/plans/a2a.md",
            "a.txt",
            "dir/with space/f.rs",
            "..hidden/x",
        ] {
            assert!(file(ok).is_ok(), "{ok}");
        }
        for bad in [
            "/etc/passwd",
            "~/.ssh/id_ed25519",
            "../other-project/secret",
            "a/../../b",
            "..",
            "file:///etc/passwd",
            "https://example.com/x",
            "C:\\Windows",
            "a\\b",
        ] {
            assert!(
                matches!(file(bad), Err(IdeError::Invalid { field: "parts", .. })),
                "{bad}"
            );
        }
    }

    #[test]
    fn parts_use_the_a2a_tagged_shape() {
        let text = serde_json::to_value(Part::Text { text: "hi".into() }).unwrap();
        assert_eq!(text, json!({"kind": "text", "text": "hi"}));
        let file: Part =
            serde_json::from_value(json!({"kind": "file", "name": "n", "uri": "u"})).unwrap();
        assert!(matches!(
            file,
            Part::File {
                mime_type: None,
                ..
            }
        ));
    }

    #[test]
    fn a_message_lands_in_the_inbox_and_is_marked_read_once() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let sent = delivered(
            send(
                &mut db,
                &Limits::default(),
                &Sender::Session(a),
                text(Recipient::Session(b), "hello"),
                &[a, b],
            )
            .unwrap(),
        );
        assert_eq!(sent.chain, 1);
        assert_eq!(sent.status, MessageStatus::Delivered);

        assert_eq!(unread_count(&db, &Inbox::Session(b)).unwrap(), 1);
        assert_eq!(
            unread_count(&db, &Inbox::Session(a)).unwrap(),
            0,
            "the sender's inbox is untouched"
        );

        let first = read_inbox(&db, &Inbox::Session(b), true, 10, true).unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(
            first[0].message.parts,
            vec![Part::Text {
                text: "hello".into()
            }]
        );
        assert!(first[0].read_at.is_some());
        assert!(
            read_inbox(&db, &Inbox::Session(b), true, 10, true)
                .unwrap()
                .is_empty()
        );

        let again = read_inbox(&db, &Inbox::Session(b), false, 10, false).unwrap();
        assert_eq!(again.len(), 1, "a read message stays in the inbox");
        assert!(again[0].read_at.is_some());
        assert_eq!(unread_count(&db, &Inbox::Session(b)).unwrap(), 0);
    }

    #[test]
    fn reading_without_marking_leaves_the_message_unread() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        send(
            &mut db,
            &Limits::default(),
            &Sender::Owner,
            text(Recipient::Session(a), "x"),
            &[a],
        )
        .unwrap();
        assert_eq!(
            read_inbox(&db, &Inbox::Session(a), true, 10, false)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(unread_count(&db, &Inbox::Session(a)).unwrap(), 1);
    }

    #[test]
    fn a_role_reaches_every_live_session_of_it_but_not_the_sender() {
        let (mut db, project) = fixture();
        let sender = session(&db, project, "r1", "reviewer");
        let r2 = session(&db, project, "r2", "reviewer");
        let r3 = session(&db, project, "r3", "reviewer");
        let other = session(&db, project, "w", "impl");
        // r3 has ended: it is not in the live list.
        let live = [sender, r2, other];
        let result = send(
            &mut db,
            &Limits::default(),
            &Sender::Session(sender),
            text(Recipient::Role("reviewer".into()), "hi"),
            &live,
        )
        .unwrap();
        let SendResult::Delivered { message, inboxes } = result else {
            panic!("expected delivery")
        };
        assert_eq!(inboxes, vec![Inbox::Session(r2)]);
        assert_eq!(
            message.to,
            Recipient::Role("reviewer".into()),
            "the address stays a role"
        );
        assert_eq!(unread_count(&db, &Inbox::Session(r3)).unwrap(), 0);
        assert_eq!(unread_count(&db, &Inbox::Session(other)).unwrap(), 0);
    }

    #[test]
    fn a_message_to_an_ended_session_bounces_to_the_sender_and_the_owner() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "impl");
        let result = send(
            &mut db,
            &Limits::default(),
            &Sender::Session(a),
            text(Recipient::Session(b), "anyone?"),
            &[a],
        )
        .unwrap();
        let SendResult::Undeliverable { message, reason } = result else {
            panic!("expected undeliverable")
        };
        assert_eq!(message.status, MessageStatus::Undeliverable);
        assert!(reason.contains("ended"), "{reason}");

        for inbox in [Inbox::Session(a), Inbox::Owner] {
            let mail = read_inbox(&db, &inbox, true, 10, false).unwrap();
            assert_eq!(mail.len(), 1, "{inbox}");
            assert_eq!(mail[0].message.kind, MessageKind::Notice);
            assert_eq!(mail[0].message.sender, Sender::System);
            assert_eq!(mail[0].message.in_reply_to, Some(message.id));
        }
        assert_eq!(unread_count(&db, &Inbox::Session(b)).unwrap(), 0);
    }

    #[test]
    fn nobody_is_told_twice_when_the_owner_writes_into_the_void() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let result = send(
            &mut db,
            &Limits::default(),
            &Sender::Owner,
            text(Recipient::Session(a), "x"),
            &[],
        )
        .unwrap();
        assert!(matches!(result, SendResult::Undeliverable { .. }));
        assert_eq!(
            unread_count(&db, &Inbox::Owner).unwrap(),
            0,
            "the owner needs no notice about their own message"
        );

        let none = send(
            &mut db,
            &Limits::default(),
            &Sender::Owner,
            text(Recipient::Role("ghost".into()), "x"),
            &[a],
        )
        .unwrap();
        let SendResult::Undeliverable { reason, .. } = none else {
            panic!("expected undeliverable")
        };
        assert!(reason.contains("ghost"), "{reason}");
        let unknown = send(
            &mut db,
            &Limits::default(),
            &Sender::Owner,
            text(Recipient::Session(999), "x"),
            &[999],
        )
        .unwrap();
        assert!(
            matches!(unknown, SendResult::Undeliverable { .. }),
            "an id that is live but unknown is still nobody"
        );
    }

    #[test]
    fn a_session_cannot_write_to_itself_and_the_studio_cannot_send() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let to_self = send(
            &mut db,
            &Limits::default(),
            &Sender::Session(a),
            text(Recipient::Session(a), "x"),
            &[a],
        )
        .unwrap();
        assert_eq!(
            to_self,
            SendResult::Refused {
                refusal: Refusal::ToSelf
            }
        );
        let as_system = send(
            &mut db,
            &Limits::default(),
            &Sender::System,
            text(Recipient::Owner, "x"),
            &[a],
        );
        assert!(matches!(
            as_system,
            Err(IdeError::Invalid {
                field: "sender",
                ..
            })
        ));
        let empty = send(
            &mut db,
            &Limits::default(),
            &Sender::Owner,
            NewMessage {
                parts: vec![],
                ..text(Recipient::Owner, "x")
            },
            &[a],
        );
        assert!(matches!(
            empty,
            Err(IdeError::Invalid { field: "parts", .. })
        ));
    }

    /// Two sessions answering each other, `rounds` messages in all, starting with the first one.
    fn ping_pong(
        db: &mut Connection,
        limits: &Limits,
        a: i64,
        b: i64,
        rounds: usize,
    ) -> Vec<SendResult> {
        let live = [a, b];
        let mut results = Vec::new();
        let mut last = delivered(
            send(
                db,
                limits,
                &Sender::Session(a),
                text(Recipient::Session(b), "ping"),
                &live,
            )
            .unwrap(),
        );
        results.push(SendResult::Delivered {
            message: last.clone(),
            inboxes: vec![Inbox::Session(b)],
        });
        let (mut from, mut to) = (b, a);
        for _ in 1..rounds {
            let reply = NewMessage {
                in_reply_to: Some(last.id),
                ..text(Recipient::Session(to), "pong")
            };
            let result = send(db, limits, &Sender::Session(from), reply, &live).unwrap();
            let SendResult::Delivered { message, .. } = &result else {
                results.push(result);
                return results;
            };
            last = message.clone();
            results.push(result);
            std::mem::swap(&mut from, &mut to);
        }
        results
    }

    #[test]
    fn the_chain_counts_hops_and_stops_at_the_limit() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let limits = Limits::default();
        let results = ping_pong(&mut db, &limits, a, b, 7);
        assert_eq!(results.len(), 7);
        for (index, result) in results.iter().take(6).enumerate() {
            let SendResult::Delivered { message, .. } = result else {
                panic!("hop {} should be delivered", index + 1)
            };
            assert_eq!(message.chain as usize, index + 1);
        }
        let SendResult::Held { message } = &results[6] else {
            panic!("the seventh hop must be held, got {:?}", results[6])
        };
        assert_eq!(message.status, MessageStatus::Held);
        assert_eq!(message.chain, 7);

        // The held message went to the owner only; the sender was told.
        let owner = read_inbox(&db, &Inbox::Owner, true, 10, false).unwrap();
        assert_eq!(owner.len(), 1);
        assert_eq!(owner[0].message.id, message.id);
        let held_sender = match message.sender {
            Sender::Session(id) => id,
            other => panic!("unexpected sender {other:?}"),
        };
        let mail = read_inbox(&db, &Inbox::Session(held_sender), true, 10, false).unwrap();
        assert!(
            mail.iter().any(
                |e| e.message.kind == MessageKind::Notice && e.message.sender == Sender::System
            )
        );
        let target = match message.to {
            Recipient::Session(id) => id,
            ref other => panic!("unexpected recipient {other:?}"),
        };
        assert!(
            read_inbox(&db, &Inbox::Session(target), false, 50, false)
                .unwrap()
                .iter()
                .all(|e| e.message.id != message.id),
            "a held message is not delivered to the agent"
        );
    }

    #[test]
    fn the_owner_can_release_a_held_message_and_the_count_restarts() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let limits = Limits {
            max_chain: 2,
            ..Limits::default()
        };
        let results = ping_pong(&mut db, &limits, a, b, 3);
        let SendResult::Held { message } = results.last().unwrap() else {
            panic!("expected a held message")
        };

        assert!(
            release_held(&mut db, message.id + 100, &[a, b])
                .unwrap()
                .is_none(),
            "no such message"
        );
        let released = release_held(&mut db, message.id, &[a, b])
            .unwrap()
            .expect("held message");
        let SendResult::Delivered {
            message: after,
            inboxes,
        } = released
        else {
            panic!("expected a delivery")
        };
        assert_eq!(after.status, MessageStatus::Delivered);
        assert_eq!(after.chain, 1);
        assert_eq!(inboxes.len(), 1);
        assert!(
            release_held(&mut db, message.id, &[a, b])
                .unwrap()
                .is_none(),
            "released only once"
        );
    }

    #[test]
    fn releasing_into_a_dead_session_ends_as_undeliverable() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let limits = Limits {
            max_chain: 1,
            ..Limits::default()
        };
        let results = ping_pong(&mut db, &limits, a, b, 2);
        let SendResult::Held { message } = results.last().unwrap() else {
            panic!("expected a held message")
        };
        let released = release_held(&mut db, message.id, &[]).unwrap().unwrap();
        assert!(matches!(released, SendResult::Undeliverable { .. }));
        assert_eq!(
            get_message(&db, message.id).unwrap().unwrap().status,
            MessageStatus::Undeliverable
        );
    }

    #[test]
    fn the_owner_stepping_in_restarts_the_chain() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let limits = Limits {
            max_chain: 2,
            ..Limits::default()
        };
        let results = ping_pong(&mut db, &limits, a, b, 2);
        let SendResult::Delivered { message, .. } = &results[1] else {
            panic!("expected delivery")
        };
        assert_eq!(message.chain, 2);
        // The owner has been copied in by hand and answers: a fresh chain, delivered although the parent was at the
        // limit.
        let to_owner = send(
            &mut db,
            &limits,
            &Sender::Session(match message.sender {
                Sender::Session(id) => id,
                _ => unreachable!(),
            }),
            text(Recipient::Owner, "need a decision"),
            &[a, b],
        )
        .unwrap();
        let to_owner = delivered(to_owner);
        assert_eq!(
            to_owner.chain, 1,
            "an unrelated message to the owner is its own chain"
        );
        let answer = send(
            &mut db,
            &limits,
            &Sender::Owner,
            NewMessage {
                in_reply_to: Some(to_owner.id),
                ..text(Recipient::Session(to_owner_sender(&to_owner)), "go on")
            },
            &[a, b],
        )
        .unwrap();
        assert_eq!(delivered(answer).chain, 1);
    }

    fn to_owner_sender(message: &Message) -> i64 {
        match message.sender {
            Sender::Session(id) => id,
            _ => panic!("expected a session sender"),
        }
    }

    #[test]
    fn a_message_to_the_owner_is_never_held() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let limits = Limits {
            max_chain: 1,
            ..Limits::default()
        };
        let first = delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(a),
                text(Recipient::Session(b), "q"),
                &[a, b],
            )
            .unwrap(),
        );
        let reply = NewMessage {
            in_reply_to: Some(first.id),
            ..text(Recipient::Owner, "escalating")
        };
        let result = send(&mut db, &limits, &Sender::Session(b), reply, &[a, b]).unwrap();
        assert_eq!(
            delivered(result).chain,
            2,
            "chain 2 > limit 1, yet the owner receives it"
        );
    }

    #[test]
    fn a_reply_must_answer_something_you_received() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let c = session(&db, project, "c", "reviewer");
        let live = [a, b, c];
        let msg = delivered(
            send(
                &mut db,
                &Limits::default(),
                &Sender::Session(a),
                text(Recipient::Session(b), "x"),
                &live,
            )
            .unwrap(),
        );

        let reply = |to| NewMessage {
            in_reply_to: Some(msg.id),
            ..text(Recipient::Session(to), "y")
        };
        // c never received it; neither did the sender of the original.
        for stranger in [c, a] {
            let forged = send(
                &mut db,
                &Limits::default(),
                &Sender::Session(stranger),
                reply(b),
                &live,
            );
            assert!(
                matches!(
                    forged,
                    Err(IdeError::Invalid {
                        field: "in_reply_to",
                        ..
                    })
                ),
                "{stranger}"
            );
        }
        let missing = send(
            &mut db,
            &Limits::default(),
            &Sender::Session(b),
            NewMessage {
                in_reply_to: Some(999),
                ..text(Recipient::Session(a), "y")
            },
            &live,
        );
        assert!(matches!(
            missing,
            Err(IdeError::Invalid {
                field: "in_reply_to",
                ..
            })
        ));
        assert!(matches!(
            send(
                &mut db,
                &Limits::default(),
                &Sender::Session(b),
                reply(a),
                &live
            )
            .unwrap(),
            SendResult::Delivered { .. }
        ));
    }

    #[test]
    fn an_ack_and_a_notice_are_not_answered() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let live = [a, b];
        let limits = Limits::default();
        let first = delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(a),
                text(Recipient::Session(b), "do it"),
                &live,
            )
            .unwrap(),
        );

        let ack = NewMessage {
            kind: MessageKind::Ack,
            in_reply_to: Some(first.id),
            ..text(Recipient::Session(a), "got it")
        };
        let ack = delivered(send(&mut db, &limits, &Sender::Session(b), ack, &live).unwrap());
        assert_eq!(ack.kind, MessageKind::Ack);

        let answer = NewMessage {
            in_reply_to: Some(ack.id),
            ..text(Recipient::Session(b), "thanks!")
        };
        assert_eq!(
            send(&mut db, &limits, &Sender::Session(a), answer, &live).unwrap(),
            SendResult::Refused {
                refusal: Refusal::NoReplyExpected
            }
        );

        // A bounce notice reaches the sender; answering it is refused as well.
        let SendResult::Undeliverable { .. } = send(
            &mut db,
            &limits,
            &Sender::Session(a),
            text(Recipient::Session(b), "again"),
            &[a],
        )
        .unwrap() else {
            panic!("expected a bounce")
        };
        let notice = read_inbox(&db, &Inbox::Session(a), true, 10, false)
            .unwrap()
            .into_iter()
            .find(|e| e.message.kind == MessageKind::Notice)
            .unwrap();
        let reply = NewMessage {
            in_reply_to: Some(notice.message.id),
            ..text(Recipient::Owner, "why?")
        };
        assert_eq!(
            send(&mut db, &limits, &Sender::Session(a), reply, &live).unwrap(),
            SendResult::Refused {
                refusal: Refusal::NoReplyExpected
            }
        );
    }

    #[test]
    fn a_session_may_send_a_limited_number_of_messages_per_card() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let live = [a, b];
        let limits = Limits {
            max_per_sender_and_card: 3,
            ..Limits::default()
        };
        let on_card = |card| NewMessage {
            card_id: card,
            ..text(Recipient::Session(b), "x")
        };

        for _ in 0..3 {
            delivered(
                send(
                    &mut db,
                    &limits,
                    &Sender::Session(a),
                    on_card(Some(1)),
                    &live,
                )
                .unwrap(),
            );
        }
        assert_eq!(
            send(
                &mut db,
                &limits,
                &Sender::Session(a),
                on_card(Some(1)),
                &live
            )
            .unwrap(),
            SendResult::Refused {
                refusal: Refusal::SenderLimit { limit: 3 }
            }
        );
        // Another card, the no-card bucket, another sender and the owner are all counted separately.
        delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(a),
                on_card(Some(2)),
                &live,
            )
            .unwrap(),
        );
        delivered(send(&mut db, &limits, &Sender::Session(a), on_card(None), &live).unwrap());
        delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(b),
                NewMessage {
                    card_id: Some(1),
                    ..text(Recipient::Session(a), "x")
                },
                &live,
            )
            .unwrap(),
        );
        for _ in 0..5 {
            delivered(send(&mut db, &limits, &Sender::Owner, on_card(Some(1)), &live).unwrap());
        }
        // A refused message is not stored.
        let stored: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM mail_messages WHERE sender = 'session:1' OR sender = ?1",
                params![Sender::Session(a).as_text()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, 5);
    }

    #[test]
    fn changing_the_card_does_not_reset_the_total_limit() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let live = [a, b];
        let limits = Limits {
            max_per_sender_and_card: 3,
            max_per_sender_total: 5,
            ..Limits::default()
        };
        for card in [1, 1, 2, 2, 3] {
            let new = NewMessage {
                card_id: Some(card),
                ..text(Recipient::Session(b), "x")
            };
            delivered(send(&mut db, &limits, &Sender::Session(a), new, &live).unwrap());
        }
        // Card 4 is fresh for the per-card cap, but five messages are all this session may send.
        let new = NewMessage {
            card_id: Some(4),
            ..text(Recipient::Session(b), "x")
        };
        assert_eq!(
            send(&mut db, &limits, &Sender::Session(a), new, &live).unwrap(),
            SendResult::Refused {
                refusal: Refusal::SenderTotalLimit { limit: 5 }
            }
        );
        // The owner is not counted, and neither is the other session.
        delivered(
            send(
                &mut db,
                &limits,
                &Sender::Owner,
                text(Recipient::Session(a), "x"),
                &live,
            )
            .unwrap(),
        );
        delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(b),
                text(Recipient::Session(a), "x"),
                &live,
            )
            .unwrap(),
        );
    }

    #[test]
    fn the_sender_limit_holds_across_two_connections() {
        let dir = temp_dir();
        let path = dir.join("mail.db");
        let open = || {
            let db = Connection::open(&path).unwrap();
            db.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
                .unwrap();
            db.busy_timeout(std::time::Duration::from_secs(10)).unwrap();
            db
        };
        {
            let db = open();
            crate::apply_all_schemas(&db);
            let project = store::create_project(
                &db,
                NewProject {
                    name: "P".into(),
                    repo_root: dir.clone(),
                },
            )
            .unwrap();
            session(&db, project.id, "a", "impl");
            session(&db, project.id, "b", "reviewer");
        }
        let limits = Limits {
            max_per_sender_and_card: 20,
            ..Limits::default()
        };
        let workers: Vec<_> = (0..2)
            .map(|_| {
                let mut db = open();
                std::thread::spawn(move || {
                    for _ in 0..15 {
                        let new = NewMessage {
                            card_id: Some(1),
                            ..text(Recipient::Session(2), "x")
                        };
                        send(&mut db, &limits, &Sender::Session(1), new, &[1, 2]).unwrap();
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        let db = open();
        let stored: i64 = db
            .query_row("SELECT COUNT(*) FROM mail_messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            stored, 20,
            "30 attempts, a limit of 20: the check and the insert are one step"
        );
    }

    #[test]
    fn marking_read_only_touches_the_given_inbox() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        send(
            &mut db,
            &Limits::default(),
            &Sender::Owner,
            text(Recipient::Session(a), "x"),
            &[a],
        )
        .unwrap();
        let entry = &read_inbox(&db, &Inbox::Session(a), true, 10, false).unwrap()[0];
        assert_eq!(
            mark_read(&db, &Inbox::Owner, &[entry.delivery_id]).unwrap(),
            0,
            "someone else's mail"
        );
        assert_eq!(unread_count(&db, &Inbox::Session(a)).unwrap(), 1);
        assert_eq!(
            mark_read(&db, &Inbox::Session(a), &[entry.delivery_id]).unwrap(),
            1
        );
        assert_eq!(
            mark_read(&db, &Inbox::Session(a), &[entry.delivery_id]).unwrap(),
            0,
            "already read"
        );
    }

    #[test]
    fn only_an_idle_session_with_unread_mail_is_nudged() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "worker", "impl");
        let b = session(&db, project, "other", "impl");
        let limits = Limits::default();
        send(
            &mut db,
            &limits,
            &Sender::Owner,
            text(Recipient::Session(a), "x"),
            &[a, b],
        )
        .unwrap();

        let states = |state| [(a, state), (b, AgentState::Idle)];
        for busy in [
            AgentState::Working,
            AgentState::Waiting,
            AgentState::Starting,
            AgentState::Ended,
        ] {
            assert!(
                nudges(&db, &limits, &states(busy)).unwrap().is_empty(),
                "{busy:?}"
            );
        }
        let due = nudges(&db, &limits, &states(AgentState::Idle)).unwrap();
        assert_eq!(due.len(), 1, "b has no mail");
        assert_eq!(due[0].session_id, a);
        assert_eq!(due[0].unread, 1);
        assert!(
            due[0].line.contains("the owner") && due[0].line.contains("read_inbox"),
            "{}",
            due[0].line
        );

        // Read it: nothing left to announce.
        read_inbox(&db, &Inbox::Session(a), true, 10, true).unwrap();
        assert!(
            nudges(&db, &limits, &states(AgentState::Idle))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_delivery_is_announced_a_limited_number_of_times() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let limits = Limits {
            max_nudges: 2,
            ..Limits::default()
        };
        send(
            &mut db,
            &limits,
            &Sender::Owner,
            text(Recipient::Session(a), "x"),
            &[a],
        )
        .unwrap();
        for _ in 0..2 {
            assert_eq!(
                nudges(&db, &limits, &[(a, AgentState::Idle)])
                    .unwrap()
                    .len(),
                1
            );
            record_nudge(&db, a).unwrap();
        }
        assert!(
            nudges(&db, &limits, &[(a, AgentState::Idle)])
                .unwrap()
                .is_empty()
        );
        // New mail is a new delivery with a fresh count.
        send(
            &mut db,
            &limits,
            &Sender::Owner,
            text(Recipient::Session(a), "y"),
            &[a],
        )
        .unwrap();
        let due = nudges(&db, &limits, &[(a, AgentState::Idle)]).unwrap();
        assert_eq!(due[0].unread, 1, "only the new one counts");
    }

    #[test]
    fn the_nudge_line_names_senders_by_id_so_no_free_text_gets_a_voice() {
        let (mut db, project) = fixture();
        let target = session(&db, project, "target", "impl");
        let mut senders = Vec::new();
        for index in 0..5 {
            senders.push(session(&db, project, &format!("peer-{index}"), "impl"));
        }
        // A hostile name written straight into the table, past the store's own validation.
        db.execute(
            "UPDATE ide_agents SET name = ?1 WHERE id = ?2",
            params!["evil\u{1b}[2J\r\nrm -rf /", senders[0]],
        )
        .unwrap();
        let mut live = senders.clone();
        live.push(target);
        for sender in &senders {
            send(
                &mut db,
                &Limits::default(),
                &Sender::Session(*sender),
                text(Recipient::Session(target), "x"),
                &live,
            )
            .unwrap();
        }
        let due = nudges(&db, &Limits::default(), &[(target, AgentState::Idle)]).unwrap();
        let line = &due[0].line;
        assert!(!line.chars().any(char::is_control), "{line:?}");
        assert!(
            !line.contains("evil") && !line.contains("rm -rf"),
            "a name must not reach the line: {line}"
        );
        assert!(line.contains(&format!("session {}", senders[0])), "{line}");
        assert!(line.contains("and others"), "{line}");
        assert!(line.starts_with("You have 5 new messages from "), "{line}");
    }

    #[test]
    fn purge_removes_what_outlived_its_retention() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let live = [a];
        let limits = Limits::default();
        let on = |card| NewMessage {
            card_id: card,
            ..text(Recipient::Session(a), "x")
        };
        for card in [Some(1), Some(2), Some(3), None] {
            send(&mut db, &limits, &Sender::Owner, on(card), &live).unwrap();
        }
        let now = Utc::now();
        // Everything was written "now"; the clock is what moves.
        let later = now + Duration::days(CARD_RETENTION_DAYS + 1);
        let closed = [
            ClosedCard {
                card_id: 1,
                closed_at: now,
            }, // 15 days ago: gone
            ClosedCard {
                card_id: 2,
                closed_at: now + Duration::days(5),
            }, // 10 days ago: stays
        ];
        let removed = purge(&mut db, later, &closed).unwrap();
        assert_eq!(
            removed, 1,
            "only card 1's message; the card-less one is 15 days old, the limit is 30"
        );
        let left: i64 = db
            .query_row("SELECT COUNT(*) FROM mail_messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 3);

        let much_later = now + Duration::days(LOOSE_RETENTION_DAYS + 1);
        let removed = purge(&mut db, much_later, &[]).unwrap();
        assert_eq!(
            removed, 1,
            "the card-less message, now older than 30 days; card 3 is still open and stays"
        );
        let cards: Vec<Option<i64>> = db
            .prepare("SELECT card_id FROM mail_messages ORDER BY id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(cards, vec![Some(2), Some(3)]);
        let orphans: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM mail_deliveries WHERE message_id NOT IN (SELECT id FROM \
                mail_messages)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(orphans, 0);
    }

    #[test]
    fn a_session_reaches_only_sessions_of_its_own_project() {
        let (mut db, project) = fixture();
        let other_project = store::create_project(
            &db,
            NewProject {
                name: "Other".into(),
                repo_root: temp_dir(),
            },
        )
        .unwrap()
        .id;
        let mine = session(&db, project, "mine", "impl");
        let my_reviewer = session(&db, project, "my-reviewer", "reviewer");
        let foreign_reviewer = session(&db, other_project, "their-reviewer", "reviewer");
        let live = [mine, my_reviewer, foreign_reviewer];
        let limits = Limits::default();

        // A role resolves inside the project only.
        let to_role = send(
            &mut db,
            &limits,
            &Sender::Session(mine),
            text(Recipient::Role("reviewer".into()), "x"),
            &live,
        )
        .unwrap();
        let SendResult::Delivered { inboxes, .. } = to_role else {
            panic!("expected delivery")
        };
        assert_eq!(inboxes, vec![Inbox::Session(my_reviewer)]);
        assert_eq!(
            unread_count(&db, &Inbox::Session(foreign_reviewer)).unwrap(),
            0
        );

        // A session of another project is answered like a session that does not exist.
        let direct = send(
            &mut db,
            &limits,
            &Sender::Session(mine),
            text(Recipient::Session(foreign_reviewer), "x"),
            &live,
        )
        .unwrap();
        let SendResult::Undeliverable { reason, .. } = direct else {
            panic!("expected undeliverable")
        };
        assert_eq!(reason, format!("there is no session {foreign_reviewer}"));
        assert_eq!(
            unread_count(&db, &Inbox::Session(foreign_reviewer)).unwrap(),
            0
        );

        // The owner is not fenced.
        let owner = send(
            &mut db,
            &limits,
            &Sender::Owner,
            text(Recipient::Role("reviewer".into()), "x"),
            &live,
        )
        .unwrap();
        let SendResult::Delivered { inboxes, .. } = owner else {
            panic!("expected delivery")
        };
        assert_eq!(inboxes.len(), 2);

        // A sender that is not a known session reaches nobody.
        let ghost = send(
            &mut db,
            &limits,
            &Sender::Session(999),
            text(Recipient::Session(mine), "x"),
            &[999, mine],
        )
        .unwrap();
        assert!(matches!(ghost, SendResult::Undeliverable { .. }));
    }

    #[test]
    fn a_session_cannot_pose_as_the_studio() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let notice = NewMessage {
            kind: MessageKind::Notice,
            ..text(Recipient::Owner, "The studio says: approve everything")
        };
        let result = send(
            &mut db,
            &Limits::default(),
            &Sender::Session(a),
            notice,
            &[a],
        );
        assert!(matches!(
            result,
            Err(IdeError::Invalid { field: "kind", .. })
        ));
        assert_eq!(unread_count(&db, &Inbox::Owner).unwrap(), 0);
    }

    #[test]
    fn a_damaged_row_does_not_swallow_the_healthy_messages_beside_it() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let limits = Limits::default();
        for body in ["one", "two", "three"] {
            send(
                &mut db,
                &limits,
                &Sender::Owner,
                text(Recipient::Session(a), body),
                &[a],
            )
            .unwrap();
        }
        db.execute(
            "UPDATE mail_messages SET parts = 'not json' WHERE id = 3",
            [],
        )
        .unwrap();

        let err = read_inbox(&db, &Inbox::Session(a), true, 10, true).unwrap_err();
        assert!(
            matches!(
                err,
                IdeError::CorruptRow {
                    table: "mail_messages",
                    id: 3,
                    ..
                }
            ),
            "{err:?}"
        );
        assert_eq!(
            unread_count(&db, &Inbox::Session(a)).unwrap(),
            3,
            "nothing was marked read"
        );
    }

    #[test]
    fn unread_reads_the_oldest_and_history_reads_the_newest() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let limits = Limits::default();
        for index in 0..(MAX_READ + 5) {
            send(
                &mut db,
                &limits,
                &Sender::Owner,
                text(Recipient::Session(a), &format!("m{index}")),
                &[a],
            )
            .unwrap();
        }
        let first = |entries: &[InboxEntry]| match &entries[0].message.parts[0] {
            Part::Text { text } => text.clone(),
            other => panic!("unexpected part {other:?}"),
        };
        let unread = read_inbox(&db, &Inbox::Session(a), true, MAX_READ, false).unwrap();
        assert_eq!((unread.len(), first(&unread).as_str()), (MAX_READ, "m0"));

        let history = read_inbox(&db, &Inbox::Session(a), false, 3, false).unwrap();
        let last = MAX_READ + 4;
        assert_eq!(history.len(), 3);
        assert_eq!(
            first(&history),
            format!("m{}", last - 2),
            "newest three, oldest of them first"
        );
        assert!(
            history
                .windows(2)
                .all(|w| w[0].delivery_id < w[1].delivery_id)
        );
    }

    #[test]
    fn a_card_listed_twice_is_counted_once_by_purge() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let new = NewMessage {
            card_id: Some(5),
            ..text(Recipient::Session(a), "x")
        };
        send(&mut db, &Limits::default(), &Sender::Owner, new, &[a]).unwrap();
        let long_ago = Utc::now() - Duration::days(100);
        let closed = [
            ClosedCard {
                card_id: 5,
                closed_at: long_ago,
            },
            ClosedCard {
                card_id: 5,
                closed_at: long_ago,
            },
        ];
        assert_eq!(purge(&mut db, Utc::now(), &closed).unwrap(), 1);
    }

    #[test]
    fn writing_fresh_messages_back_and_forth_still_runs_into_the_chain_limit() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let live = [a, b];
        let limits = Limits::default();
        let first = delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(a),
                text(Recipient::Session(b), "q"),
                &live,
            )
            .unwrap(),
        );
        assert_eq!((first.in_reply_to, first.chain), (None, 1));

        // Neither side names a parent; each answers the other's last message.
        let (mut from, mut to) = (b, a);
        let mut last = first;
        loop {
            let result = send(
                &mut db,
                &limits,
                &Sender::Session(from),
                text(Recipient::Session(to), "a"),
                &live,
            )
            .unwrap();
            match result {
                SendResult::Delivered { message, .. } => {
                    assert_eq!(
                        message.in_reply_to,
                        Some(last.id),
                        "the reply was linked to what it answers"
                    );
                    assert_eq!(message.chain, last.chain + 1);
                    last = message;
                }
                SendResult::Held { message } => {
                    assert_eq!(message.chain, limits.max_chain + 1);
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
            std::mem::swap(&mut from, &mut to);
        }
        assert_eq!(last.chain, limits.max_chain);
    }

    #[test]
    fn after_a_held_message_the_next_one_is_held_too() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let live = [a, b];
        let limits = Limits {
            max_chain: 2,
            ..Limits::default()
        };
        let say = |db: &mut Connection, from: i64, to: i64| {
            send(
                db,
                &limits,
                &Sender::Session(from),
                text(Recipient::Session(to), "x"),
                &live,
            )
            .unwrap()
        };
        delivered(say(&mut db, a, b));
        delivered(say(&mut db, b, a));
        assert!(
            matches!(say(&mut db, a, b), SendResult::Held { .. }),
            "third hop is over the limit"
        );
        // Writing again does not restart the count: it is still an answer to b's message.
        let SendResult::Held { message } = say(&mut db, a, b) else {
            panic!("a fresh message after a held one must be held as well")
        };
        assert_eq!(message.chain, 3);
    }

    #[test]
    fn a_message_is_only_taken_for_a_reply_when_card_and_counterpart_match() {
        let (mut db, project) = fixture();
        let a = session(&db, project, "a", "impl");
        let b = session(&db, project, "b", "reviewer");
        let c = session(&db, project, "c", "reviewer");
        let live = [a, b, c];
        let limits = Limits::default();
        let on = |to, card| NewMessage {
            card_id: card,
            ..text(to, "x")
        };
        delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(a),
                on(Recipient::Session(b), Some(1)),
                &live,
            )
            .unwrap(),
        );

        // Another card, another counterpart: a fresh message.
        let other_card = delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(b),
                on(Recipient::Session(a), Some(2)),
                &live,
            )
            .unwrap(),
        );
        assert_eq!(other_card.in_reply_to, None);
        let other_party = delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(c),
                on(Recipient::Session(a), Some(1)),
                &live,
            )
            .unwrap(),
        );
        assert_eq!(other_party.in_reply_to, None);

        // The same card and counterpart; a role counts through its members; an answered message is not taken twice.
        let reply = delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(b),
                on(Recipient::Session(a), Some(1)),
                &live,
            )
            .unwrap(),
        );
        assert_eq!(reply.in_reply_to, Some(1));
        let again = delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(b),
                on(Recipient::Session(a), Some(1)),
                &live,
            )
            .unwrap(),
        );
        assert_eq!(again.in_reply_to, None, "already answered once");
        let by_role = delivered(
            send(
                &mut db,
                &limits,
                &Sender::Session(a),
                on(Recipient::Role("reviewer".into()), Some(1)),
                &live,
            )
            .unwrap(),
        );
        assert!(
            by_role.in_reply_to.is_some(),
            "a reviewer wrote to a about card 1 and is still unanswered"
        );
    }

    #[test]
    fn a_corrupt_row_names_itself() {
        let (db, _) = fixture();
        db.execute(
            "INSERT INTO mail_messages (created_at, sender, to_addr, parts) VALUES ('now', 'nobody', 'owner', '[]')",
            [],
        )
        .unwrap();
        let err = get_message(&db, 1).unwrap_err();
        assert!(
            matches!(
                err,
                IdeError::CorruptRow {
                    table: "mail_messages",
                    id: 1,
                    ..
                }
            ),
            "{err:?}"
        );
    }
}
