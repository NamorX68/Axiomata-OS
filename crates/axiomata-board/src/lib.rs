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
//! Schema ownership follows from that: [`SCHEMA_SQL_V1`] is the *initial* schema,
//! handed to whoever owns the migration chain (today `axiomata_core::db`,
//! which ships it as migration 8). Once released it is frozen. A later schema
//! change ships as a **new** constant appended as its own migration — this
//! crate must never own a living, mutable schema, or two embedders would
//! disagree about what version they are on.

pub mod flow;
pub mod model;
pub mod store;

pub use model::{
    Board, Card, CardEvent, CardFields, CardStatus, Column, ColumnStage, EventKind, NewCard,
    NewColumn, Plan, PlanFields, PlanStatus, TaskState, Tier,
};

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

/// The board's **version 2** schema: the agent flow (`docs/plans/a2a.md`, CP-A2) — column roles, plans, the agent
/// fields of a card, dependencies and the card history. An ALTER-and-ADD migration of its own, number 15 in the core's
/// chain, frozen once released like version 1.
pub const SCHEMA_SQL_V2: &str = include_str!("flow.sql");

/// The board's **version 3** schema (`goal` on a plan), `docs/plans/a2a.md` CP-A7. Its own constant and migration
/// number
/// (19), frozen once released.
pub const SCHEMA_SQL_V3: &str = include_str!("plan_goal.sql");

/// The board's **version 4** schema (`project_id`, `base_branch` on a plan, `integrated_at` on a card), `docs/plans/a2a.md`
/// CP-A8. Its own constant and migration number (20), frozen once released.
pub const SCHEMA_SQL_V4: &str = include_str!("plan_line.sql");

/// The board's **version 5** schema (`line_tip` on a plan), `docs/plans/a2a.md` CP-A8. A migration of its own (21): the
/// owner's database had applied migration 20 before the column was added to it.
pub const SCHEMA_SQL_V5: &str = include_str!("plan_line_tip.sql");

/// The board's **version 6** schema (`card_proposal_depth`), `docs/plans/a2a.md` CP-A8c. Its own constant and migration
/// number (23), frozen once released.
pub const SCHEMA_SQL_V6: &str = include_str!("proposal_depth.sql");

/// The board's **version 7** schema (`plan_goal_suggestions`), `docs/plans/a2a.md` "Plan bearbeiten und grillen". Its own
/// constant and migration number (24), frozen once released.
pub const SCHEMA_SQL_V7: &str = include_str!("plan_goal_suggestion.sql");

/// The board's **version 8** schema (`card_proposers`), `docs/plans/a2a.md` "Plan bearbeiten und grillen". Its own constant
/// and migration number (25), frozen once released.
pub const SCHEMA_SQL_V8: &str = include_str!("card_proposers.sql");

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

    /// The same guard for version 2 (the agent flow), which ships as migration 15. A later change arrives as
    /// `SCHEMA_SQL_V3`, never as an edit of `flow.sql`.
    #[test]
    fn the_shipped_flow_schema_has_not_been_edited() {
        const EXPECTED: u64 = 0x54d7_1619_7f8c_e5c9;

        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in super::SCHEMA_SQL_V2.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }

        assert_eq!(
            hash, EXPECTED,
            "flow.sql changed after it shipped as migration 15. It is an ALTER-and-ADD migration — add a \
             SCHEMA_SQL_V6 and a new migration number instead. If it has never shipped, update EXPECTED here."
        );
    }
}

/// Renders a board as Markdown, for mirroring into the user's workspace.
///
/// Pure on purpose: the board crate must not know where a workspace is or how
/// to write to one — that is the embedder's business (and the embedder is the
/// only one that could ask the user where their vault lives). This produces
/// the text; somebody else decides what to do with it.
///
/// The mirror is a picture of the *live* board. Archived cards are left out:
/// the file answers "what is on this board", and something archived is not.
///
/// One-way by construction, and the header says so — nothing reads this file
/// back. That is the price of the database being the source of truth, and it
/// is cheaper than two writers disagreeing about the same board.
pub fn render_board_markdown(board: &Board, columns: &[Column], cards: &[Card]) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let _ = writeln!(out, "# {}", board.name);
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "> Erzeugt von Axiomata-OS am {}. Änderungen an dieser Datei werden beim",
        board.updated_at.format("%Y-%m-%d %H:%M")
    );
    let _ = writeln!(
        out,
        "> nächsten Schreiben überschrieben — das Brett selbst lebt in der App."
    );

    let mut ordered: Vec<&Column> = columns.iter().collect();
    ordered.sort_by(|a, b| a.position.total_cmp(&b.position).then(a.id.cmp(&b.id)));

    for column in ordered {
        let mut held: Vec<&Card> = cards
            .iter()
            .filter(|card| card.column_id == column.id && card.archived_at.is_none())
            .collect();
        held.sort_by(|a, b| a.position.total_cmp(&b.position).then(a.id.cmp(&b.id)));

        let _ = writeln!(out);
        let _ = writeln!(out, "## {} ({})", column.name, held.len());
        let _ = writeln!(out);
        if held.is_empty() {
            let _ = writeln!(out, "_Keine Karten._");
            continue;
        }
        for card in held {
            // A done column's cards are ticked, so the file reads like the
            // checklist a reader expects rather than a flat list.
            let done = if column.maps_to_status == CardStatus::Done {
                "x"
            } else {
                " "
            };
            let _ = write!(out, "- [{done}] {}", card.title);

            let mut notes: Vec<String> = Vec::new();
            if !card.labels.is_empty() {
                notes.push(card.labels.join(", "));
            }
            if let Some(due) = card.due_at {
                notes.push(format!("fällig {}", due.format("%Y-%m-%d")));
            }
            if let Some(who) = &card.assignee {
                notes.push(who.clone());
            }
            if let Some(signer) = &card.verified_by {
                notes.push(format!("geprüft von {signer}"));
            }
            if let Some(kind) = &card.kind {
                notes.push(format!("Art {kind}"));
            }
            if let Some(agent) = &card.agent {
                notes.push(format!("Rolle {agent}"));
            }
            if card.returned_count > 0 {
                notes.push(format!("{}× zurückgegeben", card.returned_count));
            }
            if !card.waiting_on.is_empty() {
                let ids: Vec<String> = card.waiting_on.iter().map(|id| format!("#{id}")).collect();
                notes.push(format!("wartet auf {}", ids.join(", ")));
            }
            if card.failed_at.is_some() {
                notes.push("gescheitert".to_string());
            }
            if card.canceled_at.is_some() {
                notes.push("abgebrochen".to_string());
            }
            if !notes.is_empty() {
                let _ = write!(out, "  ({})", notes.join(" · "));
            }
            let _ = writeln!(out);

            for line in card.body.lines().filter(|line| !line.trim().is_empty()) {
                let _ = writeln!(out, "  {line}");
            }
        }
    }
    out
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn board() -> Board {
        let stamp = Utc.with_ymd_and_hms(2026, 9, 20, 12, 0, 0).unwrap();
        Board {
            id: 1,
            name: "Mein Brett".to_string(),
            created_at: stamp,
            updated_at: stamp,
        }
    }

    fn column(id: i64, name: &str, position: f64, maps_to_status: CardStatus) -> Column {
        Column {
            id,
            board_id: 1,
            name: name.to_string(),
            position,
            maps_to_status,
            stage: None,
        }
    }

    fn card(id: i64, column_id: i64, position: f64, title: &str) -> Card {
        let stamp = Utc.with_ymd_and_hms(2026, 9, 20, 12, 0, 0).unwrap();
        Card {
            id,
            board_id: 1,
            column_id,
            position,
            title: title.to_string(),
            body: String::new(),
            labels: Vec::new(),
            assignee: None,
            claimed_by: None,
            claimed_at: None,
            verified_by: None,
            verified_at: None,
            due_at: None,
            archived_at: None,
            created_at: stamp,
            updated_at: stamp,
            plan_id: None,
            agent: None,
            agent_reason: None,
            tier: None,
            kind: None,
            acceptance: String::new(),
            returned_count: 0,
            input_required: None,
            integrated_at: None,
            taken_over_at: None,
            failed_at: None,
            canceled_at: None,
            depends_on: Vec::new(),
            waiting_on: Vec::new(),
            state: TaskState::default(),
        }
    }

    #[test]
    fn the_header_says_the_file_is_a_copy() {
        let out = render_board_markdown(&board(), &[], &[]);
        assert!(out.starts_with("# Mein Brett"));
        assert!(out.contains("überschrieben"), "the reader has to be told");
    }

    #[test]
    fn columns_come_out_in_board_order_with_their_counts() {
        let columns = vec![
            column(3, "Fertig", 3.0, CardStatus::Done),
            column(1, "Offen", 1.0, CardStatus::Open),
        ];
        let cards = vec![card(10, 1, 1.0, "eins"), card(11, 1, 2.0, "zwei")];
        let out = render_board_markdown(&board(), &columns, &cards);

        let offen = out.find("## Offen (2)").expect("Offen with its count");
        let fertig = out.find("## Fertig (0)").expect("Fertig with its count");
        assert!(offen < fertig, "position decides the order, not the vec");
    }

    #[test]
    fn a_done_columns_cards_are_ticked() {
        let columns = vec![
            column(1, "Offen", 1.0, CardStatus::Open),
            column(3, "Fertig", 3.0, CardStatus::Done),
        ];
        let cards = vec![card(10, 1, 1.0, "offen"), card(11, 3, 1.0, "fertig")];
        let out = render_board_markdown(&board(), &columns, &cards);

        assert!(out.contains("- [ ] offen"));
        assert!(out.contains("- [x] fertig"));
    }

    /// The mirror answers "what is on this board", and archived is not on it.
    #[test]
    fn archived_cards_are_left_out_and_not_counted() {
        let columns = vec![column(1, "Offen", 1.0, CardStatus::Open)];
        let mut hidden = card(11, 1, 2.0, "archiviert");
        hidden.archived_at = Some(Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap());
        let out =
            render_board_markdown(&board(), &columns, &[card(10, 1, 1.0, "sichtbar"), hidden]);

        assert!(out.contains("sichtbar"));
        assert!(!out.contains("archiviert"));
        assert!(
            out.contains("## Offen (1)"),
            "the count must not include it"
        );
    }

    #[test]
    fn a_cards_details_travel_with_it() {
        let columns = vec![column(1, "Offen", 1.0, CardStatus::Open)];
        let mut rich = card(10, 1, 1.0, "mit allem");
        rich.labels = vec!["rust".to_string(), "design".to_string()];
        rich.due_at = Some(Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap());
        rich.assignee = Some("agent:claude-1".to_string());
        rich.body = "Erste Zeile\n\nZweite Zeile".to_string();

        let out = render_board_markdown(&board(), &columns, &[rich]);
        assert!(out.contains("rust, design"));
        assert!(out.contains("fällig 2026-10-01"));
        assert!(out.contains("agent:claude-1"));
        assert!(
            out.contains("  Erste Zeile"),
            "body lines are indented under the card"
        );
        assert!(out.contains("  Zweite Zeile"));
    }

    #[test]
    fn the_flow_details_of_a_card_travel_with_it() {
        let columns = vec![column(1, "Offen", 1.0, CardStatus::Open)];
        let mut flowing = card(10, 1, 1.0, "im Ablauf");
        flowing.kind = Some("implement".to_string());
        flowing.agent = Some("implementer-light".to_string());
        flowing.returned_count = 2;
        flowing.waiting_on = vec![7, 9];
        flowing.failed_at = Some(Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap());

        let out = render_board_markdown(&board(), &columns, &[flowing]);
        assert!(out.contains("Art implement"));
        assert!(out.contains("Rolle implementer-light"));
        assert!(out.contains("2× zurückgegeben"));
        assert!(out.contains("wartet auf #7, #9"));
        assert!(out.contains("gescheitert"));
    }

    #[test]
    fn an_empty_column_says_so_rather_than_ending_abruptly() {
        let columns = vec![column(1, "Offen", 1.0, CardStatus::Open)];
        assert!(render_board_markdown(&board(), &columns, &[]).contains("_Keine Karten._"));
    }
}
