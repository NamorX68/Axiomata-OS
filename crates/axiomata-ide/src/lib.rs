//! Core of the agentic IDE for Axiomata-OS — and, deliberately, nothing else.
//!
//! This crate has no dependency on `axiomata-core` or Tauri, for the same
//! concrete reason `axiomata-board` has none: milestone M7 is meant to be
//! extractable into a standalone app the way `axiomata-terminal` is, and a
//! dependency on the embedder would kill that at the first commit. It owns
//! **neither the database file nor the connection** — every operation takes a
//! `&Connection` supplied by the caller.
//!
//! Today it holds projects. M7.2 onwards adds worktrees, the git layer, the
//! agent supervisor and the mailbox beside them; each brings its own schema
//! constant and its own migration number.
//!
//! ⚠️ One promise that is **per module, not crate-wide**: `store`'s "looks at
//! the file system, never changes it" holds for projects and is what makes
//! removing a project safe. It will not hold for the worktree module in M7.2,
//! which has to create and remove real directories under
//! `~/.axiomata/worktrees/`. That module states its own contract when it
//! lands; do not extrapolate the projects store's narrower promise to it.
//!
//! Schema ownership follows from the same cut: [`SCHEMA_SQL_V1`] is the
//! *initial* schema, handed to whoever owns the migration chain (today
//! `axiomata_core::db`, which ships it as migration 9). Once released it is
//! frozen — see the constant's own docs.

pub mod model;
pub mod store;

pub use model::{NewProject, Project};

/// The IDE's **version 1** schema (`projects`).
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

/// Everything that can go wrong in the IDE core.
///
/// Deliberately without a `NotFound` or `Conflict` variant, matching
/// [`axiomata_board::BoardError`]: "no such row" stays an `Option` and a
/// refused write stays a `false`. Both are ordinary outcomes, not errors.
///
/// [`axiomata_board::BoardError`]: https://docs.rs/axiomata-board
#[derive(Debug, thiserror::Error)]
pub enum IdeError {
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
pub type Result<T> = std::result::Result<T, IdeError>;

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
        const EXPECTED: u64 = 0xa5d9_e2a0_ffd8_1598;

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
