//! Starting an IDE agent: provisioning, the entry to the agent MCP server (`agent_entry`) and, for Opencode, its
//! session; for Glut, the run's spec in the channel (`glut_card`).
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
        // Opencode left (`docs/plans/opencode-exit.md`, O3): a generated session on it that migration 36 did not see
        // starts as Glut, never on the app's own Opencode service.
        if let Some(agent) = agent_store::get_agent(&conn, id)?
            && agent.harness == Harness::Opencode
            && !agent.is_master()
            && agent.command.trim().is_empty()
        {
            agent_store::update_agent(
                &conn,
                id,
                crate::ide::model::AgentFields {
                    name: agent.name,
                    harness: Harness::Glut,
                    command: agent.command,
                    model: agent.model,
                    env: agent.env,
                },
            )?;
            agent_store::set_opencode_session(&conn, id, None)?;
        }
        let ready = provision::prepare(&conn, &paths::ide_locations(), id)?;
        // A request a hook of the previous run left open is nobody's any more: the app must not offer it.
        crate::permission_ask::close_open(&conn, id, "system:restart")?;
        let config = core.config_read().clone();
        let role = roster::roles_for_project(&conn, &config, ready.agent.project_id)
            .into_iter()
            .find(|role| role.name == ready.agent.agent_role);
        let launch = launch_of(&conn, &ready.agent).map(|launch| launch.for_role(role.as_ref()));
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
    if ready.agent.harness != Harness::Glut {
        crate::glut_card::forget(&roots, ready.agent.id);
    }
    match ready.agent.harness {
        Harness::ClaudeCode => {
            let channel = axiomata_ide::lifecycle::Channel::for_agent(&roots, ready.agent.id);
            let (arg, mcp) =
                agent_entry::wire_claude(&roots, &ready.agent, role.as_ref(), launch.as_ref());
            if let Some(launch) = &launch {
                require_entry(&mcp)?;
                // The transcript is named after the session id, so the card's usage can be read back (A33). Every
                // start is a new session; the channel keeps them all, and a card's usage is their sum. An engine
                // whose command resumes a session names its own id: it would clash, and its usage reads as unknown.
                let session_option = if resumes_a_session(&ready.launch_command) {
                    String::new()
                } else {
                    let session = axiomata_ide::usage::new_claude_session_id().map_err(|err| {
                        unattended_needs_the_entry(&format!("no session id could be made: {err}"))
                    })?;
                    channel.record_claude_session(&session)?;
                    format!(" --session-id {session}")
                };
                // The prompt goes last, behind `--`: nothing after it may be read as an option.
                let tail = agent_entry::claude_prompt_tail(&ready.agent, launch);
                ready.launch_command = format!(
                    "{} {}{session_option} {tail}",
                    ready.launch_command,
                    arg.unwrap_or_default()
                );
            } else if let Some(arg) = arg {
                ready.launch_command = format!("{} {arg}", ready.launch_command);
            }
            ready.launch_command = typeable(&channel, &ready.launch_command)?;
            Ok(Started { ready, mcp })
        }
        // The plain Opencode tile (`docs/plans/opencode-exit.md`, O1): the owner's own `opencode` in the project folder —
        // their configuration, their service, their free models. No service of the app's, so no team tools: a
        // configuration handed to `opencode` does not reach the service it attaches to (measured). Any other Opencode
        // session was made a Glut one above, or runs a command of its own and returned before.
        Harness::Opencode => Ok(Started {
            ready,
            mcp: AgentEntry::not_applicable(
                "the Opencode tile is Opencode as you run it yourself: it has no team tools",
            ),
        }),
        Harness::Glut if ready.agent.is_master() => {
            // The master (TUI8c): the owner's own Glut — their `~/.axh`, their servers, their sessions — in the real
            // folder, with the team server added; no card spec, so nothing of a card's is left to read.
            crate::glut_card::forget(&roots, ready.agent.id);
            let (cli, mcp) = agent_entry::wire_glut_master(&roots, &ready.agent);
            let Some(cli) = cli else {
                return Err(crate::roster::refusal(
                    "Glut master",
                    mcp.note
                        .clone()
                        .unwrap_or_else(|| "its team server could not be written".into()),
                ));
            };
            ready.launch_command = format!(
                "{} harness master",
                agent_entry::shell_quote(&cli.display().to_string())
            );
            Ok(Started { ready, mcp })
        }
        Harness::Glut => {
            // Everything the run is told goes into the channel (`glut_card`); the pane types only the CLI's command,
            // which finds it there by the session's id.
            let (spec, mcp) = agent_entry::wire_glut(
                &roots,
                &ready.agent,
                &ready.cwd,
                role.as_ref(),
                launch.as_ref(),
            );
            if launch.is_some() {
                require_entry(&mcp)?;
            }
            let (Some(_), Some(cli)) = (spec, agent_entry::cli_path()) else {
                return Err(crate::roster::refusal(
                    "Glut session",
                    mcp.note
                        .clone()
                        .unwrap_or_else(|| "its start could not be written".into()),
                ));
            };
            ready.launch_command = format!(
                "{} harness card",
                agent_entry::shell_quote(&cli.display().to_string())
            );
            Ok(Started { ready, mcp })
        }
    }
}

/// Whether a Claude Code command line already names or continues a session (`--resume`, `--continue`, `--session-id`),
/// which `--session-id` cannot be combined with.
fn resumes_a_session(command: &str) -> bool {
    command.split_whitespace().any(|word| {
        matches!(
            word,
            "--resume" | "-r" | "--continue" | "-c" | "--session-id" | "--from-pr"
        )
    })
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

/// What a session was started for, if the studio started it for something that still wants it: a plan that is still a
/// draft (a planner), or a card ([`card_launch`]).
pub(crate) fn launch_of(
    db: &rusqlite::Connection,
    agent: &crate::ide::model::Agent,
) -> Option<agent_entry::Launch> {
    if let Some(plan_id) = agent.plan_id {
        return plan_launch(db, plan_id).map(agent_entry::Launch::Plan);
    }
    card_launch(db, agent).map(agent_entry::Launch::Card)
}

/// A planner is wanted while its plan is a draft: an approved or closed plan has had its say, and a restarted pane must
/// not go on proposing cards into it.
fn plan_launch(db: &rusqlite::Connection, plan_id: i64) -> Option<agent_entry::PlanLaunch> {
    let plan = crate::board::flow::get_plan(db, plan_id).ok()??;
    (plan.status == crate::board::PlanStatus::Draft).then_some(agent_entry::PlanLaunch {
        plan_id,
        interview: false,
    })
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
            .then(|| {
                agent_entry::CardLaunch::work(card_id).with_auto_rights(plan_auto_rights(db, &card))
            });
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
    Some(
        agent_entry::CardLaunch::review(card_id, &worker.name, &base)
            .with_auto_rights(plan_auto_rights(db, &card)),
    )
}

/// Whether the card's plan is in the owner's auto mode, read now, at the start: a session that runs keeps what it
/// started with. Only a released plan that still runs counts — a closed one's switch is history.
fn plan_auto_rights(db: &rusqlite::Connection, card: &crate::board::Card) -> bool {
    card.plan_id
        .and_then(|plan_id| crate::board::flow::get_plan(db, plan_id).ok().flatten())
        .is_some_and(|plan| plan.auto_rights && plan.status == crate::board::PlanStatus::Approved)
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
    fn a_command_that_resumes_a_session_gets_no_session_id_of_ours() {
        assert!(resumes_a_session("claude --resume"));
        assert!(resumes_a_session("claude --model 'x' --continue"));
        assert!(resumes_a_session("claude --session-id abc"));
        assert!(!resumes_a_session("claude --model 'x' --settings '/a/b'"));
    }
}
