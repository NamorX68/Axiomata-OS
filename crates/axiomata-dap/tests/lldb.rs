//! The Rust path against a real `lldb-dap` and a real `cargo build`. Skipped where either is missing.

use std::path::PathBuf;
use std::time::Duration;

use axiomata_dap::rust;
use axiomata_dap::{Control, DebugEvent, Session};

const MAIN: &str = "fn add(a: i32, b: i32) -> i32 {\n    let total = a + b;\n    total\n}\n\nfn main() {\n    let x = add(2, 3);\n    println!(\"result {x}\");\n}\n";

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("axiomata-dap-lldb-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/main.rs"), MAIN).unwrap();
    dir.canonicalize().unwrap()
}

fn next_stop(session: &Session) -> (String, i64) {
    for _ in 0..300 {
        match session.next_event(Duration::from_millis(100)) {
            Some(DebugEvent::Stopped(stop)) => return (stop.reason, stop.thread_id),
            Some(DebugEvent::Closed { stderr }) => panic!("the adapter closed: {stderr}"),
            Some(DebugEvent::Terminated | DebugEvent::Exited { .. }) => {
                panic!("the program ended before stopping")
            }
            _ => {}
        }
    }
    panic!("no stop within 30 s");
}

#[test]
fn a_rust_binary_is_built_stops_at_a_breakpoint_shows_its_locals_and_steps() {
    if rust::cargo().is_none() || rust::adapter_command(&PathBuf::from("/")).is_err() {
        eprintln!("skipped: cargo or lldb-dap is missing");
        return;
    }
    let project = project();
    let config = rust::detect(&project).remove(0);
    assert_eq!(config.name, "cargo: probe");
    let executable = rust::build(&project, &config).expect("the build");
    let adapter = rust::adapter_command(&project).unwrap();
    let launch = rust::launch_arguments(&config, &executable, &project, &[]);
    let source = project.join("src/main.rs").to_string_lossy().into_owned();

    let session =
        Session::start(&adapter, "lldb-dap", launch, &[(source, vec![2])]).expect("start");
    let (reason, thread) = next_stop(&session);
    assert_eq!(reason, "breakpoint");
    let frames = session.stack_trace(thread).unwrap();
    assert!(frames[0].name.contains("add"), "{:?}", frames[0]);
    assert_eq!(frames[0].line, 2);

    let scopes = session.scopes(frames[0].id).unwrap();
    let mut seen = Vec::new();
    for scope in &scopes {
        for v in session.variables(scope.variables_reference).unwrap() {
            seen.push((v.name, v.value));
        }
    }
    assert!(seen.iter().any(|(n, v)| n == "a" && v == "2"), "{seen:?}");
    assert!(seen.iter().any(|(n, v)| n == "b" && v == "3"), "{seen:?}");

    session.control(Control::Next, thread).unwrap();
    let (reason, _) = next_stop(&session);
    assert_eq!(reason, "step");
    session.end();
    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn a_build_that_fails_says_what_the_compiler_said() {
    let Some(_) = rust::cargo() else {
        eprintln!("skipped: cargo is missing");
        return;
    };
    let dir = std::env::temp_dir().join(format!("axiomata-dap-broken-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"broken\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/main.rs"),
        "fn main() { let x: i32 = \"no\"; }\n",
    )
    .unwrap();
    let config = rust::detect(&dir).remove(0);
    let err = rust::build(&dir, &config).unwrap_err();
    assert!(err.starts_with("The build failed"), "{err}");
    assert!(err.contains("mismatched types"), "{err}");
    let _ = std::fs::remove_dir_all(dir);
}
