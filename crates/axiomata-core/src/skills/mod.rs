//! Skill discovery and headless execution.
//!
//! Skills are application-level: each lives in its own directory under
//! `~/.axiomata/skills/<name>/` with a `SKILL.md` manifest. There is no
//! workspace-local skill location.
//!
//! Implemented starting in M1.

use std::fs;
use std::io::{ErrorKind, Write};

use crate::error::AxiomataError;
use crate::paths;

pub mod model;
pub mod registry;
pub mod runlog;
pub mod runner;

// Curated facade so consumers (CLI, Tauri commands) don't bind to the internal
// module layout of `skills`.
pub use model::{RunRecord, RunSource, RunStatus, RunSummary};
pub use registry::{Skill, SkippedSkill, find_skill, list_skills, list_skipped_skills};
pub use runlog::{get_run, list_runs};
pub use runner::{execute_and_record_skill, execute_prompt, execute_skill};

/// Name of the built-in example skill. No longer seeded automatically (see
/// [`seed_example_skill`]'s doc comment) — kept only so the function/constant
/// stay meaningful and independently testable.
pub const EXAMPLE_SKILL_NAME: &str = "example-skill";

/// The bundled `SKILL.md` for the example skill, embedded from the crate's own
/// `resources/` directory at build time (kept inside the crate so a clean
/// checkout compiles and the crate stays relocatable).
const EXAMPLE_SKILL_MD: &str = include_str!("../../resources/example-skill/SKILL.md");

/// The four skills bundled with the app and seeded into
/// `~/.axiomata/skills/` on first run (or after a clean reinstall) — the
/// owner's actual working skill set, not the `example-skill` smoke test.
const DEFAULT_SKILLS: &[(&str, &str)] = &[
    (
        "calendar-digest",
        include_str!("../../resources/calendar-digest/SKILL.md"),
    ),
    (
        "mail-digest",
        include_str!("../../resources/mail-digest/SKILL.md"),
    ),
    (
        "reminders-digest",
        include_str!("../../resources/reminders-digest/SKILL.md"),
    ),
    ("cleanup", include_str!("../../resources/cleanup/SKILL.md")),
];

/// Writes `content` into `~/.axiomata/skills/<name>/SKILL.md` if it isn't
/// there yet.
///
/// Idempotent and non-destructive: an existing `SKILL.md` — a user's own
/// edit, or a previous seed — is left untouched, so nothing here ever
/// overwrites a change made after the fact.
///
/// Errors:
///     [`AxiomataError::Io`] if the directory or file cannot be created.
fn seed_skill(name: &str, content: &str) -> Result<(), AxiomataError> {
    let dir = paths::global_skills_dir().join(name);
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
            .write_all(content.as_bytes())
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

/// Writes the built-in example skill into `~/.axiomata/skills/example-skill/`
/// if it isn't there yet. **Not called by [`crate::AxiomataCore::init`]**
/// (the owner had real skills and considered this smoke-test skill clutter)
/// — kept as a standalone, independently testable function in case it's
/// useful again later (e.g. a future "restore the example skill" CLI command).
///
/// Errors:
///     [`AxiomataError::Io`] if the directory or file cannot be created.
pub fn seed_example_skill() -> Result<(), AxiomataError> {
    seed_skill(EXAMPLE_SKILL_NAME, EXAMPLE_SKILL_MD)
}

/// Writes every skill in [`DEFAULT_SKILLS`] into `~/.axiomata/skills/`, each
/// only if it doesn't already exist there. Called from
/// [`crate::AxiomataCore::init`] on every start — cheap and a no-op once all
/// four exist, so a user's own edits (or a deleted skill they don't want
/// back) always survive.
///
/// Errors:
///     [`AxiomataError::Io`] if any skill's directory or file cannot be
///     created; aborts at the first failure (the skills already written stay put).
pub fn seed_default_skills() -> Result<(), AxiomataError> {
    for (name, content) in DEFAULT_SKILLS {
        seed_skill(name, content)?;
    }
    Ok(())
}

/// Writes every skill in [`DEFAULT_SKILLS`] into `~/.axiomata/skills/`,
/// optionally overwriting copies that already exist.
///
/// Without `force` this is exactly [`seed_default_skills`] (seed-if-absent —
/// a user's own edits always survive). With `force` the bundled skills are
/// re-copied from `resources/`, replacing any current `SKILL.md`: the escape
/// hatch for the seed's seed-if-absent gotcha — an edit to a bundled
/// skill's `resources/SKILL.md` does **not** reach an install whose
/// `~/.axiomata/skills/<name>/SKILL.md` already exists. Skills outside
/// [`DEFAULT_SKILLS`] (the user's own) are never touched.
///
/// Errors:
///     [`AxiomataError::Io`] if any skill's file cannot be created or
///     overwritten; aborts at the first failure (the skills already written stay put).
pub fn reseed_default_skills(force: bool) -> Result<(), AxiomataError> {
    if !force {
        return seed_default_skills();
    }
    for (name, content) in DEFAULT_SKILLS {
        write_skill(name, content)?;
    }
    Ok(())
}

/// Overwrites `~/.axiomata/skills/<name>/SKILL.md` with `content`, atomically.
///
/// The overwrite writes an `O_EXCL` temp sibling then renames it over the
/// target, so a crash mid-write can't leave a truncated manifest behind and a
/// symlink planted at the predictable temp path is never followed. Only the
/// named skill is touched.
fn write_skill(name: &str, content: &str) -> Result<(), AxiomataError> {
    let dir = paths::global_skills_dir().join(name);
    let manifest = dir.join("SKILL.md");

    fs::create_dir_all(&dir).map_err(|source| AxiomataError::Io {
        path: dir.clone(),
        source,
    })?;

    let tmp = dir.join("SKILL.md.tmp");
    let write_result = {
        // `create_new` = O_CREAT|O_EXCL, matching the workspace write path: a
        // leftover temp from a crashed write, or a planted symlink, fails the
        // open rather than being clobbered/followed.
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
            .map_err(|source| AxiomataError::Io {
                path: tmp.clone(),
                source,
            })?;
        file.write_all(content.as_bytes())
            .map_err(|source| AxiomataError::Io {
                path: tmp.clone(),
                source,
            })
    };
    if let Err(err) = write_result {
        // A failed write must not leave `SKILL.md.tmp` behind — it would
        // wedge every future forced reseed on the `create_new` open above.
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    fs::rename(&tmp, &manifest).map_err(|source| {
        let _ = fs::remove_file(&tmp);
        AxiomataError::Io {
            path: manifest,
            source,
        }
    })?;
    Ok(())
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

    #[test]
    fn seed_default_skills_creates_all_four_valid_and_parseable() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-seed-defaults-home");
        fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }

        seed_default_skills().unwrap();

        // Every bundled skill landed on disk with the right name inside...
        for (name, _) in DEFAULT_SKILLS {
            let manifest = paths::global_skills_dir().join(name).join("SKILL.md");
            assert!(manifest.is_file(), "{name} was not seeded");
            let content = fs::read_to_string(&manifest).unwrap();
            assert!(
                content.contains(&format!("name: {name}")),
                "{name}'s seeded content doesn't declare its own name"
            );
        }
        // ...and every one of them is actually valid, parseable frontmatter —
        // catches a YAML mistake in a bundled resource (e.g. an unquoted
        // colon inside `description:`) before it ever ships, not just that
        // *a* file got written.
        let found: Vec<String> = registry::list_skills()
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        for (name, _) in DEFAULT_SKILLS {
            assert!(
                found.contains(&name.to_string()),
                "{name} failed to parse via list_skills (found: {found:?})"
            );
        }

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn seed_default_skills_is_idempotent_and_preserves_a_hand_edit() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-seed-defaults-idempotent-home");
        fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }

        seed_default_skills().unwrap();

        // The owner deletes one skill entirely. `seed_skill` can only ever
        // see "the file is absent" — it has no way to distinguish "never
        // created" from "deliberately deleted", so a second seed *does*
        // recreate it. That's a real, known limitation (the same reason
        // `example-skill`'s own seeding was dropped from `init` entirely
        // rather than left running) — asserted here so a future change
        // that silently "fixes" this either does so on purpose or updates
        // this test, not by accident.
        let cleanup_dir = paths::global_skills_dir().join("cleanup");
        fs::remove_dir_all(&cleanup_dir).unwrap();

        // The owner hand-edits another one instead of deleting it.
        let mail_manifest = paths::global_skills_dir()
            .join("mail-digest")
            .join("SKILL.md");
        fs::write(
            &mail_manifest,
            "---\nname: mail-digest\ndescription: mine\n---\n",
        )
        .unwrap();

        // A second seed (simulating the next app start).
        seed_default_skills().unwrap();
        assert!(
            cleanup_dir.join("SKILL.md").is_file(),
            "a deleted default skill comes back — expected, see comment above"
        );
        assert_eq!(
            fs::read_to_string(&mail_manifest).unwrap(),
            "---\nname: mail-digest\ndescription: mine\n---\n",
            "a hand-edited default skill must never be overwritten"
        );

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn reseed_without_force_preserves_a_hand_edit() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-reseed-noop-home");
        fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }

        seed_default_skills().unwrap();
        // The owner tuned a bundled skill's SOP by hand.
        let mail_manifest = paths::global_skills_dir()
            .join("mail-digest")
            .join("SKILL.md");
        let edit = "---\nname: mail-digest\ndescription: mine\n---\n";
        fs::write(&mail_manifest, edit).unwrap();

        // Without --force the reseed is seed-if-absent: the edit survives.
        reseed_default_skills(false).unwrap();
        assert_eq!(fs::read_to_string(&mail_manifest).unwrap(), edit);

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn reseed_with_force_restores_bundled_skills_but_leaves_user_skills_alone() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-reseed-force-home");
        fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }

        seed_default_skills().unwrap();
        // Mess with a bundled skill and create one of the user's own.
        let mail_manifest = paths::global_skills_dir()
            .join("mail-digest")
            .join("SKILL.md");
        fs::write(
            &mail_manifest,
            "---\nname: mail-digest\ndescription: hacked\n---\n",
        )
        .unwrap();
        let user_dir = paths::global_skills_dir().join("mine");
        fs::create_dir_all(&user_dir).unwrap();
        let user_manifest = user_dir.join("SKILL.md");
        let user_content = "---\nname: mine\ndescription: my own\n---\n";
        fs::write(&user_manifest, user_content).unwrap();

        reseed_default_skills(true).unwrap();

        // The bundled skill is back to the resources copy (byte-identical)…
        let (_, wanted_md) = DEFAULT_SKILLS
            .iter()
            .find(|(name, _)| *name == "mail-digest")
            .unwrap();
        assert_eq!(fs::read_to_string(&mail_manifest).unwrap(), *wanted_md);
        // …and the user-owned skill is untouched.
        assert_eq!(fs::read_to_string(&user_manifest).unwrap(), user_content);

        unsafe {
            env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = fs::remove_dir_all(&home);
    }
}
