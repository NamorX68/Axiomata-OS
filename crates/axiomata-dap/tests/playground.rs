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
    assert_eq!(names("rust"), ["cargo: debug-demo", "cargo: tiny"]);
    assert_eq!(names("cpp"), ["cmake: demo"]);
    assert_eq!(names("swift"), ["swift: demo"]);
    assert!(
        names("c").is_empty(),
        "C is debugged through “Current file”"
    );
}
