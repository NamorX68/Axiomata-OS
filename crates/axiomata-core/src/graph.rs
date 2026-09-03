//! The workspace as a graph, for the dashboard's particle view ("Second
//! Brain"): every tracked file as a node (with its top-level area, title,
//! size, modification time), `[[wiki]]` / relative Markdown links between
//! files as edges, plus the skills and routines as their own node kinds and
//! the workspace `CLAUDE.md` as the hub.
//!
//! Built on the memory walker (same file set the router indexes). Files are
//! capped at [`MAX_FILES`] and link extraction at [`MAX_LINK_SCAN_BYTES`] per
//! file, so a huge vault yields a truncated-but-usable graph rather than a
//! stalled UI.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Read;
use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::AxiomataError;
use crate::memory::walker::{self, FileEntry};
use crate::routines::{self, Routine};
use crate::skills;

/// Most file nodes returned; beyond that `truncated` is set.
pub const MAX_FILES: usize = 5000;
/// Bytes of a Markdown file scanned for links.
pub const MAX_LINK_SCAN_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphFile {
    /// Workspace-relative path, forward slashes; the node id.
    pub path: String,
    /// Top-level folder, `None` for files directly in the root.
    pub area: Option<String>,
    pub title: String,
    pub bytes: u64,
    pub modified: Option<DateTime<Utc>>,
    pub is_markdown: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphArea {
    pub name: String,
    pub files: usize,
}

/// A directed link `from` → `to` (both file paths).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphLink {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphSkill {
    pub name: String,
    pub description: String,
    pub backend: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceGraph {
    pub workspace_root: String,
    /// `CLAUDE.md` in the root, if present — the centre node.
    pub hub: Option<String>,
    pub areas: Vec<GraphArea>,
    pub files: Vec<GraphFile>,
    pub links: Vec<GraphLink>,
    pub skills: Vec<GraphSkill>,
    pub routines: Vec<Routine>,
    pub total_files: usize,
    pub truncated: bool,
    pub generated_at: DateTime<Utc>,
}

/// Builds the graph for `config.workspace_root`. Routines come from `db`.
pub fn build(config: &Config, db: &Connection) -> Result<WorkspaceGraph, AxiomataError> {
    let root = &config.workspace_root;
    let scan = walker::scan(config)?;

    let mut entries: Vec<(Option<String>, FileEntry)> = Vec::new();
    for entry in scan.tree.loose {
        entries.push((None, entry));
    }
    for (area, files) in scan.tree.areas {
        for entry in files {
            entries.push((Some(area.clone()), entry));
        }
    }
    let truncated = entries.len() > MAX_FILES;
    entries.truncate(MAX_FILES);

    let mut files = Vec::with_capacity(entries.len());
    let mut area_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (area, entry) in &entries {
        let full = root.join(&entry.rel_path);
        let meta = fs::metadata(&full).ok();
        let is_markdown = entry.rel_path.to_lowercase().ends_with(".md");
        files.push(GraphFile {
            path: entry.rel_path.clone(),
            area: area.clone(),
            title: entry
                .title
                .clone()
                .unwrap_or_else(|| file_stem(&entry.rel_path)),
            bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: meta
                .and_then(|m| m.modified().ok())
                .map(DateTime::<Utc>::from),
            is_markdown,
        });
        if let Some(a) = area {
            *area_counts.entry(a.clone()).or_default() += 1;
        }
    }

    let links = extract_links(root, &files);

    let skills = skills::registry::list_skills()
        .unwrap_or_default()
        .into_iter()
        .map(|s| GraphSkill {
            name: s.name,
            description: s.description,
            backend: s.backend,
            model: s.model,
            effort: s.effort,
        })
        .collect();
    let routines = routines::store::list(db)?;

    let hub = root
        .join("CLAUDE.md")
        .is_file()
        .then(|| "CLAUDE.md".to_string());

    Ok(WorkspaceGraph {
        workspace_root: root.to_string_lossy().into_owned(),
        hub,
        areas: area_counts
            .into_iter()
            .map(|(name, files)| GraphArea { name, files })
            .collect(),
        files,
        links,
        skills,
        routines,
        total_files: scan.file_count,
        truncated,
        generated_at: Utc::now(),
    })
}

fn file_stem(rel_path: &str) -> String {
    let name = rel_path.rsplit('/').next().unwrap_or(rel_path);
    name.rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_string()
}

/// Finds `[[Target]]`, `[[Target|alias]]`, `[[Target#heading]]` and
/// `[text](relative.md)` references in every Markdown file and resolves them
/// to file paths: an exact relative path first, else a unique file stem
/// (case-insensitive) anywhere in the workspace. Unresolvable links are
/// dropped; duplicates and self-links too.
pub fn extract_links(root: &Path, files: &[GraphFile]) -> Vec<GraphLink> {
    let by_path: HashMap<String, &GraphFile> = files.iter().map(|f| (f.path.clone(), f)).collect();
    let mut by_stem: HashMap<String, Vec<&str>> = HashMap::new();
    for f in files {
        by_stem
            .entry(file_stem(&f.path).to_lowercase())
            .or_default()
            .push(&f.path);
    }
    let resolve = |from: &str, target: &str| -> Option<String> {
        let target = target.trim();
        if target.is_empty() {
            return None;
        }
        // Relative path (as written, or with .md appended), relative to the
        // linking file's folder or to the root.
        let folder = from.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        for candidate in [target.to_string(), format!("{target}.md")] {
            let normalised = normalise_rel(folder, &candidate);
            if by_path.contains_key(&normalised) {
                return Some(normalised);
            }
            if by_path.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        let stem = file_stem(target).to_lowercase();
        match by_stem.get(&stem).map(Vec::as_slice) {
            Some([only]) => Some((*only).to_string()),
            _ => None,
        }
    };

    let mut seen = std::collections::HashSet::new();
    let mut links = Vec::new();
    for f in files.iter().filter(|f| f.is_markdown) {
        let Some(text) = read_head(&root.join(&f.path), MAX_LINK_SCAN_BYTES) else {
            continue;
        };
        for target in link_targets(&text) {
            if let Some(to) = resolve(&f.path, &target)
                && to != f.path
            {
                let link = GraphLink {
                    from: f.path.clone(),
                    to,
                };
                if seen.insert(link.clone()) {
                    links.push(link);
                }
            }
        }
    }
    links
}

/// `folder/../x.md` → `x.md`; keeps forward slashes; never escapes above "".
fn normalise_rel(folder: &str, candidate: &str) -> String {
    let joined = if folder.is_empty() || candidate.starts_with('/') {
        candidate.trim_start_matches('/').to_string()
    } else {
        format!("{folder}/{candidate}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

/// The raw link targets in a Markdown text.
pub fn link_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    // [[Target|alias]] / [[Target#heading]]
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("]]") else { break };
        let inner = &after[..end];
        let target = inner.split(['|', '#']).next().unwrap_or("").trim();
        if !target.is_empty() && !target.contains('\n') {
            out.push(target.to_string());
        }
        rest = &after[end + 2..];
    }
    // [text](relative.md) — skip URLs and anchors
    let mut rest = text;
    while let Some(start) = rest.find("](") {
        let after = &rest[start + 2..];
        let Some(end) = after.find(')') else { break };
        let target = after[..end].split_whitespace().next().unwrap_or("");
        let lower = target.to_lowercase();
        if !target.is_empty()
            && !lower.contains("://")
            && !lower.starts_with('#')
            && !lower.starts_with("mailto:")
            && (lower.ends_with(".md") || !target.contains('.'))
        {
            out.push(target.split('#').next().unwrap_or(target).to_string());
        }
        rest = &after[end + 1..];
    }
    out
}

fn read_head(path: &Path, limit: u64) -> Option<String> {
    let mut buf = Vec::new();
    fs::File::open(path)
        .and_then(|f| f.take(limit).read_to_end(&mut buf))
        .ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

    fn make(files: &[(&str, &str)]) -> Vec<GraphFile> {
        files
            .iter()
            .map(|(p, _)| GraphFile {
                path: p.to_string(),
                area: p.split_once('/').map(|(a, _)| a.to_string()),
                title: file_stem(p),
                bytes: 0,
                modified: None,
                is_markdown: p.ends_with(".md"),
            })
            .collect()
    }

    #[test]
    fn extracts_wiki_and_relative_link_targets() {
        let t = link_targets(
            "See [[Cargo Cheat Sheet|cheats]] and [[Rust lernen#Basics]] and [x](../Arbeit/Snowflake.md) \
             but not [w](https://x.y/a.md) nor [a](#top) nor ![img](pic.png)",
        );
        assert_eq!(
            t,
            vec!["Cargo Cheat Sheet", "Rust lernen", "../Arbeit/Snowflake.md"]
        );
    }

    #[test]
    fn resolves_links_by_path_and_unique_stem_and_drops_the_rest() {
        let root = unique_temp_dir("axiomata-test-graph");
        let files = [
            (
                "Entwicklung/Rust lernen.md",
                "[[Cargo Cheat Sheet]] [[Nope]] [[Rust lernen]] [x](../Arbeit/Snowflake.md) [[Dup]]",
            ),
            (
                "Entwicklung/Cargo Cheat Sheet.md",
                "back to [[Rust lernen]] and [[rust lernen]]",
            ),
            ("Arbeit/Snowflake.md", ""),
            ("Arbeit/Dup.md", ""),
            ("KI/Dup.md", ""),
        ];
        for (p, c) in &files {
            let full = root.join(p);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, c).unwrap();
        }
        let links = extract_links(&root, &make(&files));
        let pairs: Vec<(&str, &str)> = links
            .iter()
            .map(|l| (l.from.as_str(), l.to.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                (
                    "Entwicklung/Rust lernen.md",
                    "Entwicklung/Cargo Cheat Sheet.md"
                ),
                ("Entwicklung/Rust lernen.md", "Arbeit/Snowflake.md"),
                (
                    "Entwicklung/Cargo Cheat Sheet.md",
                    "Entwicklung/Rust lernen.md"
                ),
            ]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn normalises_relative_paths() {
        assert_eq!(
            normalise_rel("Entwicklung", "../Arbeit/x.md"),
            "Arbeit/x.md"
        );
        assert_eq!(normalise_rel("", "a/./b.md"), "a/b.md");
        assert_eq!(normalise_rel("A", "../../../x.md"), "x.md");
        assert_eq!(file_stem("A/B/Note.Name.md"), "Note.Name");
    }

    #[test]
    fn build_groups_files_by_area_and_finds_the_hub() {
        let _guard = crate::test_support::ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-graph-home");
        let root = unique_temp_dir("axiomata-test-graph-ws");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(root.join("Dev")).unwrap();
        fs::write(root.join("CLAUDE.md"), "# hub").unwrap();
        fs::write(root.join("loose.md"), "# Loose\n[[One]]").unwrap();
        fs::write(root.join("Dev/One.md"), "# One").unwrap();
        fs::write(root.join("Dev/two.txt"), "plain").unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            std::env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
        }
        let config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };
        let db = crate::db::open_and_migrate_at(&home.join("graph-test.db")).unwrap();
        let graph = build(&config, &db).unwrap();
        unsafe {
            std::env::remove_var(crate::paths::AXIOMATA_HOME_ENV);
        }
        assert_eq!(graph.hub.as_deref(), Some("CLAUDE.md"));
        assert_eq!(
            graph.areas,
            vec![GraphArea {
                name: "Dev".into(),
                files: 2
            }]
        );
        let one = graph.files.iter().find(|f| f.path == "Dev/One.md").unwrap();
        assert_eq!(one.title, "One");
        assert!(one.is_markdown && one.bytes > 0 && one.modified.is_some());
        let txt = graph
            .files
            .iter()
            .find(|f| f.path == "Dev/two.txt")
            .unwrap();
        assert_eq!((txt.title.as_str(), txt.is_markdown), ("two", false));
        assert_eq!(
            graph.links,
            vec![GraphLink {
                from: "loose.md".into(),
                to: "Dev/One.md".into()
            }]
        );
        assert!(!graph.truncated);
        assert_eq!(graph.total_files, 3, "router CLAUDE.md is not content");
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(home);
    }
}
