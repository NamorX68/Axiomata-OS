//! Walks the workspace root and groups files by top-level area.
//!
//! Uses the `ignore` crate, so `.gitignore` rules and hidden entries (`.git/`,
//! `.claude/`, dotfiles) are skipped automatically. The generated `CLAUDE.md`
//! router files are skipped too — they are output, not content.
//!
//! Implemented in M2.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::SystemTime;

use gray_matter::Matter;
use gray_matter::engine::YAML;
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::AxiomataError;

/// How many bytes of a Markdown file we read when looking for its title.
const TITLE_SCAN_BYTES: usize = 8 * 1024;

/// One tracked file in the workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// Path relative to the workspace root, always forward-slashed.
    pub rel_path: String,
    /// Title for a Markdown file — its frontmatter `title:` or first `# `
    /// heading. `None` for non-Markdown files, or Markdown without either.
    pub title: Option<String>,
}

/// The workspace grouped by top-level area, for the router renderer.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct WorkspaceTree {
    /// Files directly in the workspace root (not inside any subdirectory).
    pub loose: Vec<FileEntry>,
    /// Top-level subdirectory name -> its files (nested files flattened).
    pub areas: BTreeMap<String, Vec<FileEntry>>,
}

/// A full scan: the grouped tree plus the total file count.
#[derive(Debug)]
pub struct WorkspaceScan {
    pub tree: WorkspaceTree,
    /// Total number of tracked files.
    pub file_count: usize,
}

/// The cheap subset of a scan: no file contents are read.
#[derive(Debug)]
pub struct Freshness {
    pub newest_mtime: Option<SystemTime>,
    pub file_count: usize,
}

/// Scans `config.workspace_root`, reading each Markdown file's head for a title.
///
/// Errors:
///     [`AxiomataError::Io`] if the workspace root cannot be read at all. Errors
///     on individual entries (an unreadable subdirectory, say) are skipped.
pub fn scan(config: &Config) -> Result<WorkspaceScan, AxiomataError> {
    let root = &config.workspace_root;
    let mut tree = WorkspaceTree::default();
    let mut file_count = 0usize;

    for tracked in tracked_files(root)? {
        file_count += 1;
        let entry = FileEntry {
            title: markdown_title(&root.join(&tracked.rel_path)),
            rel_path: tracked.rel_path.clone(),
        };
        match tracked.area {
            Some(area) => tree.areas.entry(area).or_default().push(entry),
            None => tree.loose.push(entry),
        }
    }

    sort_entries(&mut tree.loose);
    for entries in tree.areas.values_mut() {
        sort_entries(entries);
    }

    Ok(WorkspaceScan { tree, file_count })
}

/// Like [`scan`] but reads no file contents — just walks and stats. Used by the
/// stale check, which runs on every status poll.
pub fn freshness(config: &Config) -> Result<Freshness, AxiomataError> {
    let mut newest_mtime: Option<SystemTime> = None;
    let mut file_count = 0usize;
    for tracked in tracked_files(&config.workspace_root)? {
        file_count += 1;
        newest_mtime = newest_mtime.max(tracked.mtime);
    }
    Ok(Freshness {
        newest_mtime,
        file_count,
    })
}

/// A single tracked file as the walk sees it, before title extraction.
struct Tracked {
    /// Forward-slashed path relative to the workspace root.
    rel_path: String,
    /// The top-level subdirectory name, or `None` for a root-level file.
    area: Option<String>,
    mtime: Option<SystemTime>,
}

/// Runs the `ignore` walk over `root` and yields every tracked file.
fn tracked_files(root: &Path) -> Result<Vec<Tracked>, AxiomataError> {
    if !root.is_dir() {
        return Err(AxiomataError::Io {
            path: root.to_path_buf(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        });
    }

    let walk = WalkBuilder::new(root)
        .hidden(true) // skip .git/, .claude/, dotfiles
        .parents(false) // ignore .gitignore files above the workspace
        .git_global(false)
        .build();

    let mut out = Vec::new();
    for result in walk {
        let dir_entry = match result {
            Ok(entry) => entry,
            Err(_) => continue, // unreadable entry — skip, don't fail the walk
        };
        if dir_entry.depth() == 0 {
            continue; // the root itself
        }
        if !dir_entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }
        let path = dir_entry.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("CLAUDE.md"))
        {
            continue; // generated output, not content (case-insensitive FS)
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let mut components = rel.components();
        let first = components.next().and_then(|c| c.as_os_str().to_str());
        let has_more = components.next().is_some();

        let area = match (first, has_more) {
            (Some(dir), true) => Some(dir.to_owned()),
            _ => None,
        };
        let mtime = dir_entry.metadata().ok().and_then(|m| m.modified().ok());
        out.push(Tracked {
            rel_path: rel_path_string(rel),
            area,
            mtime,
        });
    }
    Ok(out)
}

/// A relative path rendered with `/` separators regardless of platform.
fn rel_path_string(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect::<Vec<_>>()
        .join("/")
}

/// Sorts entries case-insensitively by path, with the exact path as a stable
/// tiebreak — deterministic across runs and platforms.
fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| {
        a.rel_path
            .to_lowercase()
            .cmp(&b.rel_path.to_lowercase())
            .then_with(|| a.rel_path.cmp(&b.rel_path))
    });
}

/// The frontmatter of a Markdown file, for title extraction only.
#[derive(Debug, Deserialize)]
struct TitleFrontmatter {
    #[serde(default)]
    title: Option<String>,
}

/// Extracts a title from a Markdown file: the frontmatter `title:` if present,
/// otherwise the first `# ` heading. Only `.md` files are considered, and only
/// the first [`TITLE_SCAN_BYTES`] of the file are read.
fn markdown_title(path: &Path) -> Option<String> {
    if path.extension().and_then(|e| e.to_str())?.to_lowercase() != "md" {
        return None;
    }
    let head = read_head(path, TITLE_SCAN_BYTES)?;

    let matter: Matter<YAML> = Matter::new();
    if let Ok(parsed) = matter.parse::<TitleFrontmatter>(&head) {
        if let Some(title) = parsed.data.and_then(|d| d.title) {
            let normalized = normalize_title(&title);
            if !normalized.is_empty() {
                return Some(normalized);
            }
        }
        // Fall through to a heading scan of the body after the frontmatter.
        return first_heading(&parsed.content);
    }
    first_heading(&head)
}

/// Returns the text of the first ATX `# ` heading in `text`, if any.
fn first_heading(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("# "))
        .map(normalize_title)
        .filter(|title| !title.is_empty())
}

/// Collapses all whitespace (including newlines) to single spaces and trims,
/// so a title always renders as one clean line.
fn normalize_title(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Reads up to `limit` bytes from `path` as UTF-8 (lossy), or `None` on error.
fn read_head(path: &Path, limit: usize) -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(path).ok()?;
    let mut buf = Vec::with_capacity(limit.min(4096));
    file.take(limit as u64).read_to_end(&mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;
    use std::fs;

    /// Builds a scratch workspace from `(rel_path, contents)` pairs.
    fn workspace(files: &[(&str, &str)]) -> (PathBufGuard, Config) {
        let root = unique_temp_dir("axiomata-test-walker");
        for (rel, contents) in files {
            let path = root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };
        (PathBufGuard(root), config)
    }

    /// Removes the scratch directory on drop.
    struct PathBufGuard(std::path::PathBuf);
    impl Drop for PathBufGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn groups_by_top_level_area_and_keeps_loose_files_separate() {
        let (_g, config) = workspace(&[
            ("top.md", "# Top note\n"),
            ("projects/a.md", "# Project A\n"),
            ("projects/nested/deep.md", "no heading here\n"),
            ("reference/x.txt", "plain"),
        ]);
        let scan = scan(&config).unwrap();

        assert_eq!(
            scan.tree
                .loose
                .iter()
                .map(|e| &e.rel_path)
                .collect::<Vec<_>>(),
            ["top.md"]
        );
        assert_eq!(
            scan.tree.areas.keys().collect::<Vec<_>>(),
            ["projects", "reference"]
        );
        assert_eq!(
            scan.tree.areas["projects"]
                .iter()
                .map(|e| e.rel_path.as_str())
                .collect::<Vec<_>>(),
            ["projects/a.md", "projects/nested/deep.md"]
        );
        assert_eq!(scan.file_count, 4);
    }

    #[test]
    fn extracts_titles_from_frontmatter_and_headings() {
        let (_g, config) = workspace(&[
            (
                "fm.md",
                "---\ntitle: From Frontmatter\n---\n# Ignored heading\n",
            ),
            ("hd.md", "\n\n#  Spaced Heading  \nbody\n"),
            ("none.md", "just text, no title\n"),
            ("data.json", "{}"),
        ]);
        let scan = scan(&config).unwrap();
        let by_name: std::collections::HashMap<_, _> = scan
            .tree
            .loose
            .iter()
            .map(|e| (e.rel_path.as_str(), e.title.clone()))
            .collect();

        assert_eq!(by_name["fm.md"], Some("From Frontmatter".to_owned()));
        assert_eq!(by_name["hd.md"], Some("Spaced Heading".to_owned()));
        assert_eq!(by_name["none.md"], None);
        assert_eq!(by_name["data.json"], None);
    }

    #[test]
    fn skips_hidden_dirs_generated_claude_md_and_gitignored_paths() {
        let (_g, config) = workspace(&[
            ("keep.md", "# Keep\n"),
            ("CLAUDE.md", "# Root router (generated)\n"),
            ("area/CLAUDE.md", "# Area router\n"),
            (".claude/skills/s/SKILL.md", "hidden\n"),
            (".git/config", "[core]\n"),
            (".gitignore", "ignored.md\nbuild/\n"),
            ("ignored.md", "# Should be excluded\n"),
            ("build/artifact.txt", "x"),
        ]);
        let scan = scan(&config).unwrap();
        let mut paths: Vec<_> = scan
            .tree
            .loose
            .iter()
            .chain(scan.tree.areas.values().flatten())
            .map(|e| e.rel_path.clone())
            .collect();
        paths.sort();
        assert_eq!(paths, ["keep.md"]);
    }

    #[test]
    fn scan_is_deterministic_and_case_insensitively_sorted() {
        let (_g, config) = workspace(&[
            ("area/Zebra.md", "# Z\n"),
            ("area/apple.md", "# A\n"),
            ("area/Banana.md", "# B\n"),
        ]);
        let first = scan(&config).unwrap();
        let second = scan(&config).unwrap();
        let names: Vec<_> = first.tree.areas["area"]
            .iter()
            .map(|e| e.rel_path.as_str())
            .collect();
        assert_eq!(names, ["area/apple.md", "area/Banana.md", "area/Zebra.md"]);
        assert_eq!(first.tree, second.tree);
    }

    #[test]
    fn freshness_counts_files_without_reading_them() {
        let (_g, config) = workspace(&[("a.md", "# A\n"), ("b/c.md", "# C\n")]);
        let f = freshness(&config).unwrap();
        assert_eq!(f.file_count, 2);
        assert!(f.newest_mtime.is_some());
    }

    #[test]
    fn missing_workspace_root_is_an_io_error() {
        let config = Config {
            workspace_root: unique_temp_dir("axiomata-test-walker-missing"),
            ..Config::default()
        };
        assert!(matches!(scan(&config), Err(AxiomataError::Io { .. })));
    }
}
