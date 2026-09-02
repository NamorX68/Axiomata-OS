//! Data types for scheduled routines and their firing history.
//!
//! A [`Routine`] is a cron schedule ([`Routine::cron_expr`], parsed by
//! [`crate::routines::schedule`]) bound to a [`RoutineTarget`]. Each firing
//! attempt produces a [`RoutineRun`]; the underlying agent execution is still
//! recorded in the shared `runs` table (see [`crate::skills::runlog`]) and
//! linked back via [`RoutineRun::run_id`].
//!
//! There is no stored "status" column. [`RoutineState`] is derived on demand
//! from a routine's own fields plus its most recent [`RoutineRun`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::AxiomataError;

/// What a routine runs when it fires.
///
/// Serializes tagged, e.g. `{ "type": "skill", "value": "example-skill" }`,
/// so the desktop UI can build one from a radio button plus a text field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum RoutineTarget {
    /// A named skill from `~/.axiomata/skills/`, run exactly as a manual
    /// skill invocation would (`/<name>` for Claude Code, the SKILL.md body
    /// for Ollama).
    Skill(String),
    /// A literal prompt string sent straight to an agent backend, with no
    /// SKILL.md involved.
    Prompt(String),
}

impl RoutineTarget {
    /// Splits into the `(target_type, target)` pair stored in the two
    /// `routines` columns of the same name.
    pub fn to_columns(&self) -> (&'static str, &str) {
        match self {
            RoutineTarget::Skill(name) => ("skill", name),
            RoutineTarget::Prompt(text) => ("prompt", text),
        }
    }

    /// Reconstructs a target from its two stored columns.
    ///
    /// # Errors
    ///
    /// Returns [`AxiomataError::InvalidRoutine`] if `target_type` is neither
    /// `"skill"` nor `"prompt"` (a corrupted or hand-edited row).
    pub fn from_columns(target_type: &str, target: String) -> Result<Self, AxiomataError> {
        match target_type {
            "skill" => Ok(RoutineTarget::Skill(target)),
            "prompt" => Ok(RoutineTarget::Prompt(target)),
            other => Err(AxiomataError::InvalidRoutine {
                reason: format!("unknown routine target_type {other:?}"),
            }),
        }
    }
}

/// The fields a caller supplies to create a routine. [`crate::routines::store::add`]
/// validates the cron expression, computes the initial `next_fire_at`, stamps
/// the timestamps, and returns a full [`Routine`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NewRoutine {
    pub name: String,
    pub cron_expr: String,
    pub target: RoutineTarget,
    /// Backend override: `Some("claude-code")` / `Some("ollama")`, or `None`
    /// to use the target skill's own declared backend (skill target) or the
    /// configured default (prompt target).
    pub backend: Option<String>,
    /// Whether the routine starts enabled. The UI defaults this to `true`.
    pub enabled: bool,
}

/// A stored, scheduled routine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Routine {
    pub id: i64,
    pub name: String,
    /// Cron expression exactly as entered (6-7 fields, seconds required).
    pub cron_expr: String,
    pub target: RoutineTarget,
    pub backend: Option<String>,
    pub enabled: bool,
    /// The next instant this routine should fire. Authoritative and persisted:
    /// loaded from the database on startup, never recomputed from the cron
    /// expression there. `None` only if the expression has no future
    /// occurrence at all.
    pub next_fire_at: Option<DateTime<Utc>>,
    /// When this routine most recently fired (success or failure); `None` if
    /// it never has.
    pub last_fired_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Outcome of a single [`RoutineRun`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineRunStatus {
    /// The target executed and its underlying run recorded success.
    Success,
    /// The target executed and failed, or could not be executed at all
    /// (e.g. the skill no longer exists).
    Failed,
    /// The routine was due while the app was not running. Rolled forward at
    /// startup without firing; recorded for visibility only.
    Missed,
}

impl RoutineRunStatus {
    /// The lowercase token stored in `routine_runs.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            RoutineRunStatus::Success => "success",
            RoutineRunStatus::Failed => "failed",
            RoutineRunStatus::Missed => "missed",
        }
    }

    /// Parses the token stored in `routine_runs.status`.
    ///
    /// # Errors
    ///
    /// Returns a [`rusqlite::Error::FromSqlConversionFailure`] for any value
    /// this enum does not define, rather than silently coercing it.
    pub fn from_db_str(raw: &str, column: &str) -> rusqlite::Result<Self> {
        match raw {
            "success" => Ok(RoutineRunStatus::Success),
            "failed" => Ok(RoutineRunStatus::Failed),
            "missed" => Ok(RoutineRunStatus::Missed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                format!("unexpected {column} value {other:?} in routine_runs").into(),
            )),
        }
    }
}

/// One firing attempt of a routine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineRun {
    pub id: i64,
    pub routine_id: i64,
    /// The `runs.id` of the agent execution this firing produced, if any.
    /// `None` for a `Missed` row, or a failure that never reached the runner.
    pub run_id: Option<i64>,
    /// The `next_fire_at` value this attempt was satisfying.
    pub scheduled_for: DateTime<Utc>,
    /// When the scheduler actually acted on it.
    pub fired_at: DateTime<Utc>,
    pub status: RoutineRunStatus,
    /// Error message, or a short note for `Missed` rows; `None` otherwise.
    pub detail: Option<String>,
}

/// A routine's status as shown in the UI, derived from its fields plus its
/// most recent [`RoutineRun`]. Never stored.
///
/// The plan's "next" status — the single soonest-due routine highlighted in a
/// list — is not a per-row property and is computed by whatever renders the
/// list (the row with the smallest `next_fire_at`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutineState {
    /// Not enabled; the poll loop skips it.
    Disabled,
    /// Enabled and waiting for its next fire; has never fired yet.
    Scheduled,
    /// Enabled; its most recent fire succeeded.
    Fired,
    /// Enabled; its most recent fire failed or was missed.
    Failed,
}

impl RoutineState {
    /// Derives the display status. `last_run` is the routine's most recent
    /// [`RoutineRun`] (by `fired_at`), or `None` if it has never fired.
    pub fn derive(routine: &Routine, last_run: Option<&RoutineRun>) -> Self {
        if !routine.enabled {
            return RoutineState::Disabled;
        }
        match last_run.map(|run| run.status) {
            None => RoutineState::Scheduled,
            Some(RoutineRunStatus::Success) => RoutineState::Fired,
            Some(RoutineRunStatus::Failed | RoutineRunStatus::Missed) => RoutineState::Failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_routine(enabled: bool) -> Routine {
        Routine {
            id: 1,
            name: "nightly".into(),
            cron_expr: "0 0 3 * * *".into(),
            target: RoutineTarget::Skill("example-skill".into()),
            backend: None,
            enabled,
            next_fire_at: None,
            last_fired_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn sample_run(status: RoutineRunStatus) -> RoutineRun {
        RoutineRun {
            id: 7,
            routine_id: 1,
            run_id: Some(42),
            scheduled_for: Utc::now(),
            fired_at: Utc::now(),
            status,
            detail: None,
        }
    }

    #[test]
    fn target_round_trips_through_columns() {
        for target in [
            RoutineTarget::Skill("example-skill".into()),
            RoutineTarget::Prompt("summarize my day".into()),
        ] {
            let (kind, value) = target.to_columns();
            let back = RoutineTarget::from_columns(kind, value.to_string()).unwrap();
            assert_eq!(back, target);
        }
    }

    #[test]
    fn unknown_target_type_is_rejected() {
        let err = RoutineTarget::from_columns("webhook", "x".into()).unwrap_err();
        assert!(matches!(err, AxiomataError::InvalidRoutine { .. }));
    }

    #[test]
    fn run_status_tokens_round_trip() {
        for status in [
            RoutineRunStatus::Success,
            RoutineRunStatus::Failed,
            RoutineRunStatus::Missed,
        ] {
            let parsed = RoutineRunStatus::from_db_str(status.as_str(), "status").unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn unknown_run_status_token_is_rejected() {
        assert!(RoutineRunStatus::from_db_str("weird", "status").is_err());
    }

    #[test]
    fn derived_state_reflects_enabled_flag_and_last_run() {
        assert_eq!(
            RoutineState::derive(&sample_routine(false), None),
            RoutineState::Disabled
        );
        assert_eq!(
            RoutineState::derive(&sample_routine(true), None),
            RoutineState::Scheduled
        );
        assert_eq!(
            RoutineState::derive(
                &sample_routine(true),
                Some(&sample_run(RoutineRunStatus::Success))
            ),
            RoutineState::Fired
        );
        assert_eq!(
            RoutineState::derive(
                &sample_routine(true),
                Some(&sample_run(RoutineRunStatus::Failed))
            ),
            RoutineState::Failed
        );
        assert_eq!(
            RoutineState::derive(
                &sample_routine(true),
                Some(&sample_run(RoutineRunStatus::Missed))
            ),
            RoutineState::Failed
        );
    }
}
