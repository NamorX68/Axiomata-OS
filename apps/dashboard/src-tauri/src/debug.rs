//! Tauri glue for the debugger (`docs/plans/editor-projekt-werkzeuge.md`, #51): the `debug_*` commands over
//! `axiomata-dap`, on the folder of a **project** root.
//!
//! One session at a time. The webview names the project as a root id and a file as a path relative to it —
//! never an absolute path; this file builds the absolute path the adapter needs, from the project folder
//! ([`crate::files::project_folder`]) and a relative path that is checked to stay inside it. A configuration
//! comes from three places — detected, "debug the file in front", or the project's own
//! `.axiomata/debug.json`, which (like `tasks.json`) runs only after the owner confirmed its exact bytes.
//! What the adapter and the program report goes to the webview over one `Channel`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axiomata_core::paths::axiomata_home;
use axiomata_dap::config::{self, DebugConfig, PROJECT_FILE};
use axiomata_dap::python::{adapter_command, current_file_config, launch_arguments};
use axiomata_dap::{Control, DapError, DebugEvent, PythonEnv, Session};
use axiomata_tasks::trust::{self, TrustStore};
use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Channel;

use crate::commands::CoreState;
use crate::files::{FileError, blocking, project_folder};

fn trust_file() -> PathBuf {
    axiomata_home().join("debug-trust.json")
}

/// The one running session.
struct Active {
    session: Session,
    project: PathBuf,
    /// The thread the program last stopped on — what stepping and continuing address.
    thread: Mutex<Option<i64>>,
}

/// The slot holding the running session — shared (`Arc`) so the thread that forwards events can empty it
/// when the session ends by itself.
type Slot = Arc<Mutex<Option<Arc<Active>>>>;

#[derive(Default)]
pub struct DebugState {
    active: Slot,
}

fn is_current(slot: &Slot, active: &Arc<Active>) -> bool {
    slot.lock().unwrap_or_else(|p| p.into_inner()).as_ref().is_some_and(|a| Arc::ptr_eq(a, active))
}

fn clear_if(slot: &Slot, active: &Arc<Active>) {
    let mut held = slot.lock().unwrap_or_else(|p| p.into_inner());
    if held.as_ref().is_some_and(|a| Arc::ptr_eq(a, active)) {
        *held = None;
    }
}

impl DebugState {
    fn current(&self) -> Result<Arc<Active>, FileError> {
        self.active
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
            .ok_or_else(|| FileError::new("NotRunning", "There is no debug session running.".into()))
    }
}

fn dap_error(err: DapError) -> FileError {
    FileError::new("Debug", err.to_string())
}

/// Runs `work` on a blocking thread — every DAP call waits for the adapter.
async fn off_main<T, F>(work: F) -> Result<T, FileError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, FileError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| FileError::new("Io", format!("debug task failed: {err}")))?
}

async fn folder_of(state: &State<'_, CoreState>, root: String) -> Result<PathBuf, FileError> {
    blocking(state, move |config, db| project_folder(config, db, &root)).await
}

/// `rel` as an absolute path under `project`, or an error: no `..`, nothing absolute.
fn inside(project: &Path, rel: &str) -> Result<PathBuf, FileError> {
    let bad = rel.is_empty()
        || rel.starts_with('/')
        || rel.starts_with('~')
        || rel.contains('\0')
        || rel.split(['/', '\\']).any(|part| part == "..");
    if bad {
        return Err(FileError::new("Invalid", format!("“{rel}” is not a path inside the project.")));
    }
    Ok(project.join(rel))
}

#[derive(Debug, Serialize)]
pub struct DebugList {
    pub configs: Vec<DebugConfig>,
    /// The project's `debug.json`, when it has one (its configurations are in `configs` either way).
    pub project_file: Option<ProjectFile>,
    pub problems: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProjectFile {
    pub hash: String,
    pub trusted: bool,
}

/// The configurations of `project`: detected ones first, then those the project's `debug.json` writes.
fn list_configs(project: &Path) -> DebugList {
    let mut configs = config::detect(project);
    let mut problems = Vec::new();
    let mut project_file = None;
    match std::fs::read(project.join(PROJECT_FILE)) {
        Ok(bytes) => {
            let parsed = config::parse_debug_file(&bytes);
            configs.extend(parsed.configurations);
            problems.extend(parsed.problems.into_iter().map(|p| format!("{PROJECT_FILE}: {p}")));
            let hash = trust::hash(&bytes);
            let trusted = TrustStore::load(&trust_file()).is_trusted(project, &hash);
            project_file = Some(ProjectFile { hash, trusted });
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => problems.push(format!("{PROJECT_FILE}: {err}")),
    }
    DebugList { configs, project_file, problems }
}

#[tauri::command]
pub async fn debug_configs(state: State<'_, CoreState>, root: String) -> Result<DebugList, FileError> {
    let project = folder_of(&state, root).await?;
    off_main(move || Ok(list_configs(&project))).await
}

/// The owner confirmed the project's `debug.json` as shown; accepted only if the file still has that content.
#[tauri::command]
pub async fn debug_trust(state: State<'_, CoreState>, root: String, hash: String) -> Result<(), FileError> {
    let project = folder_of(&state, root).await?;
    off_main(move || {
        let bytes = std::fs::read(project.join(PROJECT_FILE))
            .map_err(|err| FileError::new("Io", format!("{PROJECT_FILE}: {err}")))?;
        if trust::hash(&bytes) != hash {
            return Err(FileError::new(
                "Conflict",
                format!("{PROJECT_FILE} changed since it was shown — look at it again."),
            ));
        }
        TrustStore::load(&trust_file())
            .trust(&project, &hash)
            .map_err(|err| FileError::new("Io", format!("could not remember the confirmation: {err}")))
    })
    .await
}

/// Changes the project's `debug.json` with `change` (given the current bytes, `None` if there is no file yet).
///
/// As for `tasks.json`: a file the owner had **not** confirmed is rewritten but stays unconfirmed — it may hold
/// somebody else's configurations, and saving one of the owner's own must not quietly approve the rest. A file
/// that was confirmed (or did not exist) stays confirmed for the new content: the owner just wrote that change.
fn edit_file(
    project: PathBuf,
    change: impl FnOnce(Option<&[u8]>) -> Result<Vec<u8>, String>,
) -> Result<(), FileError> {
    let write_error = |err: &dyn std::fmt::Display| FileError::new("Io", format!("could not save the configuration: {err}"));
    let existing = match std::fs::read(project.join(PROJECT_FILE)) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(write_error(&err)),
    };
    let new = change(existing.as_deref()).map_err(|msg| FileError::new("Invalid", msg))?;
    let mut store = TrustStore::load(&trust_file());
    let was_confirmed = existing.as_ref().is_none_or(|bytes| store.is_trusted(&project, &trust::hash(bytes)));
    config::write_project_file(&project, &new).map_err(|e| write_error(&e))?;
    if was_confirmed {
        store.trust(&project, &trust::hash(&new)).map_err(|e| write_error(&e))?;
    }
    Ok(())
}

/// Adds a configuration the owner typed into the panel, or replaces the one called `replace`.
#[tauri::command]
pub async fn debug_save(
    state: State<'_, CoreState>,
    root: String,
    config: config::NewConfig,
    replace: Option<String>,
) -> Result<(), FileError> {
    let project = folder_of(&state, root).await?;
    off_main(move || edit_file(project, |existing| config::upsert_config(existing, &config, replace.as_deref()))).await
}

/// Removes the configuration called `name` from the project's `debug.json`.
#[tauri::command]
pub async fn debug_remove(state: State<'_, CoreState>, root: String, name: String) -> Result<(), FileError> {
    let project = folder_of(&state, root).await?;
    off_main(move || {
        edit_file(project, |existing| match existing {
            Some(bytes) => config::remove_config(bytes, &name),
            None => Err("There is no debug.json.".to_string()),
        })
    })
    .await
}

/// A file's breakpoints as the editor keeps them.
#[derive(Debug, Deserialize)]
pub struct FileBreakpoints {
    pub rel: String,
    /// One-based lines.
    pub lines: Vec<u32>,
}

/// Which configuration to run: one by name, or “the file in front”.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    Named { name: String },
    CurrentFile { rel: String },
}

/// Starts a session. `on_event` receives everything the program and the adapter report until the end.
#[tauri::command]
pub async fn debug_start(
    state: State<'_, CoreState>,
    debug: State<'_, DebugState>,
    root: String,
    target: Target,
    breakpoints: Vec<FileBreakpoints>,
    on_event: Channel<DebugEvent>,
) -> Result<(), FileError> {
    let project = folder_of(&state, root).await?;
    // Only one session: the old one ends first.
    if let Ok(previous) = debug.current() {
        previous.session.end();
        clear_if(&debug.active, &previous);
    }
    let started = off_main({
        let project = project.clone();
        move || -> Result<Arc<Active>, FileError> {
            let config = match target {
                Target::CurrentFile { rel } => {
                    inside(&project, &rel)?;
                    current_file_config(&rel)
                }
                Target::Named { name } => {
                    let list = list_configs(&project);
                    let config = list
                        .configs
                        .into_iter()
                        .find(|c| c.name == name)
                        .ok_or_else(|| FileError::new("NotFound", format!("There is no configuration “{name}”.")))?;
                    if !config.detected && !list.project_file.is_some_and(|f| f.trusted) {
                        return Err(FileError::new(
                            "NeedsTrust",
                            "This configuration comes from the project's debug.json, which has not been confirmed (or has changed since).".into(),
                        ));
                    }
                    config
                }
            };
            let mut paths = Vec::new();
            for file in breakpoints {
                paths.push((inside(&project, &file.rel)?.to_string_lossy().into_owned(), file.lines));
            }
            let env = PythonEnv::detect(&project);
            let adapter = adapter_command(&project, &env).map_err(|why| FileError::new("NoAdapter", why))?;
            let launch = launch_arguments(&config, &project, &env);
            let session = Session::start(&adapter, "python", launch, &paths).map_err(dap_error)?;
            Ok(Arc::new(Active { session, project, thread: Mutex::new(None) }))
        }
    })
    .await?;

    *debug.active.lock().unwrap_or_else(|p| p.into_inner()) = Some(Arc::clone(&started));
    // Pump what happens to the webview until the session is over.
    let active = Arc::clone(&started);
    let slot = Arc::clone(&debug.active);
    std::thread::spawn(move || {
        loop {
            let Some(event) = active.session.next_event(Duration::from_millis(250)) else {
                if !is_current(&slot, &active) {
                    break;
                }
                continue;
            };
            match &event {
                DebugEvent::Stopped(stop) => {
                    *active.thread.lock().unwrap_or_else(|p| p.into_inner()) = Some(stop.thread_id);
                }
                DebugEvent::Continued { .. } => {
                    *active.thread.lock().unwrap_or_else(|p| p.into_inner()) = None;
                }
                _ => {}
            }
            let last = matches!(event, DebugEvent::Terminated | DebugEvent::Closed { .. });
            let _ = on_event.send(event);
            if last {
                clear_if(&slot, &active);
                active.session.end();
                break;
            }
        }
    });
    Ok(())
}

/// Ends the session, if there is one.
#[tauri::command]
pub async fn debug_stop(debug: State<'_, DebugState>) -> Result<(), FileError> {
    if let Ok(active) = debug.current() {
        clear_if(&debug.active, &active);
        off_main(move || {
            active.session.end();
            Ok(())
        })
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Continue,
    Next,
    StepIn,
    StepOut,
    Pause,
}

#[tauri::command]
pub async fn debug_control(debug: State<'_, DebugState>, action: Action) -> Result<(), FileError> {
    let active = debug.current()?;
    off_main(move || {
        let thread = (*active.thread.lock().unwrap_or_else(|p| p.into_inner())).unwrap_or(1);
        let control = match action {
            Action::Continue => Control::Continue,
            Action::Next => Control::Next,
            Action::StepIn => Control::StepIn,
            Action::StepOut => Control::StepOut,
            Action::Pause => Control::Pause,
        };
        active.session.control(control, thread).map_err(dap_error)
    })
    .await
}

/// The stack of the thread the program stopped on.
#[tauri::command]
pub async fn debug_stack(debug: State<'_, DebugState>) -> Result<Vec<axiomata_dap::Frame>, FileError> {
    let active = debug.current()?;
    off_main(move || {
        let thread = (*active.thread.lock().unwrap_or_else(|p| p.into_inner()))
            .ok_or_else(|| FileError::new("NotStopped", "The program is not stopped.".into()))?;
        active.session.stack_trace(thread).map_err(dap_error)
    })
    .await
}

#[tauri::command]
pub async fn debug_scopes(debug: State<'_, DebugState>, frame_id: i64) -> Result<Vec<axiomata_dap::Scope>, FileError> {
    let active = debug.current()?;
    off_main(move || active.session.scopes(frame_id).map_err(dap_error)).await
}

#[tauri::command]
pub async fn debug_variables(
    debug: State<'_, DebugState>,
    variables_reference: i64,
) -> Result<Vec<axiomata_dap::Variable>, FileError> {
    let active = debug.current()?;
    off_main(move || active.session.variables(variables_reference).map_err(dap_error)).await
}

#[tauri::command]
pub async fn debug_evaluate(
    debug: State<'_, DebugState>,
    expression: String,
    frame_id: Option<i64>,
) -> Result<axiomata_dap::Variable, FileError> {
    let active = debug.current()?;
    off_main(move || active.session.evaluate(&expression, frame_id).map_err(dap_error)).await
}

/// Replaces the breakpoints of one file in the running session.
#[tauri::command]
pub async fn debug_set_breakpoints(
    debug: State<'_, DebugState>,
    rel: String,
    lines: Vec<u32>,
) -> Result<Vec<axiomata_dap::Breakpoint>, FileError> {
    let active = debug.current()?;
    off_main(move || {
        let path = inside(&active.project, &rel)?;
        active.session.set_breakpoints(&path.to_string_lossy(), &lines).map_err(dap_error)
    })
    .await
}
