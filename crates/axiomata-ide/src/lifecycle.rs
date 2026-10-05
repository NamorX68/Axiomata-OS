//! What an agent is doing right now, and what it plans to do (M7.2 CP6/CP6b).
//!
//! A running agent is a harness in a PTY. The pane shows its pixels, but until
//! this module the app knew nothing about it: not whether it is working, not
//! whether it is waiting for somebody to approve a tool call, not what its plan
//! is. This module is the **channel** through which a harness tells us — one
//! directory per agent under `~/.axiomata/agent-events/<id>/`, handed in by the
//! embedder like every other path this crate touches.
//!
//! How it is wired, and why (`docs/plans/agent-lifecycle.md`, E9–E20):
//!
//! * **The harness writes, we read.** Claude Code gets a settings file of our
//!   own via `claude --settings`, whose hooks are plain `sh` lines writing the
//!   finished state word into a file; Rust reads one line (E11). Nothing here
//!   depends on an Axiomata binary being on the agent's `PATH` — the bundled
//!   app does not ship the CLI.
//! * **Opencode is not on this channel any more** (`docs/plans/opencode2.md`,
//!   OC3): Opencode 2 runs its sessions on a shared service, and the embedder
//!   reads their state from that service's event stream and lays it over what
//!   this module reports. The v1 plugin that used to write here is gone.
//! * **Nothing is ever written into a worktree** (E12). Both hookups live in the
//!   channel directory, so an agent can never commit them by accident, and an
//!   agent sharing a plain project folder gets a status too.
//! * **Current state, not a history** (E10): `state` holds one word and a Unix
//!   timestamp, `started` when the agent was last started. Every write is a
//!   temporary file plus a rename.
//! * **One named exception to "the harness writes our format"** (E14): Claude
//!   Code's plan is read from Claude Code's own task directory
//!   (`<claude-home>/tasks/<list>/`), because its task tools only report single
//!   changes to a hook, never the whole list. The list id is pinned per agent
//!   through `CLAUDE_CODE_TASK_LIST_ID`. That format is Claude Code's internal
//!   one, so it is read tolerantly: anything unexpected is skipped, never an
//!   error that could take a pane down.
//! * **Claude Code is nudged to keep a visible plan** (E19): a `planning.md`
//!   instruction goes to it via `--append-system-prompt-file`, additive to
//!   whatever the user already has. Without it, a small task never got a task
//!   list at all, and the Plan tab stayed empty.
//! * **Claude Code's plan-mode plan gets its own document, not just tasks**
//!   (E20): it writes that plan as Markdown to `<claude-home>/plans/<name>.md`,
//!   which a `Write`/`Edit` hook merely notices and points at
//!   (`Channel::plan_pointer_line`); Rust only reads the file the pointer
//!   names, and only after confirming its *canonical* path really lies inside
//!   the plan folder (`Channel::read_plan_document`) — the pointer comes from
//!   a foreign process's payload, so it is never trusted outright.
//!
//! The status is runtime state and deliberately **not** in the database (E9):
//! a row saying "working" that survived a restart of the app would be a lie
//! afterwards. The plan, by contrast, does survive a restart of the agent (E16)
//! and is marked as coming from an earlier session until the agent writes a new
//! one.

use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::{Harness, shell_quote};
use crate::{IdeError, Result};

/// The file holding the current state word and when it was written.
const STATE_FILE: &str = "state";
/// The file holding when the agent was last started (Unix seconds).
const STARTED_FILE: &str = "started";
/// The settings file handed to `claude --settings`.
const CLAUDE_SETTINGS_FILE: &str = "claude-settings.json";
/// Where the v1 Opencode plugin and its `node_modules/` used to live; removed
/// on the next start of an agent that still has it (OC3).
const LEGACY_OPENCODE_DIR: &str = "opencode";
/// Present while the session has used up a limit (`docs/plans/a2a.md`, A9); its text is what Claude Code is told when
/// the `PreToolUse` hook refuses a tool call because of it.
const LIMIT_FILE: &str = "limit-reached";
/// The Claude Code session ids a card session was started with, one per line: every start is a new transcript, and the
/// usage of the card is the sum of them all.
const CLAUDE_SESSIONS_FILE: &str = "claude-sessions";
/// The most session ids read back. The file is reachable by the session itself, so the real ids are the *first* ones.
const MAX_CLAUDE_SESSIONS: usize = 100;
/// The instruction that asks an agent to keep a visible plan (M7.2 CP6, (b)).
const PLANNING_FILE: &str = "planning.md";
/// The last `Write`/`Edit` payload that touched Claude Code's plan folder.
const PLAN_MODE_POINTER: &str = "plan-mode.json";
/// What [`PLANNING_FILE`] says (Claude Code's task tools; Opencode 2 has no
/// todo tool — its step list comes with Axiomata's MCP server, opencode2.md Q3).
const PLANNING_TEXT: &str = "\
When a task takes more than one step, keep a task list with your task tools \
(TaskCreate/TaskUpdate) from the start, and update it as you go, so the user \
can follow your progress in the IDE's Plan tab.
";
/// The most we read of any one channel file. Everything in it is written by a
/// foreign process and read every second; a real plan is a few kilobytes.
const MAX_READ_BYTES: u64 = 256 * 1024;
/// The most Claude Code task files looked at for one plan.
const MAX_TASK_FILES: usize = 200;

/// The two roots a status channel lives under. The crate has no paths of its
/// own; the embedder hands these in, bundled with the rest in
/// [`crate::provision::Locations`].
#[derive(Debug, Clone)]
pub struct ChannelRoots {
    /// One status channel per agent (`~/.axiomata/agent-events`).
    pub events: PathBuf,
    /// Claude Code's task lists (`$CLAUDE_CONFIG_DIR/tasks`, else
    /// `~/.claude/tasks`). Read for the plan, and written to only by
    /// [`Channel::forget`], which removes exactly one list — the agent's own.
    pub claude_tasks: PathBuf,
    /// Where Claude Code writes plan-mode plans (`<claude-home>/plans`). Only
    /// ever read, and only a file a hook saw Claude write there.
    pub claude_plans: PathBuf,
}

/// What an agent is doing, as far as its harness has told us.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Started, but the harness has not reported anything yet — or it has no
    /// channel at all (an own command that did not hook itself up, the mini
    /// harness). The frontend decides how long "starting" stays believable.
    Starting,
    /// Finished its turn; ready for the next prompt. This is when a message
    /// may be delivered to it (M7.5 CP13).
    Idle,
    /// In the middle of a turn.
    Working,
    /// Blocked on a human: a permission prompt, or a question it asked.
    Waiting,
    /// The harness has exited. The shell around it may still be running.
    Ended,
}

impl AgentState {
    /// The spelling the hooks write into the `state` file.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentState::Starting => "starting",
            AgentState::Idle => "idle",
            AgentState::Working => "working",
            AgentState::Waiting => "waiting",
            AgentState::Ended => "ended",
        }
    }

    /// Parses what a hook wrote. `None` for anything else.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "starting" => Some(AgentState::Starting),
            "idle" => Some(AgentState::Idle),
            "working" => Some(AgentState::Working),
            "waiting" => Some(AgentState::Waiting),
            "ended" => Some(AgentState::Ended),
            _ => None,
        }
    }
}

/// Where a plan step stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepState {
    Todo,
    Doing,
    Done,
    /// A dropped step; shown struck through.
    Cancelled,
}

impl StepState {
    /// Maps Claude Code's task status spelling (`pending` / `in_progress` /
    /// `completed`) — or our own words — onto ours.
    /// Unknown ⇒ `None`, and the caller decides what that means.
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "pending" | "todo" => Some(StepState::Todo),
            "in_progress" | "doing" => Some(StepState::Doing),
            "completed" | "done" => Some(StepState::Done),
            "cancelled" => Some(StepState::Cancelled),
            _ => None,
        }
    }
}

/// One step of an agent's plan, in the shape both harnesses share (E17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStep {
    pub text: String,
    pub state: StepState,
    /// Claude Code's task description. Shown as a tooltip only.
    pub detail: Option<String>,
}

/// An agent's latest plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanSnapshot {
    pub steps: Vec<PlanStep>,
    /// When the plan last changed, as far as the file system knows.
    pub updated_at: Option<DateTime<Utc>>,
    /// True when the plan is older than the agent's last start — it survived a
    /// restart (E16), and the UI must not show it as if it were current.
    pub from_earlier_session: bool,
}

/// Everything the UI shows about one agent, read in one go.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent_id: i64,
    pub state: AgentState,
    /// When the harness entered `state`; for `starting`, when it was started.
    pub since: Option<DateTime<Utc>>,
    /// When the agent was last started, if ever.
    pub started_at: Option<DateTime<Utc>>,
    pub plan: Option<PlanSnapshot>,
    /// Claude Code's plan-mode plan, if it wrote one (Markdown).
    pub plan_document: Option<PlanDocument>,
}

/// A plan-mode plan: the Markdown file Claude Code writes before it asks for
/// approval. Shown beside the task list, not instead of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanDocument {
    pub markdown: String,
    /// The plan file's name, for the heading.
    pub name: String,
    pub updated_at: Option<DateTime<Utc>>,
    pub from_earlier_session: bool,
}

/// How a harness is attached to its channel, as [`Channel::install`] returns it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hookup {
    /// Appended to the *generated* command line only — never to an own
    /// command (E13). Already shell-quoted.
    pub args: Option<String>,
    /// `KEY=value` lines added to the agent's environment on every start,
    /// own command or not.
    pub env: Vec<String>,
    /// Whether this harness reports into the channel at all.
    pub connected: bool,
}

/// One agent's status channel.
#[derive(Debug, Clone)]
pub struct Channel {
    agent_id: i64,
    dir: PathBuf,
    claude_list_id: String,
    claude_list_dir: PathBuf,
    claude_plans: PathBuf,
}

impl Channel {
    /// The channel of agent `agent_id`. Touches nothing on disk.
    pub fn for_agent(roots: &ChannelRoots, agent_id: i64) -> Self {
        let claude_list_id = claude_list_id(&roots.events, agent_id);
        Channel {
            agent_id,
            dir: roots.events.join(agent_id.to_string()),
            claude_list_dir: roots.claude_tasks.join(&claude_list_id),
            claude_plans: roots.claude_plans.clone(),
            claude_list_id,
        }
    }

    /// The channel directory itself.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Prepares the channel for a new start: drops the old state word and
    /// records the start time.
    ///
    /// The plan is deliberately left alone (E16). What the v1 Opencode plugin
    /// left here (its config directory with `node_modules/`) is removed.
    pub fn reset(&self) -> Result<()> {
        ensure_plain_dir(&self.dir)?;
        remove_file(&self.dir.join(STATE_FILE))?;
        // The stop (`limit-reached`) is left alone: a restarted pane over its limit is refused from its first call, and
        // only a look that finds the session within its limits again lifts it.
        remove_dir(&self.dir.join(LEGACY_OPENCODE_DIR))?;
        write_atomic(&self.dir.join(STARTED_FILE), &format!("{}\n", unix_now()))
    }

    /// Writes Claude Code's hookup into the channel and says how to use it.
    ///
    /// Always written, whatever the profile's harness: an own command may well
    /// start Claude Code, and the env this returns points at it. Each file is
    /// only rewritten when its content changed. Opencode needs nothing here —
    /// its state comes from the Opencode service (OC3) — but reports its
    /// status all the same, so it counts as connected.
    pub fn install(&self, harness: Harness) -> Result<Hookup> {
        let settings = self.dir.join(CLAUDE_SETTINGS_FILE);
        ensure_plain_dir(&self.dir)?;
        let planning = self.dir.join(PLANNING_FILE);
        write_if_changed(&settings, &self.claude_settings())?;
        write_if_changed(&planning, PLANNING_TEXT)?;

        let env = vec![
            format!("AXIOMATA_EVENTS={}", self.dir.display()),
            format!("AXIOMATA_CLAUDE_SETTINGS={}", settings.display()),
            format!("CLAUDE_CODE_TASK_LIST_ID={}", self.claude_list_id),
        ];
        let args = match harness {
            Harness::ClaudeCode => Some(format!(
                "--settings {} --append-system-prompt-file {}",
                shell_quote(&settings.display().to_string()),
                shell_quote(&planning.display().to_string()),
            )),
            Harness::Opencode | Harness::Mini => None,
        };
        Ok(Hookup {
            args,
            env,
            // The mini harness does not exist yet (M7.4) and brings its own
            // mapping when it does (E18).
            connected: harness != Harness::Mini,
        })
    }

    /// Writes `name` into the channel readable by this user only (`0600`), through a temporary file and a rename.
    ///
    /// For what holds a session secret — the MCP configuration of Claude Code and the hash of the secret
    /// ([`crate::session_token`]) — which must not be world-readable on a shared machine. `name` is a plain file name:
    /// anything with a separator would write outside the channel.
    pub fn write_private(&self, name: &str, content: &str) -> Result<()> {
        if name.is_empty() || name == ".." || name.contains('/') || name.contains('\\') {
            return Err(IdeError::Invalid {
                field: "channel file",
                reason: format!("{name:?} is not a plain file name"),
            });
        }
        ensure_plain_dir(&self.dir)?;
        let path = self.dir.join(name);
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        // A leftover of a crashed start would make `create_new` fail forever.
        remove_file(&tmp)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(io_error(&tmp))?;
        file.write_all(content.as_bytes()).map_err(io_error(&tmp))?;
        fs::rename(&tmp, &path).map_err(io_error(&path))
    }

    /// Adds `extra` to the system prompt Claude Code is started with: the planning hint plus the session's own
    /// instructions (its role, how to use the mailbox). Written over `planning.md`, the file
    /// `--append-system-prompt-file`
    /// already points at, so the launch command does not change. Call it after [`Channel::install`]; an empty `extra`
    /// leaves the planning hint alone.
    pub fn append_instructions(&self, extra: &str) -> Result<()> {
        let extra = extra.trim();
        if extra.is_empty() {
            return Ok(());
        }
        write_if_changed(
            &self.dir.join(PLANNING_FILE),
            &format!("{PLANNING_TEXT}\n\n{extra}\n"),
        )
    }

    /// Reads the agent's state and plan. Never fails: a channel that is
    /// missing or garbled reads as "starting, no plan".
    pub fn read_status(&self, harness: Harness) -> AgentStatus {
        let started_at =
            read_to_string(&self.dir.join(STARTED_FILE)).and_then(|text| parse_unix(text.trim()));
        let (state, since) = read_to_string(&self.dir.join(STATE_FILE))
            .and_then(|text| parse_state_line(&text))
            .map_or((AgentState::Starting, started_at), |(state, since)| {
                (state, Some(since))
            });
        AgentStatus {
            agent_id: self.agent_id,
            state,
            since,
            started_at,
            plan: self.read_plan(harness, started_at),
            // Opencode's plan agent answer comes from the service (OC3).
            plan_document: match harness {
                Harness::ClaudeCode => self.read_plan_document(started_at),
                Harness::Opencode | Harness::Mini => None,
            },
        }
    }

    /// Marks the session as having used up a limit: from now on the `PreToolUse` hook of Claude Code refuses every tool
    /// call and shows `text` as the reason. Opencode has no such hook; the studio interrupts it instead.
    pub fn set_limit_reached(&self, text: &str) -> Result<()> {
        ensure_plain_dir(&self.dir)?;
        write_atomic(&self.dir.join(LIMIT_FILE), &format!("{}\n", text.trim()))
    }

    /// The reason the session was stopped for, if it was.
    pub fn limit_reached(&self) -> Option<String> {
        read_to_string(&self.dir.join(LIMIT_FILE)).map(|text| text.trim().to_owned())
    }

    /// Lifts the stop: the limit was raised, or the session no longer reaches it.
    pub fn clear_limit_reached(&self) -> Result<()> {
        remove_file(&self.dir.join(LIMIT_FILE))
    }

    /// Remembers a Claude Code session id this agent was started with, so its transcript can be found later.
    ///
    /// # Errors
    ///
    /// [`IdeError::Invalid`] for anything that is not a session id of [`crate::usage::new_claude_session_id`]'s
    /// shape — it ends up in a file name — and [`IdeError::Io`].
    pub fn record_claude_session(&self, id: &str) -> Result<()> {
        use std::io::Write;
        if !crate::usage::is_claude_session_id(id) {
            return Err(IdeError::Invalid {
                field: "claude session id",
                reason: "not a session id".into(),
            });
        }
        ensure_plain_dir(&self.dir)?;
        let path = self.dir.join(CLAUDE_SESSIONS_FILE);
        // The channel is reachable by the session: a link put in place of the file would take the line elsewhere.
        if fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) {
            remove_file(&path)?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(io_error(&path))?;
        writeln!(file, "{id}").map_err(io_error(&path))
    }

    /// The Claude Code session ids this agent was started with, oldest first. Lines that are not session ids are
    /// skipped: the file is read back as a source of file names.
    pub fn claude_sessions(&self) -> Vec<String> {
        let text = read_to_string(&self.dir.join(CLAUDE_SESSIONS_FILE)).unwrap_or_default();
        let mut ids: Vec<String> = text
            .lines()
            .map(str::trim)
            .filter(|line| crate::usage::is_claude_session_id(line))
            .map(str::to_owned)
            .collect();
        // The oldest are kept: lines appended to the file by a session must not push the real ids out.
        ids.truncate(MAX_CLAUDE_SESSIONS);
        ids
    }

    /// Removes the channel and the agent's own Claude Code task list, and
    /// nothing else. Called when the agent is deleted. Missing is fine.
    pub fn forget(&self) -> Result<()> {
        remove_dir(&self.dir)?;
        remove_dir(&self.claude_list_dir)
    }

    fn read_plan(
        &self,
        harness: Harness,
        started_at: Option<DateTime<Utc>>,
    ) -> Option<PlanSnapshot> {
        let (steps, updated_at) = match harness {
            Harness::ClaudeCode => read_claude_tasks(&self.claude_list_dir)?,
            Harness::Opencode | Harness::Mini => return None,
        };
        if steps.is_empty() {
            return None;
        }
        let from_earlier_session = is_earlier(updated_at, started_at);
        Some(PlanSnapshot {
            steps,
            updated_at,
            from_earlier_session,
        })
    }

    /// The plan-mode plan a hook last saw Claude Code write.
    ///
    /// The pointer file is written from a foreign process's payload, so the
    /// path in it is **not** trusted: it is canonicalised and must still lie
    /// inside Claude Code's plan folder and end in `.md`, or nothing is read.
    /// Without that, a channel could be made to show any file on the disk.
    fn read_plan_document(&self, started_at: Option<DateTime<Utc>>) -> Option<PlanDocument> {
        let pointer: Value =
            serde_json::from_str(&read_to_string(&self.dir.join(PLAN_MODE_POINTER))?).ok()?;
        let raw = pointer.get("tool_input")?.get("file_path")?.as_str()?;
        let path = fs::canonicalize(raw).ok()?;
        let plans = fs::canonicalize(&self.claude_plans).ok()?;
        if !path.starts_with(&plans) || path.extension().and_then(|e| e.to_str()) != Some("md") {
            return None;
        }
        let name = path.file_stem()?.to_string_lossy().into_owned();
        read_document(&path, &name, started_at)
    }

    /// The hook that notices a plan-mode plan being written: keeps the
    /// `Write`/`Edit` payload when its path is inside Claude Code's plan
    /// folder, ignores every other file. A `case` on the payload text rather
    /// than a JSON parse, so it needs nothing but `sh`; Rust re-checks the
    /// path properly when it reads it (`read_plan_document`).
    fn plan_pointer_line(&self) -> String {
        let dir = shell_quote(&self.dir.display().to_string());
        let plans = shell_quote(&format!("{}/", self.claude_plans.display()));
        format!(
            "p=$(cat); case \"$p\" in *{plans}*) d={dir}; printf '%s' \"$p\" > \"$d/{PLAN_MODE_POINTER}.$$\" && mv -f \"$d/{PLAN_MODE_POINTER}.$$\" \"$d/{PLAN_MODE_POINTER}\";; esac"
        )
    }

    /// The settings file for `claude --settings`: one `sh` line per event.
    ///
    /// Built with `serde_json`, so the quoting of the shell line inside the
    /// JSON string is serde's problem and not a hand-rolled escape.
    fn claude_settings(&self) -> String {
        let hook =
            |state: AgentState| json!([{ "type": "command", "command": self.state_line(state) }]);
        let waiting = json!([{ "type": "command", "command": self.waiting_line() }]);
        let plan_pointer = json!([{ "type": "command", "command": self.plan_pointer_line() }]);
        let limit = json!([{ "type": "command", "command": self.limit_line() }]);
        let settings = json!({
            "_axiomata": format!(
                "Written by Axiomata-OS for agent {} on every start; edits are overwritten.",
                self.agent_id
            ),
            "hooks": {
                "SessionStart": [{ "hooks": hook(AgentState::Idle) }],
                "UserPromptSubmit": [{ "hooks": hook(AgentState::Working) }],
                // A session that used up a limit may finish the step it is in and then call nothing more (A9).
                "PreToolUse": [{ "matcher": "*", "hooks": limit }],
                // Every tool call, so an approved permission prompt leaves
                // `waiting` again instead of lasting until the turn ends.
                "PostToolUse": [
                    { "matcher": "*", "hooks": hook(AgentState::Working) },
                    // Plan mode writes its plan as a file (`~/.claude/plans/`).
                    { "matcher": "Write|Edit", "hooks": plan_pointer },
                ],
                // No matcher: the hook filters on `notification_type` itself
                // (see `waiting_line`), because the 60-second `idle_prompt`
                // made an idle agent read as "waiting" (live test, CP6).
                "Notification": [{ "hooks": waiting }],
                "Stop": [{ "hooks": hook(AgentState::Idle) }],
                "SessionEnd": [{ "hooks": hook(AgentState::Ended) }],
            },
        });
        // A `Value` built from literals always serialises.
        let mut text = serde_json::to_string_pretty(&settings).unwrap_or_default();
        text.push('\n');
        text
    }

    /// The `PreToolUse` hook: refuses the call (exit code 2, the reason on stderr goes to the model) while the limit
    /// marker exists, and does nothing otherwise. A plain file test, so it costs a tool call no more than a `stat`.
    fn limit_line(&self) -> String {
        let dir = shell_quote(&self.dir.display().to_string());
        format!(
            "d={dir}; if [ -e \"$d/{LIMIT_FILE}\" ]; then cat \"$d/{LIMIT_FILE}\" >&2; exit 2; fi"
        )
    }

    /// One hook line: write `<state> <unix time>` atomically.
    ///
    /// The temporary file carries the shell's pid (`$$`), because Claude Code
    /// runs `PostToolUse` hooks of parallel tool calls at the same time, and two
    /// of them sharing one temporary name would race each other's `mv`.
    fn state_line(&self, state: AgentState) -> String {
        let dir = shell_quote(&self.dir.display().to_string());
        format!(
            "d={dir}; printf '{} %s\\n' \"$(date +%s)\" > \"$d/{STATE_FILE}.$$\" && mv -f \"$d/{STATE_FILE}.$$\" \"$d/{STATE_FILE}\"",
            state.as_str()
        )
    }

    /// The `Notification` hook: `waiting` only when Claude Code is really
    /// blocked on the human — a permission prompt, or an MCP server asking for
    /// input (`elicitation_dialog`).
    ///
    /// Claude Code also notifies after 60 seconds of doing nothing
    /// (`idle_prompt`) and after a login (`auth_success`); neither is waiting
    /// for an answer, and the first one made an idle agent blink (live test,
    /// CP6). The type comes in the JSON on stdin, so a `grep` on it decides —
    /// no `jq`, which is not on every Mac.
    fn waiting_line(&self) -> String {
        format!(
            "if grep -Eq '\"notification_type\" *: *\"(permission_prompt|elicitation_dialog)\"'; then {}; fi",
            self.state_line(AgentState::Waiting)
        )
    }
}

/// The task list id for an agent: its id plus a short checksum of the channel
/// root, so agent #1 of a scratch `AXIOMATA_HOME` and agent #1 of the real
/// install do not share one list (E14).
///
/// FNV-1a, not `DefaultHasher`, because this must be stable across Rust
/// versions: the id names a directory that outlives the binary.
fn claude_list_id(events_root: &Path, agent_id: i64) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in events_root.display().to_string().as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("axiomata-agent-{agent_id}-{hash:08x}")
}

/// Reads Claude Code's task files (`<n>.json`, one per task) into plan steps,
/// ordered by their numeric id. `None` when there is no such list.
fn read_claude_tasks(dir: &Path) -> Option<(Vec<PlanStep>, Option<DateTime<Utc>>)> {
    let entries = fs::read_dir(dir).ok()?;
    let mut tasks: Vec<(u64, PlanStep)> = Vec::new();
    let mut newest: Option<SystemTime> = None;
    for entry in entries.flatten().take(MAX_TASK_FILES) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(value) =
            read_to_string(&path).and_then(|t| serde_json::from_str::<Value>(&t).ok())
        else {
            continue;
        };
        let Some(text) = value.get("subject").and_then(Value::as_str) else {
            continue;
        };
        // A status we do not know (a future "deleted", say) is not a step to
        // show as if it were open.
        let Some(state) = value
            .get("status")
            .and_then(Value::as_str)
            .and_then(StepState::parse)
        else {
            continue;
        };
        // Only a file that really is a task may move the plan's timestamp, or a
        // stray file would flip `from_earlier_session` without a plan change.
        if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
            newest = newest.max(Some(modified));
        }
        let order = value
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| id.parse().ok())
            .unwrap_or(u64::MAX);
        let detail = value
            .get("description")
            .and_then(Value::as_str)
            .filter(|d| !d.trim().is_empty())
            .map(str::to_string);
        tasks.push((
            order,
            PlanStep {
                text: text.to_string(),
                state,
                detail,
            },
        ));
    }
    tasks.sort_by_key(|(order, _)| *order);
    Some((
        tasks.into_iter().map(|(_, step)| step).collect(),
        newest.map(DateTime::<Utc>::from),
    ))
}

/// Reads a plan document; empty or missing ⇒ `None`.
fn read_document(
    path: &Path,
    name: &str,
    started_at: Option<DateTime<Utc>>,
) -> Option<PlanDocument> {
    let markdown = read_to_string(path)?;
    if markdown.trim().is_empty() {
        return None;
    }
    let updated_at = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .map(DateTime::<Utc>::from);
    Some(PlanDocument {
        name: name.to_string(),
        markdown,
        from_earlier_session: is_earlier(updated_at, started_at),
        updated_at,
    })
}

/// Whether something last changed before the agent's latest start — to the
/// second, so a plan written in the start's own second still counts as new.
fn is_earlier(updated_at: Option<DateTime<Utc>>, started_at: Option<DateTime<Utc>>) -> bool {
    match (updated_at, started_at) {
        (Some(updated), Some(started)) => updated.timestamp() < started.timestamp(),
        _ => false,
    }
}

/// Parses `"<state> <unix seconds>"`.
fn parse_state_line(text: &str) -> Option<(AgentState, DateTime<Utc>)> {
    let mut parts = text.split_whitespace();
    let state = AgentState::parse(parts.next()?)?;
    let since = parse_unix(parts.next()?)?;
    Some((state, since))
}

fn parse_unix(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(raw.parse().ok()?, 0)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Reads at most [`MAX_READ_BYTES`]. A longer file is cut off, which leaves
/// invalid JSON and therefore reads as "no plan" — the right answer for a file
/// no harness of ours would write.
fn read_to_string(path: &Path) -> Option<String> {
    use std::io::Read;
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(MAX_READ_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

fn io_error(path: &Path) -> impl FnOnce(std::io::Error) -> IdeError + '_ {
    move |source| IdeError::Io {
        path: path.to_path_buf(),
        source,
    }
}

fn create_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(io_error(path))
}

/// Creates `path` as a real directory, and each of its parents inside the
/// channel, **replacing a symlink rather than following it**.
///
/// The channel's path is in the agent's own environment (`AXIOMATA_EVENTS`),
/// so anything running in that shell can swap the directory for a link, and
/// the next start would then write our files wherever it points (security
/// audit, CP6). Not much of an escalation — the agent runs as the same user —
/// but refusing to be pointed elsewhere costs one `lstat` per start.
fn ensure_plain_dir(path: &Path) -> Result<()> {
    if let Ok(meta) = fs::symlink_metadata(path)
        && (meta.file_type().is_symlink() || !meta.is_dir())
    {
        // `remove_file` on a symlink removes the link, never its target.
        fs::remove_file(path).map_err(io_error(path))?;
    }
    create_dir(path)
}

fn remove_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(err) if err.kind() != ErrorKind::NotFound => Err(io_error(path)(err)),
        _ => Ok(()),
    }
}

fn remove_dir(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Err(err) if err.kind() != ErrorKind::NotFound => Err(io_error(path)(err)),
        _ => Ok(()),
    }
}

/// Writes `content` via a temporary file and a rename, so a reader never sees
/// half a file.
fn write_atomic(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        create_dir(parent)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    fs::write(&tmp, content).map_err(io_error(&tmp))?;
    fs::rename(&tmp, path).map_err(io_error(path))
}

fn write_if_changed(path: &Path, content: &str) -> Result<()> {
    if read_to_string(path).as_deref() == Some(content) {
        return Ok(());
    }
    write_atomic(path, content)
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-lifecycle-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Paths with a space and a quote in them, because that is what breaks
    /// generated shell lines.
    fn locations() -> ChannelRoots {
        let root = temp_dir("root").join("it's a home");
        ChannelRoots {
            events: root.join("agent-events"),
            claude_tasks: root.join("claude").join("tasks"),
            claude_plans: root.join("claude").join("plans"),
        }
    }

    /// Runs a hook line the way Claude Code does: through `sh`, with the
    /// event's JSON on stdin.
    fn run_hook(command: &str, stdin: &str) {
        use std::io::Write;
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        assert!(
            child.wait().unwrap().success(),
            "hook line failed: {command}"
        );
    }

    fn hook_command(settings: &str, event: &str) -> String {
        let value: Value = serde_json::from_str(settings).unwrap();
        value["hooks"][event][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn states_round_trip_and_garbage_is_refused() {
        for state in [
            AgentState::Starting,
            AgentState::Idle,
            AgentState::Working,
            AgentState::Waiting,
            AgentState::Ended,
        ] {
            assert_eq!(AgentState::parse(state.as_str()), Some(state));
        }
        assert_eq!(AgentState::parse("busy"), None);
        assert!(parse_state_line("working").is_none(), "no timestamp");
        assert!(parse_state_line("working soon").is_none());
        assert_eq!(
            parse_state_line("waiting 1790106506\n").map(|(s, t)| (s, t.timestamp())),
            Some((AgentState::Waiting, 1_790_106_506))
        );
    }

    #[test]
    fn an_empty_channel_reads_as_starting_without_a_plan() {
        let channel = Channel::for_agent(&locations(), 7);
        let status = channel.read_status(Harness::ClaudeCode);
        assert_eq!(status.state, AgentState::Starting);
        assert!(status.since.is_none() && status.started_at.is_none() && status.plan.is_none());
    }

    #[test]
    fn the_generated_claude_hooks_really_write_the_state() {
        let channel = Channel::for_agent(&locations(), 3);
        channel.reset().unwrap();
        channel.install(Harness::ClaudeCode).unwrap();
        let settings = fs::read_to_string(channel.dir().join(CLAUDE_SETTINGS_FILE)).unwrap();

        // Run the exact lines Claude Code would run, through a real `sh`.
        for (event, expected) in [
            ("SessionStart", AgentState::Idle),
            ("UserPromptSubmit", AgentState::Working),
            ("Notification", AgentState::Waiting),
            ("PostToolUse", AgentState::Working),
            ("Stop", AgentState::Idle),
            ("SessionEnd", AgentState::Ended),
        ] {
            let payload =
                json!({ "hook_event_name": event, "notification_type": "permission_prompt" });
            run_hook(&hook_command(&settings, event), &payload.to_string());
            let status = channel.read_status(Harness::ClaudeCode);
            assert_eq!(status.state, expected, "after {event}");
            assert!(status.since.is_some());
        }
        // No temporary files left behind.
        let leftovers: Vec<_> = fs::read_dir(channel.dir())
            .unwrap()
            .flatten()
            .filter(|e| {
                e.file_name().to_string_lossy().contains(".tmp")
                    || e.file_name().to_string_lossy().contains("state.")
            })
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn the_limit_hook_lets_every_call_pass_until_the_marker_exists_and_then_refuses_with_its_text()
    {
        let channel = Channel::for_agent(&locations(), 8);
        channel.reset().unwrap();
        channel.install(Harness::ClaudeCode).unwrap();
        let settings = fs::read_to_string(channel.dir().join(CLAUDE_SETTINGS_FILE)).unwrap();
        let command = hook_command(&settings, "PreToolUse");
        let run = || Command::new("sh").arg("-c").arg(&command).output().unwrap();

        let open = run();
        assert!(open.status.success() && open.stderr.is_empty());

        channel.set_limit_reached("steps: 60 of 60").unwrap();
        assert_eq!(channel.limit_reached().as_deref(), Some("steps: 60 of 60"));
        let refused = run();
        assert_eq!(
            refused.status.code(),
            Some(2),
            "exit 2 is how a hook refuses a call"
        );
        assert!(String::from_utf8_lossy(&refused.stderr).contains("steps: 60 of 60"));

        channel.clear_limit_reached().unwrap();
        assert!(run().status.success());
        assert_eq!(channel.limit_reached(), None);
    }

    #[test]
    fn a_new_start_does_not_lift_the_stop() {
        let channel = Channel::for_agent(&locations(), 9);
        channel.reset().unwrap();
        channel.set_limit_reached("tokens").unwrap();
        channel.reset().unwrap();
        assert_eq!(channel.limit_reached().as_deref(), Some("tokens"));
    }

    #[test]
    fn the_claude_sessions_of_an_agent_are_kept_across_starts_and_only_real_ids_are_read_back() {
        let channel = Channel::for_agent(&locations(), 10);
        channel.reset().unwrap();
        let first = crate::usage::new_claude_session_id().unwrap();
        let second = crate::usage::new_claude_session_id().unwrap();
        channel.record_claude_session(&first).unwrap();
        channel.reset().unwrap();
        channel.record_claude_session(&second).unwrap();
        assert_eq!(channel.claude_sessions(), vec![first, second.clone()]);

        assert!(matches!(
            channel.record_claude_session("../../etc/passwd"),
            Err(IdeError::Invalid { .. })
        ));
        // A line somebody else put there is not a file name to look up.
        let path = channel.dir().join(CLAUDE_SESSIONS_FILE);
        let mut text = fs::read_to_string(&path).unwrap();
        text.push_str("../../secret\n");
        fs::write(&path, text).unwrap();
        assert_eq!(channel.claude_sessions().len(), 2);

        // Nor may a flood of well-formed ids push the real ones out: the oldest are the ones kept.
        let mut text = fs::read_to_string(&path).unwrap();
        for _ in 0..(MAX_CLAUDE_SESSIONS + 20) {
            text.push_str(&crate::usage::new_claude_session_id().unwrap());
            text.push('\n');
        }
        fs::write(&path, text).unwrap();
        let kept = channel.claude_sessions();
        assert_eq!(kept.len(), MAX_CLAUDE_SESSIONS);
        assert_eq!(kept[1], second);
    }

    #[test]
    fn a_private_file_is_owner_only_replaced_whole_and_never_outside_the_channel() {
        use std::os::unix::fs::PermissionsExt;
        let channel = Channel::for_agent(&locations(), 6);
        channel.write_private("mcp.json", "one").unwrap();
        channel.write_private("mcp.json", "two").unwrap();
        let path = channel.dir().join("mcp.json");
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // Nothing of the temporary files is left, and a leftover of a crashed start does not block the next one.
        fs::write(
            channel
                .dir()
                .join(format!("mcp.{}.tmp", std::process::id())),
            "stale",
        )
        .unwrap();
        channel.write_private("mcp.json", "three").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "three");
        let leftovers: Vec<_> = fs::read_dir(channel.dir())
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        for bad in ["", "..", "../x", "a/b"] {
            assert!(channel.write_private(bad, "x").is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_session_instructions_go_after_the_planning_hint_and_replace_the_last_start_s() {
        let channel = Channel::for_agent(&locations(), 8);
        channel.install(Harness::ClaudeCode).unwrap();
        let planning = channel.dir().join(PLANNING_FILE);
        channel
            .append_instructions("  You are the reviewer.  ")
            .unwrap();
        let text = fs::read_to_string(&planning).unwrap();
        assert!(text.starts_with(PLANNING_TEXT), "{text}");
        assert!(text.trim_end().ends_with("You are the reviewer."), "{text}");
        // The next start installs the plain hint again and appends this start's text: nothing piles up.
        channel.install(Harness::ClaudeCode).unwrap();
        channel.append_instructions("You are the builder.").unwrap();
        let text = fs::read_to_string(&planning).unwrap();
        assert!(!text.contains("reviewer"), "{text}");
        assert!(text.contains("You are the builder."), "{text}");
        // Nothing to add leaves the hint alone.
        channel.install(Harness::ClaudeCode).unwrap();
        channel.append_instructions("  \n").unwrap();
        assert_eq!(fs::read_to_string(&planning).unwrap(), PLANNING_TEXT);
    }

    #[test]
    fn install_is_idempotent_and_only_claude_gets_an_argument() {
        let channel = Channel::for_agent(&locations(), 4);
        let first = channel.install(Harness::ClaudeCode).unwrap();
        let settings = channel.dir().join(CLAUDE_SETTINGS_FILE);
        let written = fs::metadata(&settings).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        let second = channel.install(Harness::ClaudeCode).unwrap();
        assert_eq!(first, second);
        assert_eq!(
            fs::metadata(&settings).unwrap().modified().unwrap(),
            written,
            "unchanged content must not be rewritten"
        );
        assert!(first.args.unwrap().starts_with("--settings '"));
        assert!(first.connected);

        let opencode = channel.install(Harness::Opencode).unwrap();
        assert!(
            opencode.args.is_none(),
            "opencode's state comes from its service (OC3)"
        );
        assert!(opencode.connected, "…and it reports all the same");
        assert!(
            !opencode
                .env
                .iter()
                .any(|l| l.starts_with("OPENCODE_CONFIG_DIR="))
        );
        assert!(
            opencode
                .env
                .iter()
                .any(|l| l.starts_with("CLAUDE_CODE_TASK_LIST_ID=axiomata-agent-4-"))
        );
        assert!(!channel.dir().join(LEGACY_OPENCODE_DIR).exists());

        assert!(!channel.install(Harness::Mini).unwrap().connected);
    }

    /// A Claude Code task file, as the task tools write it.
    fn claude_task(id: &str, subject: &str, status: &str) -> String {
        json!({ "id": id, "subject": subject, "description": "why", "status": status, "blocks": [], "blockedBy": [] })
            .to_string()
    }

    #[test]
    fn reset_clears_the_state_keeps_the_plan_and_drops_the_old_opencode_plugin() {
        let channel = Channel::for_agent(&locations(), 5);
        channel.install(Harness::ClaudeCode).unwrap();
        fs::write(channel.dir().join(STATE_FILE), "working 1\n").unwrap();
        fs::create_dir_all(&channel.claude_list_dir).unwrap();
        fs::write(
            channel.claude_list_dir.join("1.json"),
            claude_task("1", "a", "in_progress"),
        )
        .unwrap();
        // What the v1 Opencode plugin left behind before OC3.
        let modules = channel.dir().join(LEGACY_OPENCODE_DIR).join("node_modules");
        fs::create_dir_all(&modules).unwrap();

        channel.reset().unwrap();
        let status = channel.read_status(Harness::ClaudeCode);
        assert_eq!(status.state, AgentState::Starting);
        assert!(status.started_at.is_some());
        assert_eq!(status.since, status.started_at);
        assert!(!channel.dir().join(LEGACY_OPENCODE_DIR).exists());
        let plan = status.plan.expect("the plan survives a restart");
        assert_eq!(plan.steps[0].state, StepState::Doing);
    }

    #[test]
    fn a_plan_older_than_the_last_start_is_marked_as_such() {
        let channel = Channel::for_agent(&locations(), 6);
        fs::create_dir_all(channel.dir()).unwrap();
        fs::create_dir_all(&channel.claude_list_dir).unwrap();
        fs::write(
            channel.claude_list_dir.join("1.json"),
            claude_task("1", "a", "pending"),
        )
        .unwrap();
        // Started "in the future" relative to the plan's mtime.
        fs::write(
            channel.dir().join(STARTED_FILE),
            format!("{}\n", unix_now() + 60),
        )
        .unwrap();
        assert!(
            channel
                .read_status(Harness::ClaudeCode)
                .plan
                .unwrap()
                .from_earlier_session
        );

        fs::write(channel.dir().join(STARTED_FILE), "1\n").unwrap();
        assert!(
            !channel
                .read_status(Harness::ClaudeCode)
                .plan
                .unwrap()
                .from_earlier_session
        );
    }

    #[test]
    fn an_opencode_agent_reads_no_plan_from_the_channel() {
        // Its plan-agent answer comes from the Opencode service (OC3).
        let channel = Channel::for_agent(&locations(), 7);
        channel.reset().unwrap();
        let status = channel.read_status(Harness::Opencode);
        assert!(status.plan.is_none() && status.plan_document.is_none());
    }

    #[test]
    fn claude_tasks_are_read_in_order_and_garbage_is_skipped() {
        let locations = locations();
        let channel = Channel::for_agent(&locations, 8);
        let list = &channel.claude_list_dir;
        fs::create_dir_all(list).unwrap();
        let task = claude_task;
        fs::write(list.join("10.json"), task("10", "last", "pending")).unwrap();
        fs::write(list.join("2.json"), task("2", "second", "in_progress")).unwrap();
        fs::write(list.join("1.json"), task("1", "first", "completed")).unwrap();
        fs::write(list.join("3.json"), task("3", "gone", "deleted")).unwrap();
        fs::write(list.join("4.json"), "{ not json").unwrap();
        fs::write(list.join(".lock"), "").unwrap();

        let plan = channel.read_status(Harness::ClaudeCode).plan.unwrap();
        let summary: Vec<_> = plan
            .steps
            .iter()
            .map(|s| (s.text.as_str(), s.state))
            .collect();
        assert_eq!(
            summary,
            [
                ("first", StepState::Done),
                ("second", StepState::Doing),
                ("last", StepState::Todo)
            ]
        );
        assert_eq!(plan.steps[0].detail.as_deref(), Some("why"));
    }

    #[test]
    fn a_stale_or_garbled_state_line_falls_back_to_starting_using_the_started_time() {
        let channel = Channel::for_agent(&locations(), 10);
        channel.reset().unwrap();
        // A hook that got cut off mid-write, or a line from a future format
        // this binary does not know — either way not a word we parse.
        fs::write(channel.dir().join(STATE_FILE), "banana\n").unwrap();

        let status = channel.read_status(Harness::ClaudeCode);
        assert_eq!(status.state, AgentState::Starting);
        assert!(status.started_at.is_some());
        assert_eq!(
            status.since, status.started_at,
            "garbage must fall back to the started time, not None"
        );
    }

    #[test]
    fn write_if_changed_rewrites_only_when_the_content_differs() {
        let path = temp_dir("write-if-changed").join("f.txt");
        write_if_changed(&path, "a").unwrap();
        let first_write = fs::metadata(&path).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        write_if_changed(&path, "a").unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().modified().unwrap(),
            first_write,
            "identical content must not be rewritten"
        );

        write_if_changed(&path, "b").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "b",
            "changed content must actually be written"
        );
    }

    #[test]
    fn install_for_the_mini_harness_still_writes_both_hookups_but_returns_no_args() {
        let channel = Channel::for_agent(&locations(), 14);
        let hookup = channel.install(Harness::Mini).unwrap();
        assert!(
            hookup.args.is_none(),
            "the mini harness has no CLI to extend yet"
        );
        assert!(!hookup.connected);
        // Claude Code's hookup is always written (E13), whatever the
        // profile's own harness is — an own command could still start it.
        assert!(channel.dir().join(CLAUDE_SETTINGS_FILE).is_file());
        assert!(
            hookup
                .env
                .iter()
                .any(|l| l.starts_with("AXIOMATA_CLAUDE_SETTINGS=")),
            "{:?}",
            hookup.env
        );
    }

    #[test]
    fn forget_removes_only_this_agents_channel_and_task_list() {
        let locations = locations();
        let mine = Channel::for_agent(&locations, 11);
        let other = Channel::for_agent(&locations, 12);
        for channel in [&mine, &other] {
            channel.reset().unwrap();
            fs::create_dir_all(&channel.claude_list_dir).unwrap();
        }
        let foreign = locations.claude_tasks.join("some-claude-session");
        fs::create_dir_all(&foreign).unwrap();

        mine.forget().unwrap();
        assert!(!mine.dir().exists() && !mine.claude_list_dir.exists());
        assert!(other.dir().exists() && other.claude_list_dir.exists());
        assert!(
            foreign.exists(),
            "a task list that is not ours is never touched"
        );
        mine.forget().unwrap();
    }

    #[test]
    fn two_installs_do_not_share_a_task_list() {
        let a = claude_list_id(Path::new("/Users/x/.axiomata/agent-events"), 1);
        let b = claude_list_id(Path::new("/tmp/scratch/agent-events"), 1);
        assert_ne!(a, b);
        assert_eq!(
            a,
            claude_list_id(Path::new("/Users/x/.axiomata/agent-events"), 1)
        );
    }

    #[test]
    fn a_channel_swapped_for_a_symlink_is_replaced_not_followed() {
        let roots = locations();
        let channel = Channel::for_agent(&roots, 15);
        let elsewhere = temp_dir("elsewhere");
        fs::create_dir_all(&roots.events).unwrap();
        std::os::unix::fs::symlink(&elsewhere, channel.dir()).unwrap();

        channel.reset().unwrap();
        channel.install(Harness::ClaudeCode).unwrap();

        assert!(
            !fs::symlink_metadata(channel.dir())
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(channel.dir().join(CLAUDE_SETTINGS_FILE).is_file());
        assert_eq!(
            fs::read_dir(&elsewhere).unwrap().count(),
            0,
            "nothing may be written where the link pointed"
        );
    }

    #[test]
    fn only_a_prompt_that_needs_an_answer_counts_as_waiting() {
        let channel = Channel::for_agent(&locations(), 17);
        channel.reset().unwrap();
        channel.install(Harness::ClaudeCode).unwrap();
        let settings = fs::read_to_string(channel.dir().join(CLAUDE_SETTINGS_FILE)).unwrap();
        let notify = |kind: &str| {
            // Pretty-printed on purpose: the grep must not depend on spacing.
            let payload = serde_json::to_string_pretty(&json!({
                "hook_event_name": "Notification",
                "message": "Claude is waiting for your input",
                "notification_type": kind,
            }))
            .unwrap();
            run_hook(&hook_command(&settings, "Notification"), &payload);
            channel.read_status(Harness::ClaudeCode).state
        };

        run_hook(&hook_command(&settings, "Stop"), "{}");
        assert_eq!(
            notify("idle_prompt"),
            AgentState::Idle,
            "60 s of quiet is not a question"
        );
        assert_eq!(notify("auth_success"), AgentState::Idle);
        assert_eq!(notify("permission_prompt"), AgentState::Waiting);
        run_hook(&hook_command(&settings, "PostToolUse"), "{}");
        assert_eq!(notify("elicitation_dialog"), AgentState::Waiting);
    }

    fn plan_pointer_command(settings: &str) -> String {
        let value: Value = serde_json::from_str(settings).unwrap();
        value["hooks"]["PostToolUse"][1]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .to_string()
    }

    #[test]
    fn a_plan_mode_plan_is_found_through_the_file_claude_wrote() {
        let roots = locations();
        let channel = Channel::for_agent(&roots, 18);
        channel.reset().unwrap();
        channel.install(Harness::ClaudeCode).unwrap();
        let settings = fs::read_to_string(channel.dir().join(CLAUDE_SETTINGS_FILE)).unwrap();
        let pointer = plan_pointer_command(&settings);

        fs::create_dir_all(&roots.claude_plans).unwrap();
        let plan = roots.claude_plans.join("gentle-walrus.md");
        fs::write(&plan, "# Plan\n\n1. Read\n2. Write\n").unwrap();
        let elsewhere = roots.events.parent().unwrap().join("notes.md");
        fs::write(&elsewhere, "not a plan").unwrap();

        // An ordinary edit somewhere else leaves no pointer.
        let write = |path: &Path| {
            json!({ "tool_name": "Write", "tool_input": { "file_path": path, "content": "x" } })
                .to_string()
        };
        run_hook(&pointer, &write(&elsewhere));
        assert!(
            channel
                .read_status(Harness::ClaudeCode)
                .plan_document
                .is_none()
        );

        run_hook(&pointer, &write(&plan));
        let document = channel
            .read_status(Harness::ClaudeCode)
            .plan_document
            .unwrap();
        assert_eq!(document.name, "gentle-walrus");
        assert!(document.markdown.contains("2. Write"));
        assert!(!document.from_earlier_session);
        // Opencode has no plan mode file; nothing is looked for.
        assert!(
            channel
                .read_status(Harness::Opencode)
                .plan_document
                .is_none()
        );
    }

    #[test]
    fn a_forged_plan_pointer_cannot_show_a_file_outside_the_plan_folder() {
        let roots = locations();
        let channel = Channel::for_agent(&roots, 19);
        channel.reset().unwrap();
        fs::create_dir_all(&roots.claude_plans).unwrap();
        let secret = roots.events.parent().unwrap().join("secret.md");
        fs::write(&secret, "do not show").unwrap();
        for path in [
            secret.display().to_string(),
            // Inside by prefix, outside once resolved.
            format!("{}/../secret.md", roots.claude_plans.display()),
        ] {
            fs::write(
                channel.dir().join(PLAN_MODE_POINTER),
                json!({ "tool_input": { "file_path": path } }).to_string(),
            )
            .unwrap();
            assert!(
                channel
                    .read_status(Harness::ClaudeCode)
                    .plan_document
                    .is_none(),
                "{path}"
            );
        }
    }

    #[test]
    fn claude_code_is_asked_to_keep_a_visible_plan() {
        let channel = Channel::for_agent(&locations(), 20);
        let hookup = channel.install(Harness::ClaudeCode).unwrap();
        let planning = channel.dir().join(PLANNING_FILE);
        assert_eq!(fs::read_to_string(&planning).unwrap(), PLANNING_TEXT);
        assert!(hookup.args.unwrap().contains(&format!(
            "--append-system-prompt-file {}",
            shell_quote(&planning.display().to_string())
        )));
    }
}
