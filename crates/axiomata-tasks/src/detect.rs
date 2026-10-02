//! Tasks a project's own files define. Nothing is run or executed here — files are only read.

use std::path::Path;

use crate::{Group, Source, Task};

/// The most `package.json` scripts offered; a monorepo's hundred scripts would bury the useful ones.
const MAX_SCRIPTS: usize = 40;

pub fn detect(project: &Path) -> Vec<Task> {
    let mut tasks = Vec::new();
    cargo(project, &mut tasks);
    node(project, &mut tasks);
    python(project, &mut tasks);
    make(project, &mut tasks);
    tasks
}

fn detected(key: &str, label: &str, command: &str, group: Group) -> Task {
    Task::new(Source::Detected, key, label, command, group)
}

fn cargo(project: &Path, tasks: &mut Vec<Task>) {
    let Ok(text) = std::fs::read_to_string(project.join("Cargo.toml")) else {
        return;
    };
    let manifest: Option<toml::Table> = text.parse().ok();
    let is_workspace = manifest
        .as_ref()
        .is_some_and(|m| m.contains_key("workspace"));
    let has_bin = project.join("src/main.rs").is_file()
        || manifest.as_ref().is_some_and(|m| m.contains_key("bin"));

    tasks.push(detected(
        "cargo-build",
        "cargo build",
        "cargo build",
        Group::Build,
    ));
    tasks.push(detected(
        "cargo-check",
        "cargo check",
        "cargo check",
        Group::Lint,
    ));
    tasks.push(detected(
        "cargo-clippy",
        "cargo clippy",
        "cargo clippy --workspace -- -D warnings",
        Group::Lint,
    ));
    let test = if is_workspace {
        "cargo test --workspace"
    } else {
        "cargo test"
    };
    tasks.push(detected("cargo-test", "cargo test", test, Group::Test));
    if has_bin && !is_workspace {
        tasks.push(detected("cargo-run", "cargo run", "cargo run", Group::Run));
    }
}

/// `npm`, `pnpm`, `yarn` or `bun`, by the lockfile the project keeps.
fn package_manager(project: &Path) -> &'static str {
    let has = |f: &str| project.join(f).is_file();
    if has("pnpm-lock.yaml") {
        "pnpm"
    } else if has("yarn.lock") {
        "yarn"
    } else if has("bun.lockb") || has("bun.lock") {
        "bun"
    } else {
        "npm"
    }
}

fn node(project: &Path, tasks: &mut Vec<Task>) {
    let Ok(text) = std::fs::read_to_string(project.join("package.json")) else {
        return;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(scripts) = json.get("scripts").and_then(|s| s.as_object()) else {
        return;
    };
    let pm = package_manager(project);
    let mut added = 0;
    for name in scripts.keys() {
        // npm runs `pre<x>` and `post<x>` around `<x>` on its own; offering them would run them twice.
        let lifecycle = ["pre", "post"].iter().any(|p| {
            name.strip_prefix(p)
                .is_some_and(|rest| scripts.contains_key(rest))
        });
        if lifecycle || name.is_empty() || name.contains(char::is_whitespace) {
            continue;
        }
        if added == MAX_SCRIPTS {
            break;
        }
        added += 1;
        tasks.push(detected(
            &format!("{pm}-{name}"),
            &format!("{pm} run {name}"),
            &format!("{pm} run {name}"),
            Group::guess(name),
        ));
    }
}

fn python(project: &Path, tasks: &mut Vec<Task>) {
    let pyproject = std::fs::read_to_string(project.join("pyproject.toml")).unwrap_or_default();
    let has = |f: &str| project.join(f).is_file();
    let runner = if has("uv.lock") {
        "uv run "
    } else if has("poetry.lock") {
        "poetry run "
    } else {
        ""
    };

    // How the project starts: the console scripts it declares, else a package that can be run (`__main__.py`),
    // else a conventional entry file at the top.
    let manifest: Option<toml::Table> = pyproject.parse().ok();
    let scripts = |path: &[&str]| -> Vec<String> {
        let mut node = manifest.as_ref().map(|m| toml::Value::Table(m.clone()));
        for key in path {
            node = node.and_then(|n| n.get(key).cloned());
        }
        node.and_then(|n| n.as_table().map(|t| t.keys().cloned().collect()))
            .unwrap_or_default()
    };
    let mut starts: Vec<String> = scripts(&["project", "scripts"]);
    starts.extend(scripts(&["tool", "poetry", "scripts"]));
    starts.retain(|name| !name.is_empty() && !name.contains(char::is_whitespace));
    starts.dedup();
    for name in &starts {
        tasks.push(detected(
            &format!("py-{name}"),
            &format!("{runner}{name}"),
            &format!("{runner}{name}"),
            Group::Run,
        ));
    }
    if starts.is_empty() {
        for package in main_packages(project) {
            let command = format!("{runner}python -m {package}");
            tasks.push(detected(
                &format!("py-m-{package}"),
                &command,
                &command,
                Group::Run,
            ));
        }
        for file in ["manage.py", "main.py", "app.py"] {
            if has(file) {
                let command = if file == "manage.py" {
                    format!("{runner}python manage.py runserver")
                } else {
                    format!("{runner}python {file}")
                };
                tasks.push(detected(
                    &format!("py-{file}"),
                    &command,
                    &command,
                    Group::Run,
                ));
                break;
            }
        }
    }
    let pytest = has("pytest.ini")
        || pyproject.contains("pytest")
        || (has("tox.ini") && project.join("tests").is_dir());
    let uv = has("uv.lock");
    let prefix = if uv { "uv run " } else { "" };
    if pytest {
        tasks.push(detected(
            "pytest",
            &format!("{prefix}pytest"),
            &format!("{prefix}pytest"),
            Group::Test,
        ));
    }
    if pyproject.contains("[tool.ruff") {
        tasks.push(detected(
            "ruff",
            &format!("{prefix}ruff check"),
            &format!("{prefix}ruff check ."),
            Group::Lint,
        ));
    }
}

/// Packages with a `__main__.py` — directly under the project or under `src/`.
fn main_packages(project: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for base in [project.to_path_buf(), project.join("src")] {
        let Ok(entries) = std::fs::read_dir(&base) else {
            continue;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().join("__main__.py").is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| {
                !n.starts_with('.') && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
            .collect();
        names.sort();
        found.extend(names);
    }
    found.truncate(3);
    found
}

/// Targets of a `Makefile`: `name:` at the start of a line. Pattern rules (`%`), special targets (`.PHONY`)
/// and variable assignments (`:=`) are not tasks.
fn make(project: &Path, tasks: &mut Vec<Task>) {
    let Some(text) = ["Makefile", "makefile", "GNUmakefile"]
        .iter()
        .find_map(|f| std::fs::read_to_string(project.join(f)).ok())
    else {
        return;
    };
    let mut seen: Vec<&str> = Vec::new();
    for line in text.lines() {
        let Some((target, rest)) = line.split_once(':') else {
            continue;
        };
        let valid = !target.is_empty()
            && !target.starts_with(['.', '\t', ' ', '#'])
            && target
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
            && !rest.starts_with('=')
            && !seen.contains(&target);
        if valid && seen.len() < 20 {
            seen.push(target);
            tasks.push(detected(
                &format!("make-{target}"),
                &format!("make {target}"),
                &format!("make {target}"),
                Group::guess(target),
            ));
        }
    }
}
