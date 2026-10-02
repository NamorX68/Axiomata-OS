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

use axiomata_dap::native;

const C_SOURCE: &str = "#include <stdio.h>\n\nint add(int a, int b) {\n    int total = a + b;\n    return total;\n}\n\nint main(void) {\n    int x = add(2, 3);\n    printf(\"result %d\\n\", x);\n    return 0;\n}\n";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("axiomata-dap-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn stops_in_add(
    project: &std::path::Path,
    executable: &std::path::Path,
    config: &axiomata_dap::DebugConfig,
    file: &str,
    line: u32,
) {
    let adapter = rust::adapter_command(project).unwrap();
    let launch = rust::launch_arguments(config, executable, project, &[]);
    let source = project.join(file).to_string_lossy().into_owned();
    let session =
        Session::start(&adapter, "lldb-dap", launch, &[(source, vec![line])]).expect("start");
    let (reason, thread) = next_stop(&session);
    assert_eq!(reason, "breakpoint");
    let frames = session.stack_trace(thread).unwrap();
    assert!(frames[0].name.contains("add"), "{:?}", frames[0]);
    assert_eq!(frames[0].line, line);
    session.end();
}

#[test]
fn a_single_c_file_is_compiled_and_stops_at_its_breakpoint() {
    if native::compiler_for("a.c").is_none() || rust::adapter_command(&PathBuf::from("/")).is_err()
    {
        eprintln!("skipped: lldb-dap is missing");
        return;
    }
    let project = scratch("c");
    std::fs::write(project.join("prog.c"), C_SOURCE).unwrap();
    let config = native::single_file_config("prog.c").unwrap();
    let build_root = scratch("c-build");
    let executable = match native::build(&project, &build_root, &config) {
        Ok(e) => e,
        Err(why) if why.contains("no C/C++ compiler") => return,
        Err(why) => panic!("{why}"),
    };
    stops_in_add(&project, &executable, &config, "prog.c", 4);
}

#[test]
fn a_cmake_target_is_configured_built_and_stops_at_its_breakpoint() {
    let have = |n: &str| {
        std::env::var_os("PATH")
            .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(n).is_file()))
    };
    if !have("cmake") || !have("make") || rust::adapter_command(&PathBuf::from("/")).is_err() {
        eprintln!("skipped: cmake, make or lldb-dap is missing");
        return;
    }
    let project = scratch("cmake");
    std::fs::write(
        project.join("CMakeLists.txt"),
        "cmake_minimum_required(VERSION 3.10)\nproject(probe C)\nadd_executable(probe prog.c)\n",
    )
    .unwrap();
    std::fs::write(project.join("prog.c"), C_SOURCE).unwrap();
    let config = native::detect(&project).remove(0);
    assert_eq!(config.name, "cmake: probe");
    let build_root = scratch("cmake-build");
    let executable = native::build(&project, &build_root, &config).expect("the build");
    assert!(
        !executable.starts_with(&project),
        "built outside the repository"
    );
    stops_in_add(&project, &executable, &config, "prog.c", 4);
}

#[test]
fn a_c_compile_error_comes_back_in_the_compilers_words() {
    let project = scratch("cerr");
    std::fs::write(project.join("bad.c"), "int main(void) { return x; }\n").unwrap();
    let config = native::single_file_config("bad.c").unwrap();
    match native::build(&project, &scratch("cerr-build"), &config) {
        Err(why) if why.contains("no C/C++ compiler") => {}
        Err(why) => assert!(
            why.starts_with("The build failed") && why.contains("'x'"),
            "{why}"
        ),
        Ok(_) => panic!("a broken file must not build"),
    }
}

#[test]
fn a_rust_project_below_the_opened_folder_is_built_there_and_stops_at_its_breakpoint() {
    if rust::cargo().is_none() || rust::adapter_command(&PathBuf::from("/")).is_err() {
        eprintln!("skipped: cargo or lldb-dap is missing");
        return;
    }
    // The parent folder is the project; the Rust project lies one level below it.
    let parent = scratch("parent");
    std::fs::create_dir_all(parent.join("inner/src")).unwrap();
    std::fs::write(
        parent.join("inner/Cargo.toml"),
        "[package]\nname = \"probe\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(parent.join("inner/src/main.rs"), MAIN).unwrap();
    let config = rust::config_for_file(&parent, "inner/src/main.rs").expect("a binary");
    assert_eq!(config.dir.as_deref(), Some("inner"));
    let executable = rust::build(&parent, &config).expect("the build");
    assert!(
        executable.starts_with(parent.join("inner")),
        "{executable:?}"
    );
    let adapter = rust::adapter_command(&parent).unwrap();
    let launch = rust::launch_arguments(&config, &executable, &parent, &[]);
    let source = parent
        .join("inner/src/main.rs")
        .to_string_lossy()
        .into_owned();
    let session =
        Session::start(&adapter, "lldb-dap", launch, &[(source, vec![2])]).expect("start");
    let (reason, thread) = next_stop(&session);
    assert_eq!(reason, "breakpoint");
    assert!(session.stack_trace(thread).unwrap()[0].name.contains("add"));
    session.end();
}
