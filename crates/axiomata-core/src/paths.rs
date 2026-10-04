//! Resolution of Axiomata-OS's own runtime data directory.
//!
//! Axiomata-OS keeps its own data (config, database, logs, global skills) under
//! `~/.axiomata`, deliberately separate from the user's chosen Second-Brain
//! workspace. See `crate::config` for the workspace root setting.

use std::env;
use std::path::PathBuf;

/// Name of the environment variable that overrides the default `~/.axiomata`
/// location. Used by tests, and by anyone who wants to run an isolated
/// instance side by side with their normal one.
pub const AXIOMATA_HOME_ENV: &str = "AXIOMATA_HOME";

/// Returns Axiomata-OS's own runtime data directory.
///
/// Resolves to `$AXIOMATA_HOME` if that environment variable is set, otherwise
/// to `~/.axiomata`. Deliberately a visible dotfolder in the user's home
/// directory (mirroring `~/.claude`) rather than a hidden OS-convention path
/// such as `~/Library/Application Support/...`, since Axiomata-OS is meant to
/// be inspected by hand.
///
/// # Panics
///
/// Panics if `$AXIOMATA_HOME` is unset and the OS cannot report a home
/// directory for the current user — an environment Axiomata-OS cannot
/// meaningfully run in.
pub fn axiomata_home() -> PathBuf {
    if let Some(override_path) = env::var_os(AXIOMATA_HOME_ENV) {
        return PathBuf::from(override_path);
    }
    home::home_dir()
        .expect("could not determine the current user's home directory")
        .join(".axiomata")
}

/// Path to the app-level config file (`~/.axiomata/config.toml`).
pub fn config_path() -> PathBuf {
    axiomata_home().join("config.toml")
}

/// Path to the SQLite database (`~/.axiomata/axiomata.db`).
pub fn db_path() -> PathBuf {
    axiomata_home().join("axiomata.db")
}

/// Directory for JSONL run/routine logs (`~/.axiomata/logs/`).
pub fn logs_dir() -> PathBuf {
    axiomata_home().join("logs")
}

/// Path to the JSONL skill-run log (`~/.axiomata/logs/runs.log`), a
/// human-tailable mirror of the `runs` database table.
pub fn runs_log_path() -> PathBuf {
    logs_dir().join("runs.log")
}

/// Path to the memory-router sync marker (`~/.axiomata/memory-last-sync.json`),
/// a small `{ "<canonical workspace path>": "<rfc3339>" }` map recording when
/// each workspace was last synced. Kept in app-data, not in the workspace, so
/// the router never adds a file to the user's vault for its own bookkeeping.
pub fn memory_last_sync_path() -> PathBuf {
    axiomata_home().join("memory-last-sync.json")
}

/// Path to the file app's grant list (`~/.axiomata/file-grants.json`): the
/// files and folders the owner picked in the open dialog, which the file
/// service may then touch. See `crate::files` and `axiomata_files::grants`.
pub fn file_grants_path() -> PathBuf {
    axiomata_home().join("file-grants.json")
}

/// Path to the editor's preferences (`~/.axiomata/editor-settings.json`):
/// font, line numbers, wrapping, indentation, autosave. See
/// `crate::editor_settings`.
pub fn editor_settings_path() -> PathBuf {
    axiomata_home().join("editor-settings.json")
}

/// Path to what Vi remembers across restarts (`~/.axiomata/editor-vi.json`):
/// named registers, file marks, histories. See `crate::editor_vi`.
pub fn editor_vi_path() -> PathBuf {
    axiomata_home().join("editor-vi.json")
}

/// Directory of the editor's unsaved-work entries (`~/.axiomata/editor-recovery/`),
/// one JSON file per open file with changes not yet saved. See
/// `crate::editor_recovery`.
pub fn editor_recovery_dir() -> PathBuf {
    axiomata_home().join("editor-recovery")
}

/// Path to the dashboard layout file (`~/.axiomata/dashboard.json`): module
/// instances, their positions/sizes/config and UI settings such as the theme.
/// Hand-editable; see `crate::dashboard`.
pub fn dashboard_state_path() -> PathBuf {
    axiomata_home().join("dashboard.json")
}

/// Path to the module manifest the dashboard writes for the agent
/// (`~/.axiomata/module-context.md`): the mounted modules and their callable
/// actions, appended to the assistant's system prompt when present.
pub fn module_context_path() -> PathBuf {
    axiomata_home().join("module-context.md")
}

/// Path to the user's optional custom theme (`~/.axiomata/theme.css`): only
/// `--ax-*` token overrides, validated by the dashboard before injection.
pub fn custom_theme_path() -> PathBuf {
    axiomata_home().join("theme.css")
}

/// Path to the Terminal module's global preferences file
/// (`~/.axiomata/terminal-settings.json`) — Checkpoint 5d of
/// `docs/plans/terminal.md`. Deliberately its own file, not a section of
/// `dashboard.json`'s per-instance `canvas.instances[].config`: every
/// Terminal tile shares one preference set, and a future standalone
/// Terminal (outside this dashboard) would have something dashboard-shaped
/// state isn't. See `crate::terminal_settings`.
pub fn terminal_settings_path() -> PathBuf {
    axiomata_home().join("terminal-settings.json")
}

/// Root of the agent → module action queue (`~/.axiomata/module-actions/`),
/// with `inbox/` (requests written by the CLI) and `outbox/` (responses
/// written by the running dashboard). See `crate::bridge`.
pub fn module_actions_dir() -> PathBuf {
    axiomata_home().join("module-actions")
}

/// Directory for global, app-managed skills (`~/.axiomata/skills/`).
pub fn global_skills_dir() -> PathBuf {
    axiomata_home().join("skills")
}

/// Where the owner's agent roles live (`~/.axiomata/agents/<name>/AGENT.md`,
/// `docs/plans/a2a.md` CP-A1) — one file per role, like skills.
pub fn agent_roles_dir() -> PathBuf {
    axiomata_home().join("agents")
}

/// The owner's confirmations of project role overrides
/// (`<project>/.axiomata/agents/`), by content hash. Written from Rust only.
pub fn agent_roles_trust_path() -> PathBuf {
    axiomata_home().join("agent-roles-trust.json")
}

/// Where the agentic IDE puts one git worktree per agent
/// (`~/.axiomata/worktrees/<project>/<agent>-<id>`, M7.2 CP5).
///
/// App-owned runtime data, like everything else under `~/.axiomata` — and
/// deliberately *not* inside the user's repository, so an agent's checkout
/// never turns up as an untracked directory in their own `git status`.
pub fn worktrees_dir() -> PathBuf {
    axiomata_home().join("worktrees")
}

/// Where each IDE agent's status channel lives
/// (`~/.axiomata/agent-events/<id>/`, M7.2 CP6): its state word, its plan and
/// the hook files its harness is started with. App-owned, and never inside a
/// worktree, so no agent can commit it.
pub fn agent_events_dir() -> PathBuf {
    axiomata_home().join("agent-events")
}

/// Claude Code's own home: `$CLAUDE_CONFIG_DIR`, else `~/.claude`.
///
/// Not Axiomata's directory. The IDE reads two folders in it (below) and
/// removes exactly one thing — a deleted agent's own task list
/// (`axiomata_ide::lifecycle`).
fn claude_home() -> PathBuf {
    env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            home::home_dir()
                .expect("could not determine the current user's home directory")
                .join(".claude")
        })
}

/// Claude Code's task lists, from which a Claude Code agent's plan is read.
pub fn claude_tasks_dir() -> PathBuf {
    claude_home().join("tasks")
}

/// Where Claude Code writes plan-mode plans; read only.
pub fn claude_plans_dir() -> PathBuf {
    claude_home().join("plans")
}

/// Every path the IDE crate needs, bundled the way it takes them.
pub fn ide_locations() -> axiomata_ide::provision::Locations {
    axiomata_ide::provision::Locations {
        worktrees: worktrees_dir(),
        channels: axiomata_ide::lifecycle::ChannelRoots {
            events: agent_events_dir(),
            claude_tasks: claude_tasks_dir(),
            claude_plans: claude_plans_dir(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axiomata_home_respects_override_and_derives_paths() {
        let _guard = crate::test_support::ENV_MUTEX.lock().unwrap();

        // SAFETY: `env::set_var`/`remove_var` are `unsafe` as of edition 2024
        // because mutating process-wide env vars is not thread-safe in
        // general. `_guard` above serializes every test in this crate that
        // touches `AXIOMATA_HOME`, so there is no actual race here.
        unsafe {
            env::set_var(AXIOMATA_HOME_ENV, "/tmp/axiomata-test-home");
        }

        let expected_home = PathBuf::from("/tmp/axiomata-test-home");
        assert_eq!(axiomata_home(), expected_home);
        assert_eq!(config_path(), expected_home.join("config.toml"));
        assert_eq!(db_path(), expected_home.join("axiomata.db"));
        assert_eq!(logs_dir(), expected_home.join("logs"));
        assert_eq!(runs_log_path(), expected_home.join("logs").join("runs.log"));
        assert_eq!(global_skills_dir(), expected_home.join("skills"));
        assert_eq!(
            terminal_settings_path(),
            expected_home.join("terminal-settings.json")
        );

        unsafe {
            env::remove_var(AXIOMATA_HOME_ENV);
        }
    }
}
