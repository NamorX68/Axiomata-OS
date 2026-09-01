//! Scans and merges the global and workspace-local skill directories.
//!
//! Two locations are read (see `docs/architecture.md` §4):
//!
//! - `~/.axiomata/skills/<name>/SKILL.md` — global, always available.
//! - `<workspace_root>/.claude/skills/<name>/SKILL.md` — bound to the current
//!   Second Brain; wins over a global skill of the same name.
//!
//! The filesystem is the single source of truth — skills are never mirrored
//! into the database.
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
use crate::config::Config;
use crate::error::AxiomataError;

/// Which directory a skill was discovered in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillSource {
    /// `~/.axiomata/skills/` — global, always available.
    Global,
    /// `<workspace_root>/.claude/skills/` — bound to the active Second Brain;
    /// takes precedence over a global skill with the same name.
    Workspace,
}

impl SkillSource {
    /// The lowercase string form — the single source of truth for this
    /// mapping, matching what `#[serde(rename_all = "lowercase")]` emits and
    /// what the `runs` table stores.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Workspace => "workspace",
        }
    }
}

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
    /// Whether the skill is global or workspace-local.
    pub source: SkillSource,
    /// Absolute path to the skill's `SKILL.md`.
    pub path: PathBuf,
    /// The Markdown body after the frontmatter — the skill's actual
    /// instructions. Used as the prompt for the Ollama backend.
    pub body: String,
}

/// Returns the workspace-local skills directory for `config`
/// (`<workspace_root>/.claude/skills`).
pub fn workspace_skills_dir(config: &Config) -> PathBuf {
    config.workspace_root.join(".claude").join("skills")
}

/// Upper bound on the size of a `SKILL.md` we will read and parse, as a guard
/// against a hostile or accidental huge file (and YAML anchor-expansion blow-up
/// in the frontmatter). 256 KiB is far more than any real skill needs.
const MAX_SKILL_MD_BYTES: u64 = 256 * 1024;

/// Lists every skill from both locations, merged by name with the
/// workspace-local copy winning on a collision.
///
/// The result is sorted by name for a stable order. An individual skill
/// directory that is a symlink, or whose `SKILL.md` is missing, a symlink,
/// oversized, unreadable, or has malformed/missing frontmatter, is **skipped**
/// — one bad file never breaks discovery of the rest. (Use [`find_skill`] to
/// get the specific error for a named skill.)
///
/// Args:
///     config: Supplies `workspace_root` for the workspace-local location.
///
/// Returns:
///     All valid, discovered skills, de-duplicated by name.
///
/// Errors:
///     [`AxiomataError::Io`] only for a failure listing a skills directory
///     itself. A location that doesn't exist is treated as empty.
pub fn list_skills(config: &Config) -> Result<Vec<Skill>, AxiomataError> {
    let mut by_name: BTreeMap<String, Skill> = BTreeMap::new();

    // Global first, then let workspace-local skills overwrite on name collision.
    for skill in scan_dir(&crate::paths::global_skills_dir(), SkillSource::Global)? {
        by_name.insert(skill.name.clone(), skill);
    }
    for skill in scan_dir(&workspace_skills_dir(config), SkillSource::Workspace)? {
        by_name.insert(skill.name.clone(), skill);
    }

    Ok(by_name.into_values().collect())
}

/// Finds a single skill by name, applying the same merge rules as
/// [`list_skills`] (workspace-local wins).
///
/// Only the named skill's own `<dir>/<name>/SKILL.md` is read, so an unrelated
/// malformed skill file elsewhere never blocks the lookup.
///
/// Errors:
///     [`AxiomataError::SkillNotFound`] if no such skill directory exists in
///     either location; [`AxiomataError::InvalidSkill`] / [`AxiomataError::Io`]
///     if the named skill's own `SKILL.md` is malformed or unreadable.
pub fn find_skill(name: &str, config: &Config) -> Result<Skill, AxiomataError> {
    for (dir, source) in [
        (workspace_skills_dir(config), SkillSource::Workspace),
        (crate::paths::global_skills_dir(), SkillSource::Global),
    ] {
        let manifest = dir.join(name).join("SKILL.md");
        if !is_regular_file(&manifest) {
            continue;
        }
        let skill = load_skill(&manifest, source)?;
        // Guard against a directory named `X` whose frontmatter names it `Y`.
        if skill.name == name {
            return Ok(skill);
        }
    }
    Err(AxiomataError::SkillNotFound {
        name: name.to_owned(),
    })
}

/// Whether `path` is a regular file and **not** a symlink (the final component
/// is checked with `symlink_metadata`, so a symlinked `SKILL.md` is rejected).
fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_file())
        .unwrap_or(false)
}

/// Scans one skills directory for `<dir>/<name>/SKILL.md` entries. A missing
/// directory yields an empty list; symlinked entries and entries that fail to
/// load are skipped (see [`list_skills`]).
fn scan_dir(dir: &Path, source: SkillSource) -> Result<Vec<Skill>, AxiomataError> {
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
        // Skip symlinked skill directories outright — a synced/untrusted
        // workspace could point one at an arbitrary location.
        match entry.file_type() {
            Ok(ft) if ft.is_dir() && !ft.is_symlink() => {}
            _ => continue,
        }
        let manifest = entry.path().join("SKILL.md");
        if !is_regular_file(&manifest) {
            continue;
        }
        match load_skill(&manifest, source) {
            Ok(skill) => skills.push(skill),
            // One bad SKILL.md must not break discovery of the others.
            Err(_) => continue,
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

/// Reads and parses a single `SKILL.md`.
fn load_skill(manifest: &Path, source: SkillSource) -> Result<Skill, AxiomataError> {
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
        source,
        path: manifest.to_path_buf(),
        body: parsed.content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};
    use std::env;

    /// Writes `<root>/<name>/SKILL.md` with the given contents.
    fn write_skill(root: &Path, name: &str, contents: &str) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), contents).unwrap();
    }

    /// A minimal valid `SKILL.md` body.
    fn skill_md(name: &str, backend: &str) -> String {
        format!(
            "---\nname: {name}\ndescription: does {name} things\nbackend: {backend}\n---\n\nDo the {name}.\n"
        )
    }

    struct TestDirs {
        _guard: std::sync::MutexGuard<'static, ()>,
        home: PathBuf,
        config: Config,
    }

    impl TestDirs {
        /// Sets up an isolated `AXIOMATA_HOME` and a scratch workspace root.
        fn new(tag: &str) -> Self {
            let guard = ENV_MUTEX.lock().unwrap();
            let home = unique_temp_dir(&format!("axiomata-test-{tag}-home"));
            let workspace = unique_temp_dir(&format!("axiomata-test-{tag}-ws"));
            fs::create_dir_all(&home).unwrap();
            fs::create_dir_all(&workspace).unwrap();
            // SAFETY: serialized by `guard`, see `paths::tests`.
            unsafe {
                env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
            }
            let config = Config {
                workspace_root: workspace,
                ..Config::default()
            };
            Self {
                _guard: guard,
                home,
                config,
            }
        }

        fn global_dir(&self) -> PathBuf {
            crate::paths::global_skills_dir()
        }

        fn workspace_dir(&self) -> PathBuf {
            workspace_skills_dir(&self.config)
        }
    }

    impl Drop for TestDirs {
        fn drop(&mut self) {
            // SAFETY: still holding `_guard`.
            unsafe {
                env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
            }
            let _ = fs::remove_dir_all(&self.home);
            let _ = fs::remove_dir_all(&self.config.workspace_root);
        }
    }

    #[test]
    fn missing_directories_yield_an_empty_list() {
        let dirs = TestDirs::new("empty");
        let skills = list_skills(&dirs.config).unwrap();
        assert!(skills.is_empty());
    }

    #[test]
    fn merges_both_locations_and_reports_source() {
        let dirs = TestDirs::new("merge");
        write_skill(
            &dirs.global_dir(),
            "cleanup",
            &skill_md("cleanup", "claude-code"),
        );
        write_skill(
            &dirs.workspace_dir(),
            "triage",
            &skill_md("triage", "ollama"),
        );

        let skills = list_skills(&dirs.config).unwrap();
        let names: Vec<_> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["cleanup", "triage"]);

        let cleanup = skills.iter().find(|s| s.name == "cleanup").unwrap();
        assert_eq!(cleanup.source, SkillSource::Global);
        assert_eq!(cleanup.backend, "claude-code");

        let triage = skills.iter().find(|s| s.name == "triage").unwrap();
        assert_eq!(triage.source, SkillSource::Workspace);
        assert_eq!(triage.backend, "ollama");
        assert_eq!(triage.body.trim(), "Do the triage.");
    }

    #[test]
    fn workspace_local_skill_wins_on_name_collision() {
        let dirs = TestDirs::new("collision");
        write_skill(
            &dirs.global_dir(),
            "notes",
            &skill_md("notes", "claude-code"),
        );
        write_skill(&dirs.workspace_dir(), "notes", &skill_md("notes", "ollama"));

        let skills = list_skills(&dirs.config).unwrap();
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].source, SkillSource::Workspace);
        assert_eq!(skills[0].backend, "ollama");

        let found = find_skill("notes", &dirs.config).unwrap();
        assert_eq!(found.source, SkillSource::Workspace);
    }

    #[test]
    fn backend_defaults_to_claude_code_when_absent() {
        let dirs = TestDirs::new("default-backend");
        write_skill(
            &dirs.global_dir(),
            "plain",
            "---\nname: plain\ndescription: no backend field\n---\nbody\n",
        );
        let skill = find_skill("plain", &dirs.config).unwrap();
        assert_eq!(skill.backend, "claude-code");
    }

    #[test]
    fn find_skill_surfaces_the_error_for_a_malformed_named_skill() {
        let dirs = TestDirs::new("malformed");
        // Missing the required `description` field.
        write_skill(
            &dirs.global_dir(),
            "broken",
            "---\nname: broken\n---\nbody\n",
        );
        let err = find_skill("broken", &dirs.config).unwrap_err();
        assert!(matches!(err, AxiomataError::InvalidSkill { .. }));
    }

    #[test]
    fn one_malformed_skill_does_not_break_listing_of_the_others() {
        let dirs = TestDirs::new("malformed-skip");
        write_skill(
            &dirs.global_dir(),
            "broken",
            "---\nname: broken\n---\nbody\n",
        );
        write_skill(&dirs.global_dir(), "bare", "no frontmatter at all\n");
        write_skill(&dirs.global_dir(), "good", &skill_md("good", "ollama"));

        // The listing skips the two bad ones and still returns the valid skill.
        let skills = list_skills(&dirs.config).unwrap();
        let names: Vec<_> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["good"]);
        assert!(find_skill("good", &dirs.config).is_ok());
    }

    #[test]
    fn oversized_skill_md_is_rejected() {
        let dirs = TestDirs::new("oversized");
        let huge = format!(
            "---\nname: huge\ndescription: x\n---\n{}",
            "a".repeat((MAX_SKILL_MD_BYTES as usize) + 1)
        );
        write_skill(&dirs.global_dir(), "huge", &huge);
        assert!(list_skills(&dirs.config).unwrap().is_empty());
        assert!(matches!(
            find_skill("huge", &dirs.config).unwrap_err(),
            AxiomataError::InvalidSkill { .. }
        ));
    }

    #[test]
    fn symlinked_skill_md_is_skipped() {
        let dirs = TestDirs::new("symlink");
        // A real skill the symlink will point at.
        write_skill(&dirs.global_dir(), "real", &skill_md("real", "ollama"));
        let target = dirs.global_dir().join("real").join("SKILL.md");
        let link_dir = dirs.global_dir().join("linked");
        fs::create_dir_all(&link_dir).unwrap();
        std::os::unix::fs::symlink(&target, link_dir.join("SKILL.md")).unwrap();

        let names: Vec<_> = list_skills(&dirs.config)
            .unwrap()
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(names, ["real"]);
    }

    #[test]
    fn find_skill_reports_not_found() {
        let dirs = TestDirs::new("not-found");
        let err = find_skill("ghost", &dirs.config).unwrap_err();
        assert!(matches!(
            err,
            AxiomataError::SkillNotFound { name } if name == "ghost"
        ));
    }
}
