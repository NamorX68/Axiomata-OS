//! Core of the agentic IDE for Axiomata-OS — and, deliberately, nothing else.
//!
//! This crate has no dependency on `axiomata-core` or Tauri, for the same
//! concrete reason `axiomata-board` has none: milestone M7 is meant to be
//! extractable into a standalone app the way `axiomata-terminal` is, and a
//! dependency on the embedder would kill that at the first commit. It owns
//! **neither the database file nor the connection** — every operation takes a
//! `&Connection` supplied by the caller.
//!
//! Today it holds projects (M7.1) and agent profiles (M7.2 CP4). The rest of
//! M7.2 onwards adds worktrees, the git layer, the agent supervisor and the
//! mailbox beside them; each brings its own schema constant and its own
//! migration number.
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

pub mod agent_store;
pub mod model;
pub mod provision;
pub mod store;
pub mod worktree;

pub use model::{Agent, AgentFields, Harness, NewAgent, NewProject, Project};

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

/// The IDE's **version 2** schema (`ide_agents`), milestone M7.2 CP4.
///
/// A second constant rather than an edit to [`SCHEMA_SQL_V1`], exactly as that
/// one's documentation requires: version 1 shipped as migration 9 and will
/// never run again, so the agents table arrives as its own migration (10) with
/// its own frozen text. The same rule applies from here on — CP5's worktree
/// columns and CP6's status become `SCHEMA_SQL_V3` and so on, never edits to
/// this file.
pub const SCHEMA_SQL_V2: &str = include_str!("agents.sql");

/// The IDE's **version 3** schema (worktree, branch and port on an agent),
/// milestone M7.2 CP5.
///
/// An `ALTER TABLE` rather than a `CREATE`, since version 2 shipped and has
/// rows. Same rule as before: its own constant, its own migration number (11),
/// frozen once released.
pub const SCHEMA_SQL_V3: &str = include_str!("agent_worktrees.sql");

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

    /// A `git` invocation failed, or git could not be run at all. Carries the
    /// command and git's own stderr, which together are what makes a git
    /// failure diagnosable.
    #[error("{command} failed: {reason}")]
    Git { command: String, reason: String },
}

/// Convenience alias used throughout the crate.
pub type Result<T> = std::result::Result<T, IdeError>;

/// Applies every schema this crate ships, in order — the one place a test
/// fixture has to learn about a new migration.
///
/// Without it each module's fixture listed the versions it happened to know
/// about, and adding `SCHEMA_SQL_V3` broke the ones that did not, with an
/// error ("no such column") that says nothing about the real cause.
#[cfg(test)]
pub(crate) fn apply_all_schemas(db: &rusqlite::Connection) {
    for schema in [SCHEMA_SQL_V1, SCHEMA_SQL_V2, SCHEMA_SQL_V3] {
        db.execute_batch(schema).expect("test schema should apply");
    }
}

/// Guards the promise in the `SCHEMA_SQL_V*` docs with something more reliable
/// than a contributor reading it.
///
/// A migration already applied is never re-run, so editing a shipped schema
/// file would make the constant and the table on disk drift apart with nothing
/// failing — the worst kind of bug, because the first symptom appears
/// somewhere else entirely. These tests turn that silent drift into a loud,
/// unmissable failure. One per shipped version, and every future version adds
/// its own.
///
/// FNV-1a rather than a cryptographic hash: this defends against an accident,
/// not an attacker, and it costs no dependency.
#[cfg(test)]
mod schema_is_frozen {
    fn fnv1a(text: &str) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in text.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    #[test]
    fn the_shipped_schema_has_not_been_edited() {
        const EXPECTED: u64 = 0xa5d9_e2a0_ffd8_1598;

        assert_eq!(
            fnv1a(super::SCHEMA_SQL_V1),
            EXPECTED,
            "schema.sql changed after it shipped as a numbered migration.\n\
             A migration that already ran is never re-run, so this edit will \
             NOT reach any existing database — it only makes the constant lie \
             about what is on disk.\n\
             Add a new SCHEMA_SQL_V2 constant and a new migration number \
             instead. If you are deliberately changing the schema before it \
             has ever shipped, update EXPECTED in this test."
        );
    }

    /// The same guard for version 2 (`ide_agents`), which shipped as migration
    /// 10. Later columns arrive as V3, V4 … never as edits here.
    #[test]
    fn the_shipped_agents_schema_has_not_been_edited() {
        const EXPECTED: u64 = 0x5db8_a1cb_0f0f_a412;

        assert_eq!(
            fnv1a(super::SCHEMA_SQL_V2),
            EXPECTED,
            "agents.sql changed after it shipped as a numbered migration.\n\
             A migration that already ran is never re-run, so this edit will \
             NOT reach any existing database — it only makes the constant lie \
             about what is on disk.\n\
             Add a new SCHEMA_SQL_V3 constant and a new migration number \
             instead. If you are deliberately changing the schema before it \
             has ever shipped, update EXPECTED in this test."
        );
    }

    /// And for version 3 (worktree columns), which shipped as migration 11.
    #[test]
    fn the_shipped_worktree_schema_has_not_been_edited() {
        const EXPECTED: u64 = 0xbe2a_6f78_c386_f0aa;

        assert_eq!(
            fnv1a(super::SCHEMA_SQL_V3),
            EXPECTED,
            "agent_worktrees.sql changed after it shipped as a numbered \
             migration. It is an ALTER TABLE, so re-running it is not even \
             possible — add a SCHEMA_SQL_V4 and a new migration number \
             instead. If it has never shipped, update EXPECTED here."
        );
    }
}
