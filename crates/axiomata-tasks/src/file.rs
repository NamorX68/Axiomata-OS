//! `tasks.json`, the project's and the owner's: `{ "tasks": [ { "label", "command", "cwd"?, "env"?, "group"? } ] }`.
//!
//! Reading is strict about what could leave the project or be misread — an absolute or `..` folder, an odd
//! environment name, a NUL — and every refusal becomes a line in `problems` rather than a dropped entry.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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

/// A task as the panel's form hands it in. Checked by the same rules as a hand-written file.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NewTask {
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub group: Option<String>,
}

fn entry_of(task: &NewTask) -> Entry {
    Entry {
        label: Some(task.label.clone()),
        command: Some(task.command.clone()),
        cwd: task.cwd.clone(),
        env: task.env.clone(),
        group: task.group.clone(),
    }
}

/// `existing` (the file's bytes, or `None` for a new file) with `task` added — or, when `replace` names a
/// label, put in the place of that task. Returns the new file content.
///
/// Works on the JSON itself, so whatever else a hand-edited file holds stays; refuses to touch a file that
/// is not valid (the owner's text is never overwritten with a guess) and a label already used by another task.
pub fn upsert_task(
    existing: Option<&[u8]>,
    task: &NewTask,
    replace: Option<&str>,
) -> Result<Vec<u8>, String> {
    build(entry_of(task), Source::Personal)?; // the same checks a hand-written entry gets
    let label = task.label.trim();
    let mut root = load_root(existing)?;
    let list = tasks_of(&mut root)?;
    let at = replace.and_then(|r| list.iter().position(|t| label_of(t) == Some(r)));
    if list
        .iter()
        .enumerate()
        .any(|(i, t)| label_of(t) == Some(label) && Some(i) != at)
    {
        return Err(format!("There is already a task called “{label}”."));
    }
    let mut value = serde_json::json!({ "label": label, "command": task.command.trim() });
    let map = value.as_object_mut().expect("an object");
    if let Some(cwd) = task
        .cwd
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty() && *c != ".")
    {
        map.insert("cwd".into(), cwd.into());
    }
    if !task.env.is_empty() {
        map.insert(
            "env".into(),
            serde_json::to_value(&task.env).map_err(|e| e.to_string())?,
        );
    }
    if let Some(group) = task.group.as_deref().filter(|g| !g.is_empty()) {
        map.insert("group".into(), group.into());
    }
    match at {
        Some(i) => list[i] = value,
        None => list.push(value),
    }
    render(&root)
}

/// `existing` without the task called `label` (a file without it is returned as it is).
pub fn remove_task(existing: &[u8], label: &str) -> Result<Vec<u8>, String> {
    let mut root = load_root(Some(existing))?;
    tasks_of(&mut root)?.retain(|t| label_of(t) != Some(label));
    render(&root)
}

fn label_of(task: &serde_json::Value) -> Option<&str> {
    task.get("label").and_then(|l| l.as_str()).map(str::trim)
}

fn load_root(existing: Option<&[u8]>) -> Result<serde_json::Value, String> {
    match existing {
        None => Ok(serde_json::json!({ "tasks": [] })),
        Some(bytes) if bytes.len() > MAX_FILE_BYTES => {
            Err("The file is too large to edit here.".into())
        }
        Some(bytes) => serde_json::from_slice(bytes)
            .map_err(|err| format!("The file is not valid JSON, so it is left alone: {err}")),
    }
}

fn tasks_of(root: &mut serde_json::Value) -> Result<&mut Vec<serde_json::Value>, String> {
    let object = root
        .as_object_mut()
        .ok_or("The file is not of the form {\"tasks\": […]}.")?;
    object
        .entry("tasks")
        .or_insert_with(|| serde_json::json!([]))
        .as_array_mut()
        .ok_or_else(|| "\"tasks\" in the file is not a list.".to_string())
}

fn render(root: &serde_json::Value) -> Result<Vec<u8>, String> {
    let mut out = serde_json::to_vec_pretty(root).map_err(|e| e.to_string())?;
    out.push(b'\n');
    Ok(out)
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
