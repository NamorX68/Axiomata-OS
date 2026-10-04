//! Starting an IDE agent: provisioning, the entry to the agent MCP server (`agent_entry`) and, for Opencode, its
//! session.
//!
//! `axiomata_ide::provision::prepare` gives an agent its worktree, port and
//! status channel — local, synchronous work. An Opencode agent additionally
//! needs a session on Opencode 2's shared service (`docs/plans/opencode2.md`,
//! OC2), which is an HTTP call and belongs to core's Opencode backend. This is
//! the one place that puts the two together, so the dashboard and the CLI
//! start agents the same way.

use std::sync::OnceLock;

use serde::Serialize;

use crate::AxiomataCore;
use crate::AxiomataError;
use crate::agent_entry::{self, AgentEntry};
use crate::agents::opencode;
use crate::ide::agent_store;
use crate::ide::model::Harness;
use crate::ide::provision::{self, Provisioned};
use crate::paths;
use crate::roster;

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

/// What a start hands to the caller: everything [`provision::prepare`] says, plus what the session was given to reach
/// the team (CP-A5). Serialised flat, so a client that only knew [`Provisioned`] still reads it.
#[derive(Debug, Clone, Serialize)]
pub struct Started {
    #[serde(flatten)]
    pub ready: Provisioned,
    /// The agent MCP server's entry, for the owner to see; the secret is not in it.
    pub mcp: AgentEntry,
}

impl std::ops::Deref for Started {
    type Target = Provisioned;

    fn deref(&self) -> &Provisioned {
        &self.ready
    }
}

/// Provisions agent `id`, wires the agent MCP server into its harness and, for an Opencode agent on the generated
/// command, makes sure it has a session and starts the terminal UI on it.
///
/// The database lock is held only for the provisioning and for recording the
/// session — never across the call to the service; [`start_lock`] keeps two
/// starts from racing in between. An agent with a command of its own is
/// started as written (the E13 rule: the owner is responsible for it), so it
/// gets no session and no MCP entry from here.
pub async fn start_agent(core: &AxiomataCore, id: i64) -> Result<Started, AxiomataError> {
    let _start = start_lock().lock().await;
    let (mut ready, role, launch) = {
        let conn = core.db_lock();
        let ready = provision::prepare(&conn, &paths::ide_locations(), id)?;
        let config = core.config_read().clone();
        let role = roster::roles_for_project(&conn, &config, ready.agent.project_id)
            .into_iter()
            .find(|role| role.name == ready.agent.agent_role);
        let launch = card_launch(&conn, &ready.agent);
        (ready, role, launch)
    };
    let roots = paths::ide_locations().channels;
    if !ready.agent.command.trim().is_empty() {
        // A session of the owner's own making that took a card by itself and is restarted: started as written (E13),
        // not refused. The studio never starts a card on an engine with a command of its own (`card_session`).
        return Ok(Started {
            ready,
            mcp: AgentEntry::not_applicable(
                "this session runs a command of its own, which Axiomata does not touch",
            ),
        });
    }
    match ready.agent.harness {
        Harness::ClaudeCode => {
            let (arg, mcp) =
                agent_entry::wire_claude(&roots, &ready.agent, role.as_ref(), launch.as_ref());
            if let Some(launch) = &launch {
                require_entry(&mcp)?;
                // The prompt goes last, behind `--`: nothing after it may be read as an option.
                let tail = agent_entry::claude_prompt_tail(&ready.agent, launch);
                ready.launch_command = format!(
                    "{} {} {tail}",
                    ready.launch_command,
                    arg.unwrap_or_default()
                );
            } else if let Some(arg) = arg {
                ready.launch_command = format!("{} {arg}", ready.launch_command);
            }
            let channel = axiomata_ide::lifecycle::Channel::for_agent(&roots, ready.agent.id);
            ready.launch_command = typeable(&channel, &ready.launch_command)?;
            Ok(Started { ready, mcp })
        }
        Harness::Opencode => {
            let tools = agent_entry::role_tools(role.as_ref());
            let session = opencode::ide_session(
                ready.agent.opencode_session.as_deref(),
                &ready.cwd,
                &ready.agent.name,
                ready.agent.model.as_deref(),
                launch.as_ref().map(|launch| opencode::CardRights {
                    tools: tools.as_slice(),
                    review: launch.review.is_some(),
                }),
            )
            .await?;
            // Typed into a shell as it stands, so only an id of `[A-Za-z0-9_-]` is
            // ever appended (`ide_session` checks the ones it returns; this keeps the
            // promise local).
            if !crate::agents::valid_session_id(&session.id) {
                return Err(AxiomataError::AgentApi {
                    backend: crate::agents::BACKEND_OPENCODE,
                    message: format!(
                        "refusing to start on a malformed session id {:?}",
                        session.id
                    ),
                });
            }
            ready.launch_command = with_session(&ready.launch_command, &session.id);
            ready.agent.opencode_session = Some(session.id.clone());
            // The registration is keyed on the directory. Agents of a project that is no repository all share its
            // folder, so a second start would replace the first one's server with its own id and secret — and the
            // first session would speak as the second.
            let mcp = if ready.shared_folder {
                AgentEntry::not_applicable(
                    "this project is not a git repository, so its sessions share one folder, and one folder cannot \
                     carry one identity per session",
                )
            } else {
                agent_entry::wire_opencode(
                    &roots,
                    &ready.agent,
                    &ready.cwd,
                    role.as_ref(),
                    launch.as_ref(),
                )
                .await
            };
            if let Some(launch) = &launch {
                require_entry(&mcp)?;
                // Only a session made now gets its first message; a continued one has it already and would
                // otherwise be told the same thing again on every restart of the pane.
                if session.created {
                    let text = format!(
                        "{}\n\n{}",
                        agent_entry::instructions(&ready.agent, role.as_ref()),
                        agent_entry::start_prompt(&ready.agent, launch)
                    );
                    opencode::send_prompt(&session.id, &text).await?;
                }
            }
            // Remembered only now: a start that failed on the way (the entry, the first message) must not leave a
            // session behind that the next start would take for one that was told its card — it would sit empty.
            if ready.agent.opencode_session.as_deref() != Some(session.id.as_str()) {
                let conn = core.db_lock();
                agent_store::set_opencode_session(&conn, id, Some(&session.id))?;
            }
            Ok(Started { ready, mcp })
        }
        Harness::Mini => Ok(Started {
            ready,
            mcp: AgentEntry::not_applicable("the mini harness does not exist yet"),
        }),
    }
}

/// The longest launch command typed into a pane as it is. A terminal in line mode takes at most 1024 bytes per line and
/// silently drops the rest (macOS `MAX_CANON`) — a card session's command, with its paths, rights and start prompt, is
/// longer, and arrived cut off in the middle of the prompt. Well under the limit, so the shell's own prompt and any
/// prefix the pane adds still fit.
const MAX_TYPED_COMMAND: usize = 700;

/// The file in the channel directory a long launch command is kept in.
const LAUNCH_SCRIPT: &str = "launch.sh";

/// `command` as it can be typed: itself when it is short, otherwise a line that runs a script holding it. The script is
/// rewritten at every start, so it never outlives what the start decided, and it holds no secret (the secret is in the
/// MCP configuration it points at). `exec`, so the harness takes the script's place and nothing is left behind it.
fn typeable(
    channel: &axiomata_ide::lifecycle::Channel,
    command: &str,
) -> Result<String, AxiomataError> {
    if command.len() <= MAX_TYPED_COMMAND {
        return Ok(command.to_owned());
    }
    channel.write_private(LAUNCH_SCRIPT, &format!("#!/bin/sh\nexec {command}\n"))?;
    let path = channel.dir().join(LAUNCH_SCRIPT);
    Ok(format!(
        "sh {}",
        agent_entry::shell_quote(&path.display().to_string())
    ))
}

/// What a session was started for, if the studio started it for a card and that card still wants it: a worker's card
/// is **in work**, a reviewer's is **in review**. A card the worker already reported is not work any more — a restarted
/// pane must not be told to do it again, and `report_done` would be refused — and a card that was reviewed, called off
/// or archived is not a reviewer's any more. A session of the owner's own making is never one, whatever it claims by
/// itself.
pub(crate) fn card_launch(
    db: &rusqlite::Connection,
    agent: &crate::ide::model::Agent,
) -> Option<agent_entry::CardLaunch> {
    use crate::board::{CardStatus, ColumnStage, store};
    let card_id = agent.card_id?;
    let card = store::get_card(db, card_id).ok()??;
    if card.archived_at.is_some() || card.failed_at.is_some() || card.canceled_at.is_some() {
        return None;
    }
    let column = store::get_column(db, card.column_id).ok()??;
    let in_review = column.stage == Some(ColumnStage::Review);
    if !agent.card_review {
        // A worker is the one who holds the card now: after a release and a new start the card belongs to another
        // session, and the old pane must not go on working on it.
        let mine = crate::session::actor_from(Some(&agent.id.to_string()), Some(&agent.name));
        return (column.maps_to_status == CardStatus::Doing
            && !in_review
            && mine.is_some()
            && card.claimed_by == mine)
            .then(|| agent_entry::CardLaunch::work(card_id));
    }
    if !in_review || card.verified_by.is_some() {
        return None;
    }
    // A reviewer of an earlier report is not the reviewer of this one: its pane, restarted, must not judge again.
    let sessions = crate::ide::agent_store::list_agents(db, agent.project_id).ok()?;
    if crate::card_session::current_reviewer(db, card_id, &sessions)
        .ok()??
        .id
        != agent.id
    {
        return None;
    }
    let worker = crate::ide::agent_store::get_agent(
        db,
        crate::card_session::session_id_of(card.claimed_by.as_deref()?)?,
    )
    .ok()??;
    // What the work is measured against; without a recorded base the project folder's own branch stands in, as the
    // Diffs tab does (G1).
    let base = worker.base_branch.clone().or_else(|| {
        let project = crate::ide::store::get_project(db, worker.project_id).ok()??;
        crate::ide::worktree::current_branch(&project.repo_root)
    })?;
    Some(agent_entry::CardLaunch::review(
        card_id,
        &worker.name,
        &base,
    ))
}

/// A session working on a card without the team tools could not report it, nor be reviewed: it is not started.
fn require_entry(entry: &AgentEntry) -> Result<(), AxiomataError> {
    if entry.status == agent_entry::EntryStatus::Registered {
        return Ok(());
    }
    Err(unattended_needs_the_entry(
        entry
            .note
            .as_deref()
            .unwrap_or("its MCP entry is not in place"),
    ))
}

fn unattended_needs_the_entry(why: &str) -> AxiomataError {
    crate::roster::refusal(
        "card session",
        format!("a session working on a card needs the team tools, and {why}"),
    )
}

/// Forgets agent `id`'s Opencode session, so its next start creates a fresh
/// one ("New session", plan Q9). The old session stays in Opencode's list.
pub fn new_session(
    db: &std::sync::Mutex<rusqlite::Connection>,
    id: i64,
) -> Result<bool, AxiomataError> {
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

    fn channel(label: &str) -> axiomata_ide::lifecycle::Channel {
        let base =
            std::env::temp_dir().join(format!("axiomata-typeable-{label}-{}", std::process::id()));
        axiomata_ide::lifecycle::Channel::for_agent(
            &axiomata_ide::lifecycle::ChannelRoots {
                events: base.join("events"),
                claude_tasks: base.join("tasks"),
                claude_plans: base.join("plans"),
            },
            3,
        )
    }

    #[test]
    fn a_short_command_is_typed_as_it_is() {
        let channel = channel("short");
        assert_eq!(
            typeable(&channel, "claude --model 'x'").unwrap(),
            "claude --model 'x'"
        );
        assert!(!channel.dir().join(LAUNCH_SCRIPT).exists());
    }

    #[test]
    fn a_command_too_long_for_one_terminal_line_is_run_from_a_script() {
        let channel = channel("long");
        // The command of a card session was 1036 bytes and arrived cut at 1024, in the middle of its prompt.
        let long = format!("claude -- '{}'", "word ".repeat(300));
        assert!(long.len() > 1024);
        let typed = typeable(&channel, &long).unwrap();
        assert!(typed.len() < 200 && typed.starts_with("sh '"), "{typed}");
        let script = std::fs::read_to_string(channel.dir().join(LAUNCH_SCRIPT)).unwrap();
        assert_eq!(
            script,
            format!("#!/bin/sh\nexec {long}\n"),
            "whole, nothing cut"
        );
        // Rewritten at the next start, not appended to.
        typeable(&channel, &format!("{long} more")).unwrap();
        let again = std::fs::read_to_string(channel.dir().join(LAUNCH_SCRIPT)).unwrap();
        assert!(
            again.ends_with("more\n") && again.matches("exec").count() == 1,
            "{again}"
        );
    }

    #[test]
    fn the_session_is_appended_to_the_generated_command() {
        assert_eq!(
            with_session("opencode", "ses_abc"),
            "opencode --session ses_abc"
        );
    }
}
