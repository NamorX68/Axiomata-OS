//! Tauri glue for the editor's git panel (`docs/plans/editor-projekt-werkzeuge.md`, #48): the
//! `git_*` commands over `axiomata-git`, on the folder of a **project** root.
//!
//! The webview names the repository as a root id (`project:<id>`), never as a path
//! ([`crate::files::project_folder`] turns it into one, and refuses every other kind of root); the
//! files in it as paths relative to that folder, checked again in `axiomata-git`. Every command is
//! `async` and runs git on a blocking thread. **[`git_push`] is the only command that publishes**: the
//! checked-out branch to its upstream (or `origin`), never forced, and only when the owner presses
//! the button; [`git_fetch`] only updates remote-tracking branches.

use std::path::PathBuf;

use axiomata_git::GitError;
use axiomata_git::diff::FileDiff;
use axiomata_git::repo::{
    self, Blob, Branch, OutgoingCommit, PushResult, RepoStatus, Side, Source,
};
use serde::Serialize;
use tauri::State;

use crate::commands::CoreState;
use crate::files::{FileError, blocking, project_folder};

/// The most one side of a diff is read whole (the same as the diff view's own cap).
const MAX_SIDE_BYTES: u64 = 2 * 1024 * 1024;

fn git_error(err: GitError) -> FileError {
    FileError::new(
        match err {
            GitError::Invalid { .. } => "Invalid",
            GitError::Git { .. } => "Git",
        },
        err.to_string(),
    )
}

/// Runs `work` on the folder of `root` on a blocking thread.
async fn on_repo<T, F>(state: &State<'_, CoreState>, root: String, work: F) -> Result<T, FileError>
where
    T: Send + 'static,
    F: FnOnce(PathBuf) -> Result<T, GitError> + Send + 'static,
{
    let folder = blocking(state, move |config, db| project_folder(config, db, &root)).await?;
    tauri::async_runtime::spawn_blocking(move || work(folder))
        .await
        .map_err(|err| FileError::new("Io", format!("git task failed: {err}")))?
        .map_err(git_error)
}

/// What the panel shows for a project: not a repository, or its status.
#[derive(Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GitState {
    NotARepo,
    Ready { status: RepoStatus },
}

#[tauri::command]
pub async fn git_status(state: State<'_, CoreState>, root: String) -> Result<GitState, FileError> {
    on_repo(&state, root, |dir| {
        if !repo::is_repo(&dir) {
            return Ok(GitState::NotARepo);
        }
        repo::status(&dir).map(|status| GitState::Ready { status })
    })
    .await
}

#[tauri::command]
pub async fn git_stage(
    state: State<'_, CoreState>,
    root: String,
    paths: Vec<String>,
) -> Result<(), FileError> {
    on_repo(&state, root, move |dir| repo::stage(&dir, &paths)).await
}

#[tauri::command]
pub async fn git_unstage(
    state: State<'_, CoreState>,
    root: String,
    paths: Vec<String>,
) -> Result<(), FileError> {
    on_repo(&state, root, move |dir| repo::unstage(&dir, &paths)).await
}

#[tauri::command]
pub async fn git_stage_all(state: State<'_, CoreState>, root: String) -> Result<(), FileError> {
    on_repo(&state, root, |dir| repo::stage_all(&dir)).await
}

#[tauri::command]
pub async fn git_unstage_all(state: State<'_, CoreState>, root: String) -> Result<(), FileError> {
    on_repo(&state, root, |dir| repo::unstage_all(&dir)).await
}

#[tauri::command]
pub async fn git_diff(
    state: State<'_, CoreState>,
    root: String,
    path: String,
    old_path: Option<String>,
    side: Side,
) -> Result<FileDiff, FileError> {
    on_repo(&state, root, move |dir| {
        repo::file_diff(&dir, &path, old_path.as_deref(), side)
    })
    .await
}

/// One file as `HEAD` or the index has it — a side of the diff view.
#[tauri::command]
pub async fn git_blob(
    state: State<'_, CoreState>,
    root: String,
    path: String,
    source: Source,
) -> Result<Blob, FileError> {
    on_repo(&state, root, move |dir| {
        repo::blob(&dir, &path, source, MAX_SIDE_BYTES)
    })
    .await
}

#[tauri::command]
pub async fn git_apply_hunk(
    state: State<'_, CoreState>,
    root: String,
    path: String,
    old_path: Option<String>,
    side: Side,
    index: usize,
    header: String,
) -> Result<(), FileError> {
    on_repo(&state, root, move |dir| {
        repo::apply_hunk(&dir, &path, old_path.as_deref(), side, index, &header)
    })
    .await
}

/// Makes the project's folder a git repository (refused when it already is one, or lies in one).
#[tauri::command]
pub async fn git_init(state: State<'_, CoreState>, root: String) -> Result<(), FileError> {
    on_repo(&state, root, |dir| repo::init(&dir)).await
}

/// Throws away the unstaged changes of `paths` and deletes untracked ones. **Destroys work git cannot
/// bring back**: the page asks the owner first.
#[tauri::command]
pub async fn git_discard(
    state: State<'_, CoreState>,
    root: String,
    paths: Vec<String>,
) -> Result<(), FileError> {
    on_repo(&state, root, move |dir| repo::discard(&dir, &paths)).await
}

/// Throws away one hunk of a file's unstaged diff. **Destroys work git cannot bring back.**
#[tauri::command]
pub async fn git_discard_hunk(
    state: State<'_, CoreState>,
    root: String,
    path: String,
    index: usize,
    header: String,
) -> Result<(), FileError> {
    on_repo(&state, root, move |dir| {
        repo::discard_hunk(&dir, &path, index, &header)
    })
    .await
}

#[tauri::command]
pub async fn git_branches(
    state: State<'_, CoreState>,
    root: String,
) -> Result<Vec<Branch>, FileError> {
    on_repo(&state, root, |dir| repo::branches(&dir)).await
}

/// The commits a push of the checked-out branch would publish, each with the files it changed.
#[tauri::command]
pub async fn git_outgoing(
    state: State<'_, CoreState>,
    root: String,
) -> Result<Vec<OutgoingCommit>, FileError> {
    on_repo(&state, root, |dir| repo::outgoing(&dir)).await
}

/// Switches to a local branch; git refuses when uncommitted changes are in the way.
#[tauri::command]
pub async fn git_switch(
    state: State<'_, CoreState>,
    root: String,
    name: String,
) -> Result<(), FileError> {
    on_repo(&state, root, move |dir| repo::switch_branch(&dir, &name)).await
}

#[tauri::command]
pub async fn git_create_branch(
    state: State<'_, CoreState>,
    root: String,
    name: String,
) -> Result<(), FileError> {
    on_repo(&state, root, move |dir| repo::create_branch(&dir, &name)).await
}

/// Commits what is staged; resolves to the new commit.
#[tauri::command]
pub async fn git_commit(
    state: State<'_, CoreState>,
    root: String,
    message: String,
) -> Result<String, FileError> {
    on_repo(&state, root, move |dir| repo::commit(&dir, &message)).await
}

/// Pushes the checked-out branch to its upstream, or publishes it to `origin`; never forced.
#[tauri::command]
pub async fn git_push(state: State<'_, CoreState>, root: String) -> Result<PushResult, FileError> {
    on_repo(&state, root, |dir| repo::push(&dir)).await
}

#[tauri::command]
pub async fn git_fetch(state: State<'_, CoreState>, root: String) -> Result<(), FileError> {
    on_repo(&state, root, |dir| repo::fetch(&dir)).await
}
