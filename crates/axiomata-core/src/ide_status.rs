//! The agents that wait for the owner, across every project (`docs/plans/agent-waiting.md`, card #87), and the owner's
//! answer to a permission request in the app (`docs/plans/permission-asks.md`): read from the file channels and the
//! `permission_asks` table that Claude Code's hook and Glut's pane write. Opencode's service tracker that once laid its
//! own view over the channels left with Opencode (`docs/plans/opencode-exit.md`).

use std::collections::HashSet;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::AxiomataError;
use crate::board::store as board_store;
use crate::ide::lifecycle::{AgentState, AgentStatus, ChannelRoots};
use crate::ide::model::{Agent, Harness};
use crate::ide::{agent_store, provision, store as project_store};
use crate::permission_ask::{self, Decision};

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
    /// Claude Code's `PermissionRequest` hook or Glut's pane, waiting in `permission_asks`.
    Claude,
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
/// poll (the file channel), but across all projects, for the app's badge and notification.
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
    let statuses = provision::agent_statuses(&agents, roots);
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
            // The plain Opencode tile answers in its own terminal (opencode-exit.md, O1).
            Harness::Opencode => None,
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

/// The owner answers an agent's permission request in the app (`docs/plans/permission-asks.md`): Claude Code's and
/// Glut's through the database their hook and pane wait on. The request must be the one the agent waits on now, so an
/// answer can never land on another agent's or an old question.
///
/// # Errors
///
/// [`AxiomataError::PermissionAsk`] for an unknown agent, a request that is not (or no longer) open, or one of the
/// Opencode tile's; a database error.
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
        Harness::Opencode => Err(refused(
            "the Opencode tile answers its requests in its own terminal".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn from_ms(ms: i64) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp_millis(ms)
    }

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
}
