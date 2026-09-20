//! Kanban board core for Axiomata-OS — and, deliberately, nothing else.
//!
//! This crate has no dependency on `axiomata-core`, Tauri, or any Axiomata
//! path convention, for one concrete reason: the agentic IDE (`axiomata-ide`,
//! milestone M7.1 onwards) will use the same board as its agent task board,
//! and it must not pull `axiomata-core` in to do so. The board therefore owns
//! **neither the database file nor the connection** — every operation takes a
//! `&Connection` supplied by the caller, exactly like
//! `axiomata_core::routines::store`.
//!
//! Schema ownership follows from that: [`SCHEMA_SQL`] is the *initial* schema,
//! handed to whoever owns the migration chain (today `axiomata_core::db`,
//! which ships it as migration 8). Once released it is frozen. A later schema
//! change ships as a **new** constant appended as its own migration — this
//! crate must never own a living, mutable schema, or two embedders would
//! disagree about what version they are on.

pub mod model;
pub mod store;

pub use model::{Board, Card, CardFields, CardStatus, Column, NewCard, NewColumn};

/// The board's **version 1** schema (`boards`, `board_columns`, `cards`).
///
/// The `_V1` is the point: this constant is frozen the moment an embedder
/// ships it as a numbered migration, because that embedder has already
/// recorded the migration as applied and will never run it again. Editing
/// `schema.sql` in place after that would leave the table on disk silently
/// disagreeing with what this constant says it is. A schema change ships as a
/// **new** constant (`SCHEMA_SQL_V2`) appended as its own migration number.
/// The name says so at every call site, and `schema_is_frozen` in this module
/// fails loudly if the file is edited anyway.
///
/// Not applied by this crate: the embedder owns the migration chain.
pub const SCHEMA_SQL_V1: &str = include_str!("schema.sql");

/// Everything that can go wrong in the board core.
///
/// Deliberately without a `NotFound` or `Conflict` variant: "no such row"
/// stays an `Option`, and a lost race (two actors claiming the same card)
/// stays a `false` return. Both are ordinary outcomes here, not errors, and
/// that is the convention the rest of the workspace already uses.
#[derive(Debug, thiserror::Error)]
pub enum BoardError {
    /// A SQLite operation failed.
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    /// A stored row could not be reconstructed — a hand-edited database, or a
    /// value written by an older version. Names the row so it can be found.
    #[error("corrupt {table} row {id}: {reason}")]
    CorruptRow {
        table: &'static str,
        id: i64,
        reason: String,
    },

    /// Caller-supplied input was rejected before it reached the database.
    #[error("invalid {field}: {reason}")]
    Invalid { field: &'static str, reason: String },
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, BoardError>;

#[cfg(test)]
mod schema_is_frozen {
    /// Guards the promise in [`super::SCHEMA_SQL_V1`]'s docs with something
    /// more reliable than a contributor reading it.
    ///
    /// A migration already applied is never re-run, so editing `schema.sql`
    /// after it shipped would make the constant and the table on disk drift
    /// apart with nothing failing — the worst kind of bug, because the first
    /// symptom appears somewhere else entirely. This test turns that silent
    /// drift into a loud, unmissable failure.
    ///
    /// FNV-1a rather than a cryptographic hash: this defends against an
    /// accident, not an attacker, and it costs no dependency.
    #[test]
    fn the_shipped_schema_has_not_been_edited() {
        const EXPECTED: u64 = 0xc9fd_e4c8_3458_445b;

        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in super::SCHEMA_SQL_V1.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }

        assert_eq!(
            hash, EXPECTED,
            "schema.sql changed after it shipped as a numbered migration.\n\
             A migration that already ran is never re-run, so this edit will \
             NOT reach any existing database — it only makes the constant lie \
             about what is on disk.\n\
             Add a new SCHEMA_SQL_V2 constant and a new migration number \
             instead. If you are deliberately changing the schema before it \
             has ever shipped, update EXPECTED in this test."
        );
    }
}
