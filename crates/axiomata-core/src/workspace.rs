//! Workspace-scoped file access for the dashboard (the `md-file` module and,
//! later, the agent's one-shot instructions).
//!
//! Every path is *relative to* `config.workspace_root` and is resolved through
//! one guard: no absolute paths, no `..`, the resolved location must stay
//! under the canonicalised root (so a symlinked directory can't redirect it),
//! the file itself must be neither a symlink nor a hard link (a hard link
//! shares its content with a file that may live anywhere), and content is
//! capped at [`MAX_FILE_BYTES`]. Writes are atomic through a temp file that is
//! created with `O_EXCL` (a planted symlink at the temp path is never
//! followed) and renamed into place; they never create directories.

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::AxiomataError;
use crate::memory::guarded_root;

/// Hard cap for a single file in either direction.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// A file read from the workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceFile {
    /// The relative path as requested (normalised to `/` separators).
    pub path: String,
    pub content: String,
    /// Last modification time, if the filesystem reports one.
    pub modified: Option<DateTime<Utc>>,
}

fn invalid(path: &Path, reason: impl Into<String>) -> AxiomataError {
    AxiomataError::InvalidWorkspacePath {
        path: path.to_path_buf(),
        reason: reason.into(),
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> AxiomataError {
    let path = path.to_path_buf();
    move |source| AxiomataError::Io { path, source }
}

/// Resolves `rel` under the guarded workspace root.
///
/// Returns the joined (not necessarily existing) path. The *parent* directory
/// must exist and canonicalise inside the root; if the file exists it must be
/// a regular file (not a symlink, not a directory) that also canonicalises
/// inside the root.
pub fn resolve(config: &Config, rel: &str) -> Result<PathBuf, AxiomataError> {
    let rel_path = Path::new(rel);
    if rel.trim().is_empty() {
        return Err(invalid(rel_path, "empty path"));
    }
    for component in rel_path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::ParentDir => return Err(invalid(rel_path, "`..` is not allowed")),
            Component::RootDir | Component::Prefix(_) => {
                return Err(invalid(rel_path, "path must be relative to the workspace"));
            }
        }
    }

    let root = guarded_root(config)?;
    let full = root.join(rel_path);

    let parent = full
        .parent()
        .ok_or_else(|| invalid(rel_path, "path has no parent directory"))?;
    let parent_canon = parent.canonicalize().map_err(io(parent))?;
    if !parent_canon.starts_with(&root) {
        return Err(invalid(rel_path, "resolves outside the workspace"));
    }

    match fs::symlink_metadata(&full) {
        Ok(meta) if meta.file_type().is_symlink() => Err(invalid(rel_path, "symlinks are refused")),
        Ok(meta) if meta.is_dir() => Err(invalid(rel_path, "is a directory")),
        Ok(meta) if is_hard_linked(&meta) => {
            Err(invalid(rel_path, "hard-linked files are refused"))
        }
        Ok(_) => {
            let canon = full.canonicalize().map_err(io(&full))?;
            if canon.starts_with(&root) {
                Ok(full)
            } else {
                Err(invalid(rel_path, "resolves outside the workspace"))
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(full),
        Err(source) => Err(AxiomataError::Io {
            path: full.clone(),
            source,
        }),
    }
}

/// A regular file with more than one directory entry shares its content with
/// a path that may be outside the workspace; refuse it like a symlink.
#[cfg(unix)]
fn is_hard_linked(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.nlink() > 1
}

#[cfg(not(unix))]
fn is_hard_linked(_meta: &fs::Metadata) -> bool {
    false
}

/// One full-text hit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Workspace-relative path.
    pub path: String,
    /// 1-based line number of the first matching line.
    pub line: usize,
    /// The matching line, trimmed and capped.
    pub snippet: String,
    /// Total matching lines in the file.
    pub matches: usize,
}

/// Longest snippet returned per hit.
pub const SNIPPET_CHARS: usize = 160;
/// Files larger than this are skipped by the search.
pub const SEARCH_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

/// Case-insensitive full-text search over the tracked `.md` / `.html` / `.txt`
/// files (the memory walker's file set, so hidden and ignored paths are
/// skipped). Every whitespace-separated word must occur on the same line.
/// Returns at most `limit` files, best (most matching lines) first.
pub fn search(config: &Config, query: &str, limit: usize) -> Result<Vec<SearchHit>, AxiomataError> {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return Ok(Vec::new());
    }
    let root = guarded_root(config)?;
    let scan = crate::memory::walker::scan(config)?;
    let mut entries: Vec<String> = scan.tree.loose.iter().map(|e| e.rel_path.clone()).collect();
    for files in scan.tree.areas.values() {
        entries.extend(files.iter().map(|e| e.rel_path.clone()));
    }
    let mut hits = Vec::new();
    for rel in entries {
        let lower = rel.to_lowercase();
        if !(lower.ends_with(".md")
            || lower.ends_with(".html")
            || lower.ends_with(".htm")
            || lower.ends_with(".txt"))
        {
            continue;
        }
        let full = root.join(&rel);
        let Ok(meta) = fs::metadata(&full) else {
            continue;
        };
        if meta.len() > SEARCH_MAX_FILE_BYTES {
            continue;
        }
        let Ok(text) = fs::read_to_string(&full) else {
            continue;
        };
        let is_html = lower.ends_with(".html") || lower.ends_with(".htm");
        let mut first: Option<(usize, String)> = None;
        let mut count = 0;
        for (i, raw) in text.lines().enumerate() {
            let line = if is_html {
                strip_tags(raw)
            } else {
                raw.to_string()
            };
            let hay = line.to_lowercase();
            if words.iter().all(|w| hay.contains(w.as_str())) {
                count += 1;
                if first.is_none() {
                    first = Some((i + 1, snippet(&line)));
                }
            }
        }
        if let Some((line, snippet)) = first {
            hits.push(SearchHit {
                path: rel,
                line,
                snippet,
                matches: count,
            });
        }
    }
    hits.sort_by(|a, b| b.matches.cmp(&a.matches).then_with(|| a.path.cmp(&b.path)));
    hits.truncate(limit);
    Ok(hits)
}

fn strip_tags(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_tag = false;
    for c in line.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    crate::memory::walker::decode_entities(&out)
}

fn snippet(line: &str) -> String {
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= SNIPPET_CHARS {
        collapsed
    } else {
        let mut s: String = collapsed.chars().take(SNIPPET_CHARS).collect();
        s.push('…');
        s
    }
}

/// Like [`resolve`], but the file must already exist as a regular file; the
/// returned path is canonical (no symlinked components, no `..`) — what the
/// dashboard hands to the webview's asset protocol.
pub fn resolve_existing(config: &Config, rel: &str) -> Result<PathBuf, AxiomataError> {
    let full = resolve(config, rel)?;
    let meta = fs::metadata(&full).map_err(io(&full))?;
    if !meta.is_file() {
        return Err(invalid(Path::new(rel), "not a regular file"));
    }
    full.canonicalize().map_err(io(&full))
}

/// Reads a UTF-8 text file from the workspace.
pub fn read_file(config: &Config, rel: &str) -> Result<WorkspaceFile, AxiomataError> {
    let full = resolve(config, rel)?;
    let meta = fs::metadata(&full).map_err(io(&full))?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(invalid(
            Path::new(rel),
            format!("larger than the {MAX_FILE_BYTES}-byte limit"),
        ));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    fs::File::open(&full)
        .and_then(|f| f.take(MAX_FILE_BYTES).read_to_end(&mut bytes))
        .map_err(io(&full))?;
    let content =
        String::from_utf8(bytes).map_err(|_| invalid(Path::new(rel), "not valid UTF-8"))?;
    Ok(WorkspaceFile {
        path: rel.replace('\\', "/"),
        content,
        modified: meta.modified().ok().map(DateTime::<Utc>::from),
    })
}

/// Atomically writes `content` to a workspace file, creating it if missing.
/// The parent directory must already exist.
pub fn write_file(config: &Config, rel: &str, content: &str) -> Result<(), AxiomataError> {
    if content.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid(
            Path::new(rel),
            format!("content exceeds the {MAX_FILE_BYTES}-byte limit"),
        ));
    }
    let full = resolve(config, rel)?;
    let file_name = full
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| invalid(Path::new(rel), "missing file name"))?;
    let tmp = full.with_file_name(format!(".{file_name}.axiomata-tmp"));
    // `create_new` = O_CREAT|O_EXCL: a pre-planted symlink (or leftover) at
    // the temp path fails the open instead of being followed and overwritten.
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .and_then(|mut f| f.write_all(content.as_bytes()))
        .map_err(io(&tmp))?;
    fs::rename(&tmp, &full).map_err(|source| {
        let _ = fs::remove_file(&tmp);
        AxiomataError::Io {
            path: full.clone(),
            source,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

    fn workspace() -> (PathBuf, Config) {
        let root = unique_temp_dir("axiomata-test-workspace");
        fs::create_dir_all(root.join("notes")).unwrap();
        fs::write(root.join("notes/inbox.md"), "# Inbox\n\n- one\n").unwrap();
        let config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };
        (root, config)
    }

    #[test]
    fn reads_a_file_with_metadata() {
        let (root, config) = workspace();
        let file = read_file(&config, "notes/inbox.md").unwrap();
        assert_eq!(file.path, "notes/inbox.md");
        assert_eq!(file.content, "# Inbox\n\n- one\n");
        assert!(file.modified.is_some());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn writes_atomically_and_creates_missing_files() {
        let (root, config) = workspace();
        write_file(&config, "notes/new.md", "hello").unwrap();
        assert_eq!(
            fs::read_to_string(root.join("notes/new.md")).unwrap(),
            "hello"
        );
        write_file(&config, "notes/inbox.md", "replaced").unwrap();
        assert_eq!(
            read_file(&config, "notes/inbox.md").unwrap().content,
            "replaced"
        );
        assert!(!root.join("notes/.inbox.md.axiomata-tmp").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn refuses_escapes_absolute_paths_and_missing_parents() {
        let (root, config) = workspace();
        for rel in [
            "../outside.md",
            "notes/../../x.md",
            "/etc/hosts",
            "",
            "notes",
        ] {
            let err = read_file(&config, rel).unwrap_err();
            assert!(
                matches!(err, AxiomataError::InvalidWorkspacePath { .. }),
                "{rel}: {err}"
            );
        }
        // Missing parent directory: writes never create directories.
        assert!(matches!(
            write_file(&config, "nope/new.md", "x").unwrap_err(),
            AxiomataError::Io { .. }
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_files_and_symlinked_directories_that_escape() {
        let (root, config) = workspace();
        let outside = unique_temp_dir("axiomata-test-workspace-outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.md"), "secret").unwrap();

        std::os::unix::fs::symlink(outside.join("secret.md"), root.join("link.md")).unwrap();
        std::os::unix::fs::symlink(root.join("notes/inbox.md"), root.join("inner.md")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("dir-link")).unwrap();

        for rel in ["link.md", "inner.md", "dir-link/secret.md"] {
            let err = read_file(&config, rel).unwrap_err();
            assert!(
                matches!(err, AxiomataError::InvalidWorkspacePath { .. }),
                "{rel}: {err}"
            );
            let err = write_file(&config, rel, "x").unwrap_err();
            assert!(
                matches!(err, AxiomataError::InvalidWorkspacePath { .. }),
                "{rel} (write): {err}"
            );
        }
        assert_eq!(
            fs::read_to_string(outside.join("secret.md")).unwrap(),
            "secret"
        );
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[cfg(unix)]
    #[test]
    fn refuses_hard_links_and_a_planted_temp_symlink() {
        let (root, config) = workspace();
        let outside = unique_temp_dir("axiomata-test-workspace-hardlink");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.md"), "secret").unwrap();

        // Hard link: same inode as a file outside the workspace.
        fs::hard_link(outside.join("secret.md"), root.join("notes/linked.md")).unwrap();
        let err = read_file(&config, "notes/linked.md").unwrap_err();
        assert!(
            matches!(err, AxiomataError::InvalidWorkspacePath { .. }),
            "{err}"
        );

        // A symlink planted at the predictable temp path must not be followed.
        std::os::unix::fs::symlink(
            outside.join("secret.md"),
            root.join("notes/.inbox.md.axiomata-tmp"),
        )
        .unwrap();
        assert!(write_file(&config, "notes/inbox.md", "clobber").is_err());
        assert_eq!(
            fs::read_to_string(outside.join("secret.md")).unwrap(),
            "secret"
        );
        assert_eq!(
            read_file(&config, "notes/inbox.md").unwrap().content,
            "# Inbox\n\n- one\n"
        );

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn resolve_existing_requires_a_regular_file_and_returns_a_canonical_path() {
        let (root, config) = workspace();
        let path = resolve_existing(&config, "notes/inbox.md").unwrap();
        assert!(path.is_absolute() && path.ends_with("notes/inbox.md"));
        assert!(matches!(
            resolve_existing(&config, "notes/missing.md").unwrap_err(),
            AxiomataError::Io { .. }
        ));
        assert!(matches!(
            resolve_existing(&config, "notes").unwrap_err(),
            AxiomataError::InvalidWorkspacePath { .. }
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn full_text_search_matches_all_words_case_insensitively_and_ranks_by_count() {
        let (root, config) = workspace();
        fs::write(
            root.join("notes/rust.md"),
            "# Rust lernen\n\nOwnership und Borrowing.\nOwnership again.\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("Learning")).unwrap();
        fs::write(
            root.join("Learning/l1.html"),
            "<h1>Lektion 1</h1><p>Ownership &amp; <b>erkl\u{e4}rt</b></p>",
        )
        .unwrap();
        fs::write(root.join("notes/skip.png"), "ownership").unwrap();
        let hits = search(&config, "OWNERSHIP", 10).unwrap();
        assert_eq!(
            hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
            vec!["notes/rust.md", "Learning/l1.html"]
        );
        assert_eq!(hits[0].matches, 2);
        assert_eq!(hits[0].line, 3);
        assert_eq!(hits[1].snippet, "Lektion 1Ownership & erkl\u{e4}rt");
        assert!(search(&config, "ownership again", 10).unwrap().len() == 1);
        assert!(search(&config, "   ", 10).unwrap().is_empty());
        assert_eq!(search(&config, "ownership", 1).unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn enforces_the_size_cap_both_ways() {
        let (root, config) = workspace();
        let big = "x".repeat(MAX_FILE_BYTES as usize + 1);
        assert!(matches!(
            write_file(&config, "notes/big.md", &big).unwrap_err(),
            AxiomataError::InvalidWorkspacePath { .. }
        ));
        fs::write(root.join("notes/big.md"), &big).unwrap();
        assert!(matches!(
            read_file(&config, "notes/big.md").unwrap_err(),
            AxiomataError::InvalidWorkspacePath { .. }
        ));
        let _ = fs::remove_dir_all(root);
    }
}
