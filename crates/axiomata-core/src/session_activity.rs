//! What the studio's sessions are doing right now, for the Flow's team pane (`docs/plans/a2a.md`, CP-A9).
//!
//! [`read`] looks at each session's own record — the newest Claude Code transcript of the session, or the newest page of
//! its Opencode messages — and returns the last few steps ([`axiomata_ide::activity`]). It writes nothing and holds no
//! lock while it reads: the sessions are looked up first, then the files and the service are asked.
//!
//! A session that cannot be read (no transcript yet, the service is down) is listed with `readable: false`, not left out:
//! the tile says "nothing to show yet" instead of vanishing. Nothing here decides anything.

use std::time::Duration;

use axiomata_ide::activity::{self, Activity};
use axiomata_ide::lifecycle::Channel;
use axiomata_ide::model::{Agent, Harness};
use serde::Serialize;

use crate::AxiomataCore;
use crate::agents::opencode;
use crate::ide::agent_store;
use crate::paths;
use crate::session_limits::find_transcript;

/// How long the Opencode service may take to answer one look. The look comes every few seconds for every visible tile; a
/// service that hangs must not hold the pane back.
const SERVICE_TIMEOUT: Duration = Duration::from_secs(4);
/// How many of a session's newest messages one look asks for: a message holds a few steps, and the tile keeps eight.
const RECENT_MESSAGES: u32 = 20;
/// The most sessions one look reads: a project does not run more at once, and the call is bounded all the same.
const MAX_SESSIONS: usize = 32;

/// One session's recent steps.
#[derive(Debug, Clone, Serialize)]
pub struct SessionActivity {
    pub agent_id: i64,
    /// Oldest first; the last one is "now".
    pub steps: Vec<Activity>,
    /// `false` when the harness's record could not be read: the steps are then empty, which is not "idle".
    pub readable: bool,
}

/// The recent steps of the sessions `agent_ids` (those that exist; the rest are left out).
pub async fn read(core: &AxiomataCore, agent_ids: &[i64]) -> Vec<SessionActivity> {
    let agents: Vec<Agent> = {
        let db = core.db_lock();
        agent_ids
            .iter()
            .take(MAX_SESSIONS)
            .filter_map(|id| agent_store::get_agent(&db, *id).ok().flatten())
            .collect()
    };
    // Past the bound a session is listed as unreadable rather than left out: a tile with no entry would say nothing at all.
    let skipped: Vec<SessionActivity> = agent_ids
        .iter()
        .skip(MAX_SESSIONS)
        .map(|id| SessionActivity {
            agent_id: *id,
            steps: Vec::new(),
            readable: false,
        })
        .collect();
    let roots = paths::ide_locations().channels;
    let projects = paths::claude_projects_dir();
    let mut service: Option<Option<axiomata_opencode::Service>> = None;
    let mut out = Vec::new();
    for agent in agents {
        let steps = match agent.harness {
            Harness::ClaudeCode => {
                let channel = Channel::for_agent(&roots, agent.id);
                let projects = projects.clone();
                // Reading a file is blocking work: off the runtime's threads.
                tokio::task::spawn_blocking(move || {
                    let id = channel.claude_sessions().pop()?;
                    let path = find_transcript(&projects, &id)?;
                    activity::tail_of(&path)
                        .ok()
                        .map(|text| activity::claude_recent(&text))
                })
                .await
                .ok()
                .flatten()
            }
            Harness::Opencode => match agent.opencode_session.as_deref() {
                None => None,
                Some(session) => {
                    if service.is_none() {
                        service = Some(opencode::running_service().await);
                    }
                    match service.clone().flatten() {
                        None => None,
                        Some(running) => {
                            let asked = tokio::time::timeout(
                                SERVICE_TIMEOUT,
                                running.recent_messages(session, RECENT_MESSAGES),
                            )
                            .await;
                            // A service that did not answer in time is not asked again in this look: each further session
                            // would wait as long, and the look would outlast the next one.
                            if asked.is_err() {
                                service = Some(None);
                            }
                            asked
                                .ok()
                                .and_then(Result::ok)
                                .map(|messages| activity::opencode_recent(&messages))
                        }
                    }
                }
            },
            Harness::Mini => None,
        };
        out.push(SessionActivity {
            agent_id: agent.id,
            readable: steps.is_some(),
            steps: steps.unwrap_or_default(),
        });
    }
    out.extend(skipped);
    out
}
