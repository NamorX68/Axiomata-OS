//! Python under debugpy: which interpreter runs the adapter, and what the `launch` request says.
//!
//! The adapter is `python -m debugpy.adapter`, spoken to over stdio. Which Python matters: the project's own
//! (`.venv`) when debugpy is in it; otherwise `uv run --with debugpy …`, which adds debugpy for the run without
//! touching the project; otherwise a system Python that has it. When none of that works the answer says
//! exactly what to install, instead of a bare "could not start".

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::client::AdapterCommand;
use crate::config::{DebugConfig, Language};

/// What is installed on this machine, as far as choosing an adapter needs it.
#[derive(Debug, Clone, Default)]
pub struct PythonEnv {
    /// `<project>/.venv/bin/python` (or `Scripts\python.exe`), if there is one.
    pub venv_python: Option<PathBuf>,
    /// The project's own Python can import `debugpy`.
    pub venv_has_debugpy: bool,
    /// `uv` is on the `PATH`.
    pub uv: Option<PathBuf>,
    /// A system `python3`/`python` that can import `debugpy`.
    pub system_python_with_debugpy: Option<PathBuf>,
}

fn venv_python_of(project: &Path) -> Option<PathBuf> {
    [".venv/bin/python", ".venv/Scripts/python.exe"]
        .iter()
        .map(|p| project.join(p))
        .find(|p| p.is_file())
}

fn can_import_debugpy(python: &Path) -> bool {
    Command::new(python)
        .args(["-c", "import debugpy"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

impl PythonEnv {
    /// Looks at the machine (spawns `python -c "import debugpy"` a few times — call off the UI thread).
    pub fn detect(project: &Path) -> PythonEnv {
        let venv_python = venv_python_of(project);
        let venv_has_debugpy = venv_python.as_deref().is_some_and(can_import_debugpy);
        let system_python_with_debugpy = ["python3", "python"]
            .iter()
            .filter_map(|n| on_path(n))
            .find(|p| can_import_debugpy(p));
        PythonEnv {
            venv_python,
            venv_has_debugpy,
            uv: on_path("uv"),
            system_python_with_debugpy,
        }
    }
}

/// The adapter command for `project`, or the sentence saying what to install.
pub fn adapter_command(project: &Path, env: &PythonEnv) -> Result<AdapterCommand, String> {
    let adapter = |program: String, mut args: Vec<String>| {
        args.extend(["-m".to_string(), "debugpy.adapter".to_string()]);
        AdapterCommand {
            program,
            args,
            cwd: Some(project.to_path_buf()),
            env: Vec::new(),
        }
    };
    if let (Some(python), true) = (&env.venv_python, env.venv_has_debugpy) {
        return Ok(adapter(python.to_string_lossy().into_owned(), Vec::new()));
    }
    if let Some(uv) = &env.uv {
        // `uv run` uses the project's environment and adds debugpy for this run only.
        let mut command = adapter(uv.to_string_lossy().into_owned(), Vec::new());
        command.args = [
            "run",
            "--with",
            "debugpy",
            "python",
            "-m",
            "debugpy.adapter",
        ]
        .map(String::from)
        .to_vec();
        return Ok(command);
    }
    if let Some(python) = &env.system_python_with_debugpy {
        return Ok(adapter(python.to_string_lossy().into_owned(), Vec::new()));
    }
    Err(match &env.venv_python {
        Some(_) => "debugpy is not installed in this project's .venv. Install it once with `.venv/bin/pip install debugpy` (or `uv add --dev debugpy`).".into(),
        None => "Debugging Python needs debugpy: install uv (https://docs.astral.sh/uv/), which fetches it on demand, or run `pip install debugpy`.".into(),
    })
}

/// The arguments of the DAP `launch` request for `config` in `project`.
///
/// `current_file` is the project-relative file for a “debug this file” configuration (a configuration with
/// neither `program` nor `module` of its own never reaches here — [`DebugConfig`] requires one).
pub fn launch_arguments(config: &DebugConfig, project: &Path, env: &PythonEnv) -> Value {
    let cwd = config
        .cwd
        .as_deref()
        .map_or_else(|| project.to_path_buf(), |c| project.join(c));
    let mut launch = json!({
        "name": config.name,
        "type": "python",
        "request": "launch",
        "args": config.args,
        "cwd": cwd,
        "env": config.env.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
        "justMyCode": config.just_my_code,
        // The program's own output arrives as `output` events, in the Studio's panel.
        "console": "internalConsole",
        "redirectOutput": true,
    });
    match (&config.program, &config.module, &config.code) {
        (Some(program), _, _) => launch["program"] = json!(project.join(program)),
        (None, Some(module), _) => launch["module"] = json!(module),
        (None, None, Some(code)) => launch["code"] = json!(code),
        (None, None, None) => {}
    }
    // Run the program with the project's own Python when it has one; `uv run` already provides it.
    if let (Some(python), true) = (&env.venv_python, env.venv_has_debugpy) {
        launch["python"] = json!([python]);
    }
    launch
}

/// “Debug the file in front”: a configuration for the project-relative `rel`.
pub fn current_file_config(rel: &str) -> DebugConfig {
    DebugConfig {
        name: format!("Current file ({rel})"),
        language: Language::Python,
        program: Some(rel.to_string()),
        module: None,
        code: None,
        package: None,
        source: None,
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
        just_my_code: true,
        detected: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> PathBuf {
        PathBuf::from("/work/proj")
    }

    #[test]
    fn the_projects_own_python_is_preferred_when_it_has_debugpy() {
        let env = PythonEnv {
            venv_python: Some("/work/proj/.venv/bin/python".into()),
            venv_has_debugpy: true,
            uv: Some("/usr/bin/uv".into()),
            ..Default::default()
        };
        let c = adapter_command(&project(), &env).unwrap();
        assert_eq!(c.program, "/work/proj/.venv/bin/python");
        assert_eq!(c.args, ["-m", "debugpy.adapter"]);
    }

    #[test]
    fn uv_adds_debugpy_for_the_run_when_the_project_lacks_it() {
        let env = PythonEnv {
            venv_python: Some("/work/proj/.venv/bin/python".into()),
            uv: Some("/usr/bin/uv".into()),
            ..Default::default()
        };
        let c = adapter_command(&project(), &env).unwrap();
        assert_eq!(c.program, "/usr/bin/uv");
        assert_eq!(
            c.args,
            [
                "run",
                "--with",
                "debugpy",
                "python",
                "-m",
                "debugpy.adapter"
            ]
        );
        assert_eq!(c.cwd.as_deref(), Some(project().as_path()));
    }

    #[test]
    fn a_system_python_with_debugpy_is_the_last_resort() {
        let env = PythonEnv {
            system_python_with_debugpy: Some("/usr/bin/python3".into()),
            ..Default::default()
        };
        assert_eq!(
            adapter_command(&project(), &env).unwrap().program,
            "/usr/bin/python3"
        );
    }

    #[test]
    fn without_any_the_answer_says_what_to_install() {
        let none = adapter_command(&project(), &PythonEnv::default()).unwrap_err();
        assert!(none.contains("uv") && none.contains("pip install debugpy"));
        let venv = PythonEnv {
            venv_python: Some("/work/proj/.venv/bin/python".into()),
            ..Default::default()
        };
        assert!(
            adapter_command(&project(), &venv)
                .unwrap_err()
                .contains(".venv")
        );
    }

    #[test]
    fn launch_arguments_put_program_cwd_and_environment_under_the_project() {
        let mut config = current_file_config("src/app.py");
        config.cwd = Some("backend".into());
        config.args = vec!["--dev".into()];
        config.env = vec![("A".into(), "1".into())];
        let env = PythonEnv {
            venv_python: Some("/work/proj/.venv/bin/python".into()),
            venv_has_debugpy: true,
            ..Default::default()
        };
        let launch = launch_arguments(&config, &project(), &env);
        assert_eq!(launch["program"], "/work/proj/src/app.py");
        assert_eq!(launch["cwd"], "/work/proj/backend");
        assert_eq!(launch["env"]["A"], "1");
        assert_eq!(launch["python"][0], "/work/proj/.venv/bin/python");
        assert_eq!(launch["console"], "internalConsole");
    }

    #[test]
    fn a_module_configuration_launches_with_module_not_program() {
        let mut config = current_file_config("x.py");
        config.program = None;
        config.module = Some("pytest".into());
        let launch = launch_arguments(&config, &project(), &PythonEnv::default());
        assert_eq!(launch["module"], "pytest");
        assert!(launch.get("program").is_none());
        assert!(launch.get("python").is_none());
    }
}
