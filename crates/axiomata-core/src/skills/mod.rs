//! Skill discovery and headless execution.
//!
//! Skills are read from two locations and merged: global skills under
//! `~/.axiomata/skills/`, and workspace-local skills under
//! `<workspace_root>/.claude/skills/`, which take priority on name collisions.
//!
//! Implemented starting in M1.

use std::fs;
use std::io::{ErrorKind, Write};

use crate::error::AxiomataError;
use crate::paths;

pub mod registry;
pub mod runlog;
pub mod runner;

// Curated facade so consumers (CLI, Tauri commands) don't bind to the internal
// module layout of `skills`.
pub use registry::{Skill, SkillSource, find_skill, list_skills, workspace_skills_dir};
pub use runlog::{RunRecord, RunStatus, list_runs};
pub use runner::{execute_skill, run_skill};

/// Name of the built-in example skill, seeded into the global skills directory
/// on first run.
pub const EXAMPLE_SKILL_NAME: &str = "example-skill";

/// The bundled `SKILL.md` for the example skill, embedded from the crate's own
/// `resources/` directory at build time (kept inside the crate so a clean
/// checkout compiles and the crate stays relocatable).
const EXAMPLE_SKILL_MD: &str = include_str!("../../resources/example-skill/SKILL.md");

/// Writes the built-in example skill into `~/.axiomata/skills/example-skill/`
/// if it isn't there yet.
///
/// Idempotent and non-destructive: an existing `SKILL.md` is left untouched, so
/// a user who edits or deletes it keeps their change on the next start.
///
/// Errors:
///     [`AxiomataError::Io`] if the directory or file cannot be created.
pub fn seed_example_skill() -> Result<(), AxiomataError> {
    let dir = paths::global_skills_dir().join(EXAMPLE_SKILL_NAME);
    let manifest = dir.join("SKILL.md");

    fs::create_dir_all(&dir).map_err(|source| AxiomataError::Io {
        path: dir.clone(),
        source,
    })?;

    // Create atomically: `create_new` fails if the file already exists, so
    // there is no exists()-then-write window a symlink could be swapped into.
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&manifest)
    {
        Ok(mut file) => file
            .write_all(EXAMPLE_SKILL_MD.as_bytes())
            .map_err(|source| AxiomataError::Io {
                path: manifest,
                source,
            }),
        Err(err) if err.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(source) => Err(AxiomataError::Io {
            path: manifest,
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;

    #[test]
    fn seed_creates_example_skill_once_and_preserves_edits() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-seed-home");
        fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }

        seed_example_skill().unwrap();
        let manifest = paths::global_skills_dir()
            .join(EXAMPLE_SKILL_NAME)
            .join("SKILL.md");
        assert!(manifest.is_file());
        let original = fs::read_to_string(&manifest).unwrap();
        assert!(original.contains("name: example-skill"));

        // A user edit must survive a second seed.
        fs::write(
            &manifest,
            "---\nname: example-skill\ndescription: mine\n---\n",
        )
        .unwrap();
        seed_example_skill().unwrap();
        assert_eq!(
            fs::read_to_string(&manifest).unwrap(),
            "---\nname: example-skill\ndescription: mine\n---\n"
        );

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }
}
