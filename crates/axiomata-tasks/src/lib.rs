//! Tasks of a project: what can be run, and what may run without asking.
//!
//! Three sources, in the order they are listed:
//!
//! * **detected** — from the project's own files (`Cargo.toml` → `cargo build/test/…`, `package.json` →
//!   one task per script, `pyproject.toml`/`pytest.ini` → `pytest`). Commands the tool itself defines;
//!   they run without a question.
//! * **personal** — `~/.axiomata/tasks.json`, the owner's own file. Runs without a question.
//! * **project** — `<project>/.axiomata/tasks.json`. This is **somebody else's code** (a cloned repository
//!   brings it along), so it is read but runs only after the owner confirmed it: the confirmation is the
//!   SHA-256 of the file's exact bytes ([`trust`]), asked again after every change.
//!
//! A task is a command line typed into a shell in a terminal pane — pipes, `&&` and quoting stay the
//! shell's business. This crate builds that line ([`Task::command_line`]) and quotes what it must.

mod detect;
mod file;
pub mod trust;

use std::path::Path;

use serde::{Deserialize, Serialize};

pub use detect::detect;
pub use file::{Parsed, parse_tasks_file};

/// Where a task comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Detected,
    Personal,
    Project,
}

impl Source {
    fn prefix(self) -> &'static str {
        match self {
            Source::Detected => "detected",
            Source::Personal => "personal",
            Source::Project => "project",
        }
    }
}

/// What a task is for — only ordering and an icon; never changes what runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Run,
    Build,
    Test,
    Lint,
    Other,
}

impl Group {
    /// The group a script/task name suggests (`dev`, `start` → run; `build`; `test`; `lint`, `check`).
    pub fn guess(name: &str) -> Group {
        let n = name.to_ascii_lowercase();
        let has = |words: &[&str]| {
            words.iter().any(|w| {
                n == *w || n.starts_with(&format!("{w}:")) || n.starts_with(&format!("{w}-"))
            })
        };
        if has(&["dev", "start", "serve", "run", "watch"]) {
            Group::Run
        } else if has(&["build", "compile", "bundle"]) {
            Group::Build
        } else if has(&["test", "tests", "spec", "e2e"]) {
            Group::Test
        } else if has(&["lint", "check", "typecheck", "format", "fmt"]) {
            Group::Lint
        } else {
            Group::Other
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Task {
    /// Stable within a source: `detected:cargo-build`, `project:Build docs`.
    pub id: String,
    pub label: String,
    pub command: String,
    /// A folder inside the project the command runs in; `None` = the project folder.
    pub cwd: Option<String>,
    /// Extra environment, sorted by name.
    pub env: Vec<(String, String)>,
    pub source: Source,
    pub group: Group,
}

impl Task {
    pub(crate) fn new(source: Source, key: &str, label: &str, command: &str, group: Group) -> Task {
        Task {
            id: format!("{}:{key}", source.prefix()),
            label: label.to_string(),
            command: command.to_string(),
            cwd: None,
            env: Vec::new(),
            source,
            group,
        }
    }

    /// The line typed into the shell: `cd 'sub' && KEY='v' command`.
    ///
    /// The folder and the values are single-quoted; the command itself is the owner's (or the tool's)
    /// text and stays as written. `cd` comes first so the environment applies to the command alone.
    pub fn command_line(&self) -> String {
        let mut line = String::new();
        if let Some(dir) = &self.cwd {
            line.push_str("cd ");
            line.push_str(&shell_quote(dir));
            line.push_str(" && ");
        }
        for (key, value) in &self.env {
            line.push_str(key);
            line.push('=');
            line.push_str(&shell_quote(value));
            line.push(' ');
        }
        line.push_str(&self.command);
        line
    }
}

/// `value` as one single-quoted shell word (`it's` → `'it'\''s'`).
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// The project's `tasks.json`, as the owner is asked about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectFile {
    /// SHA-256 of the file's bytes, lower-case hex.
    pub hash: String,
    /// The owner confirmed exactly this content.
    pub trusted: bool,
}

/// What the Tasks panel shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskList {
    pub tasks: Vec<Task>,
    /// Present when the project has a `tasks.json`; its tasks are in `tasks` either way, but run only if `trusted`.
    pub project_file: Option<ProjectFile>,
    /// Files that could not be read or entries that were refused — shown, never silently dropped.
    pub problems: Vec<String>,
}

/// Where the project's file lives, relative to the project folder.
pub const PROJECT_FILE: &str = ".axiomata/tasks.json";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResolveError {
    #[error("There is no task “{0}”.")]
    Unknown(String),
    #[error(
        "This task comes from the project's tasks.json, which has not been confirmed (or has changed since)."
    )]
    NeedsTrust,
}

/// Everything a project offers, with the project file marked trusted or not.
pub fn list(project: &Path, personal_file: &Path, trust: &trust::TrustStore) -> TaskList {
    let mut problems = Vec::new();
    let mut tasks = detect(project);

    if let Ok(bytes) = std::fs::read(personal_file) {
        let parsed = parse_tasks_file(&bytes, Source::Personal);
        tasks.extend(parsed.tasks);
        problems.extend(
            parsed
                .problems
                .into_iter()
                .map(|p| format!("~/.axiomata/tasks.json: {p}")),
        );
    }

    let mut project_file = None;
    match std::fs::read(project.join(PROJECT_FILE)) {
        Ok(bytes) => {
            let parsed = parse_tasks_file(&bytes, Source::Project);
            tasks.extend(parsed.tasks);
            problems.extend(
                parsed
                    .problems
                    .into_iter()
                    .map(|p| format!("{PROJECT_FILE}: {p}")),
            );
            let hash = trust::hash(&bytes);
            project_file = Some(ProjectFile {
                trusted: trust.is_trusted(project, &hash),
                hash,
            });
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => problems.push(format!("{PROJECT_FILE}: {err}")),
    }

    tasks.sort_by(|a, b| {
        (a.source != Source::Detected, a.group, &a.label).cmp(&(
            b.source != Source::Detected,
            b.group,
            &b.label,
        ))
    });
    TaskList {
        tasks,
        project_file,
        problems,
    }
}

/// The task `id`, or why it may not run. A task of the project's file needs the owner's confirmation.
pub fn resolve(
    project: &Path,
    personal_file: &Path,
    trust: &trust::TrustStore,
    id: &str,
) -> Result<Task, ResolveError> {
    let all = list(project, personal_file, trust);
    let task = all
        .tasks
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| ResolveError::Unknown(id.to_string()))?;
    if task.source == Source::Project && !all.project_file.is_some_and(|f| f.trusted) {
        return Err(ResolveError::NeedsTrust);
    }
    Ok(task)
}

#[cfg(test)]
mod tests;
