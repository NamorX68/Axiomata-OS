//! Tells the app, and the owner, when an agent waits for them (`docs/plans/agent-waiting.md`, board card #87).
//!
//! The work is [`axiomata_core::ide_status::waiting_agents`]: every agent of every project whose harness waits on a
//! permission prompt or a question. This task asks for it every few seconds, sends the whole list to the frontend when
//! it changed (the badge in the Dock, the Orbit and the Kanban), and posts one macOS notification per wait for an agent
//! that waits while no window of the app is focused — also one that began to wait while the owner was still looking:
//! leaving the app with a question open is exactly when it is forgotten. It pushes from Rust rather than letting the page poll
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
        let mut last: Option<Vec<WaitingAgent>> = None;
        let mut announcer = Announcer::default();
        loop {
            tokio::time::sleep(TICK).await;
            let Some(now) = read(&app).await else {
                continue;
            };
            if last.as_ref() != Some(&now)
                && let Err(err) = app.emit(AGENTS_WAITING, &now)
            {
                tracing::warn!(%err, "could not tell the frontend which agents wait");
            }
            // Every tick, not only on a change: the owner leaving the app is a change the list does not show.
            for agent in announcer.due_now(&now, app_is_focused(&app), Instant::now()) {
                notify(&app, agent);
            }
            last = Some(now);
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

/// Which waits have been announced: one notification per wait, only while no window of the app is focused, and at
/// most one per agent within [`COOLDOWN`].
#[derive(Default)]
struct Announcer {
    /// Agents whose current wait was announced; an agent leaves when its wait ends, so its next wait is news again.
    announced: HashSet<i64>,
    cooldown: Cooldown,
}

impl Announcer {
    /// The agents of `waiting` to announce now. While the app is focused the badge says it, and nothing is announced —
    /// but nothing is marked either: leaving the app with the wait still open announces it then.
    fn due_now<'a>(
        &mut self,
        waiting: &'a [WaitingAgent],
        focused: bool,
        now: Instant,
    ) -> Vec<&'a WaitingAgent> {
        self.announced
            .retain(|id| waiting.iter().any(|agent| agent.agent_id == *id));
        if focused {
            return Vec::new();
        }
        let mut due = Vec::new();
        for agent in waiting {
            if !self.announced.contains(&agent.agent_id) && self.cooldown.due(agent.agent_id, now) {
                self.announced.insert(agent.agent_id);
                due.push(agent);
            }
        }
        due
    }
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
    tracing::info!(agent = agent.agent_id, "announcing a waiting agent");
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
            reason: axiomata_core::ide_status::WaitReason::Prompt,
            ask: None,
        }
    }

    #[test]
    fn a_wait_is_announced_once_and_only_while_the_owner_is_away_from_the_app() {
        let mut announcer = Announcer::default();
        let start = Instant::now();
        let ids =
            |agents: Vec<&WaitingAgent>| agents.iter().map(|a| a.agent_id).collect::<Vec<_>>();
        let one = vec![waiting(1, None)];
        // It began while the owner looked: nothing, and nothing used up.
        assert!(announcer.due_now(&one, true, start).is_empty());
        // The owner leaves the app with the wait still open: now.
        assert_eq!(ids(announcer.due_now(&one, false, start)), vec![1]);
        // Once per wait.
        assert!(
            announcer
                .due_now(&one, false, start + COOLDOWN * 2)
                .is_empty()
        );
        // A second agent waits meanwhile: it is news.
        let two = vec![waiting(1, None), waiting(2, None)];
        assert_eq!(
            ids(announcer.due_now(&two, false, start + COOLDOWN * 2)),
            vec![2]
        );
        // Agent 1's wait ends and a new one begins soon after: within the cooldown, held back; after it, news.
        let _ = announcer.due_now(&[waiting(2, None)], false, start + COOLDOWN * 2);
        assert!(
            announcer
                .due_now(&two, false, start + Duration::from_secs(10))
                .is_empty()
        );
        assert_eq!(
            ids(announcer.due_now(&two, false, start + COOLDOWN * 3)),
            vec![1]
        );
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
