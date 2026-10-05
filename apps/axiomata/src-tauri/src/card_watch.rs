//! Starts the reviewer of every card that was reported, without waiting for a click (`docs/plans/a2a.md`, A3 and A21).
//!
//! A worker session reports its card done from inside its own process; nothing in the app is told. This task looks at
//! the board every few seconds and, for a card in review that a session of the studio worked on and that nobody
//! reviews yet, makes the reviewer session ([`axiomata_core::card_session::start_review_session`]) and tells the
//! frontend, whose Studio opens its pane — the pane is what starts the harness. A review that cannot start on its own
//! (no engine other than the worker's) is reported once and left to the owner, who picks an engine in the Kanban; the
//! studio never runs a reviewer on the worker's own engine.

use std::collections::HashSet;
use std::time::Duration;

use axiomata_core::AxiomataCore;
use axiomata_core::card_session::{self, AwaitingReview, ReviewRequest};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// A reviewer was made; its pane is to be opened.
pub const REVIEW_STARTED: &str = "card:review-started";
/// A review could not start on its own; the owner is asked.
pub const REVIEW_BLOCKED: &str = "card:review-blocked";

/// How often the board is looked at.
const TICK: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewStarted {
    card_id: i64,
    project_id: i64,
    agent_id: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReviewBlocked {
    card_id: i64,
    reason: String,
}

/// Starts the watching task. It lives as long as the app does.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // What was tried for which report; a card sent back and reported again is a new entry.
        let mut tried: HashSet<AwaitingReview> = HashSet::new();
        loop {
            tokio::time::sleep(TICK).await;
            for event in tick(&app, &mut tried).await {
                event.emit(&app);
            }
        }
    });
}

enum Event {
    Started(ReviewStarted),
    Blocked(ReviewBlocked),
}

impl Event {
    fn emit(&self, app: &AppHandle) {
        let sent = match self {
            Event::Started(payload) => app.emit(REVIEW_STARTED, payload),
            Event::Blocked(payload) => app.emit(REVIEW_BLOCKED, payload),
        };
        if let Err(err) = sent {
            tracing::warn!(%err, "could not tell the frontend about a review");
        }
    }
}

/// One look at the board: starts what can be started and returns what to tell the frontend. `tried` is what was
/// attempted for which report.
async fn tick(app: &AppHandle, tried: &mut HashSet<AwaitingReview>) -> Vec<Event> {
    let core = app.state::<AxiomataCore>();
    let mut events = Vec::new();
    let waiting = match card_session::cards_awaiting_review(&core) {
        Ok(waiting) => waiting,
        Err(err) => {
            tracing::warn!(%err, "could not look for cards awaiting review");
            return events;
        }
    };
    // Reports that are not waiting any more need no memory.
    tried.retain(|entry| waiting.contains(entry));
    // At the day's cap no new paid session starts, a reviewer's included; the report is not marked as tried, so the
    // review starts at the next look after the cap is lifted. A plan that is only over its own limit still gets its
    // reviews: the work they judge is done, and the review is what lets it be integrated.
    if axiomata_core::studio_spend::day_cap_reached(&core).is_some() {
        return events;
    }
    for entry in waiting {
        if !tried.insert(entry) {
            continue;
        }
        // Never `allow_agent_config`: a work that changes what agents obey is the owner's to send to a reviewer.
        let request = ReviewRequest {
            card_id: entry.card_id,
            engine_id: None,
            allow_agent_config: false,
        };
        match card_session::start_review_session(&core, &request).await {
            Ok(session) => {
                let config = core.config_read().clone();
                let db = core.db_lock();
                axiomata_core::board_mirror::after_card_change(&db, &config, entry.card_id);
                events.push(Event::Started(ReviewStarted {
                    card_id: entry.card_id,
                    project_id: session.agent.project_id,
                    agent_id: session.agent.id,
                }));
            }
            Err(err) => {
                let reason = err.to_string();
                // The owner started it a moment ago: nothing to say.
                if !reason.contains("being reviewed already") {
                    events.push(Event::Blocked(ReviewBlocked {
                        card_id: entry.card_id,
                        reason,
                    }));
                }
            }
        }
    }
    events
}
