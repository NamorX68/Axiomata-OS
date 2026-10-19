//! Calendar writes straight through EventKit (board card #77): a **recurring** event, which the calendar module's
//! usual path — a one-shot instruct turn on the `apple-reminders` MCP server — cannot make, since that server's
//! `calendar_events` tool treats recurrence rules as read-only.
//!
//! * **Only what the MCP tool cannot do lives here.** A single event still goes through the instruct turn, so the
//!   module keeps one way of writing for everything the provider-skill pattern covers.
//! * **Full access, asked once.** Picking the calendar by name needs to read the calendar list, which write-only access
//!   does not allow. macOS shows its own dialog the first time; the reason text is the app's `Info.plist`
//!   `NSCalendarsFullAccessUsageDescription`.
//! * **Times arrive as Unix seconds**, resolved from the owner's local date and time by the caller; this module knows
//!   nothing of time zones.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

/// How often an event repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frequency {
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

/// The largest repeat interval taken ("every 99 weeks"); beyond it the input is a typo, not a plan.
pub const MAX_INTERVAL: u32 = 99;

/// The window a start or an end date must fall in, as Unix seconds (1970-01-01 to 2100-01-01). Outside it the input is
/// a typo — and a daily series reaching centuries out would be millions of occurrences in a synced calendar.
pub const EARLIEST: f64 = 0.0;
pub const LATEST: f64 = 4_102_444_800.0;

/// Longest title and location kept, in characters; they go into a calendar that syncs to every device.
pub const MAX_TITLE_CHARS: usize = 256;
pub const MAX_LOCATION_CHARS: usize = 512;

/// An event to create, repeating.
#[derive(Debug, Clone, PartialEq)]
pub struct RecurringEvent {
    pub title: String,
    /// The calendar's name as Calendar.app shows it; the first writable calendar of that name is used.
    pub calendar: String,
    /// Unix seconds.
    pub start: f64,
    /// Unix seconds; equal to `start` for an all-day event.
    pub end: f64,
    pub all_day: bool,
    pub location: Option<String>,
    pub frequency: Frequency,
    /// Every `interval` days/weeks/months/years, `1..=MAX_INTERVAL`.
    pub interval: u32,
    /// Unix seconds of the last moment an occurrence may start; `None` repeats without end.
    pub until: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalendarError {
    /// The owner has not given the app access to the calendars (or took it back in System Settings).
    AccessDenied,
    /// No calendar of that name accepts new events.
    NoSuchCalendar(String),
    /// More than one writable calendar has that name (one in iCloud, one in another account).
    AmbiguousCalendar(String),
    /// A request is already waiting (for the owner's answer to the access dialog, or for the save).
    Busy,
    /// A field the caller got wrong.
    Invalid(&'static str),
    /// EventKit refused to save; its own description.
    Save(String),
    /// Not on macOS.
    Unsupported,
}

impl fmt::Display for CalendarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AccessDenied => write!(
                f,
                "Axiomata has no access to your calendars — allow it in System Settings → Privacy & Security → Calendars"
            ),
            Self::NoSuchCalendar(name) => write!(f, "no writable calendar named “{name}”"),
            Self::AmbiguousCalendar(name) => write!(
                f,
                "more than one writable calendar is named “{name}” — rename one in Calendar.app to choose"
            ),
            Self::Busy => write!(f, "another calendar request is still waiting"),
            Self::Invalid(what) => write!(f, "invalid event: {what}"),
            Self::Save(reason) => write!(f, "the calendar refused the event: {reason}"),
            Self::Unsupported => write!(f, "calendar writes need macOS"),
        }
    }
}

impl std::error::Error for CalendarError {}

impl RecurringEvent {
    /// Checks what EventKit would otherwise reject with an exception or an unhelpful error.
    ///
    /// # Errors
    ///
    /// [`CalendarError::Invalid`] naming the first bad field.
    pub fn validate(&self) -> Result<(), CalendarError> {
        if self.title.trim().is_empty() {
            return Err(CalendarError::Invalid("the title is empty"));
        }
        if !plain_text(&self.title, MAX_TITLE_CHARS)
            || !plain_text(&self.calendar, MAX_TITLE_CHARS)
            || self
                .location
                .as_deref()
                .is_some_and(|l| !plain_text(l, MAX_LOCATION_CHARS))
        {
            return Err(CalendarError::Invalid(
                "a text is too long or holds control characters",
            ));
        }
        if !(1..=MAX_INTERVAL).contains(&self.interval) {
            // EventKit raises an Objective-C exception for an interval of 0, which would abort the process.
            return Err(CalendarError::Invalid(
                "the interval must be between 1 and 99",
            ));
        }
        let in_window = |t: f64| (EARLIEST..=LATEST).contains(&t);
        if !in_window(self.start)
            || !in_window(self.end)
            || self.until.is_some_and(|u| !in_window(u))
        {
            // `contains` is false for NaN and the infinities too.
            return Err(CalendarError::Invalid("a date lies outside 1970–2100"));
        }
        if self.end < self.start {
            return Err(CalendarError::Invalid("the event ends before it starts"));
        }
        if self
            .until
            .is_some_and(|until| !until.is_finite() || until < self.start)
        {
            return Err(CalendarError::Invalid(
                "the repetition ends before the first event",
            ));
        }
        Ok(())
    }
}

fn plain_text(text: &str, max_chars: usize) -> bool {
    text.chars().count() <= max_chars && !text.chars().any(char::is_control)
}

/// Set while a request runs: a second one is refused rather than holding another thread for up to two minutes while
/// the first waits on the owner's answer.
static IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Creates `event` with its recurrence rule and returns its EventKit identifier. Blocks while macOS asks the owner for
/// access the first time — call it off the UI thread. One request at a time.
///
/// # Errors
///
/// See [`CalendarError`].
pub fn create_recurring(event: &RecurringEvent) -> Result<String, CalendarError> {
    event.validate()?;
    if IN_FLIGHT.swap(true, Ordering::AcqRel) {
        return Err(CalendarError::Busy);
    }
    let created = imp::create_recurring(event);
    IN_FLIGHT.store(false, Ordering::Release);
    created
}

#[cfg(target_os = "macos")]
mod imp {
    use std::sync::mpsc;
    use std::time::Duration;

    use block2::RcBlock;
    use objc2::AllocAnyThread;
    use objc2::runtime::Bool;
    use objc2_event_kit::{
        EKAuthorizationStatus, EKEntityType, EKEvent, EKEventStore, EKRecurrenceEnd,
        EKRecurrenceFrequency, EKRecurrenceRule, EKSpan,
    };
    use objc2_foundation::{NSDate, NSError, NSString};

    use super::{CalendarError, Frequency, RecurringEvent};

    /// How long the owner gets to answer macOS's access dialog.
    const ACCESS_ANSWER_TIMEOUT: Duration = Duration::from_secs(120);

    pub(super) fn create_recurring(event: &RecurringEvent) -> Result<String, CalendarError> {
        // SAFETY: plain EventKit calls on objects created and used on this one thread; the completion block owns only
        // a channel sender, and EventKit copies the block before returning.
        unsafe {
            let store = EKEventStore::new();
            ensure_access(&store)?;
            let calendars = store.calendarsForEntityType(EKEntityType::Event);
            let mut named = calendars.iter().filter(|c| {
                c.allowsContentModifications() && c.title().to_string() == event.calendar
            });
            let calendar = named
                .next()
                .ok_or_else(|| CalendarError::NoSuchCalendar(event.calendar.clone()))?;
            // The MCP path tells same-named calendars apart by account; a name alone cannot, so it does not guess.
            if named.next().is_some() {
                return Err(CalendarError::AmbiguousCalendar(event.calendar.clone()));
            }

            let item = EKEvent::eventWithEventStore(&store);
            item.setTitle(Some(&NSString::from_str(event.title.trim())));
            item.setCalendar(Some(&calendar));
            item.setAllDay(event.all_day);
            item.setStartDate(Some(&NSDate::dateWithTimeIntervalSince1970(event.start)));
            item.setEndDate(Some(&NSDate::dateWithTimeIntervalSince1970(event.end)));
            if let Some(location) = event.location.as_deref().filter(|l| !l.trim().is_empty()) {
                item.setLocation(Some(&NSString::from_str(location.trim())));
            }
            let end = event.until.map(|until| {
                EKRecurrenceEnd::recurrenceEndWithEndDate(&NSDate::dateWithTimeIntervalSince1970(
                    until,
                ))
            });
            let rule = EKRecurrenceRule::initRecurrenceWithFrequency_interval_end(
                EKRecurrenceRule::alloc(),
                frequency(event.frequency),
                event.interval as isize,
                end.as_deref(),
            );
            item.addRecurrenceRule(&rule);
            store
                .saveEvent_span_error(&item, EKSpan::FutureEvents)
                .map_err(|err| CalendarError::Save(err.localizedDescription().to_string()))?;
            item.eventIdentifier()
                .map(|id| id.to_string())
                .ok_or_else(|| CalendarError::Save("the saved event has no identifier".into()))
        }
    }

    fn frequency(frequency: Frequency) -> EKRecurrenceFrequency {
        match frequency {
            Frequency::Daily => EKRecurrenceFrequency::Daily,
            Frequency::Weekly => EKRecurrenceFrequency::Weekly,
            Frequency::Monthly => EKRecurrenceFrequency::Monthly,
            Frequency::Yearly => EKRecurrenceFrequency::Yearly,
        }
    }

    /// Full access to events, asking the owner when nobody has yet.
    fn ensure_access(store: &EKEventStore) -> Result<(), CalendarError> {
        // SAFETY: see `create_recurring`.
        let status = unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) };
        match status {
            EKAuthorizationStatus::FullAccess => return Ok(()),
            EKAuthorizationStatus::NotDetermined => {}
            _ => return Err(CalendarError::AccessDenied),
        }
        let (tx, rx) = mpsc::channel();
        let answer = RcBlock::new(move |granted: Bool, _error: *mut NSError| {
            // The receiver may be gone after a timeout; nobody is left to tell.
            let _ = tx.send(granted.as_bool());
        });
        // SAFETY: `answer` is a valid block for the duration of the call; EventKit copies it.
        unsafe { store.requestFullAccessToEventsWithCompletion(&*answer as *const _ as *mut _) };
        match rx.recv_timeout(ACCESS_ANSWER_TIMEOUT) {
            Ok(true) => Ok(()),
            _ => Err(CalendarError::AccessDenied),
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{CalendarError, RecurringEvent};

    pub(super) fn create_recurring(_event: &RecurringEvent) -> Result<String, CalendarError> {
        Err(CalendarError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event() -> RecurringEvent {
        RecurringEvent {
            title: "Training".into(),
            calendar: "Privat".into(),
            start: 1_800_000_000.0,
            end: 1_800_003_600.0,
            all_day: false,
            location: None,
            frequency: Frequency::Weekly,
            interval: 1,
            until: Some(1_810_000_000.0),
        }
    }

    #[test]
    fn a_complete_event_is_valid() {
        assert_eq!(event().validate(), Ok(()));
        assert_eq!(
            RecurringEvent {
                until: None,
                ..event()
            }
            .validate(),
            Ok(())
        );
    }

    #[test]
    fn the_fields_eventkit_would_choke_on_are_refused_first() {
        let bad = [
            RecurringEvent {
                title: "  ".into(),
                ..event()
            },
            RecurringEvent {
                interval: 0,
                ..event()
            },
            RecurringEvent {
                interval: MAX_INTERVAL + 1,
                ..event()
            },
            RecurringEvent {
                end: 1_799_999_999.0,
                ..event()
            },
            RecurringEvent {
                start: f64::NAN,
                ..event()
            },
            RecurringEvent {
                until: Some(1_799_000_000.0),
                ..event()
            },
            RecurringEvent {
                until: Some(f64::INFINITY),
                ..event()
            },
            RecurringEvent {
                until: Some(LATEST + 1.0),
                ..event()
            },
            RecurringEvent {
                start: -1.0,
                ..event()
            },
            RecurringEvent {
                title: "x".repeat(MAX_TITLE_CHARS + 1),
                ..event()
            },
            RecurringEvent {
                title: "line\nbreak".into(),
                ..event()
            },
            RecurringEvent {
                location: Some("y".repeat(MAX_LOCATION_CHARS + 1)),
                ..event()
            },
        ];
        for event in bad {
            assert!(
                matches!(event.validate(), Err(CalendarError::Invalid(_))),
                "{event:?}"
            );
        }
    }
}
