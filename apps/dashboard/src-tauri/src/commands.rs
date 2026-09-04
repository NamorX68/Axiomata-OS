//! Tauri command handlers exposed to the dashboard frontend.
//!
//! The managed state is the `AxiomataCore` itself: `config` is read directly
//! (no lock — it is never mutated at runtime), and only `core.db` sits behind a
//! `Mutex`. `run_skill` calls `execute_and_record_skill`, which runs the agent
//! with no lock held and takes the database lock only to write the result — so
//! a `MutexGuard` is never held across an `.await`.

use axiomata_core::AxiomataCore;
use axiomata_core::agents::{self, ChatMode, ChatReply};
use axiomata_core::bridge::{self, ActionRequest, ActionResponse, ManifestEntry};
use axiomata_core::dashboard::{self, LoadedState};
use axiomata_core::graph::{self, WorkspaceGraph};
use axiomata_core::importer;
use axiomata_core::memory::{self, MemoryStatus, SyncReport};
use axiomata_core::notes;
use axiomata_core::routines::{self, NewRoutine, Routine, RoutineRun};
use axiomata_core::skills::{self, RunRecord, RunSummary, Skill};
use axiomata_core::workspace::{self, SearchHit, WorkspaceFile};
use serde::Serialize;
use tauri::State;

/// The Tauri-managed core engine.
pub type CoreState = AxiomataCore;

/// Static facts the shell shows in its top bar. Read once at startup.
#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    /// `config.owner`; empty when the user hasn't set one.
    pub owner: String,
    /// Last path component of `config.workspace_root` (e.g. "Axiomata-Workspace").
    pub workspace_name: String,
    /// Absolute workspace root, for tooltips / settings.
    pub workspace_root: String,
    /// The dashboard crate version.
    pub version: String,
}

/// Returns the owner line and workspace facts for the top bar.
#[tauri::command]
pub fn get_app_info(state: State<'_, CoreState>) -> AppInfo {
    let root = &state.config.workspace_root;
    AppInfo {
        owner: state.config.owner.clone(),
        workspace_name: root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        workspace_root: root.to_string_lossy().into_owned(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Reads `~/.axiomata/dashboard.json` (raw text) or the defaults; a corrupt
/// file is moved to `.bak` and reported in `recovered_backup`.
#[tauri::command]
pub fn get_dashboard_state() -> Result<LoadedState, String> {
    dashboard::load_state().map_err(|err| err.to_string())
}

/// Validates and atomically writes the dashboard state handed in by the
/// frontend. The core only checks "object with numeric `version`".
#[tauri::command]
pub fn save_dashboard_state(json: String) -> Result<(), String> {
    dashboard::save_state(&json).map_err(|err| err.to_string())
}

/// The workspace as a graph for the particle view: files (with area, title,
/// size, mtime), wiki/relative links, skills, routines, the CLAUDE.md hub.
/// Walks the workspace and reads Markdown heads; takes the DB lock only for
/// the routine list.
#[tauri::command]
pub fn get_workspace_graph(state: State<'_, CoreState>) -> Result<WorkspaceGraph, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    graph::build(&state.config, &db).map_err(|err| err.to_string())
}

/// Renders the mounted-module manifest into `~/.axiomata/module-context.md`
/// for the agent. Returns `true` if the file changed.
#[tauri::command]
pub fn write_module_manifest(entries: Vec<ManifestEntry>) -> Result<bool, String> {
    bridge::write_manifest(&entries).map_err(|err| err.to_string())
}

/// Takes every pending agent → module action request out of the inbox queue
/// (each is returned exactly once). The frontend dispatches them and answers
/// with `complete_module_action`.
#[tauri::command]
pub fn poll_module_actions() -> Result<Vec<ActionRequest>, String> {
    bridge::drain_inbox().map_err(|err| err.to_string())
}

/// Writes the frontend's answer to a polled request into the outbox queue.
#[tauri::command]
pub fn complete_module_action(response: ActionResponse) -> Result<(), String> {
    bridge::complete(&response).map_err(|err| err.to_string())
}

/// One assistant turn. `mode` is `"chat"` (never asks, read-mostly) or
/// `"instruct"` (may edit workspace files). Pass the `session_id` from the
/// previous reply to continue that conversation. Runs the agent with no lock
/// held; the reply is not recorded in the run log.
#[tauri::command]
pub async fn assistant_send(
    state: State<'_, CoreState>,
    message: String,
    session_id: Option<String>,
    mode: String,
) -> Result<ChatReply, String> {
    let mode = match mode.as_str() {
        "chat" => ChatMode::Chat,
        "instruct" => ChatMode::Instruct,
        other => return Err(format!("unknown assistant mode {other:?}")),
    };
    let config = state.config.clone();
    agents::chat(&config, message, session_id, mode)
        .await
        .map_err(|err| err.to_string())
}

/// Reads the user's custom theme CSS (`~/.axiomata/theme.css`, or the
/// absolute `.css` path from the dashboard settings). `None` if absent. The
/// frontend validates it before injecting.
#[tauri::command]
pub fn load_custom_css(path: Option<String>) -> Result<Option<String>, String> {
    dashboard::load_custom_css(path.as_deref().map(std::path::Path::new))
        .map_err(|err| err.to_string())
}

/// Case-insensitive full-text search over the workspace's text files; every
/// word must occur on one line. At most `limit` files, most hits first.
#[tauri::command]
pub fn search_workspace(
    state: State<'_, CoreState>,
    query: String,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    workspace::search(&state.config, &query, limit.clamp(1, 200)).map_err(|err| err.to_string())
}

/// Reads a UTF-8 file by workspace-relative path (≤ 1 MiB, no `..`, no
/// symlinks, must resolve inside `config.workspace_root`).
#[tauri::command]
pub fn read_workspace_file(
    state: State<'_, CoreState>,
    rel: String,
) -> Result<WorkspaceFile, String> {
    workspace::read_file(&state.config, &rel).map_err(|err| err.to_string())
}

/// Atomically writes a workspace file under the same guard as
/// `read_workspace_file`. Creates the file, never directories.
#[tauri::command]
pub fn write_workspace_file(
    state: State<'_, CoreState>,
    rel: String,
    content: String,
) -> Result<(), String> {
    workspace::write_file(&state.config, &rel, &content).map_err(|err| err.to_string())
}

/// Creates a new Markdown note from `content` alone — no separate title:
/// if `content` starts with its own `#` heading that wins, otherwise the
/// agent proposes one as part of the same turn. The agent also picks which
/// workspace area it belongs in — reusing an existing one, proposing a new
/// one, or `Inbox` as a last resort (see `notes::placement_prompt`) — and a
/// file name; this command then does the actual write (sanitized, never
/// overwriting), the same "agent decides, code writes" split
/// `axiomata-cli import obsidian` uses. Returns the workspace-relative path
/// written.
#[tauri::command]
pub async fn create_note(state: State<'_, CoreState>, content: String) -> Result<String, String> {
    let config = state.config.clone();
    let existing = importer::existing_areas(&config.workspace_root);
    let prompt = notes::placement_prompt(&content, &existing);
    let reply = agents::chat(&config, prompt, None, ChatMode::Instruct)
        .await
        .map_err(|err| err.to_string())?;
    let placement = notes::parse_placement(&reply.reply_markdown).map_err(|err| err.to_string())?;
    notes::write_placed_note(
        &config.workspace_root,
        &content,
        &placement.area,
        &placement.file_name,
        placement.title.as_deref(),
    )
    .map_err(|err| err.to_string())
}

/// Lists every discovered skill (`~/.axiomata/skills/`).
#[tauri::command]
pub fn list_skills() -> Result<Vec<Skill>, String> {
    skills::list_skills().map_err(|err| err.to_string())
}

/// Returns the most recent skill runs as slim summaries, newest first. `limit`
/// is clamped to `skills::MAX_RUN_LIMIT` in the core.
#[tauri::command]
pub fn list_runs(state: State<'_, CoreState>, limit: usize) -> Result<Vec<RunSummary>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    skills::list_runs(&db, limit).map_err(|err| err.to_string())
}

/// Returns one full run (with captured output) by id, or `null` if unknown.
#[tauri::command]
pub fn get_run(state: State<'_, CoreState>, id: i64) -> Result<Option<RunRecord>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    skills::get_run(&db, id).map_err(|err| err.to_string())
}

/// Runs a skill by name and returns the persisted run record.
#[tauri::command]
pub async fn run_skill(state: State<'_, CoreState>, name: String) -> Result<RunRecord, String> {
    skills::execute_and_record_skill(&name, &state.config, &state.db)
        .await
        .map_err(|err| err.to_string())
}

/// Regenerates the workspace router `CLAUDE.md` blocks. Reads no database.
#[tauri::command]
pub fn sync_memory(state: State<'_, CoreState>) -> Result<SyncReport, String> {
    memory::sync(&state.config).map_err(|err| err.to_string())
}

/// Reports whether the memory router is stale — a plain walk-and-compare.
#[tauri::command]
pub fn get_memory_status(state: State<'_, CoreState>) -> Result<MemoryStatus, String> {
    memory::status(&state.config).map_err(|err| err.to_string())
}

/// Lists every routine, soonest next-fire first.
#[tauri::command]
pub fn list_routines(state: State<'_, CoreState>) -> Result<Vec<Routine>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    routines::store::list(&db).map_err(|err| err.to_string())
}

/// Creates a routine. `new.target` arrives as `{ "type": "skill" | "prompt",
/// "value": "..." }`. Returns the stored routine (with its computed next fire).
#[tauri::command]
pub fn add_routine(state: State<'_, CoreState>, new: NewRoutine) -> Result<Routine, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    routines::store::add(&db, new).map_err(|err| err.to_string())
}

/// Enables or disables a routine by id. Returns `false` if there is no such
/// routine. Re-enabling recomputes the next fire from now.
#[tauri::command]
pub fn set_routine_enabled(
    state: State<'_, CoreState>,
    id: i64,
    enabled: bool,
) -> Result<bool, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    routines::store::set_enabled(&db, id, enabled).map_err(|err| err.to_string())
}

/// Returns a routine's firing history, newest first. `limit` is clamped in the
/// core to `routines::store::MAX_ROUTINE_RUN_LIMIT`.
#[tauri::command]
pub fn routine_history(
    state: State<'_, CoreState>,
    id: i64,
    limit: usize,
) -> Result<Vec<RoutineRun>, String> {
    let db = state.db.lock().map_err(|err| err.to_string())?;
    routines::store::list_runs(&db, id, limit).map_err(|err| err.to_string())
}
