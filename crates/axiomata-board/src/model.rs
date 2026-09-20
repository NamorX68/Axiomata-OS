//! Domain types for the Kanban board.
//!
//! Note what [`Card`] does *not* have: a status field. A card's status is the
//! `maps_to_status` of the column it sits in (see `schema.sql`), so it is
//! resolved by the caller, which already holds the column list. Storing it
//! twice would let the two drift, and the drift would be silent.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What a column means for the cards in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardStatus {
    Open,
    Doing,
    Done,
}

impl CardStatus {
    /// The value stored in `board_columns.maps_to_status`.
    pub fn as_str(self) -> &'static str {
        match self {
            CardStatus::Open => "open",
            CardStatus::Doing => "doing",
            CardStatus::Done => "done",
        }
    }

    /// Parses the stored form. `None` for anything else — callers turn that
    /// into a corruption error naming the row, rather than guessing a status.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "open" => Some(CardStatus::Open),
            "doing" => Some(CardStatus::Doing),
            "done" => Some(CardStatus::Done),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Board {
    pub id: i64,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Column {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    pub position: f64,
    pub maps_to_status: CardStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Card {
    pub id: i64,
    pub board_id: i64,
    pub column_id: i64,
    pub position: f64,
    pub title: String,
    pub body: String,
    pub labels: Vec<String>,
    /// Actor string (`"human:owner"`, `"agent:claude-1"`), or unassigned.
    pub assignee: Option<String>,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub verified_by: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub due_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// The writable fields of a card. Used for both create and update; update is a
/// full replace of these fields, matching how `routines::store::update` works,
/// so there is one shape to reason about rather than a partial-patch grammar.
/// The signatures (`claimed_by` / `verified_by`) are deliberately absent — they
/// only ever change through [`crate::store::claim_card`],
/// [`crate::store::release_card`] and [`crate::store::verify_card`], each of
/// which enforces its rule in SQL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardFields {
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub assignee: Option<String>,
    #[serde(default)]
    pub due_at: Option<DateTime<Utc>>,
}

/// A new card: where it goes, plus what is on it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCard {
    pub column_id: i64,
    #[serde(flatten)]
    pub fields: CardFields,
}

/// A new column. `position` is chosen by the store (appended to the end).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewColumn {
    pub name: String,
    pub maps_to_status: CardStatus,
}

/// The three columns a freshly created board starts with: the smallest board
/// that works, with each status represented exactly once so the mapping needs
/// no explaining.
pub fn default_columns() -> Vec<NewColumn> {
    vec![
        NewColumn {
            name: "Offen".to_string(),
            maps_to_status: CardStatus::Open,
        },
        NewColumn {
            name: "In Arbeit".to_string(),
            maps_to_status: CardStatus::Doing,
        },
        NewColumn {
            name: "Fertig".to_string(),
            maps_to_status: CardStatus::Done,
        },
    ]
}
