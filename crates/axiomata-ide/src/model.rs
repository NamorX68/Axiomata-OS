//! Domain types for the agentic IDE.
//!
//! Today that is one type, [`Project`]. The crate is cut this way from the
//! start because M7.2 onwards adds agents, worktrees and a mailbox beside it,
//! and they must land next to the project rather than inside `axiomata-core`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A folder the IDE works in, plus the dock layout left behind in it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    /// Absolute and canonicalised. See `schema.sql` for why that matters.
    pub repo_root: PathBuf,
    /// The frontend's serialised dock tree, opaque here. `None` = never
    /// opened, which is what makes the IDE build its starting layout.
    pub layout_json: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_opened_at: Option<DateTime<Utc>>,
    /// **Computed on read, never stored**: does `repo_root` still point at a
    /// directory?
    ///
    /// The database and the file system drift — an external disk is not
    /// mounted, a folder was renamed, a repository in iCloud got evicted. The
    /// list is the right place to notice, because noticing at open time means
    /// the user already clicked something that then failed. Costs one `is_dir`
    /// per project, against a list of maybe a dozen.
    ///
    /// ⚠️ `Project` derives `Deserialize` so it can travel back over IPC, which
    /// means a command could in principle *accept* one — including a
    /// client-supplied `root_exists` that nothing checked. Mutating commands
    /// therefore take narrow requests (an id, a name, a path), never a whole
    /// `Project`, so this field can only ever come from the store's own
    /// `is_dir`.
    pub root_exists: bool,
}

/// What a caller supplies to create a project. Validated by the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProject {
    pub name: String,
    pub repo_root: PathBuf,
}

/// Which harness runs an agent.
///
/// Stored as text (`schema.sql` says why) and refused rather than defaulted
/// when it is anything else: a row whose harness nobody recognises is a row
/// that would otherwise be started with the wrong program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Harness {
    /// Anthropic's Claude Code CLI.
    ClaudeCode,
    /// The Opencode CLI — the same harness skills and routines already run on.
    Opencode,
    /// Axiomata's own agent loop, `axiomata-miniagent` (M7.4). A profile can
    /// name it before it exists; starting one then fails, which is honest.
    Mini,
}

impl Harness {
    /// The stored spelling, and what the CLI accepts.
    pub fn as_str(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude_code",
            Harness::Opencode => "opencode",
            Harness::Mini => "mini",
        }
    }

    /// Parses the stored spelling. `None` for anything else — see the type's docs.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "claude_code" => Some(Harness::ClaudeCode),
            "opencode" => Some(Harness::Opencode),
            "mini" => Some(Harness::Mini),
            _ => None,
        }
    }

    /// The command line used when an agent's own `command` is empty.
    ///
    /// Resolved here rather than written into every row, so changing what
    /// "the default Opencode agent" means does not need a data migration —
    /// and so a row can still pin its own command when the default moves.
    pub fn default_command(self) -> &'static str {
        match self {
            Harness::ClaudeCode => "claude",
            Harness::Opencode => "opencode",
            // No binary yet (M7.4). Naming the crate rather than an empty
            // string makes the failure message say what is missing.
            Harness::Mini => "axiomata-miniagent",
        }
    }
}

/// An agent profile: what to start, not something running.
///
/// The running side — a PTY session, whether it is alive, what it is doing —
/// belongs to the pane showing it and does not survive the app. That is the
/// deliberate consequence of owning the PTY engine instead of using tmux
/// (plan question F7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub harness: Harness,
    /// Empty means [`Harness::default_command`]; [`Agent::effective_command`]
    /// is the one place that resolves it.
    pub command: String,
    /// `None` = the harness picks.
    pub model: Option<String>,
    /// `KEY=value` per line.
    pub env: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Where this agent works: its own git worktree, or `None` when the
    /// project is not a git repository (then it runs in the project folder,
    /// sharing it, as agents did before CP5).
    pub worktree_path: Option<PathBuf>,
    /// The branch checked out in that worktree.
    pub branch: Option<String>,
    /// A port reserved for this agent so two dev servers do not collide.
    pub port: Option<u16>,
    /// **Computed on read, never stored**: the command line that actually
    /// runs — `command` if it has one, else the harness's default.
    ///
    /// Sent along rather than left to the caller for the same reason
    /// [`Project::root_exists`] is: otherwise every frontend needs its own
    /// copy of the harness-to-default table, and a copy that drifts would show
    /// one command in the profile form while starting another.
    pub effective_command: String,
    /// **Computed on read, never stored**: the profile's own `env` plus the
    /// identity this agent runs with — `AXIOMATA_AGENT_ID`, `_NAME`,
    /// `_WORKTREE`, `_BRANCH`, `_PORT`.
    ///
    /// Identity lines come **last**, so a profile cannot quietly claim to be a
    /// different agent by declaring `AXIOMATA_AGENT_ID` itself: later wins,
    /// both here and in the frontend's `mergeEnv`.
    pub effective_env: String,
}

impl Agent {
    /// Builds what [`Agent::effective_env`] holds.
    ///
    /// The identity is what makes an agent addressable from inside its own
    /// shell — a tool can ask which agent it is running as, and a dev server
    /// can take `AXIOMATA_PORT` instead of guessing 1420 like every other one
    /// on the machine (the collision amux hit, recorded in the plan's §9).
    pub fn resolve_env(
        profile_env: &str,
        id: i64,
        name: &str,
        worktree: Option<&Path>,
        branch: Option<&str>,
        port: Option<u16>,
    ) -> String {
        let mut lines: Vec<String> = profile_env
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.trim().is_empty())
            .map(str::to_string)
            .collect();

        lines.push(format!("AXIOMATA_AGENT_ID={id}"));
        lines.push(format!("AXIOMATA_AGENT_NAME={name}"));
        if let Some(worktree) = worktree {
            lines.push(format!("AXIOMATA_WORKTREE={}", worktree.display()));
        }
        if let Some(branch) = branch {
            lines.push(format!("AXIOMATA_BRANCH={branch}"));
        }
        if let Some(port) = port {
            lines.push(format!("AXIOMATA_PORT={port}"));
        }
        lines.join("\n")
    }

    /// Resolves what [`Agent::effective_command`] holds. Used by the store
    /// when it builds one; a caller reads the field.
    ///
    /// The model is appended as `--model <value>` — both harnesses spell it
    /// that way (`opencode -m/--model provider/model`, `claude --model`) —
    /// but **only when the agent has no command of its own**. Somebody who
    /// wrote their own command line is responsible for it; pushing an extra
    /// flag into it could easily contradict what they typed.
    ///
    /// The value is single-quoted, because this string is written into a
    /// shell. A model id has no business containing a space or a bracket, but
    /// "has no business" is not a guarantee, and `(` is a glob character in
    /// zsh.
    pub fn resolve_command(command: &str, harness: Harness, model: Option<&str>) -> String {
        let own = command.trim();
        if !own.is_empty() {
            return own.to_string();
        }
        let base = harness.default_command();
        match model.map(str::trim).filter(|m| !m.is_empty()) {
            Some(model) => format!("{base} --model {}", shell_quote(model)),
            None => base.to_string(),
        }
    }
}

/// Wraps a value in single quotes so a shell takes it verbatim.
///
/// The one thing single quotes cannot hold is a single quote, which is why the
/// closing-reopening dance around `'\''` exists.
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// What a caller supplies to create an agent. Validated by the store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAgent {
    pub project_id: i64,
    pub fields: AgentFields,
}

/// Everything an update sets, all of it.
///
/// A **full replace**, not a patch — the same choice `routines::update` made,
/// and for the same reason: with a patch, "leave this alone" and "set this to
/// nothing" are the same absent field over IPC, and the two mean opposite
/// things for `model`. The editing form holds every field anyway.
///
/// `project_id` is not in here: an agent does not move between projects. From
/// CP5 it owns a worktree under its project, so moving it would mean moving a
/// directory, which is a different operation with a different name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentFields {
    pub name: String,
    pub harness: Harness,
    /// Empty for the harness's default.
    pub command: String,
    /// `None` = the harness picks.
    pub model: Option<String>,
    pub env: String,
}
