//! Scans the skills directory.
//!
//! Skills live in exactly one place: `~/.axiomata/skills/<name>/SKILL.md`.
//! They are application-level — always available, independent of which
//! Second-Brain workspace is active — and are managed only by the user (no
//! sync process writes there). The filesystem is the single source of truth;
//! skills are never mirrored into the database.
//!
//! Implemented in M1.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use gray_matter::Matter;
use gray_matter::engine::YAML;
use serde::{Deserialize, Serialize};

use crate::agents::BACKEND_CLAUDE_CODE;
use crate::error::AxiomataError;

/// The YAML frontmatter of a `SKILL.md`, as authored by the user.
#[derive(Debug, Clone, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    trigger: Option<String>,
    #[serde(default = "default_backend")]
    backend: String,
}

/// Default agent backend for a skill that doesn't name one.
fn default_backend() -> String {
    BACKEND_CLAUDE_CODE.to_owned()
}

/// A discovered skill: its frontmatter metadata, its instruction body, and
/// where it lives on disk.
#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    /// Skill identifier, from the frontmatter `name` field.
    pub name: String,
    /// One-line summary from the frontmatter `description` field.
    pub description: String,
    /// Optional model hint from the frontmatter.
    pub model: Option<String>,
    /// Optional effort hint from the frontmatter.
    pub effort: Option<String>,
    /// Optional trigger description from the frontmatter.
    pub trigger: Option<String>,
    /// Agent backend identifier (`"claude-code"` or `"ollama"`). Not validated
    /// here — [`crate::agents::AgentBackend::resolve`] checks it at run time.
    pub backend: String,
    /// Absolute path to the skill's `SKILL.md`.
    pub path: PathBuf,
    /// The Markdown body after the frontmatter — the skill's actual
    /// instructions. Used as the prompt for the Ollama backend.
    pub body: String,
}

/// Upper bound on the size of a `SKILL.md` we will read and parse, as a guard
/// against a hostile or accidental huge file (and YAML anchor-expansion blow-up
/// in the frontmatter). 256 KiB is far more than any real skill needs.
const MAX_SKILL_MD_BYTES: u64 = 256 * 1024;

/// Lists every skill under `~/.axiomata/skills/`, sorted by name.
///
/// An individual skill directory that is a symlink, or whose `SKILL.md` is
/// missing, a symlink, oversized, unreadable, or has malformed/missing
/// frontmatter, is **skipped** — one bad file never breaks discovery of the
/// rest. (Use [`find_skill`] to get the specific error for a named skill.)
///
/// Errors:
///     [`AxiomataError::Io`] only for a failure listing the skills directory
///     itself. A missing directory is treated as empty.
pub fn list_skills() -> Result<Vec<Skill>, AxiomataError> {
    let mut by_name: BTreeMap<String, Skill> = BTreeMap::new();
    for skill in scan_dir(&crate::paths::global_skills_dir())? {
        by_name.insert(skill.name.clone(), skill);
    }
    Ok(by_name.into_values().collect())
}

/// Finds a single skill by name.
///
/// Only the named skill's own `~/.axiomata/skills/<name>/SKILL.md` is read, so
/// an unrelated malformed skill file never blocks the lookup.
///
/// Errors:
///     [`AxiomataError::SkillNotFound`] if no such skill directory exists;
///     [`AxiomataError::InvalidSkill`] / [`AxiomataError::Io`] if the named
///     skill's own `SKILL.md` is malformed or unreadable.
pub fn find_skill(name: &str) -> Result<Skill, AxiomataError> {
    let manifest = crate::paths::global_skills_dir()
        .join(name)
        .join("SKILL.md");
    if !is_regular_file(&manifest) {
        return Err(AxiomataError::SkillNotFound {
            name: name.to_owned(),
        });
    }
    let skill = load_skill(&manifest)?;
    // Guard against a directory named `X` whose frontmatter names it `Y`.
    if skill.name != name {
        return Err(AxiomataError::SkillNotFound {
            name: name.to_owned(),
        });
    }
    Ok(skill)
}

/// Whether `path` is a regular file and **not** a symlink (the final component
/// is checked with `symlink_metadata`, so a symlinked `SKILL.md` is rejected).
fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_file())
        .unwrap_or(false)
}

/// Scans the skills directory for `<dir>/<name>/SKILL.md` entries. A missing
/// directory yields an empty list; symlinked entries and entries that fail to
/// load are skipped (see [`list_skills`]).
fn scan_dir(dir: &Path) -> Result<Vec<Skill>, AxiomataError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(AxiomataError::Io {
                path: dir.to_path_buf(),
                source: err,
            });
        }
    };

    let mut skills = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| AxiomataError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        // Skip symlinked skill directories outright.
        match entry.file_type() {
            Ok(ft) if ft.is_dir() && !ft.is_symlink() => {}
            _ => continue,
        }
        let manifest = entry.path().join("SKILL.md");
        if !is_regular_file(&manifest) {
            continue;
        }
        match load_skill(&manifest) {
            Ok(skill) => skills.push(skill),
            // One bad SKILL.md must not break discovery of the others.
            Err(_) => continue,
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// Reads and parses a single `SKILL.md`.
fn load_skill(manifest: &Path) -> Result<Skill, AxiomataError> {
    if fs::symlink_metadata(manifest)
        .map(|meta| meta.len())
        .unwrap_or(0)
        > MAX_SKILL_MD_BYTES
    {
        return Err(AxiomataError::InvalidSkill {
            path: manifest.to_path_buf(),
            reason: format!("SKILL.md is larger than the {MAX_SKILL_MD_BYTES}-byte limit"),
        });
    }

    let raw = fs::read_to_string(manifest).map_err(|err| AxiomataError::Io {
        path: manifest.to_path_buf(),
        source: err,
    })?;

    let matter: Matter<YAML> = Matter::new();
    let parsed =
        matter
            .parse::<SkillFrontmatter>(&raw)
            .map_err(|err| AxiomataError::InvalidSkill {
                path: manifest.to_path_buf(),
                reason: format!("malformed frontmatter: {err}"),
            })?;

    let frontmatter = parsed.data.ok_or_else(|| AxiomataError::InvalidSkill {
        path: manifest.to_path_buf(),
        reason: "missing YAML frontmatter block".to_owned(),
    })?;

    if frontmatter.name.trim().is_empty() {
        return Err(AxiomataError::InvalidSkill {
            path: manifest.to_path_buf(),
            reason: "frontmatter `name` is empty".to_owned(),
        });
    }

    Ok(Skill {
        name: frontmatter.name,
        description: frontmatter.description,
        model: frontmatter.model,
        effort: frontmatter.effort,
        trigger: frontmatter.trigger,
        backend: frontmatter.backend,
        path: manifest.to_path_buf(),
        body: parsed.content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;

    /// Writes `<root>/<name>/SKILL.md` with the given contents.
    fn write_skill(root: &Path, name: &str, contents: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), contents).unwrap();
    }

    /// A minimal valid `SKILL.md` with the given backend.
    fn skill_md(name: &str, backend: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: does {name} things\nbackend: {backend}\n---\n\nDo the {name}.\n"
        )
    }

    /// An isolated `AXIOMATA_HOME` for one test.
    struct TestHome {
        _guard: std::sync::MutexGuard<'static, ()>,
        home: PathBuf,
    }

    impl TestHome {
        fn new(tag: &str) -> Self {
            let guard = ENV_MUTEX.lock().unwrap();
            let home = unique_temp_dir(&format!("axiomata-test-{tag}-home"));
            fs::create_dir_all(&home).unwrap();
            // SAFETY: serialized by `guard`, see `paths::tests`.
            unsafe {
                env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
            }
            Self {
                _guard: guard,
                home,
            }
        }

        fn skills_dir(&self) -> PathBuf {
            crate::paths::global_skills_dir()
        }
    }

    impl Drop for TestHome {
        fn drop(&mut self) {
            // SAFETY: still holding `_guard`.
            unsafe {
                env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
            }
            let _ = fs::remove_dir_all(&self.home);
        }
    }

    #[test]
    fn missing_directory_yields_an_empty_list() {
        let _h = TestHome::new("empty");
        assert!(list_skills().unwrap().is_empty());
    }

    #[test]
    fn lists_skills_sorted_by_name_with_frontmatter_and_body() {
        let h = TestHome::new("list");
        write_skill(&h.skills_dir(), "triage", &skill_md("triage", "ollama"));
        write_skill(
            &h.skills_dir(),
            "cleanup",
            &skill_md("cleanup", "claude-code"),
        );

        let skills = list_skills().unwrap();
        let names: Vec<_> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["cleanup", "triage"]);

        let triage = skills.iter().find(|s| s.name == "triage").unwrap();
        assert_eq!(triage.backend, "ollama");
        assert_eq!(triage.body.trim(), "Do the triage.");
    }

    #[test]
    fn backend_defaults_to_claude_code_when_absent() {
        let h = TestHome::new("default-backend");
        write_skill(
            &h.skills_dir(),
            "plain",
            "---\nname: plain\ndescription: no backend field\n---\nbody\n",
        );
        assert_eq!(find_skill("plain").unwrap().backend, "claude-code");
    }

    #[test]
    fn find_skill_surfaces_the_error_for_a_malformed_named_skill() {
        let h = TestHome::new("malformed");
        // Missing the required `description` field.
        write_skill(&h.skills_dir(), "broken", "---\nname: broken\n---\nbody\n");
        assert!(matches!(
            find_skill("broken").unwrap_err(),
            AxiomataError::InvalidSkill { .. }
        ));
    }

    #[test]
    fn one_malformed_skill_does_not_break_listing_of_the_others() {
        let h = TestHome::new("malformed-skip");
        write_skill(&h.skills_dir(), "broken", "---\nname: broken\n---\nbody\n");
        write_skill(&h.skills_dir(), "bare", "no frontmatter at all\n");
        write_skill(&h.skills_dir(), "good", &skill_md("good", "ollama"));

        let names: Vec<_> = list_skills().unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["good"]);
        assert!(find_skill("good").is_ok());
    }

    #[test]
    fn oversized_skill_md_is_rejected() {
        let h = TestHome::new("oversized");
        let huge = format!(
            "---\nname: huge\ndescription: x\n---\n{}",
            "a".repeat((MAX_SKILL_MD_BYTES as usize) + 1)
        );
        write_skill(&h.skills_dir(), "huge", &huge);
        assert!(list_skills().unwrap().is_empty());
        assert!(matches!(
            find_skill("huge").unwrap_err(),
            AxiomataError::InvalidSkill { .. }
        ));
    }

    #[test]
    fn symlinked_skill_md_is_skipped() {
        let h = TestHome::new("symlink");
        write_skill(&h.skills_dir(), "real", &skill_md("real", "ollama"));
        let target = h.skills_dir().join("real").join("SKILL.md");
        let link_dir = h.skills_dir().join("linked");
        fs::create_dir_all(&link_dir).unwrap();
        std::os::unix::fs::symlink(&target, link_dir.join("SKILL.md")).unwrap();

        let names: Vec<_> = list_skills().unwrap().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["real"]);
    }

    #[test]
    fn find_skill_reports_not_found() {
        let _h = TestHome::new("not-found");
        assert!(matches!(
            find_skill("ghost").unwrap_err(),
            AxiomataError::SkillNotFound { name } if name == "ghost"
        ));
    }
}
