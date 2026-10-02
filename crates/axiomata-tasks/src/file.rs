//! `tasks.json`, the project's and the owner's: `{ "tasks": [ { "label", "command", "cwd"?, "env"?, "group"? } ] }`.
//!
//! Reading is strict about what could leave the project or be misread — an absolute or `..` folder, an odd
//! environment name, a NUL — and every refusal becomes a line in `problems` rather than a dropped entry.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::{Group, Source, Task};

const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_TASKS: usize = 100;
const MAX_LABEL: usize = 80;
const MAX_COMMAND: usize = 2000;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Parsed {
    pub tasks: Vec<Task>,
    pub problems: Vec<String>,
}

#[derive(Deserialize)]
struct FileShape {
    #[serde(default)]
    tasks: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    label: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    group: Option<String>,
}

pub fn parse_tasks_file(bytes: &[u8], source: Source) -> Parsed {
    let mut out = Parsed::default();
    if bytes.len() > MAX_FILE_BYTES {
        out.problems.push(format!(
            "larger than {} KiB — not read",
            MAX_FILE_BYTES / 1024
        ));
        return out;
    }
    let shape: FileShape = match serde_json::from_slice(bytes) {
        Ok(shape) => shape,
        Err(err) => {
            out.problems.push(format!(
                "not valid JSON of the form {{\"tasks\": […]}}: {err}"
            ));
            return out;
        }
    };
    for (i, entry) in shape.tasks.into_iter().enumerate() {
        if out.tasks.len() == MAX_TASKS {
            out.problems.push(format!(
                "more than {MAX_TASKS} tasks — the rest are ignored"
            ));
            break;
        }
        let at = format!("task {}", i + 1);
        match build(entry, source) {
            Ok(task) if out.tasks.iter().any(|t| t.id == task.id) => {
                out.problems.push(format!(
                    "{at}: the label “{}” is used twice — skipped",
                    task.label
                ));
            }
            Ok(task) => out.tasks.push(task),
            Err(why) => out.problems.push(format!("{at}: {why}")),
        }
    }
    out
}

fn build(entry: Entry, source: Source) -> Result<Task, String> {
    let label = entry
        .label
        .map(|l| l.trim().to_string())
        .unwrap_or_default();
    if label.is_empty() || label.chars().count() > MAX_LABEL || label.contains(char::is_control) {
        return Err("needs a label of 1–80 characters".into());
    }
    let command = entry
        .command
        .map(|c| c.trim().to_string())
        .unwrap_or_default();
    if command.is_empty() || command.chars().count() > MAX_COMMAND || command.contains('\0') {
        return Err(format!(
            "“{label}” needs a command of up to {MAX_COMMAND} characters"
        ));
    }
    let cwd = match entry
        .cwd
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty() && c != ".")
    {
        None => None,
        Some(dir) => {
            let bad = dir.starts_with('/')
                || dir.starts_with('~')
                || dir.contains('\0')
                || dir.split(['/', '\\']).any(|part| part == "..");
            if bad {
                return Err(format!(
                    "“{label}”: the folder must lie inside the project (no absolute path, no ..)"
                ));
            }
            Some(dir)
        }
    };
    for key in entry.env.keys() {
        let mut chars = key.chars();
        let ok = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !ok {
            return Err(format!(
                "“{label}”: “{key}” is not a name an environment variable can have"
            ));
        }
    }
    if entry.env.values().any(|v| v.contains('\0')) {
        return Err(format!("“{label}”: an environment value contains a NUL"));
    }
    let group = match entry
        .group
        .as_deref()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("run") => Group::Run,
        Some("build") => Group::Build,
        Some("test") => Group::Test,
        Some("lint") => Group::Lint,
        Some("other") => Group::Other,
        _ => Group::guess(&label),
    };
    let mut task = Task::new(source, &label, &label, &command, group);
    task.cwd = cwd;
    task.env = entry.env.into_iter().collect();
    Ok(task)
}
