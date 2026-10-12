//! Tells the app, and the owner, when an agent waits for them (`docs/plans/agent-waiting.md`, board card #87).
//!
//! The work is [`axiomata_core::ide_status::waiting_agents`]: every agent of every project whose harness waits on a
//! permission prompt or a question. This task asks for it every few seconds, sends the whole list to the frontend when
//! it changed (the badge in the Dock, the Orbit and the Kanban), and posts one macOS notification for an agent that has
//! just begun to wait while no window of the app is focused. It pushes from Rust rather than letting the page poll
//! because WebKit throttles the timers of a window in the background — exactly when the notification matters.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axiomata_core::AxiomataCore;
use axiomata_core::ide_status::{self, WaitingAgent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// The list of waiting agents changed; the payload is the whole new list.
pub const AGENTS_WAITING: &str = "agents:waiting";

/// How often the channels are read. A prompt nobody answers holds a session still, so a few seconds count; the reading
/// is a few small files and two brief database locks.
const TICK: Duration = Duration::from_secs(2);

/// The least time between two notifications for one agent: a session that flips in and out of `waiting` (a prompt
/// answered and the next one asked at once, or a harness that misbehaves) must not post one every tick.
const COOLDOWN: Duration = Duration::from_secs(60);

/// How many characters of an agent's name and of a card's title the notification shows. Both can be written by an
/// agent (a card title through MCP), so they are also cut down to one plain line.
const NAME_CHARS: usize = 40;
const TITLE_CHARS: usize = 80;

/// Starts the watching task. It lives as long as the app does.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // The Opencode agents' statuses come from the service's event stream; without its watcher they would read as
        // their file channel says, which never says `waiting`.
        ide_status::ensure_running(Arc::clone(&app.state::<AxiomataCore>().db));
        let mut last: Vec<WaitingAgent> = Vec::new();
        // The first reading only sets the scene: an agent that was already waiting when the app started is shown by
        // the badge, not announced as news.
        let mut seeded = false;
        let mut cooldown = Cooldown::default();
        loop {
            tokio::time::sleep(TICK).await;
            let Some(now) = read(&app).await else {
                continue;
            };
            if seeded && now == last {
                continue;
            }
            if let Err(err) = app.emit(AGENTS_WAITING, &now) {
                tracing::warn!(%err, "could not tell the frontend which agents wait");
            }
            if seeded && !app_is_focused(&app) {
                for agent in newly_waiting(&last, &now) {
                    if cooldown.due(agent.agent_id, Instant::now()) {
                        notify(&app, agent);
                    }
                }
            }
            seeded = true;
            last = now;
        }
    });
}

/// The waiting agents now, read off the async runtime (file reads and database locks); `None` when that failed — the
/// last list then stands, so a brief hiccup does not read as "nobody waits".
async fn read(app: &AppHandle) -> Option<Vec<WaitingAgent>> {
    let db = Arc::clone(&app.state::<AxiomataCore>().db);
    let roots = axiomata_core::paths::ide_locations().channels;
    match tauri::async_runtime::spawn_blocking(move || ide_status::waiting_agents(&db, &roots))
        .await
    {
        Ok(Ok(waiting)) => Some(waiting),
        Ok(Err(err)) => {
            tracing::warn!(%err, "could not read which agents wait");
            None
        }
        Err(err) => {
            tracing::warn!(%err, "the waiting-agents task failed");
            None
        }
    }
}

/// The agents in `now` that were not waiting in `before` — a wait that went on is not new; one that ended and began
/// again is (its agent left the list in between).
fn newly_waiting<'a>(before: &[WaitingAgent], now: &'a [WaitingAgent]) -> Vec<&'a WaitingAgent> {
    let known: HashSet<i64> = before.iter().map(|agent| agent.agent_id).collect();
    now.iter()
        .filter(|agent| !known.contains(&agent.agent_id))
        .collect()
}

/// When each agent was last announced, for [`COOLDOWN`].
#[derive(Default)]
struct Cooldown {
    announced: HashMap<i64, Instant>,
}

impl Cooldown {
    /// Whether `agent_id` may be announced at `now`; if so, `now` is noted as its last announcement.
    fn due(&mut self, agent_id: i64, now: Instant) -> bool {
        if let Some(last) = self.announced.get(&agent_id)
            && now.duration_since(*last) < COOLDOWN
        {
            return false;
        }
        self.announced.insert(agent_id, now);
        true
    }
}

/// Whether the owner is looking at the app: then the badge says it and a notification would only be noise.
fn app_is_focused(app: &AppHandle) -> bool {
    app.webview_windows()
        .values()
        .any(|window| window.is_focused().unwrap_or(false))
}

/// The notification for one agent. Its text comes from the database, never from the page; no sound (W5).
fn notify(app: &AppHandle, agent: &WaitingAgent) {
    let (title, body) = notification_text(agent);
    if let Err(err) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!(%err, agent = agent.agent_id, "could not post the waiting notification");
    }
}

/// The notification's title and body.
fn notification_text(agent: &WaitingAgent) -> (String, String) {
    let title = format!("{} is waiting for you", one_line(&agent.name, NAME_CHARS));
    let body = match (agent.card_id, &agent.card_title) {
        (Some(id), Some(card)) => format!("Card #{id} · {}", one_line(card, TITLE_CHARS)),
        (Some(id), None) => format!("Card #{id}"),
        (None, _) => "A permission prompt or a question in the Studio".to_owned(),
    };
    (title, body)
}

/// `text` as one plain line of at most `max` characters: control characters (line breaks among them) and the
/// invisible direction marks that could make it read as something else become spaces, runs of spaces one.
fn one_line(text: &str, max: usize) -> String {
    let is_bidi = |c: char| matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
    let plain: String = text
        .chars()
        .map(|c| if c.is_control() || is_bidi(c) { ' ' } else { c })
        .collect();
    let line = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        return line;
    }
    let cut: String = line.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting(agent_id: i64, card: Option<(i64, &str)>) -> WaitingAgent {
        WaitingAgent {
            agent_id,
            project_id: 1,
            name: format!("implementer-{agent_id}"),
            card_id: card.map(|(id, _)| id),
            card_title: card.map(|(_, title)| title.to_owned()),
            since: None,
        }
    }

    #[test]
    fn only_an_agent_that_was_not_waiting_before_is_new() {
        let before = vec![waiting(1, None), waiting(2, None)];
        let now = vec![waiting(2, None), waiting(3, None)];
        let new: Vec<i64> = newly_waiting(&before, &now)
            .iter()
            .map(|a| a.agent_id)
            .collect();
        assert_eq!(new, vec![3]);
        assert!(newly_waiting(&now, &now).is_empty());
    }

    #[test]
    fn an_agent_is_announced_at_most_once_a_minute() {
        let mut cooldown = Cooldown::default();
        let start = Instant::now();
        assert!(cooldown.due(1, start));
        assert!(!cooldown.due(1, start + Duration::from_secs(10)));
        assert!(cooldown.due(2, start + Duration::from_secs(10)));
        assert!(cooldown.due(1, start + COOLDOWN));
    }

    #[test]
    fn a_title_written_by_an_agent_becomes_one_short_plain_line() {
        assert_eq!(one_line("Fix\nthe\t parser", 80), "Fix the parser");
        assert_eq!(one_line("a\u{202E}b\u{2066}c", 80), "a b c");
        assert_eq!(one_line("abcdefghij", 5), "abcd…");
        assert_eq!(one_line("abcde", 5), "abcde");
        let long = notification_text(&waiting(7, Some((1, &"x".repeat(500)))));
        assert_eq!(
            long.1.chars().count(),
            "Card #1 · ".chars().count() + TITLE_CHARS
        );
    }

    #[test]
    fn the_notification_names_the_agent_and_its_card() {
        assert_eq!(
            notification_text(&waiting(4, Some((12, "Fix the parser")))),
            (
                "implementer-4 is waiting for you".to_owned(),
                "Card #12 · Fix the parser".to_owned()
            )
        );
        assert_eq!(
            notification_text(&waiting(5, Some((13, "")))).1,
            "Card #13 · "
        );
        assert!(notification_text(&waiting(6, None)).1.contains("Studio"));
    }
}
