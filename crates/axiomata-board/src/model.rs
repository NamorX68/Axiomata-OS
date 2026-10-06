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

/// The role a column plays in the agent flow (`docs/plans/a2a.md`, A13). It refines the column's status instead of
/// adding statuses: a `Proposal` column is `open`, a `Review` column is `doing`, so a card's status is still resolved
/// from its column alone and nothing about the old three-status rule changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnStage {
    /// Cards waiting for the owner's yes (a planner's draft, an agent's suggestion).
    Proposal,
    /// Work reported done and waiting for a reviewer.
    Review,
}

impl ColumnStage {
    pub fn as_str(self) -> &'static str {
        match self {
            ColumnStage::Proposal => "proposal",
            ColumnStage::Review => "review",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "proposal" => Some(ColumnStage::Proposal),
            "review" => Some(ColumnStage::Review),
            _ => None,
        }
    }

    /// The only status a column with this role may map to.
    pub fn required_status(self) -> CardStatus {
        match self {
            ColumnStage::Proposal => CardStatus::Open,
            ColumnStage::Review => CardStatus::Doing,
        }
    }
}

/// How strong an agent has to be for a card — the same three steps as an agent role's tier, kept here as plain words
/// because the board does not know the roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Light,
    Medium,
    Heavy,
}

impl Tier {
    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Light => "light",
            Tier::Medium => "medium",
            Tier::Heavy => "heavy",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "light" => Some(Tier::Light),
            "medium" => Some(Tier::Medium),
            "heavy" => Some(Tier::Heavy),
            _ => None,
        }
    }
}

/// Where a card stands in the agent flow, **derived** from its column, its signatures and a few fields — never stored
/// (A12), so it cannot contradict the column. Follows the A2A task states where they fit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// In a proposal column: waits for the owner's yes.
    Proposed,
    /// Open, but something it needs first is not reviewed yet.
    Blocked,
    /// Open and free to start.
    #[default]
    Ready,
    /// In a doing column.
    Working,
    /// The working agent asked something and waits for the answer.
    InputRequired,
    /// In a review column: reported done, waiting for a reviewer.
    InReview,
    /// In a done column, not signed off.
    Done,
    /// Signed off by somebody other than the one who did the work.
    Verified,
    /// Signed off and merged into its plan's integration line; the cards that build on it start from there.
    Integrated,
    /// Taken over into the main line by the owner.
    TakenOver,
    Failed,
    Canceled,
}

/// What the history of a card records (`card_events`, A19).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A session was started on the card.
    Started,
    /// The working agent reported done (the card went to review).
    Reported,
    /// The reviewer signed it off.
    Approved,
    /// The reviewer sent it back; the text says why.
    Returned,
    /// Moved to a stronger role or another engine.
    Escalated,
    /// A session limit stopped the work.
    LimitStop,
    /// The agent asked something and waits.
    InputRequired,
    /// The question was answered.
    InputProvided,
    Failed,
    Canceled,
    /// The claim was given back or taken from a dead session.
    Released,
    /// The owner took the work over into the main line.
    TakenOver,
    /// The work was merged into its plan's integration line.
    Integrated,
    /// Anything written down in words.
    Note,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Started => "started",
            EventKind::Reported => "reported",
            EventKind::Approved => "approved",
            EventKind::Returned => "returned",
            EventKind::Escalated => "escalated",
            EventKind::LimitStop => "limit_stop",
            EventKind::InputRequired => "input_required",
            EventKind::InputProvided => "input_provided",
            EventKind::Failed => "failed",
            EventKind::Canceled => "canceled",
            EventKind::Released => "released",
            EventKind::TakenOver => "taken_over",
            EventKind::Integrated => "integrated",
            EventKind::Note => "note",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "started" => EventKind::Started,
            "reported" => EventKind::Reported,
            "approved" => EventKind::Approved,
            "returned" => EventKind::Returned,
            "escalated" => EventKind::Escalated,
            "limit_stop" => EventKind::LimitStop,
            "input_required" => EventKind::InputRequired,
            "input_provided" => EventKind::InputProvided,
            "failed" => EventKind::Failed,
            "canceled" => EventKind::Canceled,
            "released" => EventKind::Released,
            "taken_over" => EventKind::TakenOver,
            "integrated" => EventKind::Integrated,
            "note" => EventKind::Note,
            _ => return None,
        })
    }
}

/// One line of a card's history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardEvent {
    pub id: i64,
    pub card_id: i64,
    pub at: DateTime<Utc>,
    pub actor: String,
    pub kind: EventKind,
    pub text: String,
}

/// Where a plan stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanStatus {
    /// Being drafted; its cards sit in the proposal column.
    Draft,
    /// The owner said yes; its cards are open.
    Approved,
    Closed,
}

impl PlanStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanStatus::Draft => "draft",
            PlanStatus::Approved => "approved",
            PlanStatus::Closed => "closed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "draft" => Some(PlanStatus::Draft),
            "approved" => Some(PlanStatus::Approved),
            "closed" => Some(PlanStatus::Closed),
            _ => None,
        }
    }
}

/// A plan: the unit of approval, automation, limits and the graph (A14).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub id: i64,
    pub board_id: i64,
    pub name: String,
    /// What the plan is for, in the owner's words; the planner reads it (`get_plan`).
    pub goal: String,
    /// The project (repository) the plan's sessions run in; `None` until the owner says (the Flow does when it makes the
    /// plan). A plan that runs by itself needs one.
    pub project_id: Option<i64>,
    /// The branch the plan's integration line was cut from, once there is one.
    pub base_branch: Option<String>,
    /// The commit the studio last made on the line; the line is refused when it is anywhere else.
    pub line_tip: Option<String>,
    pub status: PlanStatus,
    /// `None` = cards are started by hand; a number = start ready cards by themselves, up to that many at once.
    pub auto_start_max: Option<u32>,
    pub max_cost_usd: Option<f64>,
    pub max_tokens: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub approved_at: Option<DateTime<Utc>>,
}

/// What a caller sets on a plan (everything but the status, which only [`crate::flow::approve_plan`] and
/// [`crate::flow::close_plan`] move). A full replace, like [`CardFields`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanFields {
    pub name: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub auto_start_max: Option<u32>,
    #[serde(default)]
    pub max_cost_usd: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<u64>,
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
    /// The column's role in the agent flow, if it has one (A13).
    #[serde(default)]
    pub stage: Option<ColumnStage>,
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
    #[serde(default)]
    pub plan_id: Option<i64>,
    /// The role this card is meant for (a slug), proposed by the planner and confirmed at approval — an intention;
    /// the session that works on it is `claimed_by`.
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub agent_reason: Option<String>,
    #[serde(default)]
    pub tier: Option<Tier>,
    #[serde(default)]
    pub kind: Option<String>,
    /// Acceptance criteria, Markdown.
    #[serde(default)]
    pub acceptance: String,
    /// How often a reviewer sent the card back.
    #[serde(default)]
    pub returned_count: u32,
    /// What the working agent asked and waits for.
    #[serde(default)]
    pub input_required: Option<String>,
    /// The work is on its plan's integration line (CP-A8): reviewed and merged, but not yet in the main line.
    #[serde(default)]
    pub integrated_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub taken_over_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub failed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub canceled_at: Option<DateTime<Utc>>,
    /// **Computed on read, never stored**: the cards this one needs first.
    #[serde(default)]
    pub depends_on: Vec<i64>,
    /// **Computed on read**: those of [`Card::depends_on`] that are not signed off yet — what a blocked card waits
    /// for. A failed or canceled one stays in here, so the block stays visible.
    #[serde(default)]
    pub waiting_on: Vec<i64>,
    /// **Computed on read**: see [`TaskState`]. Ignored when a client sends a card back.
    #[serde(default, skip_deserializing)]
    pub state: TaskState,
}

impl Card {
    /// The writable fields of the card as they are now: what an edit starts from.
    pub fn fields(&self) -> CardFields {
        CardFields {
            title: self.title.clone(),
            body: self.body.clone(),
            labels: self.labels.clone(),
            assignee: self.assignee.clone(),
            due_at: self.due_at,
            plan_id: self.plan_id,
            agent: self.agent.clone(),
            agent_reason: self.agent_reason.clone(),
            tier: self.tier,
            kind: self.kind.clone(),
            acceptance: self.acceptance.clone(),
        }
    }
}

/// The writable fields of a card. Used for both create and update; update is a
/// full replace of these fields, matching how `routines::store::update` works,
/// so there is one shape to reason about rather than a partial-patch grammar.
/// The signatures (`claimed_by` / `verified_by`) are deliberately absent — they
/// only ever change through [`crate::store::claim_card`],
/// [`crate::store::release_card`] and [`crate::store::verify_card`], each of
/// which enforces its rule in SQL.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    #[serde(default)]
    pub plan_id: Option<i64>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub agent_reason: Option<String>,
    #[serde(default)]
    pub tier: Option<Tier>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub acceptance: String,
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
    /// The column's role in the agent flow; must go with the status ([`ColumnStage::required_status`]).
    #[serde(default)]
    pub stage: Option<ColumnStage>,
}

/// The columns a freshly created board starts with (A13): the smallest board that works with the agent flow, each
/// status once plus the two roles. "Vorschlag" sits at the left edge and is hidden by the views while empty.
pub fn default_columns() -> Vec<NewColumn> {
    vec![
        NewColumn {
            name: "Vorschlag".to_string(),
            maps_to_status: CardStatus::Open,
            stage: Some(ColumnStage::Proposal),
        },
        NewColumn {
            name: "Offen".to_string(),
            maps_to_status: CardStatus::Open,
            stage: None,
        },
        NewColumn {
            name: "In Arbeit".to_string(),
            maps_to_status: CardStatus::Doing,
            stage: None,
        },
        NewColumn {
            name: "Review".to_string(),
            maps_to_status: CardStatus::Doing,
            stage: Some(ColumnStage::Review),
        },
        NewColumn {
            name: "Fertig".to_string(),
            maps_to_status: CardStatus::Done,
            stage: None,
        },
    ]
}
