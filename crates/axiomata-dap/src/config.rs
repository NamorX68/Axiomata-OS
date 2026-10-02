//! `debug.json` and the configurations a project suggests.
//!
//! ```json
//! { "configurations": [ { "name": "Server", "type": "python", "module": "ocht", "args": ["--dev"],
//!                         "cwd": "backend", "env": {"DEBUG": "1"}, "justMyCode": true } ] }
//! ```
//!
//! One of `program` (a file) or `module` (`python -m`) names what runs. The same strictness as `tasks.json`:
//! folders stay inside the project, environment names are plain, and every refusal is a `problems` line
//! rather than a silently dropped entry. The file belongs to the repository, so it runs only after the owner
//! confirmed its exact content (`axiomata_tasks::trust`, in the Tauri glue).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    Python,
}

/// Where the project keeps its own configurations, relative to the project folder.
pub const PROJECT_FILE: &str = ".axiomata/debug.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DebugConfig {
    pub name: String,
    pub language: Language,
    /// A file to run, relative to the project folder.
    pub program: Option<String>,
    /// A module to run (`python -m <module>`); `pytest` debugs the tests.
    pub module: Option<String>,
    pub args: Vec<String>,
    /// A folder inside the project; `None` = the project folder.
    pub cwd: Option<String>,
    pub env: Vec<(String, String)>,
    pub just_my_code: bool,
    /// Where it came from: detected, or written in the file.
    pub detected: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct DebugFile {
    pub configurations: Vec<DebugConfig>,
    pub problems: Vec<String>,
}

#[derive(Deserialize)]
struct FileShape {
    #[serde(default)]
    configurations: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    program: Option<String>,
    module: Option<String>,
    #[serde(default)]
    args: Vec<String>,
    cwd: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(rename = "justMyCode")]
    just_my_code: Option<bool>,
}

const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_CONFIGS: usize = 50;

/// Reads a `debug.json`.
pub fn parse_debug_file(bytes: &[u8]) -> DebugFile {
    let mut out = DebugFile::default();
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
                "not valid JSON of the form {{\"configurations\": […]}}: {err}"
            ));
            return out;
        }
    };
    for (i, entry) in shape.configurations.into_iter().enumerate() {
        if out.configurations.len() == MAX_CONFIGS {
            out.problems.push(format!(
                "more than {MAX_CONFIGS} configurations — the rest are ignored"
            ));
            break;
        }
        match build(entry) {
            Ok(config) if out.configurations.iter().any(|c| c.name == config.name) => {
                out.problems.push(format!(
                    "configuration {}: the name “{}” is used twice — skipped",
                    i + 1,
                    config.name
                ));
            }
            Ok(config) => out.configurations.push(config),
            Err(why) => out.problems.push(format!("configuration {}: {why}", i + 1)),
        }
    }
    out
}

fn inside(path: &str) -> bool {
    !(path.starts_with('/')
        || path.starts_with('~')
        || path.contains('\0')
        || path.split(['/', '\\']).any(|p| p == ".."))
}

fn build(entry: Entry) -> Result<DebugConfig, String> {
    let name = entry.name.map(|n| n.trim().to_string()).unwrap_or_default();
    if name.is_empty() || name.chars().count() > 80 || name.contains(char::is_control) {
        return Err("needs a name of 1–80 characters".into());
    }
    let language = match entry.kind.as_deref() {
        Some("python") | None => Language::Python,
        Some(other) => {
            return Err(format!(
                "“{name}”: the type “{other}” is not supported yet (python is)"
            ));
        }
    };
    let program = entry
        .program
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    let module = entry
        .module
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty());
    match (&program, &module) {
        (None, None) => return Err(format!("“{name}” needs a “program” (a file) or a “module”")),
        (Some(_), Some(_)) => {
            return Err(format!(
                "“{name}” has both “program” and “module” — choose one"
            ));
        }
        _ => {}
    }
    if let Some(p) = &program
        && !inside(p)
    {
        return Err(format!(
            "“{name}”: the program must lie inside the project (no absolute path, no ..)"
        ));
    }
    if let Some(m) = &module
        && !m
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
    {
        return Err(format!("“{name}”: “{m}” is not a module name"));
    }
    let cwd = entry
        .cwd
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty() && c != ".");
    if let Some(c) = &cwd
        && !inside(c)
    {
        return Err(format!(
            "“{name}”: the folder must lie inside the project (no absolute path, no ..)"
        ));
    }
    for key in entry.env.keys() {
        let mut chars = key.chars();
        let ok = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
        if !ok {
            return Err(format!(
                "“{name}”: “{key}” is not a name an environment variable can have"
            ));
        }
    }
    if entry.args.iter().any(|a| a.contains('\0')) || entry.env.values().any(|v| v.contains('\0')) {
        return Err(format!(
            "“{name}”: arguments and values must not contain a NUL"
        ));
    }
    Ok(DebugConfig {
        name,
        language,
        program,
        module,
        args: entry.args,
        cwd,
        env: entry.env.into_iter().collect(),
        just_my_code: entry.just_my_code.unwrap_or(true),
        detected: false,
    })
}

fn detected(name: &str, program: Option<&str>, module: Option<&str>) -> DebugConfig {
    DebugConfig {
        name: name.to_string(),
        language: Language::Python,
        program: program.map(str::to_string),
        module: module.map(str::to_string),
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
        just_my_code: true,
        detected: true,
    }
}

/// What a Python project suggests: its test suite, a runnable package, an entry file. ("Debug the file
/// in front" is not here — it depends on the editor, and the Studio adds it.)
pub fn detect(project: &Path) -> Vec<DebugConfig> {
    let has = |f: &str| project.join(f).is_file();
    let pyproject = std::fs::read_to_string(project.join("pyproject.toml")).unwrap_or_default();
    let is_python = !pyproject.is_empty()
        || has("setup.py")
        || has("requirements.txt")
        || has("main.py")
        || has("manage.py");
    if !is_python {
        return Vec::new();
    }
    let mut out = Vec::new();
    if has("pytest.ini") || pyproject.contains("pytest") || project.join("tests").is_dir() {
        out.push(detected("pytest", None, Some("pytest")));
    }
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
        for name in names.into_iter().take(3) {
            out.push(detected(&name, None, Some(&name)));
        }
    }
    for file in ["manage.py", "main.py", "app.py"] {
        if has(file) {
            out.push(detected(file, Some(file), None));
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configuration_is_read_with_folder_environment_and_just_my_code() {
        let json = br#"{"configurations":[{"name":"Server","type":"python","module":"ocht","args":["--dev"],"cwd":"backend","env":{"B":"2","A":"1"},"justMyCode":false}]}"#;
        let parsed = parse_debug_file(json);
        assert!(parsed.problems.is_empty(), "{:?}", parsed.problems);
        let c = &parsed.configurations[0];
        assert_eq!(
            (c.module.as_deref(), c.cwd.as_deref(), c.just_my_code),
            (Some("ocht"), Some("backend"), false)
        );
        assert_eq!(
            c.env,
            [
                ("A".to_string(), "1".to_string()),
                ("B".to_string(), "2".to_string())
            ]
        );
        assert!(!c.detected);
    }

    #[test]
    fn what_could_leave_the_project_or_be_misread_is_refused_and_said() {
        let json = br#"{"configurations":[
            {"name":"a","program":"/etc/x.py"},
            {"name":"b","program":"../x.py"},
            {"name":"c","module":"not a module"},
            {"name":"d"},
            {"name":"e","program":"a.py","module":"m"},
            {"name":"f","program":"a.py","cwd":"../up"},
            {"name":"g","program":"a.py","env":{"1X":"v"}},
            {"name":"h","type":"rust","program":"a"},
            {"program":"a.py"},
            {"name":"ok","program":"a.py"},
            {"name":"ok","program":"b.py"}
        ]}"#;
        let parsed = parse_debug_file(json);
        assert_eq!(parsed.configurations.len(), 1);
        assert_eq!(parsed.problems.len(), 10, "{:?}", parsed.problems);
    }

    #[test]
    fn broken_or_oversized_input_is_a_problem_not_a_panic() {
        assert_eq!(parse_debug_file(b"{ nope").problems.len(), 1);
        assert!(
            parse_debug_file(&vec![b' '; 300 * 1024])
                .configurations
                .is_empty()
        );
    }

    fn project(files: &[&str]) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "axiomata-dap-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        for file in files {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_python_project_suggests_tests_a_runnable_package_and_an_entry_file() {
        let dir = project(&[
            "pyproject.toml",
            "tests/test_a.py",
            "src/ocht/__main__.py",
            "main.py",
        ]);
        let names: Vec<_> = detect(&dir).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["pytest", "ocht", "main.py"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn something_that_is_not_python_suggests_nothing() {
        let dir = project(&["Cargo.toml"]);
        assert!(detect(&dir).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
