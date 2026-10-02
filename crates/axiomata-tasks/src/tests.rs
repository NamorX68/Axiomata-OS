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

#[test]
fn a_python_project_offers_its_console_scripts_as_run() {
    let dir = project(&[
        (
            "pyproject.toml",
            "[project]\nname=\"x\"\n[project.scripts]\nocht = \"ocht.cli:main\"\n",
        ),
        ("uv.lock", ""),
    ]);
    let tasks = detect(dir.path());
    let run: Vec<_> = tasks
        .iter()
        .filter(|t| t.group == Group::Run)
        .map(|t| t.command.as_str())
        .collect();
    assert_eq!(run, ["uv run ocht"]);
}

#[test]
fn without_scripts_a_runnable_package_or_an_entry_file_is_the_start() {
    let pkg = project(&[
        ("pyproject.toml", "[project]\nname=\"x\"\n"),
        ("src/ocht/__main__.py", ""),
        ("uv.lock", ""),
    ]);
    let commands: Vec<_> = detect(pkg.path()).into_iter().map(|t| t.command).collect();
    assert!(
        commands.contains(&"uv run python -m ocht".to_string()),
        "{commands:?}"
    );

    let file = project(&[("main.py", "print(1)")]);
    assert_eq!(detect(file.path())[0].command, "python main.py");

    let django = project(&[("manage.py", "")]);
    assert_eq!(
        detect(django.path())[0].command,
        "python manage.py runserver"
    );
}

#[test]
fn makefile_targets_become_tasks_but_not_patterns_or_assignments() {
    let dir = project(&[(
        "Makefile",
        ".PHONY: all\nCC := gcc\nall: build\nbuild:\n\tcc x\n%.o: %.c\n\tcc -c $<\ntest:\n\techo\nbuild:\n",
    )]);
    let commands: Vec<_> = detect(dir.path()).into_iter().map(|t| t.command).collect();
    for expected in ["make all", "make build", "make test"] {
        assert!(
            commands.contains(&expected.to_string()),
            "{expected} missing in {commands:?}"
        );
    }
    assert_eq!(
        commands.len(),
        3,
        "no .PHONY, no CC, no pattern, no duplicate: {commands:?}"
    );
}

fn new_task(label: &str, command: &str) -> NewTask {
    NewTask {
        label: label.into(),
        command: command.into(),
        cwd: None,
        env: Default::default(),
        group: None,
    }
}

#[test]
fn a_task_from_the_form_lands_in_a_new_file_and_reads_back() {
    let mut task = new_task("Server", "uv run uvicorn app:app --reload");
    task.group = Some("run".into());
    task.cwd = Some("backend".into());
    let bytes = upsert_task(None, &task, None).unwrap();
    let parsed = parse_tasks_file(&bytes, Source::Project);
    assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
    assert_eq!(
        parsed.tasks[0].command_line(),
        "cd 'backend' && uv run uvicorn app:app --reload"
    );
    assert_eq!(parsed.tasks[0].group, Group::Run);
}

#[test]
fn adding_keeps_what_the_file_already_holds_and_refuses_a_second_label() {
    let existing = br#"{"note":"mine","tasks":[{"label":"A","command":"a"}]}"#;
    let bytes = upsert_task(Some(existing), &new_task("B", "b"), None).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["note"], "mine");
    assert_eq!(json["tasks"].as_array().unwrap().len(), 2);
    assert!(
        upsert_task(Some(&bytes), &new_task("A", "other"), None)
            .unwrap_err()
            .contains("already")
    );
}

#[test]
fn editing_replaces_in_place_and_may_rename() {
    let bytes = upsert_task(None, &new_task("A", "a"), None).unwrap();
    let bytes = upsert_task(Some(&bytes), &new_task("B", "b"), None).unwrap();
    let edited = upsert_task(Some(&bytes), &new_task("A2", "a2"), Some("A")).unwrap();
    let labels: Vec<_> = parse_tasks_file(&edited, Source::Project)
        .tasks
        .into_iter()
        .map(|t| t.label)
        .collect();
    assert_eq!(labels, ["A2", "B"]);
}

#[test]
fn the_form_is_held_to_the_same_rules_as_a_hand_written_file() {
    let mut escape = new_task("x", "y");
    escape.cwd = Some("../up".into());
    assert!(upsert_task(None, &escape, None).is_err());
    assert!(upsert_task(None, &new_task("", "y"), None).is_err());
    assert!(upsert_task(None, &new_task("x", "  "), None).is_err());
}

#[test]
fn a_file_that_is_not_json_is_never_overwritten() {
    assert!(
        upsert_task(Some(b"{ my notes"), &new_task("x", "y"), None)
            .unwrap_err()
            .contains("left alone")
    );
    assert!(remove_task(b"nope", "x").is_err());
}

#[test]
fn removing_drops_one_task_by_label() {
    let bytes = upsert_task(None, &new_task("A", "a"), None).unwrap();
    let bytes = upsert_task(Some(&bytes), &new_task("B", "b"), None).unwrap();
    let labels: Vec<_> = parse_tasks_file(&remove_task(&bytes, "A").unwrap(), Source::Project)
        .tasks
        .into_iter()
        .map(|t| t.label)
        .collect();
    assert_eq!(labels, ["B"]);
}

#[test]
fn the_project_file_is_written_into_a_new_dot_axiomata_and_never_through_a_link() {
    let dir = project(&[]);
    write_project_file(dir.path(), b"{\"tasks\":[]}").unwrap();
    assert!(dir.path().join(PROJECT_FILE).is_file());
    #[cfg(unix)]
    {
        let evil = project(&[]);
        let outside = project(&[]);
        std::os::unix::fs::symlink(outside.path(), evil.path().join(".axiomata")).unwrap();
        assert!(write_project_file(evil.path(), b"{}").is_err());
        assert!(!outside.path().join("tasks.json").exists());
    }
}
