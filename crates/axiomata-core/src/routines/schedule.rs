//! Cron expression handling for routines.
//!
//! Thin wrapper over the [`cron`] crate. Its native format is **6 or 7
//! fields** with seconds first — `second minute hour day-of-month month
//! day-of-week [year]` — not the 5-field crontab format. So "every two
//! minutes" is `0 */2 * * * *`, and "every 30 seconds" is `*/30 * * * * *`.
//!
//! The [`cron::Schedule`] type is kept an implementation detail here: callers
//! pass the expression string and get back a concrete next-fire instant, so
//! the rest of the routines module never depends on the `cron` crate.

use std::str::FromStr;

use chrono::{DateTime, Utc};
use cron::Schedule;

use crate::error::AxiomataError;

/// Checks that `expr` is a well-formed cron expression.
///
/// Called when a routine is created or edited so a bad expression is rejected
/// up front rather than silently never firing.
///
/// # Errors
///
/// Returns [`AxiomataError::InvalidCron`] with the parser's message if the
/// expression is malformed (including a 5-field crontab expression — this
/// crate needs the leading seconds field). This function only ever sees a
/// freshly supplied expression, so it's always the user-input variant — a
/// caller re-parsing an already-stored, previously-valid expression (see
/// [`crate::routines::store::advance`]) translates a failure here into
/// [`AxiomataError::CorruptRoutineRow`] instead, since at that point it can
/// only mean the stored row was corrupted after the fact.
pub fn validate(expr: &str) -> Result<(), AxiomataError> {
    parse(expr).map(|_| ())
}

/// Returns the first instant strictly after `after` at which `expr` fires, or
/// `None` if the expression has no further occurrence (e.g. it pins a year
/// that is already past).
///
/// # Errors
///
/// Returns [`AxiomataError::InvalidCron`] if `expr` does not parse — see
/// [`validate`] for the user-input-vs-corrupt-row distinction.
pub fn next_after(
    expr: &str,
    after: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, AxiomataError> {
    Ok(parse(expr)?.after(&after).next())
}

/// Parses `expr` into a [`Schedule`], mapping the crate's error into ours.
fn parse(expr: &str) -> Result<Schedule, AxiomataError> {
    let trimmed = expr.trim();
    Schedule::from_str(trimmed).map_err(|source| AxiomataError::InvalidCron {
        expr: trimmed.to_owned(),
        reason: source.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn accepts_a_six_field_expression() {
        assert!(validate("0 */2 * * * *").is_ok());
        assert!(validate("*/30 * * * * *").is_ok());
        assert!(validate("0 0 3 * * Mon").is_ok());
    }

    #[test]
    fn rejects_a_five_field_crontab_expression() {
        let err = validate("*/2 * * * *").unwrap_err();
        assert!(matches!(err, AxiomataError::InvalidCron { .. }));
    }

    #[test]
    fn rejects_obvious_garbage() {
        assert!(validate("not a cron expression").is_err());
        assert!(validate("").is_err());
    }

    #[test]
    fn next_after_advances_by_one_occurrence() {
        // Every two minutes on the minute.
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 30).unwrap();
        let next = next_after("0 */2 * * * *", from).unwrap().unwrap();
        assert_eq!(next, Utc.with_ymd_and_hms(2026, 1, 1, 12, 2, 0).unwrap());

        // From exactly on a firing instant, the *next* one is returned (the
        // boundary is exclusive), so a routine cannot re-fire the same slot.
        let on_slot = Utc.with_ymd_and_hms(2026, 1, 1, 12, 2, 0).unwrap();
        let after = next_after("0 */2 * * * *", on_slot).unwrap().unwrap();
        assert_eq!(after, Utc.with_ymd_and_hms(2026, 1, 1, 12, 4, 0).unwrap());
    }

    #[test]
    fn a_past_pinned_year_has_no_next_occurrence() {
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        // Valid expression, but the year is in the past.
        let next = next_after("0 0 0 1 1 * 2020", from).unwrap();
        assert!(next.is_none());
    }
}
