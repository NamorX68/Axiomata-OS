//! C, C++ and Swift under `lldb-dap`: how a project's programs are found and built.
//!
//! Like Rust these are compiled first; the adapter is the same `lldb-dap` (see [`crate::rust`]). Three ways in:
//! a **CMake** project (`add_executable(...)` targets, built into a folder outside the repository), a **Swift
//! package** (`swift build --product …`), and a **single C/C++ file** compiled on the spot with the system
//! compiler. A prebuilt program can always be named in `debug.json` (`"type": "cpp"`, `"program": "build/app"`).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config::{DebugConfig, Language};

const MAX_TARGETS: usize = 12;

/// The C and C++ source files a single-file run can compile, with the compiler that suits them.
pub fn compiler_for(rel: &str) -> Option<&'static str> {
    let lower = rel.to_lowercase();
    if lower.ends_with(".c") {
        Some("cc")
    } else if [".cc", ".cpp", ".cxx", ".c++"]
        .iter()
        .any(|e| lower.ends_with(e))
    {
        Some("c++")
    } else {
        None
    }
}

fn native(name: String, language: Language) -> DebugConfig {
    DebugConfig {
        name,
        language,
        program: None,
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

/// “Debug the file in front” for a C/C++ file: compiled alone with debug information.
pub fn single_file_config(rel: &str) -> Option<DebugConfig> {
    compiler_for(rel)?;
    let mut config = native(format!("Current file ({rel})"), Language::Cpp);
    config.source = Some(rel.to_string());
    Some(config)
}

/// The names in `add_executable(name …)` of the project's `CMakeLists.txt` (top level only).
pub fn cmake_targets(project: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(project.join("CMakeLists.txt")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    let lower = text.to_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find("add_executable") {
        let start = from + at + "add_executable".len();
        from = start;
        let rest = text[start..].trim_start();
        let Some(rest) = rest.strip_prefix('(') else {
            continue;
        };
        let name: String = rest
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '.')
            .collect();
        if !name.is_empty() && !name.starts_with('$') && !out.contains(&name) {
            out.push(name);
        }
    }
    out.truncate(MAX_TARGETS);
    out
}

/// The executable products of a Swift package: `.executableTarget(name: "X")`, `.executable(name: "X")`, and
/// `Sources/*/main.swift`.
pub fn swift_products(project: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(project.join("Package.swift")) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for marker in [".executableTarget(", ".executable("] {
        let mut from = 0;
        while let Some(at) = text[from..].find(marker) {
            let start = from + at + marker.len();
            from = start;
            let window: String = text[start..].chars().take(200).collect();
            if let Some(name) = quoted_after(&window, "name:")
                && !out.contains(&name)
            {
                out.push(name);
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(project.join("Sources")) {
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().join("main.swift").is_file())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        for name in names {
            if !out.contains(&name) {
                out.push(name);
            }
        }
    }
    out.retain(|n| {
        n.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    });
    out.truncate(MAX_TARGETS);
    out
}

fn quoted_after(text: &str, key: &str) -> Option<String> {
    let after = &text[text.find(key)? + key.len()..];
    let after = after.trim_start().strip_prefix('"')?;
    Some(after[..after.find('"')?].to_string())
}

/// What a C/C++/Swift project suggests.
pub fn detect(project: &Path) -> Vec<DebugConfig> {
    let mut out = Vec::new();
    for target in cmake_targets(project) {
        let mut config = native(format!("cmake: {target}"), Language::Cpp);
        config.program = Some(target);
        config.package = Some("cmake".into());
        out.push(config);
    }
    for product in swift_products(project) {
        let mut config = native(format!("swift: {product}"), Language::Swift);
        config.program = Some(product);
        out.push(config);
    }
    out
}

/// A Swift file's executable: the product whose folder under `Sources/` holds it.
pub fn swift_config_for_file(project: &Path, rel: &str) -> Option<DebugConfig> {
    let target = rel.strip_prefix("Sources/")?.split('/').next()?;
    detect(project)
        .into_iter()
        .find(|c| c.language == Language::Swift && c.program.as_deref() == Some(target))
}

fn on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|c| c.is_file())
            .or_else(|| {
                ["/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"]
                    .iter()
                    .map(|d| Path::new(d).join(name))
                    .find(|c| c.is_file())
            })
    })
}

fn run(mut command: Command, what: &str) -> Result<String, String> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not run {what}: {e}"))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let lines: Vec<&str> = text.lines().collect();
    let tail = lines[lines.len().saturating_sub(40)..].join("\n");
    Err(format!("The build failed ({what}):\n{tail}"))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.is_file()
            && path
                .metadata()
                .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// Where CMake puts `target` below `dir` (single-config or multi-config generators).
fn find_built(dir: &Path, target: &str) -> Option<PathBuf> {
    let direct = [
        dir.join(target),
        dir.join("Debug").join(target),
        dir.join("bin").join(target),
        dir.join("bin").join("Debug").join(target),
    ];
    direct.into_iter().find(|p| is_executable(p)).or_else(|| {
        // One more level, for projects that place programs in a subfolder of their own.
        std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
            let candidate = e.path().join(target);
            (e.path().is_dir() && is_executable(&candidate)).then_some(candidate)
        })
    })
}

/// Builds the program of `config` and returns the executable. `build_root` is a folder outside the
/// repository, per project (CMake and single-file builds go there). Slow: call it off the UI thread.
pub fn build(project: &Path, build_root: &Path, config: &DebugConfig) -> Result<PathBuf, String> {
    std::fs::create_dir_all(build_root)
        .map_err(|e| format!("could not make {}: {e}", build_root.display()))?;
    match config.language {
        Language::Swift => {
            let product = config
                .program
                .as_deref()
                .ok_or("the configuration names no product")?;
            let swift = on_path("swift")
                .ok_or("swift was not found — install Xcode or the Swift toolchain")?;
            let mut command = Command::new(&swift);
            command
                .current_dir(project)
                .args(["build", "--product", product]);
            run(command, "swift build")?;
            let mut show = Command::new(swift);
            show.current_dir(project).args(["build", "--show-bin-path"]);
            let bin_dir = run(show, "swift build --show-bin-path")?;
            let exe = Path::new(bin_dir.trim()).join(product);
            exe.is_file()
                .then_some(exe)
                .ok_or_else(|| format!("swift built, but there is no “{product}” executable"))
        }
        Language::Cpp => {
            if let Some(source) = &config.source {
                let compiler = compiler_for(source).ok_or("this is not a C or C++ file")?;
                let compiler = on_path(compiler).ok_or("no C/C++ compiler found — install the Xcode command line tools (xcode-select --install)")?;
                let stem = Path::new(source)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("program");
                let out = build_root.join(format!("file-{stem}"));
                let mut command = Command::new(compiler);
                command
                    .current_dir(project)
                    .args(["-g", "-O0", "-o"])
                    .arg(&out)
                    .arg(project.join(source));
                run(command, "the compiler")?;
                return Ok(out);
            }
            let program = config
                .program
                .as_deref()
                .ok_or("the configuration names no program")?;
            if config.package.as_deref() == Some("cmake") {
                let cmake = on_path("cmake")
                    .ok_or("cmake was not found — install it (brew install cmake)")?;
                let dir = build_root.join("cmake");
                let mut configure = Command::new(&cmake);
                configure
                    .current_dir(project)
                    .arg("-S")
                    .arg(project)
                    .arg("-B")
                    .arg(&dir)
                    .arg("-DCMAKE_BUILD_TYPE=Debug");
                run(configure, "cmake")?;
                let mut compile = Command::new(cmake);
                compile
                    .current_dir(project)
                    .arg("--build")
                    .arg(&dir)
                    .args(["--target", program, "--config", "Debug"]);
                run(compile, "cmake --build")?;
                return find_built(&dir, program).ok_or_else(|| {
                    format!("cmake built, but no executable “{program}” was found")
                });
            }
            // A prebuilt program inside the project.
            let path = project.join(program);
            is_executable(&path)
                .then_some(path)
                .ok_or_else(|| format!("“{program}” is not an executable file — build it first"))
        }
        _ => Err("not a native configuration".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "axiomata-dap-nat-{}-{}",
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
    fn cmake_executables_are_found_by_their_add_executable() {
        let dir = project(&[(
            "CMakeLists.txt",
            "project(x)\nadd_library(core a.cpp)\nADD_EXECUTABLE( app main.cpp )\nadd_executable(tool-2 t.cpp)\nadd_executable(${NAME} z.cpp)\n",
        )]);
        assert_eq!(cmake_targets(&dir), ["app", "tool-2"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_swift_package_offers_its_executables() {
        let dir = project(&[
            (
                "Package.swift",
                "let package = Package(name: \"P\", products: [.executable(name: \"cli\", targets: [\"cli\"])], targets: [.executableTarget(name: \"cli\"), .executableTarget(\n name: \"daemon\"), .target(name: \"Kit\")])",
            ),
            ("Sources/cli/main.swift", ""),
            ("Sources/other/main.swift", ""),
        ]);
        assert_eq!(swift_products(&dir), ["cli", "daemon", "other"]);
        assert_eq!(
            swift_config_for_file(&dir, "Sources/cli/main.swift")
                .unwrap()
                .program
                .as_deref(),
            Some("cli")
        );
        assert!(swift_config_for_file(&dir, "Tests/x.swift").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_single_c_or_cpp_file_is_a_configuration_of_its_own() {
        assert_eq!(compiler_for("src/a.c"), Some("cc"));
        assert_eq!(compiler_for("a.CPP"), Some("c++"));
        assert_eq!(compiler_for("a.rs"), None);
        let config = single_file_config("src/a.c").unwrap();
        assert_eq!(config.source.as_deref(), Some("src/a.c"));
        assert!(single_file_config("a.txt").is_none());
    }
}
