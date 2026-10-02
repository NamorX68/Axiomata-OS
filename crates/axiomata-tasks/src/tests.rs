use std::path::PathBuf;

use super::*;
use crate::trust::{TrustStore, hash};

fn project(files: &[(&str, &str)]) -> tempdir::Dir {
    let dir = tempdir::Dir::new();
    for (name, content) in files {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    dir
}

/// A minimal temp dir (no extra dependency): removed on drop.
mod tempdir {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU32, Ordering};

    static N: AtomicU32 = AtomicU32::new(0);

    pub struct Dir(PathBuf);

    impl Dir {
        pub fn new() -> Dir {
            let path = std::env::temp_dir().join(format!(
                "axiomata-tasks-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Dir(path)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn ids(tasks: &[Task]) -> Vec<&str> {
    tasks.iter().map(|t| t.id.as_str()).collect()
}

fn none() -> PathBuf {
    PathBuf::from("/nonexistent/axiomata/tasks.json")
}

#[test]
fn a_cargo_project_offers_build_check_clippy_test_and_run() {
    let dir = project(&[
        ("Cargo.toml", "[package]\nname=\"x\"\n"),
        ("src/main.rs", "fn main(){}"),
    ]);
    let tasks = detect(dir.path());
    assert_eq!(
        ids(&tasks),
        [
            "detected:cargo-build",
            "detected:cargo-check",
            "detected:cargo-clippy",
            "detected:cargo-test",
            "detected:cargo-run"
        ]
    );
}

#[test]
fn a_cargo_workspace_tests_all_members_and_has_no_run() {
    let dir = project(&[
        ("Cargo.toml", "[workspace]\nmembers=[\"a\"]\n"),
        ("src/main.rs", "fn main(){}"),
    ]);
    let tasks = detect(dir.path());
    assert!(!ids(&tasks).contains(&"detected:cargo-run"));
    assert_eq!(
        tasks
            .iter()
            .find(|t| t.id == "detected:cargo-test")
            .unwrap()
            .command,
        "cargo test --workspace"
    );
}

#[test]
fn package_json_scripts_use_the_package_manager_of_the_lockfile_and_skip_pre_post_hooks() {
    let dir = project(&[
        (
            "package.json",
            r#"{"scripts":{"dev":"vite","build":"vite build","prebuild":"x","test":"vitest"}}"#,
        ),
        ("pnpm-lock.yaml", ""),
    ]);
    let tasks = detect(dir.path());
    let commands: Vec<_> = tasks.iter().map(|t| t.command.as_str()).collect();
    assert_eq!(
        commands,
        ["pnpm run build", "pnpm run dev", "pnpm run test"]
    );
    assert_eq!(tasks[1].group, Group::Run);
    assert_eq!(tasks[0].group, Group::Build);
}

#[test]
fn python_projects_get_pytest_through_uv_when_there_is_a_uv_lock() {
    let dir = project(&[
        ("pyproject.toml", "[tool.pytest.ini_options]\n[tool.ruff]\n"),
        ("uv.lock", ""),
    ]);
    let commands: Vec<_> = detect(dir.path()).into_iter().map(|t| t.command).collect();
    assert_eq!(commands, ["uv run pytest", "uv run ruff check ."]);
    let plain = project(&[("pytest.ini", "")]);
    assert_eq!(detect(plain.path())[0].command, "pytest");
}

#[test]
fn an_empty_project_offers_nothing() {
    assert!(detect(project(&[]).path()).is_empty());
}

#[test]
fn tasks_json_is_read_with_folder_environment_and_group() {
    let json = br#"{"tasks":[{"label":"Docs","command":"mkdocs build","cwd":"site","env":{"B":"2","A":"1"},"group":"build"}]}"#;
    let parsed = parse_tasks_file(json, Source::Project);
    assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
    let task = &parsed.tasks[0];
    assert_eq!(task.id, "project:Docs");
    assert_eq!(task.group, Group::Build);
    assert_eq!(task.command_line(), "cd 'site' && A='1' B='2' mkdocs build");
}

#[test]
fn tasks_json_refuses_what_could_leave_the_project_or_be_misread_and_says_so() {
    let json = br#"{"tasks":[
        {"label":"a","command":"x","cwd":"/etc"},
        {"label":"b","command":"x","cwd":"../up"},
        {"label":"c","command":"x","cwd":"ok/../../up"},
        {"label":"d","command":"x","env":{"1BAD":"v"}},
        {"label":"","command":"x"},
        {"label":"f"},
        {"label":"g","command":"echo ok"},
        {"label":"g","command":"echo twice"}
    ]}"#;
    let parsed = parse_tasks_file(json, Source::Project);
    assert_eq!(ids(&parsed.tasks), ["project:g"]);
    assert_eq!(parsed.problems.len(), 7, "{:?}", parsed.problems);
}

#[test]
fn tasks_json_that_is_not_json_or_too_large_is_a_problem_not_a_panic() {
    assert_eq!(
        parse_tasks_file(b"{ nope", Source::Personal).problems.len(),
        1
    );
    let big = vec![b' '; 300 * 1024];
    assert!(parse_tasks_file(&big, Source::Personal).tasks.is_empty());
}

#[test]
fn quoting_survives_quotes_in_folders_and_values() {
    let mut task = Task::new(Source::Personal, "q", "q", "run", Group::Other);
    task.cwd = Some("it's here".into());
    task.env = vec![("K".into(), "a b'c".into())];
    assert_eq!(
        task.command_line(),
        r"cd 'it'\''s here' && K='a b'\''c' run"
    );
}

#[test]
fn a_project_tasks_json_runs_only_after_the_owner_confirmed_exactly_that_content() {
    let json = r#"{"tasks":[{"label":"Deploy","command":"./deploy.sh"}]}"#;
    let dir = project(&[(PROJECT_FILE, json), ("Cargo.toml", "[package]\n")]);
    let mut trust = TrustStore::in_memory();

    let listed = list(dir.path(), &none(), &trust);
    let file = listed.project_file.clone().unwrap();
    assert!(!file.trusted);
    assert_eq!(
        resolve(dir.path(), &none(), &trust, "project:Deploy"),
        Err(ResolveError::NeedsTrust)
    );
    // A detected task never needs it.
    assert!(resolve(dir.path(), &none(), &trust, "detected:cargo-build").is_ok());

    trust.trust(dir.path(), &file.hash).unwrap();
    assert_eq!(
        resolve(dir.path(), &none(), &trust, "project:Deploy")
            .unwrap()
            .command,
        "./deploy.sh"
    );

    // The file changes: the confirmation does not carry over.
    std::fs::write(
        dir.path().join(PROJECT_FILE),
        r#"{"tasks":[{"label":"Deploy","command":"./evil.sh"}]}"#,
    )
    .unwrap();
    assert_eq!(
        resolve(dir.path(), &none(), &trust, "project:Deploy"),
        Err(ResolveError::NeedsTrust)
    );
}

#[test]
fn unknown_ids_are_unknown() {
    let dir = project(&[]);
    assert_eq!(
        resolve(
            dir.path(),
            &none(),
            &TrustStore::in_memory(),
            "detected:nope"
        ),
        Err(ResolveError::Unknown("detected:nope".into()))
    );
}

#[test]
fn personal_tasks_come_from_the_owners_file_and_need_no_question() {
    let dir = project(&[]);
    let home = project(&[(
        "tasks.json",
        r#"{"tasks":[{"label":"Mine","command":"echo hi"}]}"#,
    )]);
    let personal = home.path().join("tasks.json");
    let task = resolve(
        dir.path(),
        &personal,
        &TrustStore::in_memory(),
        "personal:Mine",
    )
    .unwrap();
    assert_eq!(task.source, Source::Personal);
}

#[test]
fn detected_tasks_come_first_then_by_group() {
    let dir = project(&[
        ("Cargo.toml", "[package]\n"),
        (PROJECT_FILE, r#"{"tasks":[{"label":"A","command":"x"}]}"#),
    ]);
    let listed = list(dir.path(), &none(), &TrustStore::in_memory());
    assert_eq!(listed.tasks.last().unwrap().id, "project:A");
}

#[test]
fn the_hash_is_sha256_hex() {
    assert_eq!(
        hash(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn the_trust_file_round_trips_and_an_unreadable_one_trusts_nothing() {
    let dir = project(&[]);
    let file = dir.path().join("task-trust.json");
    let proj = project(&[]);
    let mut store = TrustStore::load(&file);
    store.trust(proj.path(), "abc").unwrap();
    assert!(TrustStore::load(&file).is_trusted(proj.path(), "abc"));
    assert!(!TrustStore::load(&file).is_trusted(proj.path(), "abd"));
    std::fs::write(&file, "garbage").unwrap();
    assert!(!TrustStore::load(&file).is_trusted(proj.path(), "abc"));
}
