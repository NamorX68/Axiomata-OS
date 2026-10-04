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

/// Which harness runs an agent — defined in the roster, which owns the engine catalog (CP-A1).
pub use axiomata_roster::Harness;

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
    /// The branch the worktree was cut from — what the agent's diff is
    /// measured against (M7.3, G1). `None` for a worktree from before M7.3 or
    /// one cut from a detached HEAD; then the project folder's current branch
    /// stands in.
    pub base_branch: Option<String>,
    /// The Opencode session this agent runs in (`docs/plans/opencode2.md`,
    /// OC2): created by the IDE on the shared Opencode service and continued
    /// on every start. `None` before the first start, for other harnesses,
    /// and after "New session".
    pub opencode_session: Option<String>,
    /// The engine this session runs on, an id of the owner's catalog (`axiomata-roster`, CP-A1). `None` for a row
    /// that predates the catalog and has not been assigned yet; the profile's own harness, command, model and env
    /// stay in force until CP-A6 moves starting over to the engine.
    pub engine_id: Option<String>,
    /// The role this session plays, a name under `~/.axiomata/agents/`. Existing agents are `allrounder`.
    pub agent_role: String,
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
    /// For Claude Code the model is appended as `--model <value>`, but **only
    /// when the agent has no command of its own**. Somebody who wrote their
    /// own command line is responsible for it; pushing an extra flag into it
    /// could easily contradict what they typed.
    ///
    /// Opencode 2's terminal UI takes no `--model`: an Opencode agent's model
    /// travels on the session the IDE creates for it on the shared service,
    /// and the start adds `--session <id>` instead (`docs/plans/opencode2.md`,
    /// OC2).
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
            Some(model) if harness != Harness::Opencode => {
                format!("{base} --model {}", shell_quote(model))
            }
            _ => base.to_string(),
        }
    }
}

/// Wraps a value in single quotes so a shell takes it verbatim.
///
/// The one thing single quotes cannot hold is a single quote, which is why the
/// closing-reopening dance around `'\''` exists.
pub(crate) fn shell_quote(value: &str) -> String {
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
