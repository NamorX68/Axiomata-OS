//! Tauri glue for Run/Tasks (`docs/plans/editor-projekt-werkzeuge.md`, #50): the `task*` commands over
//! `axiomata-tasks`, on the folder of a **project** root.
//!
//! The webview names the project as a root id (`project:<id>`), never a path
//! ([`crate::files::project_folder`] turns it into one). Nothing here runs a command: [`task_command_line`]
//! hands back the line a task would type into a shell, and the webview types it into a terminal pane it
//! mounts itself — the same route an agent pane's start command takes. What this file guards is **which**
//! line that is: a task of the project's own `tasks.json` is refused until the owner confirmed that file's
//! exact content ([`tasks_trust`]), and the confirmation is written here, from the hash of what is on disk
//! right now — the webview cannot hand in a hash of its own choosing.

use std::path::PathBuf;

use axiomata_core::paths::axiomata_home;
use axiomata_tasks::trust::{self, TrustStore};
use axiomata_tasks::{NewTask, PROJECT_FILE, ResolveError, TaskList};
use serde::Deserialize;
use tauri::State;

use crate::commands::CoreState;
use crate::files::{FileError, blocking, project_folder};

fn personal_file() -> PathBuf {
    axiomata_home().join("tasks.json")
}

fn trust_file() -> PathBuf {
    axiomata_home().join("task-trust.json")
}

/// Runs `work` on the project's folder on a blocking thread.
async fn on_project<T, F>(state: &State<'_, CoreState>, root: String, work: F) -> Result<T, FileError>
where
    T: Send + 'static,
    F: FnOnce(PathBuf) -> Result<T, FileError> + Send + 'static,
{
    let folder = blocking(state, move |config, db| project_folder(config, db, &root)).await?;
    tauri::async_runtime::spawn_blocking(move || work(folder))
        .await
        .map_err(|err| FileError::new("Io", format!("task failed: {err}")))?
}

/// Every task the project offers, the project's own file marked confirmed or not.
#[tauri::command]
pub async fn tasks_list(state: State<'_, CoreState>, root: String) -> Result<TaskList, FileError> {
    on_project(&state, root, |dir| {
        Ok(axiomata_tasks::list(&dir, &personal_file(), &TrustStore::load(&trust_file())))
    })
    .await
}

/// The owner confirmed the project's `tasks.json` as shown. `hash` is what the panel showed; it is
/// accepted only if the file still has exactly that content.
#[tauri::command]
pub async fn tasks_trust(
    state: State<'_, CoreState>,
    root: String,
    hash: String,
) -> Result<(), FileError> {
    on_project(&state, root, move |dir| {
        let bytes = std::fs::read(dir.join(PROJECT_FILE))
            .map_err(|err| FileError::new("Io", format!("{PROJECT_FILE}: {err}")))?;
        if trust::hash(&bytes) != hash {
            return Err(FileError::new(
                "Conflict",
                format!("{PROJECT_FILE} changed since it was shown — look at it again."),
            ));
        }
        let mut store = TrustStore::load(&trust_file());
        store
            .trust(&dir, &hash)
            .map_err(|err| FileError::new("Io", format!("could not remember the confirmation: {err}")))
    })
    .await
}

/// The shell line of task `id`, or why it may not run.
#[tauri::command]
pub async fn task_command_line(
    state: State<'_, CoreState>,
    root: String,
    id: String,
) -> Result<String, FileError> {
    on_project(&state, root, move |dir| {
        axiomata_tasks::resolve(&dir, &personal_file(), &TrustStore::load(&trust_file()), &id)
            .map(|task| task.command_line())
            .map_err(|err| {
                FileError::new(
                    match err {
                        ResolveError::Unknown(_) => "NotFound",
                        ResolveError::NeedsTrust => "NeedsTrust",
                    },
                    err.to_string(),
                )
            })
    })
    .await
}

/// Where a task the owner writes in the panel is kept.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// `<project>/.axiomata/tasks.json` — travels with the repository, so others can use it too.
    Project,
    /// `~/.axiomata/tasks.json` — for every project of this owner.
    Personal,
}

fn write_error(err: impl std::fmt::Display) -> FileError {
    FileError::new("Io", format!("could not save the task: {err}"))
}

/// Changes the file of `scope` with `change` (given the current bytes, `None` if there is no file yet).
///
/// A project file the owner had **not** confirmed is rewritten but stays unconfirmed: it may hold somebody
/// else's tasks, and saving one of the owner's own must not quietly approve the rest. A file that was
/// confirmed (or did not exist) stays confirmed for the new content — the owner just wrote that change.
fn edit_tasks(
    dir: PathBuf,
    scope: Scope,
    change: impl FnOnce(Option<&[u8]>) -> Result<Vec<u8>, String>,
) -> Result<(), FileError> {
    let path = match scope {
        Scope::Project => dir.join(PROJECT_FILE),
        Scope::Personal => personal_file(),
    };
    let existing = match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => return Err(write_error(err)),
    };
    let new = change(existing.as_deref()).map_err(|msg| FileError::new("Invalid", msg))?;
    match scope {
        Scope::Personal => axiomata_tasks::write_personal_file(&path, &new).map_err(write_error),
        Scope::Project => {
            let mut store = TrustStore::load(&trust_file());
            let was_confirmed = existing.as_ref().is_none_or(|bytes| store.is_trusted(&dir, &trust::hash(bytes)));
            axiomata_tasks::write_project_file(&dir, &new).map_err(write_error)?;
            if was_confirmed {
                store.trust(&dir, &trust::hash(&new)).map_err(write_error)?;
            }
            Ok(())
        }
    }
}

/// Adds a task the owner typed into the panel, or replaces the one called `replace`.
#[tauri::command]
pub async fn tasks_save(
    state: State<'_, CoreState>,
    root: String,
    scope: Scope,
    task: NewTask,
    replace: Option<String>,
) -> Result<(), FileError> {
    on_project(&state, root, move |dir| {
        edit_tasks(dir, scope, |existing| axiomata_tasks::upsert_task(existing, &task, replace.as_deref()))
    })
    .await
}

/// Removes the task called `label` from the file of `scope`.
#[tauri::command]
pub async fn tasks_remove(
    state: State<'_, CoreState>,
    root: String,
    scope: Scope,
    label: String,
) -> Result<(), FileError> {
    on_project(&state, root, move |dir| {
        edit_tasks(dir, scope, |existing| match existing {
            Some(bytes) => axiomata_tasks::remove_task(bytes, &label),
            None => Err("There is no tasks file.".to_string()),
        })
    })
    .await
}
