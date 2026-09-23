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
//!   own via `claude --settings`, whose hooks are plain `sh` lines. Opencode
//!   gets a plugin via `OPENCODE_CONFIG_DIR`. Both write the finished state word
//!   into a file; Rust reads one line (E11). Nothing here depends on an Axiomata
//!   binary being on the agent's `PATH` — the bundled app does not ship the CLI.
//! * **Nothing is ever written into a worktree** (E12). Both hookups live in the
//!   channel directory, so an agent can never commit them by accident, and an
//!   agent sharing a plain project folder gets a status too.
//! * **Current state, not a history** (E10): `state` holds one word and a Unix
//!   timestamp, `started` when the agent was last started, `plan.json` the
//!   latest Opencode plan. Every write is a temporary file plus a rename.
//! * **One named exception to "the harness writes our format"** (E14): Claude
//!   Code's plan is read from Claude Code's own task directory
//!   (`<claude-home>/tasks/<list>/`), because its task tools only report single
//!   changes to a hook, never the whole list. The list id is pinned per agent
//!   through `CLAUDE_CODE_TASK_LIST_ID`. That format is Claude Code's internal
//!   one, so it is read tolerantly: anything unexpected is skipped, never an
//!   error that could take a pane down.
//! * **Both harnesses are nudged to keep a visible plan** (E19): a shared
//!   `planning.md` instruction goes to Claude Code via
//!   `--append-system-prompt-file` and to Opencode via its config's
//!   `instructions`, additive to whatever the user already has. Without it, a
//!   small task never got a task list at all, and the Plan tab stayed empty.
//! * **Claude Code's plan-mode plan gets its own document, not just tasks**
//!   (E20): it writes that plan as Markdown to `<claude-home>/plans/<name>.md`,
//!   which a `Write`/`Edit` hook merely notices and points at
//!   (`Channel::plan_pointer_line`); Rust only reads the file the pointer
//!   names, and only after confirming its *canonical* path really lies inside
//!   the plan folder (`Channel::read_plan_document`) — the pointer comes from
//!   a foreign process's payload, so it is never trusted outright. Opencode has
//!   no separate plan mode; its plan agent's reply is kept the same way instead
//!   (`plan-mode.md`, written by the plugin).
//!
//! The status is runtime state and deliberately **not** in the database (E9):
//! a row saying "working" that survived a restart of the app would be a lie
//! afterwards. The plan, by contrast, does survive a restart of the agent (E16)
//! and is marked as coming from an earlier session until the agent writes a new
//! one.

use std::fs;
use std::io::ErrorKind;
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
/// The latest plan snapshot, in our own format. Written by the Opencode plugin.
const PLAN_FILE: &str = "plan.json";
/// The settings file handed to `claude --settings`.
const CLAUDE_SETTINGS_FILE: &str = "claude-settings.json";
/// Opencode's extra config directory. Opencode also installs `node_modules/`
/// into it, which is why [`Channel::reset`] never touches it.
const OPENCODE_DIR: &str = "opencode";
/// The plugin file inside [`OPENCODE_DIR`].
const OPENCODE_PLUGIN: &str = "plugin/axiomata-lifecycle.js";
/// The instruction that asks an agent to keep a visible plan (M7.2 CP6, (b)).
const PLANNING_FILE: &str = "planning.md";
/// Opencode's config inside [`OPENCODE_DIR`]: points at [`PLANNING_FILE`].
const OPENCODE_CONFIG: &str = "opencode.json";
/// Opencode's plan-agent answer, written by the plugin (Markdown).
const OPENCODE_PLAN_FILE: &str = "plan-mode.md";
/// The last `Write`/`Edit` payload that touched Claude Code's plan folder.
const PLAN_MODE_POINTER: &str = "plan-mode.json";
/// What [`PLANNING_FILE`] says. Harness-neutral on purpose: one text, both
/// harnesses, each with its own name for the tool.
const PLANNING_TEXT: &str = "\
When a task takes more than one step, keep a task list with your task/todo tool \
(TaskCreate/TaskUpdate in Claude Code, todowrite in Opencode) from the start, \
and update it as you go, so the user can follow your progress in the IDE's Plan tab.
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
    /// Only Opencode has this; shown struck through.
    Cancelled,
}

impl StepState {
    /// Maps either harness's status spelling onto ours. Both use `pending` /
    /// `in_progress` / `completed`; our own `plan.json` uses our own words.
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
    /// The plan is deliberately left alone (E16) — and so is Opencode's config
    /// directory, which holds the `node_modules/` Opencode installed there.
    pub fn reset(&self) -> Result<()> {
        ensure_plain_dir(&self.dir)?;
        remove_file(&self.dir.join(STATE_FILE))?;
        write_atomic(&self.dir.join(STARTED_FILE), &format!("{}\n", unix_now()))
    }

    /// Writes both harnesses' hookups into the channel and says how to use them.
    ///
    /// Both are always written, whatever the profile's harness: an own command
    /// may well start the other one, and the env this returns points at both.
    /// Each file is only rewritten when its content changed, so a restart does
    /// not make Opencode reinstall its plugin dependencies.
    pub fn install(&self, harness: Harness) -> Result<Hookup> {
        let settings = self.dir.join(CLAUDE_SETTINGS_FILE);
        let opencode = self.dir.join(OPENCODE_DIR);
        // Each level on its own: `create_dir_all` would follow a link at any
        // of them.
        for dir in [self.dir.clone(), opencode.clone(), opencode.join("plugin")] {
            ensure_plain_dir(&dir)?;
        }
        let planning = self.dir.join(PLANNING_FILE);
        write_if_changed(&settings, &self.claude_settings())?;
        write_if_changed(&planning, PLANNING_TEXT)?;
        write_if_changed(&opencode.join(OPENCODE_PLUGIN), &self.opencode_plugin())?;
        // `instructions` is additive to the user's own config (checked with
        // `opencode debug config`); nothing else is set here.
        let opencode_config = json!({ "instructions": [planning.display().to_string()] });
        write_if_changed(
            &opencode.join(OPENCODE_CONFIG),
            &format!(
                "{}\n",
                serde_json::to_string_pretty(&opencode_config).unwrap_or_default()
            ),
        )?;

        let env = vec![
            format!("AXIOMATA_EVENTS={}", self.dir.display()),
            format!("AXIOMATA_CLAUDE_SETTINGS={}", settings.display()),
            format!("CLAUDE_CODE_TASK_LIST_ID={}", self.claude_list_id),
            format!("OPENCODE_CONFIG_DIR={}", opencode.display()),
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
            plan_document: match harness {
                Harness::ClaudeCode => self.read_plan_document(started_at),
                Harness::Opencode => {
                    read_document(&self.dir.join(OPENCODE_PLAN_FILE), "plan agent", started_at)
                }
                Harness::Mini => None,
            },
        }
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
            Harness::Opencode => read_plan_file(&self.dir.join(PLAN_FILE))?,
            Harness::Mini => return None,
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
        let settings = json!({
            "_axiomata": format!(
                "Written by Axiomata-OS for agent {} on every start; edits are overwritten.",
                self.agent_id
            ),
            "hooks": {
                "SessionStart": [{ "hooks": hook(AgentState::Idle) }],
                "UserPromptSubmit": [{ "hooks": hook(AgentState::Working) }],
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

    /// The Opencode plugin: the same states from Opencode's own events, plus
    /// the plan from `todo.updated`, normalised into our format.
    fn opencode_plugin(&self) -> String {
        // A JSON string literal is a valid JavaScript string literal, so this is
        // the whole of the escaping.
        let dir = Value::String(self.dir.display().to_string()).to_string();
        OPENCODE_PLUGIN_TEMPLATE
            .replace("__AGENT_ID__", &self.agent_id.to_string())
            .replace("__DIR__", &dir)
    }
}

/// The plugin source. `__DIR__` and `__AGENT_ID__` are filled in by
/// [`Channel::opencode_plugin`]; everything else is fixed.
///
/// Only the main session counts (E15): Opencode's sub-agents run in sessions
/// with a `parentID`, and their `idle` would otherwise report the main agent as
/// finished while it is still working.
const OPENCODE_PLUGIN_TEMPLATE: &str = r#"// Written by Axiomata-OS for agent __AGENT_ID__ on every start; edits are overwritten.
// Reports this agent's state and plan into its status channel (M7.2 CP6).
import { renameSync, writeFileSync } from "node:fs";

const DIR = __DIR__;
const STEP = { pending: "todo", in_progress: "doing", completed: "done", cancelled: "cancelled" };
const children = new Set();
// Opencode's plan agent writes no todos: its plan is its answer. Text parts of
// the plan agent's replies are kept per message until the reply finishes.
const planReplies = new Map();

function put(name, text) {
  const tmp = `${DIR}/${name}.${process.pid}.tmp`;
  writeFileSync(tmp, text);
  renameSync(tmp, `${DIR}/${name}`);
}

function state(word) {
  try {
    put("state", `${word} ${Math.floor(Date.now() / 1000)}\n`);
  } catch {}
}

export const AxiomataLifecycle = async () => {
  state("idle");
  process.on("exit", () => state("ended"));
  return {
    event: async ({ event }) => {
      const props = event.properties ?? {};
      const info = props.info ?? {};
      if (event.type === "session.created" && info.parentID) {
        children.add(info.id);
        return;
      }
      const session =
        props.sessionID ?? info.sessionID ?? props.part?.sessionID ?? (event.type.startsWith("session.") ? info.id : undefined);
      if (session && children.has(session)) return;
      switch (event.type) {
        case "session.status":
          if (props.status?.type === "idle") state("idle");
          else if (props.status?.type) state("working");
          break;
        case "permission.asked":
        case "question.asked":
          state("waiting");
          break;
        case "permission.replied":
        case "question.replied":
        case "question.rejected":
          state("working");
          break;
        case "message.updated":
          if (info.role === "assistant" && info.agent === "plan") {
            if (!planReplies.has(info.id)) planReplies.set(info.id, new Map());
            if (info.finish === "stop") {
              const text = [...planReplies.get(info.id).values()].join("\n\n").trim();
              planReplies.delete(info.id);
              if (text) {
                try {
                  put("plan-mode.md", text + "\n");
                } catch {}
              }
            }
          }
          break;
        case "message.part.updated": {
          const part = props.part ?? {};
          const reply = planReplies.get(part.messageID);
          if (reply && part.type === "text") reply.set(part.id, String(part.text ?? ""));
          break;
        }
        case "todo.updated":
          try {
            const steps = (props.todos ?? []).map((todo) => ({
              text: String(todo.content ?? ""),
              state: STEP[todo.status] ?? "todo",
            }));
            put("plan.json", JSON.stringify({ steps }));
          } catch {}
          break;
      }
    },
  };
};
"#;

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

/// Reads our own `plan.json`, as the Opencode plugin writes it.
fn read_plan_file(path: &Path) -> Option<(Vec<PlanStep>, Option<DateTime<Utc>>)> {
    let value: Value = serde_json::from_str(&read_to_string(path)?).ok()?;
    let steps = value
        .get("steps")?
        .as_array()?
        .iter()
        .filter_map(|step| {
            Some(PlanStep {
                text: step.get("text")?.as_str()?.to_string(),
                state: step
                    .get("state")
                    .and_then(Value::as_str)
                    .and_then(StepState::parse)
                    .unwrap_or(StepState::Todo),
                detail: None,
            })
        })
        .collect();
    let updated_at = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .map(DateTime::<Utc>::from);
    Some((steps, updated_at))
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
            "opencode is hooked up through env only"
        );
        assert!(
            opencode
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
        assert!(
            channel
                .dir()
                .join(OPENCODE_DIR)
                .join(OPENCODE_PLUGIN)
                .is_file()
        );

        assert!(!channel.install(Harness::Mini).unwrap().connected);
    }

    #[test]
    fn reset_clears_the_state_but_keeps_the_plan_and_opencodes_folder() {
        let channel = Channel::for_agent(&locations(), 5);
        channel.install(Harness::Opencode).unwrap();
        fs::write(channel.dir().join(STATE_FILE), "working 1\n").unwrap();
        fs::write(
            channel.dir().join(PLAN_FILE),
            r#"{"steps":[{"text":"a","state":"doing"}]}"#,
        )
        .unwrap();
        let modules = channel.dir().join(OPENCODE_DIR).join("node_modules");
        fs::create_dir_all(&modules).unwrap();

        channel.reset().unwrap();
        let status = channel.read_status(Harness::Opencode);
        assert_eq!(status.state, AgentState::Starting);
        assert!(status.started_at.is_some());
        assert_eq!(status.since, status.started_at);
        assert!(
            modules.is_dir(),
            "opencode's own install must survive a restart"
        );
        let plan = status.plan.expect("the plan survives a restart");
        assert_eq!(plan.steps[0].state, StepState::Doing);
    }

    #[test]
    fn a_plan_older_than_the_last_start_is_marked_as_such() {
        let channel = Channel::for_agent(&locations(), 6);
        fs::create_dir_all(channel.dir()).unwrap();
        fs::write(
            channel.dir().join(PLAN_FILE),
            r#"{"steps":[{"text":"a","state":"todo"}]}"#,
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
                .read_status(Harness::Opencode)
                .plan
                .unwrap()
                .from_earlier_session
        );

        fs::write(channel.dir().join(STARTED_FILE), "1\n").unwrap();
        assert!(
            !channel
                .read_status(Harness::Opencode)
                .plan
                .unwrap()
                .from_earlier_session
        );
    }

    #[test]
    fn claude_tasks_are_read_in_order_and_garbage_is_skipped() {
        let locations = locations();
        let channel = Channel::for_agent(&locations, 8);
        let list = &channel.claude_list_dir;
        fs::create_dir_all(list).unwrap();
        let task = |id: &str, subject: &str, status: &str| {
            json!({ "id": id, "subject": subject, "description": "why", "status": status, "blocks": [], "blockedBy": [] })
                .to_string()
        };
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
    fn a_garbled_plan_file_is_no_plan_not_an_error() {
        let channel = Channel::for_agent(&locations(), 9);
        fs::create_dir_all(channel.dir()).unwrap();
        for garbage in ["", "null", "{\"steps\":7}", "{\"steps\":[]}", "[1,2]"] {
            fs::write(channel.dir().join(PLAN_FILE), garbage).unwrap();
            assert!(
                channel.read_status(Harness::Opencode).plan.is_none(),
                "{garbage:?}"
            );
        }
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
        // Both harnesses' hookups are always written (E13), whatever the
        // profile's own harness is — an own command could still start either.
        assert!(channel.dir().join(CLAUDE_SETTINGS_FILE).is_file());
        assert!(
            channel
                .dir()
                .join(OPENCODE_DIR)
                .join(OPENCODE_PLUGIN)
                .is_file()
        );
        assert!(
            hookup
                .env
                .iter()
                .any(|l| l.starts_with("AXIOMATA_CLAUDE_SETTINGS=")),
            "{:?}",
            hookup.env
        );
        assert!(
            hookup
                .env
                .iter()
                .any(|l| l.starts_with("OPENCODE_CONFIG_DIR=")),
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

    /// Runs the generated plugin under Node, when Node is installed, and feeds
    /// it the events Opencode sends. Skipped (not failed) without Node: the
    /// plugin's runtime is Opencode's own Bun, which a test cannot assume.
    #[test]
    fn the_generated_opencode_plugin_maps_events_and_ignores_sub_agents() {
        if Command::new("node").arg("--version").output().is_err() {
            eprintln!("node not installed — skipping the plugin test");
            return;
        }
        let channel = Channel::for_agent(&locations(), 13);
        channel.reset().unwrap();
        channel.install(Harness::Opencode).unwrap();
        let plugin = channel.dir().join(OPENCODE_DIR).join(OPENCODE_PLUGIN);
        let driver = format!(
            r#"
            const {{ pathToFileURL }} = await import("node:url");
            const {{ AxiomataLifecycle }} = await import(pathToFileURL({plugin}).href);
            const hooks = await AxiomataLifecycle({{}});
            const send = (type, properties) => hooks.event({{ event: {{ type, properties }} }});
            const fs = await import("node:fs");
            const read = () => fs.readFileSync({state}, "utf8").split(" ")[0];
            const out = [read()];
            await send("session.created", {{ info: {{ id: "main" }} }});
            await send("session.status", {{ sessionID: "main", status: {{ type: "busy" }} }}); out.push(read());
            await send("permission.asked", {{ sessionID: "main" }}); out.push(read());
            await send("permission.replied", {{ sessionID: "main" }}); out.push(read());
            await send("session.created", {{ info: {{ id: "child", parentID: "main" }} }});
            await send("session.status", {{ sessionID: "child", status: {{ type: "idle" }} }}); out.push(read());
            await send("todo.updated", {{ sessionID: "main", todos: [
              {{ content: "alpha", status: "in_progress", priority: "high" }},
              {{ content: "beta", status: "pending" }} ] }});
            await send("session.status", {{ sessionID: "main", status: {{ type: "idle" }} }}); out.push(read());
            // A plan-agent reply: its text becomes the plan document once it finishes.
            await send("message.updated", {{ info: {{ id: "m1", sessionID: "main", role: "assistant", agent: "plan" }} }});
            await send("message.part.updated", {{ part: {{ id: "p1", messageID: "m1", sessionID: "main", type: "text", text: "1. Read" }} }});
            await send("message.part.updated", {{ part: {{ id: "p1", messageID: "m1", sessionID: "main", type: "text", text: "1. Read\n2. Write" }} }});
            await send("message.updated", {{ info: {{ id: "m1", sessionID: "main", role: "assistant", agent: "plan", finish: "stop" }} }});
            // A build-agent reply is not a plan.
            await send("message.updated", {{ info: {{ id: "m2", sessionID: "main", role: "assistant", agent: "build" }} }});
            await send("message.part.updated", {{ part: {{ id: "p2", messageID: "m2", sessionID: "main", type: "text", text: "done" }} }});
            await send("message.updated", {{ info: {{ id: "m2", sessionID: "main", role: "assistant", agent: "build", finish: "stop" }} }});
            console.log(out.join(","));
            "#,
            plugin = Value::String(plugin.display().to_string()),
            state = Value::String(channel.dir().join(STATE_FILE).display().to_string()),
        );
        let output = Command::new("node")
            .args(["--input-type=module", "-e", &driver])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            // A sub-agent going idle must not report the main agent idle.
            "idle,working,waiting,working,working,idle"
        );
        let status = channel.read_status(Harness::Opencode);
        let document = status
            .plan_document
            .expect("the plan agent's reply is the plan");
        assert_eq!(document.markdown, "1. Read\n2. Write\n");
        let plan = status.plan.unwrap();
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].state, StepState::Doing);
        // The exit handler writes `ended` when the process goes away.
        assert_eq!(
            channel.read_status(Harness::Opencode).state,
            AgentState::Ended
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
    fn an_oversized_plan_file_is_no_plan() {
        let channel = Channel::for_agent(&locations(), 16);
        fs::create_dir_all(channel.dir()).unwrap();
        let huge = format!(
            r#"{{"steps":[{{"text":"{}","state":"todo"}}]}}"#,
            "x".repeat(MAX_READ_BYTES as usize)
        );
        fs::write(channel.dir().join(PLAN_FILE), huge).unwrap();
        assert!(channel.read_status(Harness::Opencode).plan.is_none());
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
    fn both_harnesses_are_asked_to_keep_a_visible_plan() {
        let channel = Channel::for_agent(&locations(), 20);
        let hookup = channel.install(Harness::ClaudeCode).unwrap();
        let planning = channel.dir().join(PLANNING_FILE);
        assert_eq!(fs::read_to_string(&planning).unwrap(), PLANNING_TEXT);
        assert!(hookup.args.unwrap().contains(&format!(
            "--append-system-prompt-file {}",
            shell_quote(&planning.display().to_string())
        )));
        let config: Value = serde_json::from_str(
            &fs::read_to_string(channel.dir().join(OPENCODE_DIR).join(OPENCODE_CONFIG)).unwrap(),
        )
        .unwrap();
        assert_eq!(config["instructions"][0], planning.display().to_string());
    }
}
