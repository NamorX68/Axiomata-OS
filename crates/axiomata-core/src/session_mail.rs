//! A session's mail as the owner reads it, and the owner's own message to a session (`docs/plans/a2a.md`, CP-A9).
//!
//! The Inbox tab of a session and the Flow's team tiles show the conversation: what reached the session and what it wrote,
//! oldest first. [`conversation`] only reads — it never marks a delivery read, because that is the agent's own act
//! (`read_inbox`) and a message the owner merely looked at must still be handed to the agent. [`send_as_owner`] is the
//! owner's voice, the one sender the chain limit does not bind.

use axiomata_ide::mailbox::{
    self, Inbox, Message, NewMessage, Part, Recipient, SendResult, Sender,
};
use axiomata_ide::{model::Agent, presence};
use rusqlite::Connection;
use serde::Serialize;

use crate::AxiomataCore;
use crate::AxiomataError;
use crate::ide::agent_store;
use crate::paths;
use crate::roster::refusal;

/// How many messages of each direction one look returns: a tile shows a conversation's end, not its whole history.
const SHOWN: usize = 30;

/// One message of a session's conversation, in words for the Studio to show.
#[derive(Debug, Clone, Serialize)]
pub struct MailLine {
    pub id: i64,
    pub at: String,
    /// Who wrote it: a session's name, `Owner`, `Studio`.
    pub from: String,
    /// Who it was for: a session's name, `@role`, `Owner`.
    pub to: String,
    /// `true` when it reached this session, `false` when this session wrote it.
    pub incoming: bool,
    /// The card it is about.
    pub card_id: Option<i64>,
    /// `message`, `ack` or `notice`.
    pub kind: String,
    /// The text parts joined, and a line per file or data part.
    pub text: String,
    /// `delivered`, `held` or `undeliverable`.
    pub status: String,
    /// For an incoming one: the session has read it (`read_inbox`).
    pub read: bool,
}

fn name_of(db: &Connection, id: i64) -> String {
    agent_store::get_agent(db, id)
        .ok()
        .flatten()
        .map_or_else(|| format!("session #{id}"), |agent| agent.name)
}

fn from_label(db: &Connection, sender: &Sender) -> String {
    match sender {
        Sender::Session(id) => name_of(db, *id),
        Sender::Owner => "Owner".to_owned(),
        Sender::System => "Studio".to_owned(),
    }
}

fn to_label(db: &Connection, to: &Recipient) -> String {
    match to {
        Recipient::Session(id) => name_of(db, *id),
        Recipient::Role(role) => format!("@{role}"),
        Recipient::Owner => "Owner".to_owned(),
    }
}

/// The text of a message for reading: its text parts, and a line naming each file or data part.
fn text_of(parts: &[Part]) -> String {
    parts
        .iter()
        .map(|part| match part {
            Part::Text { text } => text.clone(),
            Part::File { name, .. } => format!("[file: {name}]"),
            Part::Data { .. } => "[data]".to_owned(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn line(db: &Connection, message: &Message, incoming: bool, read: bool) -> MailLine {
    MailLine {
        id: message.id,
        at: message.created_at.to_rfc3339(),
        from: from_label(db, &message.sender),
        to: to_label(db, &message.to),
        incoming,
        card_id: message.card_id,
        kind: serde_json::to_value(message.kind)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default(),
        text: text_of(&message.parts),
        status: serde_json::to_value(message.status)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default(),
        read,
    }
}

/// The conversation of session `agent_id`: what reached it and what it wrote, oldest first. Nothing is marked read.
///
/// # Errors
///
/// A database error, or a row the mailbox could not have written.
pub fn conversation(db: &Connection, agent_id: i64) -> Result<Vec<MailLine>, AxiomataError> {
    let mut lines: Vec<MailLine> =
        mailbox::read_inbox(db, &Inbox::Session(agent_id), false, SHOWN, false)?
            .iter()
            .map(|entry| line(db, &entry.message, true, entry.read_at.is_some()))
            .collect();
    lines.extend(
        mailbox::sent_by(db, &Sender::Session(agent_id), SHOWN)?
            .iter()
            .map(|message| line(db, message, false, true)),
    );
    lines.sort_by_key(|entry| entry.id);
    Ok(lines)
}

/// How many messages wait unread in session `agent_id`'s inbox.
///
/// # Errors
///
/// A database error.
pub fn unread(db: &Connection, agent_id: i64) -> Result<usize, AxiomataError> {
    Ok(mailbox::unread_count(db, &Inbox::Session(agent_id))?)
}

/// What the owner's message came to, in words.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum OwnerSend {
    /// It is in the session's inbox; the studio announces it in the terminal when the session waits for input.
    Delivered { id: i64 },
    /// It is stored, but the session has ended: nobody will read it.
    Undeliverable { id: i64, reason: String },
}

/// The owner writes to session `agent_id`, about the card it works on. The studio nudges the session's terminal once it is
/// idle (`ide_mailbox_nudge`); a session that is not running gets the message stored and the owner is told so.
///
/// # Errors
///
/// A refusal for an empty text, a session that does not exist, or a message the mailbox refused.
pub fn send_as_owner(
    core: &AxiomataCore,
    agent_id: i64,
    message: &str,
) -> Result<OwnerSend, AxiomataError> {
    if message.trim().is_empty() {
        return Err(refusal("message", "the message is empty".to_owned()));
    }
    let roots = paths::ide_locations().channels;
    // Looking for a running server is file work: done before the database is locked, not under it.
    let live = presence::live_sessions(&roots, &[agent_id]);
    let mut db = core.db_lock();
    let agent: Agent = agent_store::get_agent(&db, agent_id)?
        .ok_or_else(|| refusal("session", format!("no session {agent_id}")))?;
    let mut new = NewMessage::text(Recipient::Session(agent.id), message.trim());
    new.card_id = agent.card_id;
    match mailbox::send(
        &mut db,
        &mailbox::Limits::default(),
        &Sender::Owner,
        new,
        &live,
    )? {
        SendResult::Delivered { message, .. } | SendResult::Held { message } => {
            Ok(OwnerSend::Delivered { id: message.id })
        }
        SendResult::Undeliverable { message, reason } => Ok(OwnerSend::Undeliverable {
            id: message.id,
            reason,
        }),
        SendResult::Refused { refusal: why } => Err(refusal("message", why.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_parts_and_named_files_read_as_lines() {
        let parts = [
            Part::Text { text: "see".into() },
            Part::File {
                name: "a.rs".into(),
                uri: "src/a.rs".into(),
                mime_type: None,
            },
            Part::Data {
                data: serde_json::json!({"x": 1}),
            },
        ];
        assert_eq!(text_of(&parts), "see\n[file: a.rs]\n[data]");
    }

    fn db() -> (Connection, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-sessionmail-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (
            crate::db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap(),
            dir,
        )
    }

    #[test]
    fn a_conversation_has_both_directions_oldest_first_and_looking_marks_nothing_read() {
        let (mut conn, dir) = db();
        let limits = mailbox::Limits::default();
        // A session exists for the mailbox only as a row: the fence reads its project.
        conn.execute_batch(
            "INSERT INTO projects (id, name, repo_root, created_at)
                 VALUES (1, 'P', '/tmp/p', '2026-10-06T10:00:00Z');
             INSERT INTO ide_agents (id, project_id, name, harness, command, env, created_at, updated_at)
                 VALUES (7, 1, 'builder-7', 'claude_code', '', '', '2026-10-06T10:00:00Z', '2026-10-06T10:00:00Z');",
        )
        .unwrap();
        // The owner writes to session 7; session 7 answers the owner.
        let SendResult::Delivered { message: first, .. } = mailbox::send(
            &mut conn,
            &limits,
            &Sender::Owner,
            NewMessage::text(Recipient::Session(7), "Please add a test."),
            &[7],
        )
        .unwrap() else {
            panic!("the owner's message is delivered");
        };
        let SendResult::Delivered { .. } = mailbox::send(
            &mut conn,
            &limits,
            &Sender::Session(7),
            NewMessage {
                in_reply_to: Some(first.id),
                ..NewMessage::text(Recipient::Owner, "Done.")
            },
            &[7],
        )
        .unwrap() else {
            panic!("the reply is delivered");
        };

        let lines = conversation(&conn, 7).unwrap();
        let seen: Vec<(&str, &str, bool)> = lines
            .iter()
            .map(|l| (l.from.as_str(), l.text.as_str(), l.incoming))
            .collect();
        assert_eq!(
            seen,
            [
                ("Owner", "Please add a test.", true),
                ("builder-7", "Done.", false)
            ]
        );
        assert_eq!(
            unread(&conn, 7).unwrap(),
            1,
            "the owner only looked: the session still has it to read"
        );
        assert!(!lines[0].read);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
