//! Tags and links in the vault, the way Obsidian has them (`docs/plans/tags-links.md`, card #75).
//!
//! - [`detect`]: which `#tag`s and `tags:` a note has (Q1), and where its `[[links]]` are.
//! - [`lift`]: moving a note's inline `#tags` into its frontmatter, once, for the owner (concept note Q7).
//! - [`list`]: the one tag list, `<workspace>/TAGS.md` (Q6, V1), written only by the app.
//! - [`note`]: setting a note's `tags:` and its `## Related` links (V2, Q8) without touching the rest of it.
//! - [`proposals`]: a tag an agent wants that is not on the list waits for the owner (Q10).
//! - [`rename`]: renaming a note pulls the `[[links]]` to it along (V3); renaming a tag pulls its uses along (TL4).
//! - [`vault_run`]: the one run over the whole vault — a proposed list, then a preview, then the owner writes (Q9).
//!
//! Every read and write of a vault file goes through the workspace's strict guard (`axiomata-files`, no link
//! followed, nothing outside the workspace), and every write is checked against the version just read, so a note the
//! owner saved in between is not overwritten.

pub mod detect;
pub mod lift;
pub mod list;
pub mod note;
pub mod proposals;
pub mod rename;
pub mod vault_run;

use std::collections::HashMap;

use axiomata_files::{self as files, FilesError, Version};

use crate::config::Config;
use crate::error::AxiomataError;
use crate::graph::{file_stem, normalise_rel};
use crate::workspace::{MAX_FILE_BYTES, from_files, workspace_root};

/// The tag list's file, in the workspace root beside `AGENTS.md` (V1).
pub const TAGS_FILE: &str = "TAGS.md";

/// Files a harness reads as instructions or configuration, compared without case: an agent's edit there would speak
/// to every later agent, so no tag or link is ever written into one. The tag list is the app's own.
const PROTECTED_FILES: [&str; 6] = [
    "AGENTS.md",
    "CLAUDE.md",
    "AGENT.md",
    "GEMINI.md",
    "CLAUDE.local.md",
    TAGS_FILE,
];

/// Whether `rel` names one of the [`PROTECTED_FILES`] (by its file name, without case): such a file is no topic, so it
/// is never an agent's related note either — the tag list was offered as one (owner, 2026-10-29).
pub fn is_protected(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    PROTECTED_FILES.iter().any(|known| same_name(known, name))
}

/// Top-level folders whose files the app rewrites from its own data (`Kanban/`, the board mirror): a tag written
/// there would be gone at the next change.
const APP_FOLDERS: [&str; 1] = ["Kanban"];

pub(crate) fn refused(reason: impl Into<String>) -> AxiomataError {
    AxiomataError::Tags {
        reason: reason.into(),
    }
}

/// Whether APFS takes `a` and `b` for one name (Unicode case folding, as `inbox move` compares).
fn same_name(a: &str, b: &str) -> bool {
    let fold = |text: &str| text.to_uppercase().to_lowercase();
    fold(a) == fold(b)
}

/// `rel` as a note the app may set tags and links in: a workspace-relative `.md` path with forward slashes, no `..`,
/// no hidden part, not an instruction file or the tag list, not in a folder the app rewrites itself. Returns it with
/// `\` turned into `/` and a leading `./` dropped.
///
/// # Errors
///
/// When `rel` is none of that. Whether the file exists and is no link is the guard's to say, at the read.
pub fn note_path(rel: &str) -> Result<String, AxiomataError> {
    let cleaned = rel.trim().replace('\\', "/");
    let cleaned = cleaned.strip_prefix("./").unwrap_or(&cleaned).to_owned();
    if cleaned.is_empty() || cleaned.starts_with('/') {
        return Err(refused(format!("{rel:?} is no note path inside the vault")));
    }
    let parts: Vec<&str> = cleaned.split('/').collect();
    if parts
        .iter()
        .any(|part| part.is_empty() || *part == "." || *part == ".." || part.starts_with('.'))
    {
        return Err(refused(format!(
            "{rel:?} must be a path below the vault without `..`, `.` or hidden folders"
        )));
    }
    let name = parts.last().copied().unwrap_or_default();
    if !name.to_lowercase().ends_with(".md") {
        return Err(refused(format!("{rel:?} is no Markdown note")));
    }
    if PROTECTED_FILES.iter().any(|known| same_name(known, name)) {
        return Err(refused(format!(
            "{name} is read as instructions or kept by the app; no tag or link goes into it"
        )));
    }
    if parts.len() > 1 && APP_FOLDERS.iter().any(|folder| same_name(folder, parts[0])) {
        return Err(refused(format!(
            "{}/ is rewritten by the app; a tag there would not stay",
            parts[0]
        )));
    }
    Ok(cleaned)
}

/// A file of the workspace as read through the guard: its text and the version a write is checked against.
pub(crate) struct ReadFile {
    pub content: String,
    pub version: Version,
}

/// Reading and writing vault files through the workspace's strict guard.
pub(crate) mod workspace {
    use super::*;

    /// `rel` as text, or `None` when there is no such file.
    pub fn read(config: &Config, rel: &str) -> Result<Option<ReadFile>, AxiomataError> {
        let root = workspace_root(config)?;
        match files::read_text(&root, rel, MAX_FILE_BYTES) {
            Ok(file) => Ok(Some(ReadFile {
                content: file.content,
                version: file.version,
            })),
            Err(FilesError::NotFound { .. }) => Ok(None),
            Err(err) => Err(from_files(err)),
        }
    }

    /// Writes `content` to `rel` atomically; with `expected`, only while the file is still at that version.
    pub fn write(
        config: &Config,
        rel: &str,
        content: &str,
        expected: Option<&Version>,
    ) -> Result<(), AxiomataError> {
        let root = workspace_root(config)?;
        match files::write_text(&root, rel, content, expected, MAX_FILE_BYTES) {
            Ok(_) => Ok(()),
            Err(FilesError::Conflict { .. }) => Err(refused(format!(
                "{rel} changed while it was being edited; nothing was written, try again"
            ))),
            Err(err) => Err(from_files(err)),
        }
    }
}

/// The vault's Markdown notes, to resolve a `[[link]]` the way the graph does (`graph::extract_links`): a path
/// relative to the linking note's folder or to the root, with or without `.md`, else a file name that only one note
/// has. Paths are compared without case, as APFS finds files.
pub struct Vault {
    by_path: HashMap<String, String>,
    by_stem: HashMap<String, Vec<String>>,
}

impl Vault {
    /// The notes of `config`'s workspace (the walk the router and the graph use).
    ///
    /// # Errors
    ///
    /// The workspace cannot be walked.
    pub fn load(config: &Config) -> Result<Self, AxiomataError> {
        Ok(Self::from_paths(crate::memory::walker::file_paths(config)?))
    }

    /// The notes among `paths` (workspace-relative, forward slashes).
    pub fn from_paths(paths: impl IntoIterator<Item = String>) -> Self {
        let mut by_path = HashMap::new();
        let mut by_stem: HashMap<String, Vec<String>> = HashMap::new();
        for path in paths {
            if !path.to_lowercase().ends_with(".md") {
                continue;
            }
            by_stem
                .entry(file_stem(&path).to_lowercase())
                .or_default()
                .push(path.clone());
            by_path.insert(path.to_lowercase(), path);
        }
        Self { by_path, by_stem }
    }

    /// Every note's path.
    pub fn notes(&self) -> impl Iterator<Item = &String> {
        self.by_path.values()
    }

    /// Whether `rel` is a note of the vault.
    pub fn contains(&self, rel: &str) -> bool {
        self.by_path.contains_key(&rel.to_lowercase())
    }

    /// The note a link `target` written in the note `from` points to, if exactly one.
    pub fn resolve(&self, from: &str, target: &str) -> Option<String> {
        let target = target.trim();
        if target.is_empty() {
            return None;
        }
        let folder = from.rsplit_once('/').map(|(dir, _)| dir).unwrap_or("");
        for candidate in [target.to_owned(), format!("{target}.md")] {
            for path in [
                normalise_rel(folder, &candidate),
                normalise_rel("", &candidate),
            ] {
                if let Some(found) = self.by_path.get(&path.to_lowercase()) {
                    return Some(found.clone());
                }
            }
        }
        if target.contains('/') {
            return None;
        }
        match self
            .by_stem
            .get(&file_stem(target).to_lowercase())
            .map(Vec::as_slice)
        {
            Some([only]) => Some(only.clone()),
            _ => None,
        }
    }

    /// How a link to `rel` is written: its file name when no other note has it (Obsidian's shortest form), else its
    /// path; either without `.md`.
    pub fn link_name(&self, rel: &str) -> String {
        let stem = file_stem(rel);
        let unique = self
            .by_stem
            .get(&stem.to_lowercase())
            .is_some_and(|all| all.len() <= 1);
        if unique {
            stem
        } else {
            strip_md(rel).to_owned()
        }
    }
}

/// `rel` without a trailing `.md` (any case).
pub(crate) fn strip_md(rel: &str) -> &str {
    if rel.to_lowercase().ends_with(".md") {
        &rel[..rel.len() - 3]
    } else {
        rel
    }
}

/// The line break a text uses: `\r\n` when its first line ends so, else `\n`.
pub(crate) fn newline_of(text: &str) -> &'static str {
    match text.find('\n') {
        Some(at) if at > 0 && text.as_bytes()[at - 1] == b'\r' => "\r\n",
        _ => "\n",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_path_is_a_plain_markdown_file_below_the_vault() {
        assert_eq!(note_path("Arbeit/x.md").unwrap(), "Arbeit/x.md");
        assert_eq!(note_path("./a\\b.MD").unwrap(), "a/b.MD");
        for bad in [
            "",
            "/abs.md",
            "../x.md",
            "a/../x.md",
            ".hidden/x.md",
            "a/.x.md",
            "x.txt",
            "AGENTS.md",
            "Area/claude.md",
            "TAGS.md",
            "kanban/1-board.md",
            "a//b.md",
        ] {
            assert!(note_path(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn the_vault_resolves_paths_and_unique_names() {
        let vault = Vault::from_paths(
            [
                "Dev/Rust lernen.md",
                "Dev/Cargo.md",
                "Arbeit/Dup.md",
                "KI/Dup.md",
                "pic.png",
            ]
            .map(String::from),
        );
        assert_eq!(
            vault.resolve("Dev/x.md", "rust lernen").as_deref(),
            Some("Dev/Rust lernen.md")
        );
        assert_eq!(
            vault.resolve("Dev/x.md", "Cargo.md").as_deref(),
            Some("Dev/Cargo.md")
        );
        assert_eq!(
            vault.resolve("x.md", "KI/Dup").as_deref(),
            Some("KI/Dup.md")
        );
        assert_eq!(
            vault.resolve("x.md", "Dup"),
            None,
            "two notes are called Dup"
        );
        assert_eq!(vault.resolve("x.md", "pic"), None, "no note");
        assert_eq!(vault.resolve("x.md", "Nope/Cargo"), None);
        assert_eq!(vault.link_name("Dev/Cargo.md"), "Cargo");
        assert_eq!(vault.link_name("KI/Dup.md"), "KI/Dup");
    }
}
