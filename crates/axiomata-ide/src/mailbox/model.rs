//! The mailbox's vocabulary: addresses, message parts and the stored message.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{IdeError, Result};

/// The most parts one message may carry. A message is a short note with, at most, a few files and one data blob.
pub const MAX_PARTS: usize = 16;
/// The most text bytes over all text parts of one message.
pub const MAX_TEXT_BYTES: usize = 64 * 1024;
/// The most bytes of one data part, serialised.
pub const MAX_DATA_BYTES: usize = 64 * 1024;
/// The longest file name or location a file part may carry.
pub const MAX_FILE_REF_LEN: usize = 1000;

/// Who a message comes from. Never a role: a role is a *kind* of session, and an answer needs one concrete sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sender {
    /// An agent session (`ide_agents.id`).
    Session(i64),
    /// The owner, at the keyboard.
    Owner,
    /// The studio itself — bounces and chain stops. Nobody answers it.
    System,
}

/// Who a message is addressed to (A29): a session, a role resolved to its live sessions, or the owner. There is no
/// broadcast.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Recipient {
    /// One session, by id.
    Session(i64),
    /// Every live session playing this role (a slug, see `axiomata_roster::check_slug`).
    Role(String),
    /// The owner's inbox.
    Owner,
}

/// One concrete mailbox: where a delivery lands and what `read_inbox` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Inbox {
    /// A session's inbox.
    Session(i64),
    /// The owner's inbox.
    Owner,
}

fn parse_session(rest: &str) -> Option<i64> {
    // `parse::<i64>` also accepts a leading `+`; ids as written by this crate never have one.
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok().filter(|id| *id > 0)
}

impl Sender {
    /// The stored spelling.
    pub fn as_text(&self) -> String {
        match self {
            Sender::Session(id) => format!("session:{id}"),
            Sender::Owner => "owner".to_owned(),
            Sender::System => "system".to_owned(),
        }
    }

    /// Parses the stored spelling; `None` for anything else.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "owner" => Some(Sender::Owner),
            "system" => Some(Sender::System),
            _ => raw
                .strip_prefix("session:")
                .and_then(parse_session)
                .map(Sender::Session),
        }
    }

    /// The inbox this sender reads, if it has one.
    pub fn inbox(&self) -> Option<Inbox> {
        match self {
            Sender::Session(id) => Some(Inbox::Session(*id)),
            Sender::Owner => Some(Inbox::Owner),
            Sender::System => None,
        }
    }
}

impl Recipient {
    /// The stored spelling.
    pub fn as_text(&self) -> String {
        match self {
            Recipient::Session(id) => format!("session:{id}"),
            Recipient::Role(name) => format!("role:{name}"),
            Recipient::Owner => "owner".to_owned(),
        }
    }

    /// Parses the stored spelling; `None` for anything else, including a role that is not a valid slug.
    pub fn parse(raw: &str) -> Option<Self> {
        if raw == "owner" {
            return Some(Recipient::Owner);
        }
        if let Some(rest) = raw.strip_prefix("session:") {
            return parse_session(rest).map(Recipient::Session);
        }
        let name = raw.strip_prefix("role:")?;
        axiomata_roster::check_slug("role", name)
            .ok()
            .map(|()| Recipient::Role(name.to_owned()))
    }
}

impl Inbox {
    /// The stored spelling.
    pub fn as_text(&self) -> String {
        match self {
            Inbox::Session(id) => format!("session:{id}"),
            Inbox::Owner => "owner".to_owned(),
        }
    }

    /// Parses the stored spelling; `None` for anything else.
    pub fn parse(raw: &str) -> Option<Self> {
        if raw == "owner" {
            return Some(Inbox::Owner);
        }
        raw.strip_prefix("session:")
            .and_then(parse_session)
            .map(Inbox::Session)
    }
}

/// The three address types are sent to the webview and the MCP server as their stored text, so a client never has to
/// know the shape of an enum.
macro_rules! text_serde {
    ($ty:ty, $what:literal) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.as_text())
            }
        }
        impl Serialize for $ty {
            fn serialize<S: serde::Serializer>(
                &self,
                serializer: S,
            ) -> std::result::Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.as_text())
            }
        }
        impl<'de> Deserialize<'de> for $ty {
            fn deserialize<D: serde::Deserializer<'de>>(
                deserializer: D,
            ) -> std::result::Result<Self, D::Error> {
                let raw = String::deserialize(deserializer)?;
                <$ty>::parse(&raw).ok_or_else(|| {
                    serde::de::Error::custom(format!("not a valid {}: {raw:?}", $what))
                })
            }
        }
    };
}

text_serde!(Sender, "sender");
text_serde!(Recipient, "recipient");
text_serde!(Inbox, "inbox");

/// One part of a message, in the shape of the A2A standard's parts (A1): text, a reference to a file, or structured
/// data.
///
/// A file part **names** a file, it never carries bytes, and nothing in this crate opens it — a path in a message is
/// something the receiving agent may look at with its own tools, under its own rights.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Part {
    /// Plain text.
    Text { text: String },
    /// A file the sender points at (a path in a worktree, say).
    File {
        /// A short display name.
        name: String,
        /// Where it is — free text, never resolved by the mailbox.
        uri: String,
        /// Its media type, when the sender knows it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mime_type: Option<String>,
    },
    /// A JSON value for the receiver to process rather than read.
    Data { data: Value },
}

/// Whether a file part's location is a plain relative path. The mailbox never opens it, but the *receiver* may, with
/// its
/// own rights: a message that sends an agent to `~/.ssh/id_ed25519` or `../../other-project` is a way to make it read
/// what it was never given, so the sender can only name things inside the project tree.
fn is_plain_relative(uri: &str) -> bool {
    let uri = uri.trim();
    !(uri.starts_with('/')
        || uri.starts_with('~')
        || uri.starts_with('\\')
        || uri.contains(':')
        || uri.contains('\\')
        || uri.split('/').any(|part| part == ".."))
}

/// Checks a message's parts before they are stored.
///
/// # Errors
///
/// [`IdeError::Invalid`] for no parts, too many parts, no text at all (a message made only of a file or data is
/// allowed, but an *empty* text part is not), or any size over its limit.
pub fn check_parts(parts: &[Part]) -> Result<()> {
    let invalid = |reason: &str| IdeError::Invalid {
        field: "parts",
        reason: reason.to_owned(),
    };
    if parts.is_empty() {
        return Err(invalid("a message needs at least one part"));
    }
    if parts.len() > MAX_PARTS {
        return Err(invalid(&format!("at most {MAX_PARTS} parts")));
    }
    let mut text_bytes = 0usize;
    for part in parts {
        match part {
            Part::Text { text } => {
                if text.trim().is_empty() {
                    return Err(invalid("a text part must not be empty"));
                }
                text_bytes += text.len();
            }
            Part::File {
                name,
                uri,
                mime_type,
            } => {
                let long = |s: &str| s.len() > MAX_FILE_REF_LEN || s.chars().any(char::is_control);
                if name.trim().is_empty() || uri.trim().is_empty() {
                    return Err(invalid("a file part needs a name and a location"));
                }
                if !is_plain_relative(uri) {
                    return Err(invalid(
                        "a file part points at a relative path inside the project: no absolute path, `..`, \
                            `~` or scheme",
                    ));
                }
                if long(name) || long(uri) || mime_type.as_deref().is_some_and(long) {
                    return Err(invalid(
                        "a file part's name, location and type must be short single lines",
                    ));
                }
            }
            Part::Data { data } => {
                // Serialising only to measure: a value that cannot be written out cannot be stored either.
                let size = serde_json::to_vec(data)
                    .map(|v| v.len())
                    .unwrap_or(usize::MAX);
                if size > MAX_DATA_BYTES {
                    return Err(invalid(&format!(
                        "a data part is at most {MAX_DATA_BYTES} bytes"
                    )));
                }
            }
        }
    }
    if text_bytes > MAX_TEXT_BYTES {
        return Err(invalid(&format!("at most {MAX_TEXT_BYTES} bytes of text")));
    }
    Ok(())
}

/// What a message is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    /// An ordinary message; answering it is fine.
    Message,
    /// A bare confirmation ("got it"). Never answered (A8) — the end of a conversation.
    Ack,
    /// Written by the studio (a bounce, a chain stop). Never answered.
    Notice,
}

impl MessageKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            MessageKind::Message => "message",
            MessageKind::Ack => "ack",
            MessageKind::Notice => "notice",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "message" => Some(MessageKind::Message),
            "ack" => Some(MessageKind::Ack),
            "notice" => Some(MessageKind::Notice),
            _ => None,
        }
    }

    /// Whether a reply to a message of this kind is accepted.
    pub fn expects_reply(self) -> bool {
        matches!(self, MessageKind::Message)
    }
}

/// What became of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    /// Landed in at least one inbox.
    Delivered,
    /// The chain limit stopped it; the owner decides whether it goes on (A8).
    Held,
    /// Nobody could receive it — the session had ended or no live session plays the role.
    Undeliverable,
}

impl MessageStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            MessageStatus::Delivered => "delivered",
            MessageStatus::Held => "held",
            MessageStatus::Undeliverable => "undeliverable",
        }
    }

    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "delivered" => Some(MessageStatus::Delivered),
            "held" => Some(MessageStatus::Held),
            "undeliverable" => Some(MessageStatus::Undeliverable),
            _ => None,
        }
    }
}

/// A stored message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    /// Stamped by the sender's side (the MCP server, later), never taken from the message's own text.
    pub sender: Sender,
    /// The address as it was written; a role stays a role here even though it was resolved on delivery.
    pub to: Recipient,
    /// The card (task) this is about.
    pub card_id: Option<i64>,
    pub kind: MessageKind,
    pub parts: Vec<Part>,
    pub in_reply_to: Option<i64>,
    /// Hops into a chain of replies (A8).
    pub chain: u32,
    pub status: MessageStatus,
}

/// What a caller hands to [`crate::mailbox::send`].
#[derive(Debug, Clone)]
pub struct NewMessage {
    pub to: Recipient,
    pub card_id: Option<i64>,
    pub kind: MessageKind,
    pub parts: Vec<Part>,
    /// The message this answers. The sender must have received it.
    pub in_reply_to: Option<i64>,
}

impl NewMessage {
    /// A plain text message with no card and no parent — the shape most tests and the CLI need.
    pub fn text(to: Recipient, text: impl Into<String>) -> Self {
        NewMessage {
            to,
            card_id: None,
            kind: MessageKind::Message,
            parts: vec![Part::Text { text: text.into() }],
            in_reply_to: None,
        }
    }
}

/// One message as seen from one inbox.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InboxEntry {
    pub delivery_id: i64,
    pub message: Message,
    pub read_at: Option<DateTime<Utc>>,
}

/// The numbers of A8, adjustable by the owner later ("einstellbar"); the defaults are the plan's start values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    /// Replies in one chain before it stops and asks the owner.
    pub max_chain: u32,
    /// Messages one session may send about one card (or, with no card, in total).
    pub max_per_sender_and_card: u32,
    /// Messages one session may send in all, whatever card they are about. The per-card cap alone could be reset by
    /// changing the card (finishing or failing it), so this is the ceiling over a session's whole life.
    pub max_per_sender_total: u32,
    /// How often a delivery is announced in a session's terminal before it is left alone.
    pub max_nudges: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_chain: 6,
            max_per_sender_and_card: 20,
            max_per_sender_total: 60,
            max_nudges: 3,
        }
    }
}

/// Why [`crate::mailbox::send`] did not accept a message. Not an error: an agent is told and carries on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Refusal {
    /// The sender has used up its messages about this card.
    SenderLimit { limit: u32 },
    /// The sender has used up its messages altogether.
    SenderTotalLimit { limit: u32 },
    /// The message answers an ack or a notice, which nobody answers (A8).
    NoReplyExpected,
    /// A session wrote to itself.
    ToSelf,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::SenderLimit { limit } => write!(
                f,
                "message limit reached: at most {limit} messages per card; ask the owner if more is needed"
            ),
            Refusal::SenderTotalLimit { limit } => write!(
                f,
                "message limit reached: at most {limit} messages per session; ask the owner if more is needed"
            ),
            Refusal::NoReplyExpected => {
                f.write_str("that message is a confirmation or a notice and is not answered")
            }
            Refusal::ToSelf => f.write_str("a session cannot send a message to itself"),
        }
    }
}

/// The outcome of [`crate::mailbox::send`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SendResult {
    /// Stored and put into these inboxes.
    Delivered {
        message: Message,
        inboxes: Vec<Inbox>,
    },
    /// Stored, but the chain limit stopped it: only the owner's inbox has it, with a notice.
    Held { message: Message },
    /// Stored, but nobody can receive it; the sender (if a session) and the owner get a notice (A29).
    Undeliverable { message: Message, reason: String },
    /// Not stored.
    Refused { refusal: Refusal },
}
