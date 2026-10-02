//! Rust under `lldb-dap`: which binaries a project has, how they are built, and what the `launch` request says.
//!
//! Unlike Python, Rust is compiled first: a session starts with `cargo build --bin <name>` and launches the
//! executable cargo reports. The adapter is LLVM's `lldb-dap` (Xcode 16+ and Homebrew's `llvm` ship it; older
//! builds call it `lldb-vscode`), spoken to over stdio exactly like debugpy.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

use crate::client::AdapterCommand;
use crate::config::{DebugConfig, Language};

/// The most workspace members and binaries offered; a monorepo's hundred crates would bury the useful ones.
const MAX_MEMBERS: usize = 20;
const MAX_BINS: usize = 12;

/// A binary target of the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustBin {
    pub name: String,
    pub package: String,
    /// The source file of its `main`, relative to the project folder.
    pub source: String,
    /// The folder of the project it belongs to, relative to the project folder (`""` = the project itself).
    pub dir: String,
}

fn read_toml(path: &Path) -> Option<toml::Table> {
    std::fs::read_to_string(path).ok()?.parse().ok()
}

/// The binaries of the package whose `Cargo.toml` lives in `dir` (`rel` = `dir` relative to the project).
fn package_bins(dir: &Path, rel: &str) -> Vec<RustBin> {
    let Some(manifest) = read_toml(&dir.join("Cargo.toml")) else {
        return Vec::new();
    };
    let Some(package) = manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
    else {
        return Vec::new();
    };
    let at = |file: &str| {
        if rel.is_empty() {
            file.to_string()
        } else {
            format!("{rel}/{file}")
        }
    };
    let mut bins: Vec<RustBin> = Vec::new();
    let mut add = |name: &str, source: String| {
        if !bins.iter().any(|b| b.name == name) {
            bins.push(RustBin {
                name: name.to_string(),
                package: package.to_string(),
                source,
                dir: String::new(),
            });
        }
    };
    if let Some(list) = manifest.get("bin").and_then(|b| b.as_array()) {
        for bin in list {
            if let Some(name) = bin.get("name").and_then(|n| n.as_str()) {
                let path = bin
                    .get("path")
                    .and_then(|p| p.as_str())
                    .map_or_else(|| format!("src/bin/{name}.rs"), str::to_string);
                add(name, at(&path));
            }
        }
    }
    if dir.join("src/main.rs").is_file() {
        add(package, at("src/main.rs"));
    }
    if let Ok(entries) = std::fs::read_dir(dir.join("src/bin")) {
        let mut found: Vec<(String, String)> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let name = e.file_name().into_string().ok()?;
                if path.is_file() && name.ends_with(".rs") {
                    let stem = name.trim_end_matches(".rs").to_string();
                    Some((stem.clone(), at(&format!("src/bin/{stem}.rs"))))
                } else if path.join("main.rs").is_file() {
                    Some((name.clone(), at(&format!("src/bin/{name}/main.rs"))))
                } else {
                    None
                }
            })
            .collect();
        found.sort();
        for (name, source) in found {
            add(&name, source);
        }
    }
    bins
}

/// The binaries of the project in `base` (`rel_base` = where it lies below the project; `""` = the project):
/// its root package and, for a workspace, its members. Sources are named relative to the *project*.
fn bins_in(base: &Path, rel_base: &str) -> Vec<RustBin> {
    let Some(root) = read_toml(&base.join("Cargo.toml")) else {
        return Vec::new();
    };
    let join = |rel: &str| {
        if rel_base.is_empty() {
            rel.to_string()
        } else {
            format!("{rel_base}/{rel}")
        }
    };
    let mut out = package_bins(base, rel_base);
    if let Some(members) = root
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
    {
        let mut dirs: Vec<String> = Vec::new();
        for member in members.iter().filter_map(|m| m.as_str()) {
            if let Some(parent) = member.strip_suffix("/*") {
                if let Ok(entries) = std::fs::read_dir(base.join(parent)) {
                    let mut names: Vec<String> = entries
                        .flatten()
                        .filter(|e| e.path().join("Cargo.toml").is_file())
                        .filter_map(|e| e.file_name().into_string().ok())
                        .collect();
                    names.sort();
                    dirs.extend(names.into_iter().map(|n| format!("{parent}/{n}")));
                }
            } else if !member.contains('*') {
                dirs.push(member.trim_end_matches('/').to_string());
            }
        }
        for rel in dirs.into_iter().take(MAX_MEMBERS) {
            if rel.starts_with('/') || rel.split('/').any(|p| p == "..") {
                continue;
            }
            for bin in package_bins(&base.join(&rel), &join(&rel)) {
                if !out
                    .iter()
                    .any(|b| b.name == bin.name && b.package == bin.package)
                {
                    out.push(bin);
                }
            }
        }
    }
    for bin in &mut out {
        bin.dir = rel_base.to_string();
    }
    out
}

/// Every binary target below the project: of its own `Cargo.toml` or, when the folder has none, of the Rust
/// projects in the folders beneath it.
pub fn bins(project: &Path) -> Vec<RustBin> {
    let mut out = Vec::new();
    for rel in crate::config::manifest_dirs(project, "Cargo.toml") {
        out.extend(bins_in(&project.join(&rel), &rel));
    }
    out.truncate(MAX_BINS);
    out
}

/// The folder a configuration builds and runs in: its manifest's folder, else the project.
pub fn base_of(project: &Path, config: &DebugConfig) -> PathBuf {
    config
        .dir
        .as_deref()
        .map_or_else(|| project.to_path_buf(), |d| project.join(d))
}

fn config_of(bin: &RustBin) -> DebugConfig {
    DebugConfig {
        name: if bin.dir.is_empty() {
            format!("cargo: {}", bin.name)
        } else {
            format!("cargo: {} ({})", bin.name, bin.dir)
        },
        language: Language::Rust,
        program: Some(bin.name.clone()),
        module: None,
        code: None,
        package: Some(bin.package.clone()),
        source: None,
        dir: (!bin.dir.is_empty()).then(|| bin.dir.clone()),
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
        just_my_code: true,
        detected: true,
    }
}

/// What a Rust project suggests: one configuration per binary.
pub fn detect(project: &Path) -> Vec<DebugConfig> {
    bins(project).iter().map(config_of).collect()
}

/// “Debug the file in front” for a `.rs` file: the binary whose `main` it is, if it is one.
pub fn config_for_file(project: &Path, rel: &str) -> Option<DebugConfig> {
    bins(project)
        .iter()
        .find(|b| b.source == rel)
        .map(config_of)
}

/// A program named in a `debug.json` or the form: letters, digits, `-` and `_` — what a cargo target can be called.
pub fn is_target_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn on_path(name: &str, extra: &[PathBuf]) -> Option<PathBuf> {
    let from_env = std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .unwrap_or_default();
    from_env
        .iter()
        .chain(extra.iter())
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Where a program that a GUI app does not find on its `PATH` usually lives.
fn usual_places() -> Vec<PathBuf> {
    let mut places = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/opt/homebrew/opt/llvm/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/local/opt/llvm/bin"),
        PathBuf::from("/usr/bin"),
    ];
    if let Some(home) = home() {
        places.push(home.join(".cargo/bin"));
    }
    if let Ok(entries) = std::fs::read_dir("/usr/lib") {
        let mut llvm: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("llvm-"))
            .map(|e| e.path().join("bin"))
            .collect();
        llvm.sort();
        llvm.reverse();
        places.extend(llvm);
    }
    places
}

/// `cargo` — on the `PATH`, else where rustup puts it.
pub fn cargo() -> Option<PathBuf> {
    on_path("cargo", &usual_places())
}

/// LLVM's DAP adapter, if this machine has one.
fn find_lldb_dap() -> Option<PathBuf> {
    let places = usual_places();
    for name in ["lldb-dap", "lldb-vscode"] {
        if let Some(found) = on_path(name, &places) {
            return Some(found);
        }
    }
    // Xcode's command line tools: `xcrun -f lldb-dap` names the tool inside the active toolchain.
    for name in ["lldb-dap", "lldb-vscode"] {
        let found = Command::new("xcrun")
            .args(["-f", name])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
            .filter(|p| p.is_file());
        if found.is_some() {
            return found;
        }
    }
    // A versioned name (`lldb-dap-18`) next to the others.
    places.iter().find_map(|dir| {
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("lldb-dap-"))
            .map(|e| e.path())
            .next()
    })
}

/// The adapter command for `project`, or the sentence saying what to install.
pub fn adapter_command(project: &Path) -> Result<AdapterCommand, String> {
    let program = find_lldb_dap().ok_or_else(|| {
        "Debugging Rust needs lldb-dap: on a Mac install the Xcode command line tools (xcode-select --install, Xcode 16 or newer) or `brew install llvm`.".to_string()
    })?;
    Ok(AdapterCommand {
        program: program.to_string_lossy().into_owned(),
        args: Vec::new(),
        cwd: Some(project.to_path_buf()),
        env: Vec::new(),
    })
}

/// The path of the executable in cargo's `--message-format=json` output (`compiler-artifact` of a bin).
pub fn executable_in(output: &str, bin: &str) -> Option<PathBuf> {
    output
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|v| v["reason"] == "compiler-artifact")
        .filter(|v| v["target"]["name"] == bin)
        .filter(|v| {
            v["target"]["kind"]
                .as_array()
                .is_some_and(|k| k.iter().any(|k| k == "bin"))
        })
        .filter_map(|v| v["executable"].as_str().map(PathBuf::from))
        .next_back()
}

/// Builds the binary of `config` in `project` and returns the path of the executable. Slow (a compile):
/// call it off the UI thread. A failed build answers with the compiler's own words.
pub fn build(project: &Path, config: &DebugConfig) -> Result<PathBuf, String> {
    let bin = config
        .program
        .as_deref()
        .ok_or("the configuration names no binary")?;
    let cargo = cargo().ok_or("cargo was not found — install Rust from https://rustup.rs")?;
    let mut command = Command::new(cargo);
    command.current_dir(base_of(project, config)).args([
        "build",
        "--bin",
        bin,
        "--message-format=json-render-diagnostics",
    ]);
    if let Some(package) = &config.package {
        command.args(["-p", package]);
    }
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(40).collect();
        let text = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
        return Err(format!("The build failed:\n{text}"));
    }
    executable_in(&String::from_utf8_lossy(&output.stdout), bin)
        .filter(|p| p.is_file())
        .ok_or_else(|| format!("cargo built, but reported no executable for “{bin}”"))
}

/// Commands that teach lldb to print Rust values readably (`Vec`, `String`, `Option` …), from the toolchain's
/// own `lldb_lookup.py` — what `rust-lldb` loads. Empty when the files are not there.
pub fn init_commands() -> Vec<String> {
    let Some(rustc) = on_path("rustc", &usual_places()) else {
        return Vec::new();
    };
    let Some(sysroot) = Command::new(rustc)
        .args(["--print", "sysroot"])
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()))
    else {
        return Vec::new();
    };
    let etc = sysroot.join("lib/rustlib/etc");
    let (lookup, commands) = (etc.join("lldb_lookup.py"), etc.join("lldb_commands"));
    if !lookup.is_file() || !commands.is_file() {
        return Vec::new();
    }
    vec![
        format!("command script import \"{}\"", lookup.display()),
        format!("command source -s 0 \"{}\"", commands.display()),
    ]
}

/// The arguments of the DAP `launch` request for the built `executable`.
pub fn launch_arguments(
    config: &DebugConfig,
    executable: &Path,
    project: &Path,
    init: &[String],
) -> Value {
    let cwd = config
        .cwd
        .as_deref()
        .map_or_else(|| base_of(project, config), |c| project.join(c));
    json!({
        "name": config.name,
        "type": "lldb-dap",
        "request": "launch",
        "program": executable,
        "args": config.args,
        "cwd": cwd,
        // lldb-dap takes the environment as `KEY=VALUE` strings.
        "env": config.env.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>(),
        "stopOnEntry": false,
        "initCommands": init,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "axiomata-dap-rs-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        for (file, text) in files {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        dir
    }

    #[test]
    fn a_package_offers_its_main_and_its_extra_binaries() {
        let dir = project(&[
            (
                "Cargo.toml",
                "[package]\nname = \"tool\"\nversion = \"0.1.0\"\n",
            ),
            ("src/main.rs", "fn main() {}"),
            ("src/bin/helper.rs", "fn main() {}"),
            ("src/bin/big/main.rs", "fn main() {}"),
        ]);
        let names: Vec<_> = detect(&dir).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["cargo: tool", "cargo: big", "cargo: helper"]);
        assert_eq!(
            config_for_file(&dir, "src/bin/helper.rs")
                .unwrap()
                .program
                .as_deref(),
            Some("helper")
        );
        assert!(config_for_file(&dir, "src/lib.rs").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_workspace_offers_the_binaries_of_its_members() {
        let dir = project(&[
            (
                "Cargo.toml",
                "[workspace]\nmembers = [\"crates/*\", \"app\"]\n",
            ),
            (
                "crates/cli/Cargo.toml",
                "[package]\nname = \"cli\"\nversion = \"0.1.0\"\n",
            ),
            ("crates/cli/src/main.rs", "fn main() {}"),
            (
                "crates/lib/Cargo.toml",
                "[package]\nname = \"lib\"\nversion = \"0.1.0\"\n",
            ),
            ("crates/lib/src/lib.rs", ""),
            (
                "app/Cargo.toml",
                "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"server\"\npath = \"src/server.rs\"\n",
            ),
            ("app/src/server.rs", "fn main() {}"),
        ]);
        let found = bins(&dir);
        let names: Vec<_> = found
            .iter()
            .map(|b| (b.package.as_str(), b.name.as_str()))
            .collect();
        assert_eq!(names, [("cli", "cli"), ("app", "server")]);
        assert_eq!(found[1].source, "app/src/server.rs");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_executable_is_read_from_cargos_json_lines() {
        let output = concat!(
            "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"dep\",\"kind\":[\"lib\"]},\"executable\":null}\n",
            "{\"reason\":\"compiler-artifact\",\"target\":{\"name\":\"tool\",\"kind\":[\"bin\"]},\"executable\":\"/p/target/debug/tool\"}\n",
            "{\"reason\":\"build-finished\",\"success\":true}\n"
        );
        assert_eq!(
            executable_in(output, "tool"),
            Some(PathBuf::from("/p/target/debug/tool"))
        );
        assert_eq!(executable_in(output, "dep"), None);
    }

    #[test]
    fn launch_arguments_pass_the_environment_as_strings() {
        let mut config = detect(&project(&[
            (
                "Cargo.toml",
                "[package]\nname = \"t\"\nversion = \"0.1.0\"\n",
            ),
            ("src/main.rs", ""),
        ]))
        .remove(0);
        config.env = vec![("A".into(), "1".into())];
        config.args = vec!["--x".into()];
        let launch = launch_arguments(
            &config,
            Path::new("/p/target/debug/t"),
            Path::new("/p"),
            &[],
        );
        assert_eq!(launch["env"][0], "A=1");
        assert_eq!(launch["program"], "/p/target/debug/t");
        assert_eq!(launch["cwd"], "/p");
    }
}
