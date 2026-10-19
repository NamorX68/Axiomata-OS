//! The calendar module's one direct write (board card #77): a recurring event, through EventKit
//! (`axiomata_macos::calendar`), since the MCP tool behind every other calendar write cannot set a recurrence rule.
//!
//! The form speaks the owner's local date and time (`YYYY-MM-DD`, `HH:MM`); this resolves them to instants in the
//! Mac's time zone before EventKit sees them.

use axiomata_macos::calendar::{Frequency, MAX_INTERVAL, RecurringEvent};
use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use serde::Deserialize;

/// What the form sends; mirrors `NewRecurringEvent` in `src/core/calendar.ts`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecurringEventInput {
    pub title: String,
    pub calendar: String,
    /// `YYYY-MM-DD`, the first occurrence.
    pub date: String,
    /// `HH:MM`; `None` for an all-day event.
    pub start_time: Option<String>,
    /// `HH:MM`; `None` = an hour, capped at 23:59 of the same day (as `resolveEventTimes` in `core/calendar.ts`).
    pub end_time: Option<String>,
    pub location: Option<String>,
    /// `daily` | `weekly` | `monthly` | `yearly`.
    pub frequency: String,
    pub interval: u32,
    /// `YYYY-MM-DD`, the last day an occurrence may fall on; `None` repeats without end.
    pub until: Option<String>,
}

/// A timed event without an end lasts this long, as in Calendar.app — but never past the day's last minute, so a late
/// start does not make a series that crosses midnight.
const DEFAULT_DURATION: chrono::TimeDelta = chrono::TimeDelta::hours(1);

/// Resolves the form's fields to the event EventKit gets.
///
/// # Errors
///
/// A message naming the field that does not parse or does not fit.
pub fn resolve(input: &RecurringEventInput) -> Result<RecurringEvent, String> {
    let date = parse_date(&input.date, "date")?;
    let frequency = match input.frequency.as_str() {
        "daily" => Frequency::Daily,
        "weekly" => Frequency::Weekly,
        "monthly" => Frequency::Monthly,
        "yearly" => Frequency::Yearly,
        other => return Err(format!("unknown repetition {other:?}")),
    };
    if !(1..=MAX_INTERVAL).contains(&input.interval) {
        return Err(format!("the interval must be between 1 and {MAX_INTERVAL}"));
    }
    let (start, end, all_day) = match input.start_time.as_deref().filter(|t| !t.is_empty()) {
        None => {
            let midnight = local_seconds(date.and_time(NaiveTime::MIN))?;
            (midnight, midnight, true)
        }
        Some(start_time) => {
            let start = local_seconds(date.and_time(parse_time(start_time, "start time")?))?;
            let end = match input.end_time.as_deref().filter(|t| !t.is_empty()) {
                Some(end_time) => local_seconds(date.and_time(parse_time(end_time, "end time")?))?,
                None => {
                    let start_time = parse_time(start_time, "start time")?;
                    let last_minute = NaiveTime::from_hms_opt(23, 59, 0).ok_or("no last minute")?;
                    let (end_time, wrapped) = start_time.overflowing_add_signed(DEFAULT_DURATION);
                    let end_time = if wrapped != 0 {
                        last_minute
                    } else {
                        end_time.min(last_minute)
                    };
                    local_seconds(date.and_time(end_time.max(start_time)))?
                }
            };
            if end < start {
                return Err("the event ends before it starts".into());
            }
            (start, end, false)
        }
    };
    let until = match input.until.as_deref().filter(|u| !u.is_empty()) {
        None => None,
        Some(until) => {
            let last_day = parse_date(until, "end date")?;
            if last_day < date {
                return Err("the repetition ends before the first event".into());
            }
            // The whole last day counts: an occurrence that starts on it is still included.
            let end_of_day = last_day
                .and_hms_opt(23, 59, 59)
                .ok_or("the end date is out of range")?;
            Some(local_seconds(end_of_day)?)
        }
    };
    Ok(RecurringEvent {
        title: input.title.trim().to_string(),
        calendar: input.calendar.clone(),
        start,
        end,
        all_day,
        location: input.location.clone(),
        frequency,
        interval: input.interval,
        until,
    })
}

fn parse_date(text: &str, what: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| format!("the {what} {text:?} is not YYYY-MM-DD"))
}

fn parse_time(text: &str, what: &str) -> Result<NaiveTime, String> {
    NaiveTime::parse_from_str(text, "%H:%M")
        .map_err(|_| format!("the {what} {text:?} is not HH:MM"))
}

/// A local wall-clock time as Unix seconds. In the hour a clock change skips there is no such time; in the hour it
/// repeats, the earlier one is taken.
fn local_seconds(at: NaiveDateTime) -> Result<f64, String> {
    Local
        .from_local_datetime(&at)
        .earliest()
        .map(|t| t.timestamp() as f64)
        .ok_or_else(|| format!("{at} does not exist in this time zone (a clock change)"))
}

/// Creates a recurring event in Calendar and returns its identifier. The first call asks the owner for calendar access.
#[tauri::command]
pub async fn calendar_create_recurring(input: RecurringEventInput) -> Result<String, String> {
    let event = resolve(&input)?;
    tokio::task::spawn_blocking(move || axiomata_macos::calendar::create_recurring(&event))
        .await
        .map_err(|err| format!("calendar task failed: {err}"))?
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> RecurringEventInput {
        RecurringEventInput {
            title: " Training ".into(),
            calendar: "Privat".into(),
            date: "2026-10-20".into(),
            start_time: Some("18:30".into()),
            end_time: Some("20:00".into()),
            location: None,
            frequency: "weekly".into(),
            interval: 2,
            until: Some("2026-12-22".into()),
        }
    }

    #[test]
    fn a_timed_event_resolves_in_local_time_with_the_whole_last_day() {
        let event = resolve(&input()).unwrap();
        assert_eq!(event.title, "Training");
        assert_eq!(event.end - event.start, 90.0 * 60.0);
        assert!(!event.all_day);
        assert_eq!(event.frequency, Frequency::Weekly);
        let start = Local.timestamp_opt(event.start as i64, 0).unwrap();
        assert_eq!(
            start.format("%Y-%m-%d %H:%M").to_string(),
            "2026-10-20 18:30"
        );
        let until = Local.timestamp_opt(event.until.unwrap() as i64, 0).unwrap();
        assert_eq!(
            until.format("%Y-%m-%d %H:%M:%S").to_string(),
            "2026-12-22 23:59:59"
        );
    }

    #[test]
    fn all_day_and_open_ended_events_and_a_missing_end_time() {
        let all_day = resolve(&RecurringEventInput {
            start_time: None,
            until: None,
            ..input()
        })
        .unwrap();
        assert!(all_day.all_day);
        assert_eq!(all_day.start, all_day.end);
        assert_eq!(all_day.until, None);
        let no_end = resolve(&RecurringEventInput {
            end_time: None,
            ..input()
        })
        .unwrap();
        assert_eq!(no_end.end - no_end.start, 3600.0);
        let late = resolve(&RecurringEventInput {
            start_time: Some("23:30".into()),
            end_time: None,
            ..input()
        })
        .unwrap();
        assert_eq!(late.end - late.start, 29.0 * 60.0);
        let last_day_is_first_day = resolve(&RecurringEventInput {
            until: Some("2026-10-20".into()),
            interval: 99,
            ..input()
        });
        assert!(last_day_is_first_day.is_ok());
    }

    #[test]
    fn bad_fields_are_named() {
        let cases = [
            RecurringEventInput {
                date: "20.10.2026".into(),
                ..input()
            },
            RecurringEventInput {
                start_time: Some("6pm".into()),
                ..input()
            },
            RecurringEventInput {
                end_time: Some("17:00".into()),
                ..input()
            },
            RecurringEventInput {
                frequency: "hourly".into(),
                ..input()
            },
            RecurringEventInput {
                interval: 0,
                ..input()
            },
            RecurringEventInput {
                until: Some("2026-10-19".into()),
                ..input()
            },
        ];
        for case in cases {
            assert!(resolve(&case).is_err(), "{case:?}");
        }
    }
}
