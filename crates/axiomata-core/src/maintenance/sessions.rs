//! Ended sessions without work of their own (`docs/plans/agents-model.md`, Q17): the Agents tab no longer lists them,
//! and the weekly clean-up removes them — their worktree, their branch and their row — after looking again.
//!
//! A session goes only when every check says so, right before it goes:
//!
//! * not a master, and made (and last seen working) more than [`MIN_AGE`] ago;
//! * its harness ended — the status channel says `ended`, or it never reported anything — and no process holds its
//!   session lock (`presence`);
//! * no open card holds it (as worker or reviewer) and no plan it was started for still exists;
//! * its checkout holds no work: no uncommitted change, no commit beyond its base (`AgentCheckout::has_unsaved_work`,
//!   which fails closed). The removal itself refuses work once more (`AgentCheckout::remove(false)`).
//!
//! Anything with work stays, whatever else is true: a clean-up never throws work away.

use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, TimeDelta, Utc};
use rusqlite::Connection;
use serde::Serialize;

use super::{Item, lock};
use crate::board::{TaskState, flow, store as board_store};
use crate::ide::lifecycle::{AgentState, Channel, ChannelRoots};
use crate::ide::model::Agent;
use crate::ide::{agent_store, presence, provision};

/// How long a session must have been quiet before the clean-up looks at it: a session just made, whose pane is about to
/// start, is never "ended".
pub const MIN_AGE: TimeDelta = TimeDelta::days(1);

/// A session the clean-up removed, or would remove in a dry run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndedSession {
    pub id: i64,
    pub name: String,
    pub project_id: i64,
    /// The clean-up removed it (in a dry run: never).
    pub removed: bool,
}

/// Why `agent` stays, judged from the database alone; `None` when it may be looked at further.
fn kept_by_rows(db: &Connection, agent: &Agent, now: DateTime<Utc>) -> Option<&'static str> {
    if agent.is_master() {
        return Some("a master");
    }
    if now - agent.created_at < MIN_AGE || now - agent.updated_at < MIN_AGE {
        return Some("too recent");
    }
    if let Some(card_id) = agent.card_id {
        let open = match board_store::get_card(db, card_id) {
            Ok(Some(card)) => {
                card.archived_at.is_none()
                    && card.canceled_at.is_none()
                    && !matches!(card.state, TaskState::Done | TaskState::Verified)
            }
            Ok(None) => false,
            // Unreadable: it might be open.
            Err(_) => true,
        };
        if open {
            return Some("its card is open");
        }
    }
    if let Some(plan_id) = agent.plan_id
        && !matches!(flow::get_plan(db, plan_id), Ok(None))
    {
        return Some("its plan still exists");
    }
    None
}

/// Why `agent` stays, judged from its channel; `None` when its harness ended and nothing holds it.
fn kept_by_channel(
    roots: &ChannelRoots,
    agent: &Agent,
    now: DateTime<Utc>,
) -> Option<&'static str> {
    // Unknown counts as alive.
    if presence::is_live(roots, agent.id).unwrap_or(true) {
        return Some("a process holds it");
    }
    let channel = Channel::for_agent(roots, agent.id);
    if !channel.dir().exists() {
        return None;
    }
    let status = channel.read_status(agent.harness);
    let quiet = status.since.is_none_or(|since| now - since >= MIN_AGE);
    match status.state {
        AgentState::Ended if quiet => None,
        // Never reported a state: never ran, or ran without hooks long ago.
        AgentState::Starting if status.since.is_none() && status.started_at.is_none() => None,
        _ => Some("its harness has not ended"),
    }
}

/// One session found removable, with what removing it needs.
struct Candidate {
    agent: Agent,
    checkout: Option<provision::AgentCheckout>,
}

/// Finds and (unless `dry_run`) removes the ended sessions without work. The database is held only to read the rows and
/// to delete one; git and the channels run without it.
pub(super) fn sweep(
    db: &Mutex<Connection>,
    roots: &ChannelRoots,
    worktrees: &Path,
    now: DateTime<Utc>,
    dry_run: bool,
) -> (Item, Vec<EndedSession>) {
    let mut item = Item::new(
        "ended_sessions",
        "Ended sessions without work",
        "sessions",
        format!(
            "ended, quiet for {} day(s), no open card, nothing uncommitted and no commit beyond its base",
            MIN_AGE.num_days()
        ),
    );
    let candidates: Vec<Candidate> = {
        let db = lock(db);
        let agents = match all_agents(&db) {
            Ok(agents) => agents,
            Err(err) => {
                item.note_error(format!("the sessions could not be read: {err}"));
                return (item, Vec::new());
            }
        };
        item.count = u64::try_from(agents.len()).unwrap_or(u64::MAX);
        agents
            .into_iter()
            .filter(|agent| kept_by_rows(&db, agent, now).is_none())
            .map(|agent| Candidate {
                checkout: provision::AgentCheckout::read(&db, agent.id).ok().flatten(),
                agent,
            })
            .collect()
    };
    let mut ended = Vec::new();
    for candidate in candidates {
        if kept_by_channel(roots, &candidate.agent, now).is_some() {
            continue;
        }
        // Only a worktree where the studio puts them is the clean-up's: a row naming another folder is never acted on.
        let contained = candidate
            .checkout
            .as_ref()
            .and_then(|checkout| checkout.worktree())
            .is_none_or(|path| inside(path, worktrees));
        if !contained {
            continue;
        }
        // Work, or a checkout that cannot be judged, keeps the session.
        let has_work = candidate
            .checkout
            .as_ref()
            .is_none_or(|checkout| checkout.has_unsaved_work().unwrap_or(true));
        if has_work {
            continue;
        }
        let mut session = EndedSession {
            id: candidate.agent.id,
            name: candidate.agent.name.clone(),
            project_id: candidate.agent.project_id,
            removed: false,
        };
        item.freeable_count += 1;
        if !dry_run {
            match remove(db, roots, &candidate, now) {
                Ok(removed) => session.removed = removed,
                Err(err) => item.note_error(format!("{}: {err}", candidate.agent.name)),
            }
        }
        ended.push(session);
    }
    (item, ended)
}

/// Removes one session: looks at its row once more under the lock (a card may have taken it meanwhile), removes its
/// checkout — which refuses work once more — and then the row and its channel. `false` when the row no longer allows
/// it and stays as it is.
fn remove(
    db: &Mutex<Connection>,
    roots: &ChannelRoots,
    candidate: &Candidate,
    now: DateTime<Utc>,
) -> Result<bool, String> {
    let still = |db: &Connection| -> Result<bool, String> {
        Ok(agent_store::get_agent(db, candidate.agent.id)
            .map_err(|err| err.to_string())?
            .is_some_and(|agent| kept_by_rows(db, &agent, now).is_none()))
    };
    if !still(&lock(db))? {
        return Ok(false);
    }
    // Its harness may have started again since the sweep looked.
    if kept_by_channel(roots, &candidate.agent, now).is_some() {
        return Ok(false);
    }
    if let Some(checkout) = &candidate.checkout {
        let removed = checkout.remove(false).map_err(|err| err.to_string())?;
        if let Some(reason) = removed.branch_kept {
            tracing::warn!(id = candidate.agent.id, %reason, "maintenance removed a session but kept its branch");
        }
    }
    let deleted = {
        let db = lock(db);
        if still(&db)? {
            agent_store::delete_agent(&db, candidate.agent.id).map_err(|err| err.to_string())?
        } else {
            false
        }
    };
    if deleted {
        provision::forget_channel(roots, candidate.agent.id).map_err(|err| err.to_string())?;
    }
    Ok(deleted)
}

/// Whether `path` lies inside `root`, both resolved (a folder that is gone resolves as written).
fn inside(path: &Path, root: &Path) -> bool {
    let resolve = |p: &Path| p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    let (path, root) = (resolve(path), resolve(root));
    path != root && path.starts_with(&root)
}

/// Every session of every project.
fn all_agents(db: &Connection) -> crate::ide::Result<Vec<Agent>> {
    let mut agents = Vec::new();
    for project in crate::ide::store::list_projects(db)? {
        agents.extend(agent_store::list_agents(db, project.id)?);
    }
    Ok(agents)
}
