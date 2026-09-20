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
    fn an_empty_column_says_so_rather_than_ending_abruptly() {
        let columns = vec![column(1, "Offen", 1.0, CardStatus::Open)];
        assert!(render_board_markdown(&board(), &columns, &[]).contains("_Keine Karten._"));
    }
}
