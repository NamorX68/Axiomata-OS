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

use std::collections::HashSet;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use axiomata_opencode::{
    EventStream, OpencodeError, PermissionRequest, Service, SessionState, Tracker,
};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::AxiomataError;
use crate::agents::opencode;
use crate::board::store as board_store;
use crate::ide::lifecycle::{AgentState, AgentStatus, ChannelRoots, PlanDocument};
use crate::ide::model::{Agent, Harness};
use crate::ide::{agent_store, provision, store as project_store};
use crate::permission_ask::{self, Decision};

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
    /// Per instance (`Instance as usize`): [`UNKNOWN`], [`FOLLOWED`] (its
    /// stream is up and the tracker seeded from it) or [`NOT_RUNNING`].
    reach: [AtomicU8; opencode::Instance::ALL.len()],
}

/// The watcher has not looked at an instance yet, or lost its stream.
const UNKNOWN: u8 = 0;
/// The instance's stream is followed and the tracker seeded from it.
const FOLLOWED: u8 = 1;
/// No service of the instance runs: nothing works on it.
const NOT_RUNNING: u8 = 2;

impl Watch {
    /// Whether "no entry" means "idle" rather than "unknown": every instance is followed or not running, and one is
    /// followed. A session on an instance still unknown would otherwise read as idle while it works.
    fn settled(&self) -> bool {
        let reach: Vec<u8> = self
            .reach
            .iter()
            .map(|r| r.load(Ordering::Relaxed))
            .collect();
        reach.iter().all(|&r| r != UNKNOWN) && reach.contains(&FOLLOWED)
    }

    fn set_reach(&self, instance: opencode::Instance, reach: u8) {
        self.reach[instance as usize].store(reach, Ordering::Relaxed);
    }
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
        // One follower per instance: a reviewer's session is on the read instance, a worker's on the write one.
        for instance in opencode::Instance::ALL {
            tokio::spawn(run(Arc::clone(&watch), Arc::clone(&db), instance));
        }
        watch
    });
}

/// Lays the Opencode service's view over the file channel's statuses, for
/// Opencode agents on the generated command (an own command gets no session).
pub fn overlay(statuses: &mut [AgentStatus], agents: &[Agent]) {
    let Some(watch) = WATCH.get() else {
        return;
    };
    let connected = watch.settled();
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
    let mut tracker = Tracker::default();
    let mut connected = false;
    for instance in opencode::Instance::ALL {
        let Ok(service) = opencode::find(instance).await else {
            continue;
        };
        let Ok(active) = service.active_sessions().await else {
            continue;
        };
        connected = true;
        let sessions = agents
            .iter()
            .filter(|a| a.harness == Harness::Opencode && a.command.trim().is_empty())
            .filter_map(|a| a.opencode_session.as_deref());
        // A session of the other instance is unknown here (404) and skipped.
        for session in sessions {
            if let Ok(snapshot) = service.session_snapshot(session, &active).await {
                tracker.set(session, snapshot.state, now_ms());
                if let Some(plan) = snapshot.plan {
                    tracker.set_plan(session, plan, now_ms());
                }
                if let Some(request) = snapshot.permission {
                    tracker.set_permission(session, request, now_ms());
                }
            }
        }
    }
    if connected {
        overlay_with(statuses, agents, &tracker, true);
    }
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

/// Looks for `instance`'s service, follows it while it is there, and looks
/// again with a growing pause when it is not.
async fn run(watch: Arc<Watch>, db: Arc<Mutex<Connection>>, instance: opencode::Instance) {
    let mut pause = RETRY_MIN;
    loop {
        match opencode::find(instance).await {
            Ok(service) => {
                pause = RETRY_MIN;
                if let Err(err) = follow(&watch, &service, &db, instance).await {
                    tracing::debug!(%err, ?instance, "ide status: lost the Opencode event stream");
                }
                watch.set_reach(instance, UNKNOWN);
            }
            Err(_) => watch.set_reach(instance, NOT_RUNNING),
        }
        tokio::time::sleep(pause).await;
        pause = (pause * 2).min(RETRY_MAX);
    }
}

/// Opens the stream, seeds the tracker, then applies every event.
async fn follow(
    watch: &Watch,
    service: &Service,
    db: &Mutex<Connection>,
    instance: opencode::Instance,
) -> Result<(), OpencodeError> {
    // The stream first, so nothing between the seed and the first event is lost.
    let mut events = EventStream::open(service).await?;
    seed(watch, service, db).await?;
    watch.set_reach(instance, FOLLOWED);
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
        if let Some(request) = snapshot.permission {
            tracker.set_permission(&session, request, now);
        }
    }
    Ok(())
}

/// The permission request an Opencode session waits on, as the watcher last saw it.
fn tracked_permission(session: &str) -> Option<PermissionRequest> {
    let watch = WATCH.get()?;
    let tracker = watch
        .tracker
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    tracker.get(session)?.permission.clone()
}

/// The Opencode session of an agent the service runs (the generated command), if it has one.
fn service_session(agent: &Agent) -> Option<&str> {
    (agent.harness == Harness::Opencode && agent.command.trim().is_empty())
        .then_some(agent.opencode_session.as_deref())
        .flatten()
}

/// Why an agent counts as waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    /// Its harness waits on a prompt: a permission request or a question.
    Prompt,
    /// A session made for a plan sits idle while the plan is a draft: it has asked or proposed, and the next step is
    /// the owner's (`docs/plans/permission-asks.md`, R6).
    PlanDraft,
}

/// Which harness a permission request came from: how it is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AskSource {
    /// Claude Code's `PermissionRequest` hook, waiting in `permission_asks`.
    Claude,
    /// The Opencode service's own request.
    Opencode,
}

/// A permission request the owner can answer in the app (`docs/plans/permission-asks.md`). Written by the agent:
/// shown as plain text only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaitingAsk {
    pub id: String,
    pub source: AskSource,
    pub tool: String,
    pub detail: String,
}

/// An agent that waits for the owner — a permission prompt or a question — in whichever project it is
/// (`docs/plans/agent-waiting.md`, board card #87).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WaitingAgent {
    pub agent_id: i64,
    pub project_id: i64,
    pub name: String,
    /// The card the session works on or reviews, if any.
    pub card_id: Option<i64>,
    pub card_title: Option<String>,
    /// When it began to wait.
    pub since: Option<DateTime<Utc>>,
    pub reason: WaitReason,
    /// The permission request it waits on, when the app can answer it (card #88).
    pub ask: Option<WaitingAsk>,
}

/// Every agent of every project that waits for the owner, oldest wait first. The same reading as the Studio's status
/// poll (the file channel, with the Opencode service's view laid over it), but across all projects, for the app's
/// badge and notification.
///
/// The database is locked only to list the agents and, afterwards, to name the cards — never while the channel files
/// are read: this runs every few seconds and the connection is shared by every command.
///
/// # Errors
///
/// A database error.
pub fn waiting_agents(
    db: &Mutex<Connection>,
    roots: &ChannelRoots,
) -> std::result::Result<Vec<WaitingAgent>, AxiomataError> {
    let (agents, draft_plans) = {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        let mut agents = Vec::new();
        for project in project_store::list_projects(&conn)? {
            agents.extend(agent_store::list_agents(&conn, project.id)?);
        }
        let draft_plans = draft_plans_of(&conn, &agents)?;
        (agents, draft_plans)
    };
    let mut statuses = provision::agent_statuses(&agents, roots);
    overlay(&mut statuses, &agents);
    let mut waiting = waiting_of(&agents, &statuses, &draft_plans);
    let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
    for entry in &mut waiting {
        if let Some(card_id) = entry.card_id {
            entry.card_title = board_store::get_card(&conn, card_id)?.map(|card| card.title);
        }
        if entry.reason != WaitReason::Prompt {
            continue;
        }
        let Some(agent) = agents.iter().find(|agent| agent.id == entry.agent_id) else {
            continue;
        };
        entry.ask = match agent.harness {
            // Glut's pane stores its asks where Claude Code's hook does (`docs/plans/glut-cards.md`, C5).
            Harness::ClaudeCode | Harness::Glut => {
                permission_ask::open_ask(&conn, agent.id)?.map(|ask| WaitingAsk {
                    id: ask.id.to_string(),
                    source: AskSource::Claude,
                    tool: ask.tool,
                    detail: ask.detail,
                })
            }
            Harness::Opencode => service_session(agent)
                .and_then(tracked_permission)
                .map(|request| opencode_ask(&request)),
        };
    }
    Ok(waiting)
}

/// The plans of `agents` that are still drafts — for R6.
fn draft_plans_of(
    conn: &Connection,
    agents: &[Agent],
) -> std::result::Result<HashSet<i64>, AxiomataError> {
    let mut drafts = HashSet::new();
    for plan_id in agents.iter().filter_map(|agent| agent.plan_id) {
        if !drafts.contains(&plan_id)
            && crate::board::flow::get_plan(conn, plan_id)?
                .is_some_and(|plan| plan.status == crate::board::PlanStatus::Draft)
        {
            drafts.insert(plan_id);
        }
    }
    Ok(drafts)
}

/// An Opencode request as the app shows it: the action as the tool, its resources as the detail.
fn opencode_ask(request: &PermissionRequest) -> WaitingAsk {
    WaitingAsk {
        id: request.id.clone(),
        source: AskSource::Opencode,
        tool: permission_ask::one_line(&request.action),
        detail: permission_ask::one_line(&request.resources.join(", ")),
    }
}

/// The agents that wait for the owner, oldest wait first; `statuses` are matched to `agents` by id. An agent waits when
/// its harness waits on a prompt, or when it was made for a plan in `draft_plans` and sits idle (R6).
fn waiting_of(
    agents: &[Agent],
    statuses: &[AgentStatus],
    draft_plans: &HashSet<i64>,
) -> Vec<WaitingAgent> {
    let mut waiting: Vec<WaitingAgent> = statuses
        .iter()
        .filter_map(|status| {
            let agent = agents.iter().find(|agent| agent.id == status.agent_id)?;
            let reason = match status.state {
                AgentState::Waiting => WaitReason::Prompt,
                AgentState::Idle
                    if agent
                        .plan_id
                        .is_some_and(|plan| draft_plans.contains(&plan)) =>
                {
                    WaitReason::PlanDraft
                }
                _ => return None,
            };
            Some(WaitingAgent {
                agent_id: agent.id,
                project_id: agent.project_id,
                name: agent.name.clone(),
                card_id: agent.card_id,
                card_title: None,
                since: status.since,
                reason,
                ask: None,
            })
        })
        .collect();
    waiting.sort_by_key(|entry| (entry.since, entry.agent_id));
    waiting
}

/// The owner answers an agent's permission request in the app (`docs/plans/permission-asks.md`): Claude Code's through
/// the database its hook waits on, Opencode's through the service. The request must be the one the agent waits on now,
/// so an answer can never land on another agent's or an old question.
///
/// # Errors
///
/// [`AxiomataError::PermissionAsk`] for an unknown agent, a request that is not (or no longer) open, or "in terminal"
/// for Opencode (its pane prompt is open anyway); a database or service error.
pub async fn answer_permission(
    db: &Mutex<Connection>,
    agent_id: i64,
    request_id: &str,
    decision: Decision,
) -> std::result::Result<(), AxiomataError> {
    let refused = |reason: String| AxiomataError::PermissionAsk { reason };
    let agent = {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        agent_store::get_agent(&conn, agent_id)?
    }
    .ok_or_else(|| refused(format!("no agent {agent_id}")))?;
    match agent.harness {
        Harness::ClaudeCode | Harness::Glut => {
            let id = request_id
                .parse::<i64>()
                .map_err(|_| refused(format!("malformed request id {request_id:?}")))?;
            let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
            permission_ask::decide(&conn, agent_id, id, decision, "human:owner")
        }
        Harness::Opencode => {
            let reply = match decision {
                Decision::Allow => "once",
                Decision::Deny => "reject",
                Decision::Terminal => {
                    return Err(refused(
                        "an Opencode request is answered here or in its pane".into(),
                    ));
                }
            };
            let session = service_session(&agent).ok_or_else(|| {
                refused(format!("agent {agent_id} has no session on the service"))
            })?;
            if tracked_permission(session).is_none_or(|request| request.id != request_id) {
                return Err(refused(format!(
                    "request {request_id} is not the one agent {agent_id} waits on"
                )));
            }
            let services = opencode::running_services().await;
            let service = services.holding(session).await.ok_or_else(|| {
                refused("no running Opencode service of the app knows the session".into())
            })?;
            service
                .reply_permission(session, request_id, reply)
                .await
                .map_err(|err| refused(format!("the Opencode service refused the answer: {err}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_entry_reads_as_idle_only_once_every_instance_is_followed_or_not_running() {
        let watch = Watch::default();
        assert!(!watch.settled(), "nothing looked at yet");
        watch.set_reach(opencode::Instance::Write, FOLLOWED);
        assert!(
            !watch.settled(),
            "a reviewer on the read instance could be working unseen"
        );
        watch.set_reach(opencode::Instance::Read, NOT_RUNNING);
        assert!(watch.settled());
        watch.set_reach(opencode::Instance::Read, UNKNOWN);
        assert!(!watch.settled(), "its stream was lost");
        watch.set_reach(opencode::Instance::Write, NOT_RUNNING);
        watch.set_reach(opencode::Instance::Read, NOT_RUNNING);
        assert!(!watch.settled(), "no service at all: unknown, as before");
    }
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
            engine_id: None,
            agent_role: "allrounder".into(),
            card_id: None,
            card_review: false,
            start_ref: None,
            plan_id: None,
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

    #[test]
    fn only_waiting_agents_are_listed_with_their_card_and_the_oldest_wait_first() {
        let mut on_card = agent(2, Harness::ClaudeCode, "", None);
        on_card.project_id = 7;
        on_card.card_id = Some(12);
        let agents = vec![
            agent(1, Harness::ClaudeCode, "", None),
            on_card,
            agent(3, Harness::Opencode, "", None),
            agent(4, Harness::ClaudeCode, "", None),
        ];
        let state = |agent_id, state, since_ms| AgentStatus {
            state,
            ..starting(agent_id, since_ms)
        };
        let statuses = vec![
            state(1, AgentState::Working, 1_000),
            state(2, AgentState::Waiting, 5_000),
            state(3, AgentState::Waiting, 2_000),
            // A status without its agent (removed in between) is left out.
            state(9, AgentState::Waiting, 1_000),
        ];
        let waiting = waiting_of(&agents, &statuses, &HashSet::new());
        assert_eq!(
            waiting.iter().map(|w| w.agent_id).collect::<Vec<_>>(),
            vec![3, 2]
        );
        assert_eq!(waiting[1].project_id, 7);
        assert_eq!(waiting[1].card_id, Some(12));
        assert_eq!(waiting[1].since, from_ms(5_000));
    }

    #[test]
    fn an_idle_session_of_a_draft_plan_waits_for_the_owner_and_others_do_not() {
        let mut planner = agent(1, Harness::ClaudeCode, "", None);
        planner.plan_id = Some(4);
        let mut done_planner = agent(2, Harness::ClaudeCode, "", None);
        done_planner.plan_id = Some(5);
        let plain = agent(3, Harness::ClaudeCode, "", None);
        let mut busy_planner = agent(4, Harness::ClaudeCode, "", None);
        busy_planner.plan_id = Some(4);
        let agents = vec![planner, done_planner, plain, busy_planner];
        let state = |agent_id, state| AgentStatus {
            state,
            ..starting(agent_id, 1_000)
        };
        let statuses = vec![
            state(1, AgentState::Idle),
            state(2, AgentState::Idle),
            state(3, AgentState::Idle),
            state(4, AgentState::Working),
        ];
        let drafts = HashSet::from([4]);
        let waiting = waiting_of(&agents, &statuses, &drafts);
        assert_eq!(waiting.len(), 1, "{waiting:?}");
        assert_eq!(waiting[0].agent_id, 1);
        assert_eq!(waiting[0].reason, WaitReason::PlanDraft);
        assert_eq!(waiting[0].ask, None);
    }

    #[test]
    fn an_opencode_request_shows_its_action_and_resources() {
        let ask = opencode_ask(&PermissionRequest {
            id: "per_1".into(),
            action: "shell".into(),
            resources: vec!["npm ci".into(), "git status\nx".into()],
        });
        assert_eq!(
            ask,
            WaitingAsk {
                id: "per_1".into(),
                source: AskSource::Opencode,
                tool: "shell".into(),
                detail: "npm ci, git status x".into(),
            }
        );
    }

    #[test]
    fn the_watchers_view_names_the_opencode_request_its_session_waits_on() {
        let mut tracker = Tracker::default();
        tracker.set_permission(
            "ses_w",
            PermissionRequest {
                id: "per_2".into(),
                action: "edit".into(),
                resources: vec!["src/a.rs".into()],
            },
            1,
        );
        assert_eq!(tracker.get("ses_w").unwrap().state, SessionState::Waiting);
        let own = agent(1, Harness::Opencode, "opencode", Some("ses_w"));
        assert_eq!(
            service_session(&own),
            None,
            "an own command runs outside the service"
        );
        let generated = agent(2, Harness::Opencode, "", Some("ses_w"));
        assert_eq!(service_session(&generated), Some("ses_w"));
    }

    /// A database with one project and an agent on `harness`.
    fn db_with_agent(harness: Harness) -> (Mutex<Connection>, i64) {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-answer-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let db = crate::db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
        let project = project_store::create_project(
            &db,
            axiomata_ide::NewProject {
                name: "P".into(),
                repo_root: dir.clone(),
            },
        )
        .unwrap();
        let agent = agent_store::create_agent(
            &db,
            axiomata_ide::NewAgent {
                project_id: project.id,
                fields: axiomata_ide::AgentFields {
                    name: "worker".into(),
                    harness,
                    command: String::new(),
                    model: None,
                    env: String::new(),
                },
            },
        )
        .unwrap();
        (Mutex::new(db), agent.id)
    }

    #[tokio::test]
    async fn the_owner_answers_a_claude_request_of_that_agent_only() {
        let (db, agent) = db_with_agent(Harness::ClaudeCode);
        let id = {
            let conn = db.lock().unwrap();
            permission_ask::ask(&conn, agent, "Bash", "npm ci").unwrap()
        };
        let refused = |result: std::result::Result<(), AxiomataError>| {
            matches!(result, Err(AxiomataError::PermissionAsk { .. }))
        };
        assert!(refused(
            answer_permission(&db, agent + 1, &id.to_string(), Decision::Allow).await
        ));
        assert!(refused(
            answer_permission(&db, agent, "x", Decision::Allow).await
        ));
        answer_permission(&db, agent, &id.to_string(), Decision::Allow)
            .await
            .unwrap();
        let conn = db.lock().unwrap();
        assert_eq!(
            permission_ask::decision_of(&conn, id).unwrap(),
            Some(Decision::Allow)
        );
    }

    #[tokio::test]
    async fn an_opencode_request_is_answered_only_while_it_is_the_tracked_one() {
        let (db, agent) = db_with_agent(Harness::Opencode);
        // "In terminal" is no answer for Opencode, and without a tracked request nothing is sent to the service.
        let refused = |result: std::result::Result<(), AxiomataError>| {
            matches!(result, Err(AxiomataError::PermissionAsk { .. }))
        };
        assert!(refused(
            answer_permission(&db, agent, "per_1", Decision::Terminal).await
        ));
        assert!(refused(
            answer_permission(&db, agent, "per_1", Decision::Allow).await
        ));
    }
}
