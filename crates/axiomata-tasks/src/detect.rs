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
