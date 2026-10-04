//! The mailbox between agent sessions and the owner (`docs/plans/a2a.md`, CP-A3; decisions A1, A8, A29).
//!
//! The core is independent of the transport. CP-A4 puts an MCP server in front of it (`send_message`, `read_inbox`),
//! the CLI mirrors it (A31), and a later A2A endpoint could too — which is why messages are built from **parts** (text,
//! a reference to a file, structured data) the way the A2A standard has them, and why the task a message is about is
//! simply the **card** it names: the card is the A2A task, its state is derived by the board (CP-A2), and the mailbox
//! only carries the conversation around it.
//!
//! What lives here:
//!
//! * **Addresses** ([`Sender`], [`Recipient`], [`Inbox`]): a session, a role (resolved to its live sessions at send
//!   time), the owner. No broadcast.
//! * **Sending** ([`send`]) with the two guards against runaway conversations (A8): a **chain counter** — each reply
//!   is one hop, and past [`Limits::max_chain`] the message is held and the owner is asked ([`release_held`] lets it
//!   go on) — and a **cap per sender and card**. A bare confirmation ([`MessageKind::Ack`]) is never answered.
//! * **Reading** ([`read_inbox`], [`unread_count`], [`mark_read`]).
//! * **Delivery to a waiting agent** ([`nudges`], [`record_nudge`]): MCP is request-response, the server cannot talk
//!   to a terminal agent by itself, so the studio types a one-line hint into a session that finished its turn
//!   (A8, way 2). This module only decides *who* and *what line*; typing is the embedder's.
//! * **Retention** ([`purge`], A29).
//!
//! Persistence is the embedder's connection, as everywhere in this crate (A11); the schema is
//! [`crate::SCHEMA_SQL_V7`]. Which sessions are alive is runtime state the caller passes in.

mod model;
mod store;

pub use model::{
    Inbox, InboxEntry, Limits, MAX_DATA_BYTES, MAX_FILE_REF_LEN, MAX_PARTS, MAX_TEXT_BYTES,
    Message, MessageKind, MessageStatus, NewMessage, Part, Recipient, Refusal, SendResult, Sender,
    check_parts,
};
pub use store::{
    CARD_RETENTION_DAYS, ClosedCard, LOOSE_RETENTION_DAYS, MAX_READ, Nudge, get_message, mark_read,
    nudges, purge, read_inbox, record_nudge, release_held, send, unread_count,
};
