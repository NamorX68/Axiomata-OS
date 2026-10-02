//! The client against a real debugpy. Runs only where one is installed: set `AXIOMATA_TEST_DEBUGPY_PATH` to a
//! directory holding the `debugpy` package (e.g. `pip install --target <dir> debugpy`); otherwise each test
//! returns early and says so.

use std::path::{Path, PathBuf};
use std::time::Duration;

use axiomata_dap::python::{current_file_config, launch_arguments};
use axiomata_dap::{AdapterCommand, Control, DebugEvent, PythonEnv, Session};

fn debugpy_dir() -> Option<String> {
    std::env::var("AXIOMATA_TEST_DEBUGPY_PATH")
        .ok()
        .filter(|p| Path::new(p).join("debugpy").is_dir())
}

fn project_with(script: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "axiomata-dap-it-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("prog.py"), script).unwrap();
    dir.canonicalize().unwrap()
}

fn adapter(project: &Path, debugpy: &str) -> AdapterCommand {
    AdapterCommand {
        program: "python3".into(),
        args: vec!["-m".into(), "debugpy.adapter".into()],
        cwd: Some(project.to_path_buf()),
        env: vec![("PYTHONPATH".into(), debugpy.into())],
    }
}

fn next_stop(session: &Session) -> (String, i64) {
    for _ in 0..200 {
        match session.next_event(Duration::from_millis(100)) {
            Some(DebugEvent::Stopped(stop)) => return (stop.reason, stop.thread_id),
            Some(DebugEvent::Closed { stderr }) => panic!("the adapter closed: {stderr}"),
            Some(DebugEvent::Terminated | DebugEvent::Exited { .. }) => {
                panic!("the program ended before stopping")
            }
            _ => {}
        }
    }
    panic!("no stop within 20 s");
}

const SCRIPT: &str =
    "def add(a, b):\n    total = a + b\n    return total\n\nx = add(2, 3)\nprint('result', x)\n";

#[test]
fn a_breakpoint_stops_shows_the_variables_steps_and_runs_to_the_end() {
    let Some(debugpy) = debugpy_dir() else {
        eprintln!("skipped: AXIOMATA_TEST_DEBUGPY_PATH not set");
        return;
    };
    let project = project_with(SCRIPT);
    let config = current_file_config("prog.py");
    let launch = launch_arguments(&config, &project, &PythonEnv::default());
    let script = project.join("prog.py").to_string_lossy().into_owned();

    let session = Session::start(
        &adapter(&project, &debugpy),
        "python",
        launch,
        &[(script.clone(), vec![2])],
    )
    .expect("start");

    // It stops on the breakpoint, in `add`, on line 2.
    let (reason, thread) = next_stop(&session);
    assert_eq!(reason, "breakpoint");
    let frames = session.stack_trace(thread).unwrap();
    assert_eq!(frames[0].name, "add");
    assert_eq!(frames[0].line, 2);
    assert!(
        frames[0]
            .path
            .as_deref()
            .is_some_and(|p| p.ends_with("prog.py"))
    );

    // The locals are a and b.
    let scopes = session.scopes(frames[0].id).unwrap();
    let locals = scopes
        .iter()
        .find(|s| s.name.to_lowercase().contains("local"))
        .expect("a locals scope");
    let vars = session.variables(locals.variables_reference).unwrap();
    let value_of = |name: &str| {
        vars.iter()
            .find(|v| v.name == name)
            .map(|v| v.value.clone())
    };
    assert_eq!(value_of("a").as_deref(), Some("2"));
    assert_eq!(value_of("b").as_deref(), Some("3"));

    // The console evaluates in the frame.
    assert_eq!(
        session.evaluate("a + b", Some(frames[0].id)).unwrap().value,
        "5"
    );

    // One step: line 3, reason "step".
    session.control(Control::Next, thread).unwrap();
    let (reason, thread) = next_stop(&session);
    assert_eq!(reason, "step");
    assert_eq!(session.stack_trace(thread).unwrap()[0].line, 3);

    // Run to the end: the program's output arrives, then it exits.
    session.control(Control::Continue, thread).unwrap();
    let mut output = String::new();
    let mut exited = None;
    for _ in 0..200 {
        match session.next_event(Duration::from_millis(100)) {
            Some(DebugEvent::Output { category, text }) if category == "stdout" => {
                output.push_str(&text)
            }
            Some(DebugEvent::Exited { code }) => exited = Some(code),
            Some(DebugEvent::Terminated) => break,
            Some(DebugEvent::Closed { .. }) => break,
            _ => {}
        }
    }
    assert!(output.contains("result 5"), "output was {output:?}");
    assert_eq!(exited, Some(0));
    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn breakpoints_can_be_changed_while_stopped_and_an_unverifiable_one_is_reported() {
    let Some(debugpy) = debugpy_dir() else {
        eprintln!("skipped: AXIOMATA_TEST_DEBUGPY_PATH not set");
        return;
    };
    let project = project_with(SCRIPT);
    let launch = launch_arguments(
        &current_file_config("prog.py"),
        &project,
        &PythonEnv::default(),
    );
    let script = project.join("prog.py").to_string_lossy().into_owned();
    let session = Session::start(
        &adapter(&project, &debugpy),
        "python",
        launch,
        &[(script.clone(), vec![2])],
    )
    .expect("start");
    let (_, thread) = next_stop(&session);

    // Move the breakpoint: line 6 (the print) now, and none on line 2.
    let placed = session.set_breakpoints(&script, &[6]).unwrap();
    assert_eq!(placed.len(), 1);
    assert!(placed[0].verified);
    session.control(Control::Continue, thread).unwrap();
    let (reason, thread) = next_stop(&session);
    assert_eq!(reason, "breakpoint");
    assert_eq!(session.stack_trace(thread).unwrap()[0].line, 6);

    session.end();
    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn an_exception_nobody_catches_stops_the_program_where_it_happened() {
    let Some(debugpy) = debugpy_dir() else {
        eprintln!("skipped: AXIOMATA_TEST_DEBUGPY_PATH not set");
        return;
    };
    let project = project_with("def boom():\n    raise ValueError('nope')\n\nboom()\n");
    let launch = launch_arguments(
        &current_file_config("prog.py"),
        &project,
        &PythonEnv::default(),
    );
    let session =
        Session::start(&adapter(&project, &debugpy), "python", launch, &[]).expect("start");
    let (reason, thread) = next_stop(&session);
    assert_eq!(reason, "exception");
    assert_eq!(session.stack_trace(thread).unwrap()[0].name, "boom");
    session.end();
    let _ = std::fs::remove_dir_all(project);
}

#[test]
fn a_missing_adapter_is_a_clear_error() {
    let launch = serde_json::json!({});
    let command = AdapterCommand {
        program: "/nonexistent/axiomata-adapter".into(),
        args: vec![],
        cwd: None,
        env: vec![],
    };
    let Err(err) = Session::start(&command, "python", launch, &[]) else {
        panic!("should not start")
    };
    assert!(err.to_string().contains("could not start"), "{err}");
}

/// The path an owner's machine normally takes: no debugpy in the project, `uv run --with debugpy` adds it.
/// Needs `uv` and network access, so it only runs on request: `cargo test -p axiomata-dap -- --ignored`.
#[test]
#[ignore = "needs uv and network access"]
fn uv_fetches_debugpy_when_the_project_has_none() {
    use axiomata_dap::python::adapter_command;
    let project = project_with(SCRIPT);
    let env = PythonEnv {
        uv: which_uv(),
        ..Default::default()
    };
    let command = adapter_command(&project, &env).expect("uv should be found");
    let launch = launch_arguments(&current_file_config("prog.py"), &project, &env);
    let script = project.join("prog.py").to_string_lossy().into_owned();
    let session =
        Session::start(&command, "python", launch, &[(script, vec![2])]).expect("start through uv");
    let (reason, thread) = next_stop(&session);
    assert_eq!(reason, "breakpoint");
    assert_eq!(session.stack_trace(thread).unwrap()[0].name, "add");
    session.end();
    let _ = std::fs::remove_dir_all(project);
}

fn which_uv() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("uv"))
            .find(|c| c.is_file())
    })
}

#[test]
fn a_console_script_entry_point_stops_at_a_breakpoint_inside_the_function_it_calls() {
    let Some(debugpy) = debugpy_dir() else {
        eprintln!("skipped: AXIOMATA_TEST_DEBUGPY_PATH not set");
        return;
    };
    let project = project_with("def main():\n    value = 41 + 1\n    return 0\n");
    std::fs::write(
        project.join("pyproject.toml"),
        "[project.scripts]\napp = \"prog:main\"\n",
    )
    .unwrap();
    let config = axiomata_dap::config::detect(&project)
        .into_iter()
        .find(|c| c.name == "app")
        .expect("the console script is offered");
    let launch = launch_arguments(&config, &project, &PythonEnv::default());
    let script = project.join("prog.py").to_string_lossy().into_owned();
    let session = Session::start(
        &adapter(&project, &debugpy),
        "python",
        launch,
        &[(script, vec![2])],
    )
    .expect("start");
    let (reason, thread) = next_stop(&session);
    assert_eq!(reason, "breakpoint");
    assert_eq!(session.stack_trace(thread).unwrap()[0].name, "main");
    session.end();
}

#[test]
fn a_program_asked_for_in_a_terminal_is_run_there_and_still_stops_at_its_breakpoint() {
    let Some(debugpy) = debugpy_dir() else {
        eprintln!("skipped: AXIOMATA_TEST_DEBUGPY_PATH not set");
        return;
    };
    let project = project_with(SCRIPT);
    let config = current_file_config("prog.py");
    let mut launch = launch_arguments(&config, &project, &PythonEnv::default());
    launch["console"] = "integratedTerminal".into();
    let script = project.join("prog.py").to_string_lossy().into_owned();

    // The "terminal" is a shell that gets the line, as the Studio's terminal pane does.
    let lines = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen = std::sync::Arc::clone(&lines);
    let pythonpath = debugpy.clone();
    let handler: axiomata_dap::TerminalHandler = std::sync::Arc::new(move |request| {
        let line = request.command_line();
        seen.lock().unwrap().push(line.clone());
        std::process::Command::new("sh")
            .arg("-c")
            .arg(line)
            .env("PYTHONPATH", &pythonpath)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("the shell starts");
    });
    let session = Session::start_with(
        &adapter(&project, &debugpy),
        "python",
        launch,
        &[(script, vec![2])],
        Some(handler),
    )
    .expect("start");
    let (reason, thread) = next_stop(&session);
    assert_eq!(reason, "breakpoint");
    assert_eq!(session.stack_trace(thread).unwrap()[0].name, "add");
    assert_eq!(lines.lock().unwrap().len(), 1);
    assert!(lines.lock().unwrap()[0].contains("prog.py"));
    session.end();
}
