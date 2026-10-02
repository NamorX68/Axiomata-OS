//! The example programs under `examples/debug-playground` must keep offering the configurations the README names.

use std::path::PathBuf;

use axiomata_dap::config::detect;

fn playground(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/debug-playground")
        .join(name)
}

fn names(name: &str) -> Vec<String> {
    detect(&playground(name))
        .into_iter()
        .map(|c| c.name)
        .collect()
}

#[test]
fn every_playground_offers_what_the_readme_says() {
    assert!(
        names("python").contains(&"main.py".to_string()),
        "{:?}",
        names("python")
    );
    let rust = names("rust");
    for expected in [
        "cargo: debug-demo",
        "cargo: tiny",
        "cargo: boom",
        "cargo test: debug-demo (bin)",
    ] {
        assert!(
            rust.contains(&expected.to_string()),
            "{expected} in {rust:?}"
        );
    }
    assert_eq!(names("cpp"), ["cmake: demo"]);
    assert_eq!(names("swift"), ["swift: demo"]);
    assert!(
        names("c").is_empty(),
        "C is debugged through “Current file”"
    );
}

#[test]
fn the_parent_folder_offers_the_projects_beneath_it_and_runs_their_files() {
    let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/debug-playground");
    let found = names("");
    for expected in [
        "cargo: debug-demo (rust)",
        "cargo: tiny (rust)",
        "cmake: demo (cpp)",
        "swift: demo (swift)",
    ] {
        assert!(
            found.contains(&expected.to_string()),
            "{expected} in {found:?}"
        );
    }
    // “Current file” on a Rust or Swift source finds the project it lies in.
    let tiny = axiomata_dap::rust::config_for_file(&parent, "rust/src/bin/tiny.rs")
        .expect("tiny.rs is a binary");
    assert_eq!(
        (tiny.program.as_deref(), tiny.dir.as_deref()),
        (Some("tiny"), Some("rust"))
    );
    let swift =
        axiomata_dap::native::swift_config_for_file(&parent, "swift/Sources/demo/main.swift")
            .expect("a swift target");
    assert_eq!(swift.dir.as_deref(), Some("swift"));
}
