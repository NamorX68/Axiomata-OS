//! Stops a card session that has used up its limits (`docs/plans/a2a.md`, A9 and CP-A6c).
//!
//! The work is [`axiomata_core::session_limits::Meter::check`]: it reads what each session the studio started for a
//! card has used, writes the stop and, for Opencode, interrupts the session. This task only calls it every few seconds
//! and tells the frontend about a stop made just now, so the owner hears of it without having to open the card. It
//! lives as long as the app does; without the app nothing is enforced on an Opencode session, and a Claude Code
//! session is refused tool calls only by a stop that was written while the app ran.

use std::time::Duration;

use axiomata_core::AxiomataCore;
use axiomata_core::session_limits::Meter;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// A card session was stopped at a limit.
pub const LIMIT_REACHED: &str = "card:limit-reached";

/// How often the sessions are looked at. Slower than the review watcher: reading a transcript and asking Opencode for
/// a session's messages is more work than a look at the board, and a limit is not a thing a few seconds decide.
const TICK: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LimitReached {
    card_id: i64,
    agent_name: String,
    reason: String,
}

/// Starts the watching task. It lives as long as the app does.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut meter = Meter::default();
        loop {
            tokio::time::sleep(TICK).await;
            let core = app.state::<AxiomataCore>();
            for breach in meter.check(&core).await {
                let payload = LimitReached {
                    card_id: breach.card_id,
                    agent_name: breach.agent_name,
                    reason: breach.reason,
                };
                if let Err(err) = app.emit(LIMIT_REACHED, &payload) {
                    tracing::warn!(%err, "could not tell the frontend about a stopped session");
                }
            }
        }
    });
}
