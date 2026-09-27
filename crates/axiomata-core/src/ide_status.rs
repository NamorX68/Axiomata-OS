//! The IDE's Opencode agents' status, from the Opencode service (plan OC3).
//!
//! Opencode 2 runs every session on one shared service, so what an Opencode
//! agent is doing is read from that service's event stream rather than from a
//! file its harness writes (Claude Code still uses the file channel of
//! `axiomata_ide::lifecycle`). One watcher per process follows the stream into
//! an [`axiomata_opencode::Tracker`]; [`overlay`] lays what it knows over the
//! statuses the file channel reported.
//!
//! The watcher starts the first time the IDE asks ([`ensure_running`]), never
//! starts the Opencode service itself (no service, no running agent), and
//! re-reads every agent session after each (re)connect — the stream replays
//! nothing, so a permission request that arrived while it was down is found by
//! asking the service.
//!
//! While the stream is down, a session the tracker already knows keeps its
//! last state until the next seed corrects it (after 2–30 s) — accepted
//! rather than letting every dot flicker to "unknown" on a brief drop.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use axiomata_opencode::{EventStream, OpencodeError, Service, SessionState, Tracker};
use chrono::{DateTime, Utc};
use rusqlite::Connection;

use crate::agents::opencode;
use crate::ide::agent_store;
use crate::ide::lifecycle::{AgentState, AgentStatus, PlanDocument};
use crate::ide::model::{Agent, Harness};

/// First pause before looking for the service again.
const RETRY_MIN: Duration = Duration::from_secs(2);
/// Longest pause between two looks.
const RETRY_MAX: Duration = Duration::from_secs(30);
/// The heading the Plan tab shows above a plan-agent answer.
const PLAN_NAME: &str = "plan agent";

/// The watcher's shared state.
#[derive(Default)]
struct Watch {
    tracker: Mutex<Tracker>,
    /// Whether the stream is up and the tracker seeded — only then does "no
    /// entry" mean "idle" rather than "unknown".
    connected: AtomicBool,
}

static WATCH: OnceLock<Arc<Watch>> = OnceLock::new();

/// Starts the watcher, once per process. Call from within the tokio runtime.
/// A no-op under `cfg(test)`.
pub fn ensure_running(db: Arc<Mutex<Connection>>) {
    if cfg!(test) {
        return;
    }
    WATCH.get_or_init(|| {
        let watch = Arc::new(Watch::default());
        tokio::spawn(run(Arc::clone(&watch), db));
        watch
    });
}

/// Lays the Opencode service's view over the file channel's statuses, for
/// Opencode agents on the generated command (an own command gets no session).
pub fn overlay(statuses: &mut [AgentStatus], agents: &[Agent]) {
    let Some(watch) = WATCH.get() else {
        return;
    };
    let connected = watch.connected.load(Ordering::Relaxed);
    let tracker = watch
        .tracker
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    overlay_with(statuses, agents, &tracker, connected);
}

/// [`overlay`] from one fresh look at the service instead of the watcher —
/// for a one-shot caller like the CLI. Leaves the statuses as they are when
/// no service is running.
pub async fn overlay_once(statuses: &mut [AgentStatus], agents: &[Agent]) {
    let Ok(service) = opencode::find().await else {
        return;
    };
    let Ok(active) = service.active_sessions().await else {
        return;
    };
    let mut tracker = Tracker::default();
    let sessions = agents
        .iter()
        .filter(|a| a.harness == Harness::Opencode && a.command.trim().is_empty())
        .filter_map(|a| a.opencode_session.as_deref());
    for session in sessions {
        if let Ok(snapshot) = service.session_snapshot(session, &active).await {
            tracker.set(session, snapshot.state, now_ms());
            if let Some(plan) = snapshot.plan {
                tracker.set_plan(session, plan, now_ms());
            }
        }
    }
    overlay_with(statuses, agents, &tracker, true);
}

/// [`overlay`] against a given tracker, so it is testable without the watcher.
fn overlay_with(
    statuses: &mut [AgentStatus],
    agents: &[Agent],
    tracker: &Tracker,
    connected: bool,
) {
    for status in statuses.iter_mut() {
        let Some(session) = agents
            .iter()
            .find(|a| a.id == status.agent_id)
            .filter(|a| a.harness == Harness::Opencode && a.command.trim().is_empty())
            .and_then(|a| a.opencode_session.as_deref())
        else {
            continue;
        };
        match tracker.get(session) {
            Some(entry) => {
                status.state = agent_state(entry.state);
                status.since = from_ms(entry.since_ms);
                status.plan_document = entry.plan.as_ref().map(|plan| {
                    let updated_at = from_ms(plan.at_ms);
                    PlanDocument {
                        markdown: plan.markdown.clone(),
                        name: PLAN_NAME.to_string(),
                        updated_at,
                        from_earlier_session: matches!(
                            (updated_at, status.started_at),
                            (Some(at), Some(started)) if at < started
                        ),
                    }
                });
            }
            // A session nothing has happened in since the watcher connected.
            None if connected => {
                status.state = AgentState::Idle;
                status.since = status.started_at;
            }
            None => {}
        }
    }
}

fn agent_state(state: SessionState) -> AgentState {
    match state {
        SessionState::Idle => AgentState::Idle,
        SessionState::Working => AgentState::Working,
        SessionState::Waiting => AgentState::Waiting,
    }
}

fn from_ms(ms: i64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(ms)
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Looks for the service, follows it while it is there, and looks again
/// with a growing pause when it is not.
async fn run(watch: Arc<Watch>, db: Arc<Mutex<Connection>>) {
    let mut pause = RETRY_MIN;
    loop {
        if let Ok(service) = opencode::find().await {
            pause = RETRY_MIN;
            if let Err(err) = follow(&watch, &service, &db).await {
                tracing::debug!(%err, "ide status: lost the Opencode event stream");
            }
        }
        watch.connected.store(false, Ordering::Relaxed);
        tokio::time::sleep(pause).await;
        pause = (pause * 2).min(RETRY_MAX);
    }
}

/// Opens the stream, seeds the tracker, then applies every event.
async fn follow(
    watch: &Watch,
    service: &Service,
    db: &Mutex<Connection>,
) -> Result<(), OpencodeError> {
    // The stream first, so nothing between the seed and the first event is lost.
    let mut events = EventStream::open(service).await?;
    seed(watch, service, db).await?;
    watch.connected.store(true, Ordering::Relaxed);
    while let Some(event) = events.next().await? {
        watch
            .tracker
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .apply(&event, now_ms());
    }
    Ok(())
}

/// Asks the service what every agent session is doing right now.
async fn seed(
    watch: &Watch,
    service: &Service,
    db: &Mutex<Connection>,
) -> Result<(), OpencodeError> {
    let sessions = {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        agent_store::opencode_sessions(&conn).unwrap_or_default()
    };
    let active = service.active_sessions().await?;
    for session in sessions {
        let snapshot = match service.session_snapshot(&session, &active).await {
            Ok(snapshot) => snapshot,
            // Gone from the service: the next start replaces it.
            Err(OpencodeError::Http { status: 404, .. }) => continue,
            Err(err) => return Err(err),
        };
        let now = now_ms();
        let mut tracker = watch
            .tracker
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        tracker.set(&session, snapshot.state, now);
        if let Some(plan) = snapshot.plan {
            tracker.set_plan(&session, plan, now);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiomata_opencode::{Event, PlanAnswer};

    fn agent(id: i64, harness: Harness, command: &str, session: Option<&str>) -> Agent {
        let now = Utc::now();
        Agent {
            id,
            project_id: 1,
            name: format!("a{id}"),
            harness,
            command: command.into(),
            model: None,
            env: String::new(),
            created_at: now,
            updated_at: now,
            worktree_path: None,
            branch: None,
            port: None,
            base_branch: None,
            opencode_session: session.map(str::to_owned),
            effective_command: String::new(),
            effective_env: String::new(),
        }
    }

    fn starting(agent_id: i64, started_ms: i64) -> AgentStatus {
        AgentStatus {
            agent_id,
            state: AgentState::Starting,
            since: from_ms(started_ms),
            started_at: from_ms(started_ms),
            plan: None,
            plan_document: None,
        }
    }

    #[test]
    fn service_states_map_onto_agent_states() {
        assert_eq!(agent_state(SessionState::Idle), AgentState::Idle);
        assert_eq!(agent_state(SessionState::Working), AgentState::Working);
        assert_eq!(agent_state(SessionState::Waiting), AgentState::Waiting);
        assert_eq!(from_ms(1_000).unwrap().timestamp(), 1);
    }

    #[test]
    fn opencode_agents_take_the_services_state_and_plan() {
        let mut tracker = Tracker::default();
        tracker.apply(
            &Event {
                kind: "permission.asked".into(),
                data: serde_json::json!({"sessionID": "ses_a"}),
            },
            5_000,
        );
        tracker.set_plan(
            "ses_a",
            PlanAnswer {
                markdown: "1. A".into(),
                at_ms: 1_000,
            },
            5_000,
        );
        let agents = [
            agent(1, Harness::Opencode, "", Some("ses_a")),
            agent(2, Harness::ClaudeCode, "", None),
            agent(
                3,
                Harness::Opencode,
                "opencode --agent plan",
                Some("ses_own"),
            ),
            agent(4, Harness::Opencode, "", Some("ses_quiet")),
        ];
        let mut statuses: Vec<_> = (1..=4).map(|id| starting(id, 2_000)).collect();
        overlay_with(&mut statuses, &agents, &tracker, true);

        assert_eq!(statuses[0].state, AgentState::Waiting);
        assert_eq!(statuses[0].since, from_ms(5_000));
        let plan = statuses[0].plan_document.as_ref().unwrap();
        assert_eq!(
            (plan.markdown.as_str(), plan.name.as_str()),
            ("1. A", PLAN_NAME)
        );
        assert!(plan.from_earlier_session, "written before this start");
        assert_eq!(
            statuses[1].state,
            AgentState::Starting,
            "Claude Code keeps its file channel"
        );
        assert_eq!(
            statuses[2].state,
            AgentState::Starting,
            "an own command gets no session"
        );
        assert_eq!(
            statuses[3].state,
            AgentState::Idle,
            "connected, and nothing happened in it"
        );
    }

    #[test]
    fn without_a_connection_an_unknown_session_stays_as_the_channel_said() {
        let agents = [agent(1, Harness::Opencode, "", Some("ses_x"))];
        let mut statuses = vec![starting(1, 2_000)];
        overlay_with(&mut statuses, &agents, &Tracker::default(), false);
        assert_eq!(statuses[0].state, AgentState::Starting);
    }
}
