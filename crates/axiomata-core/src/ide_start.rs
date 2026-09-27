//! Starting an IDE agent: provisioning plus, for Opencode, its session.
//!
//! `axiomata_ide::provision::prepare` gives an agent its worktree, port and
//! status channel — local, synchronous work. An Opencode agent additionally
//! needs a session on Opencode 2's shared service (`docs/plans/opencode2.md`,
//! OC2), which is an HTTP call and belongs to core's Opencode backend. This is
//! the one place that puts the two together, so the dashboard and the CLI
//! start agents the same way.

use std::sync::Mutex;
use std::sync::OnceLock;

use rusqlite::Connection;

use crate::AxiomataError;
use crate::agents::opencode;
use crate::ide::agent_store;
use crate::ide::model::Harness;
use crate::ide::provision::{self, Provisioned};
use crate::paths;

/// Serialises agent starts in this process: two starts of the same agent at
/// once (a double-clicked Restart) would otherwise each create a session and
/// orphan one. Starts are rare and short, so one lock for all agents is
/// enough. It does not reach across processes — the CLI and the app starting
/// the same agent at the same moment can still leave one spare session in
/// Opencode's list, which is harmless.
fn start_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Provisions agent `id` and, for an Opencode agent on the generated command,
/// makes sure it has a session and starts the terminal UI on it.
///
/// The database lock is held only for the provisioning and for recording the
/// session — never across the call to the service; [`start_lock`] keeps two
/// starts from racing in between. An agent with a command of its own is
/// started as written (the E13 rule: the owner is responsible for it), so it
/// gets no session from here.
pub async fn start_agent(db: &Mutex<Connection>, id: i64) -> Result<Provisioned, AxiomataError> {
    let _start = start_lock().lock().await;
    let mut ready = {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        provision::prepare(&conn, &paths::ide_locations(), id)?
    };
    if ready.agent.harness != Harness::Opencode || !ready.agent.command.trim().is_empty() {
        return Ok(ready);
    }
    let session = opencode::ide_session(
        ready.agent.opencode_session.as_deref(),
        &ready.cwd,
        &ready.agent.name,
        ready.agent.model.as_deref(),
    )
    .await?;
    if ready.agent.opencode_session.as_deref() != Some(session.as_str()) {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        agent_store::set_opencode_session(&conn, id, Some(&session))?;
    }
    // Typed into a shell as it stands, so only an id of `[A-Za-z0-9_-]` is
    // ever appended (`ide_session` checks the ones it returns; this keeps the
    // promise local).
    if !crate::agents::valid_session_id(&session) {
        return Err(AxiomataError::AgentApi {
            backend: crate::agents::BACKEND_OPENCODE,
            message: format!("refusing to start on a malformed session id {session:?}"),
        });
    }
    ready.launch_command = with_session(&ready.launch_command, &session);
    ready.agent.opencode_session = Some(session);
    Ok(ready)
}

/// Forgets agent `id`'s Opencode session, so its next start creates a fresh
/// one ("New session", plan Q9). The old session stays in Opencode's list.
pub fn new_session(db: &Mutex<Connection>, id: i64) -> Result<bool, AxiomataError> {
    let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
    Ok(agent_store::set_opencode_session(&conn, id, None)?)
}

/// The launch command with `--session <id>` added.
fn with_session(command: &str, session: &str) -> String {
    format!("{command} --session {session}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_is_appended_to_the_generated_command() {
        assert_eq!(
            with_session("opencode", "ses_abc"),
            "opencode --session ses_abc"
        );
    }
}
