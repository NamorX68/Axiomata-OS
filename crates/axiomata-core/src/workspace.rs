//! Workspace-scoped file access for the dashboard (the `md-file` module, ToDo,
//! Mail, the Second-Brain view) and core's own writers (`board_mirror`).
//!
//! Every path is *relative to* `config.workspace_root`. The guard itself
//! lives in `axiomata-files` since ED0 of `docs/plans/editor.md`: the
//! workspace is an `axiomata_files::Root` with [`LinkPolicy::Strict`] — no
//! absolute paths, no `..`, the resolved location must stay under the
//! canonicalised root (so a symlinked directory can't redirect it), the file
//! itself must be neither a symlink nor a hard link (a hard link shares its
//! content with a file that may live anywhere). This module keeps what is
//! specific to the workspace: [`guarded_root`]'s refusal of `/` and the home
//! directory, the 1 MiB cap of [`MAX_FILE_BYTES`], the single new top-level
//! directory a write may create, and mapping errors onto [`AxiomataError`].
//! Writes are atomic through an `O_EXCL` temp file renamed into place.

use std::fs;
use std::path::{Path, PathBuf};

use axiomata_files::{self as files, FilesError, LinkPolicy, Root};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::AxiomataError;
use crate::memory::guarded_root;

/// Hard cap for a single file in either direction.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Hard cap for a single image read via [`read_image`] — larger than
/// [`MAX_FILE_BYTES`] since photos routinely exceed 1 MiB.
pub const MAX_IMAGE_BYTES: u64 = files::MAX_IMAGE_BYTES;

/// A file read from the workspace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceFile {
    /// The relative path as requested (normalised to `/` separators).
    pub path: String,
    pub content: String,
    /// Last modification time, if the filesystem reports one.
    pub modified: Option<DateTime<Utc>>,
}

/// A raster image read from the workspace, ready to inline as a `data:` URI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceImage {
    /// The relative path as requested (normalised to `/` separators).
    pub path: String,
    /// One of the raster MIME types `axiomata_files::read_image` knows —
    /// inferred from the file extension; anything else is rejected before a
    /// `WorkspaceImage` is ever constructed.
    pub mime: &'static str,
    /// Base64-encoded file content — the caller wraps this into a
    /// `data:<mime>;base64,<...>` URI itself.
    pub base64: String,
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

/// The workspace as a strict `axiomata-files` root.
fn workspace_root(config: &Config) -> Result<Root, AxiomataError> {
    let dir = guarded_root(config)?;
    Root::dir(&dir, LinkPolicy::Strict).map_err(from_files)
}

/// Maps the file service's error onto the two variants callers of this
/// module have always matched on: a guard failure is
/// [`AxiomataError::InvalidWorkspacePath`], anything the file system itself
/// reported — a missing file or parent included — is [`AxiomataError::Io`].
fn from_files(err: FilesError) -> AxiomataError {
    match err {
        FilesError::Io { path, source } => AxiomataError::Io { path, source },
        FilesError::NotFound { path } => AxiomataError::Io {
            path,
            source: std::io::ErrorKind::NotFound.into(),
        },
        FilesError::Refused { path, reason } => {
            AxiomataError::InvalidWorkspacePath { path, reason }
        }
        FilesError::TooLarge { path, limit } => {
            invalid(&path, format!("larger than the {limit}-byte limit"))
        }
        FilesError::NotUtf8 { path } => invalid(&path, "not valid UTF-8"),
        FilesError::Conflict { path } => invalid(&path, "changed since it was read"),
        FilesError::UnknownRoot(id) => invalid(Path::new(&id), "unknown file root"),
        FilesError::BadPattern(message) => invalid(Path::new(""), &message),
    }
}

/// Resolves `rel` under the guarded workspace root.
///
/// Returns the (not necessarily existing) path, with a canonical parent
/// directory inside the root; if the file exists it is a regular file that
/// is neither a symlink nor a hard link.
pub fn resolve(config: &Config, rel: &str) -> Result<PathBuf, AxiomataError> {
    workspace_root(config)?.resolve(rel).map_err(from_files)
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

/// Extensions the full-text search **skips** — binary / opaque formats where
/// line scanning is meaningless. Everything else in the (hand-curated)
/// workspace is indexed as long as it reads as UTF-8 and is under
/// [`SEARCH_MAX_FILE_BYTES`] — so notes, HTML, and source files (`.rs`,
/// `.py`, `.toml`, …) all match.
const SEARCH_SKIP_EXTS: &[&str] = &[
    // images
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "tif", "tiff", "heic", "heif", "avif", "ico",
    "icns", // audio / video
    "mp3", "wav", "flac", "aac", "ogg", "m4a", "mp4", "m4v", "mov", "avi", "mkv", "webm",
    // archives
    "zip", "gz", "tgz", "bz2", "xz", "7z", "rar", "tar", "jar", "war",
    // documents / office
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "key", "pages", "numbers",
    // fonts
    "ttf", "otf", "woff", "woff2", "eot", // compiled / db / other binary
    "so", "dylib", "dll", "a", "o", "exe", "bin", "wasm", "class", "pyc", "db", "sqlite",
    "sqlite3",
];

/// Whether the full-text search should scan this (lowercased) workspace path.
fn is_search_indexed(rel_lower: &str) -> bool {
    match rel_lower.rsplit_once('.') {
        Some((_, ext)) => !SEARCH_SKIP_EXTS.contains(&ext),
        None => true, // README, Dockerfile, Makefile, LICENSE, …
    }
}

/// Case-insensitive full-text search over the workspace's text files (the
/// memory walker's file set, so hidden and ignored paths are skipped; binary
/// extensions are skipped too — see [`SEARCH_SKIP_EXTS`]). Every
/// whitespace-separated word must occur on the same line. Returns at most
/// `limit` files, best (most matching lines) first.
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
        if !is_search_indexed(&lower) {
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
/// returned path is canonical (no symlinked components, no `..`).
pub fn resolve_existing(config: &Config, rel: &str) -> Result<PathBuf, AxiomataError> {
    let full = resolve(config, rel)?;
    let meta = fs::metadata(&full).map_err(io(&full))?;
    if !meta.is_file() {
        return Err(invalid(Path::new(rel), "not a regular file"));
    }
    full.canonicalize().map_err(io(&full))
}

/// Reads a UTF-8 text file from the workspace, up to [`MAX_FILE_BYTES`].
pub fn read_file(config: &Config, rel: &str) -> Result<WorkspaceFile, AxiomataError> {
    let file =
        files::read_text(&workspace_root(config)?, rel, MAX_FILE_BYTES).map_err(from_files)?;
    Ok(WorkspaceFile {
        path: file.rel,
        content: file.content,
        modified: file.modified,
    })
}

/// Reads a raster image file from the workspace, base64-encoded — the
/// binary counterpart to [`read_file`]. Used by the Markdown viewer to
/// inline a note's own relatively-referenced images (`![alt](photo.jpg)`) as
/// `data:` URIs, since this app has no other image-serving mechanism: an
/// `asset://` + `<img src=…>` design would need Rust-side scope-granting
/// that risks the same class of Tauri asset-protocol failure that broke the
/// HTML lesson viewer's original design (see `md-file.svelte`'s doc
/// comment). HEIC and TIFF are offered because the owner asked for them, not
/// because WebKit is verified to render them (HEIC is a known gap, TIFF
/// decoders have a CVE history upstream).
///
/// Errors:
///     [`AxiomataError::InvalidWorkspacePath`] for a path outside the
///     workspace, a symlink/hard link, a file over [`MAX_IMAGE_BYTES`], or
///     an extension that is not a supported raster type.
///     [`AxiomataError::Io`] on any other read failure.
pub fn read_image(config: &Config, rel: &str) -> Result<WorkspaceImage, AxiomataError> {
    let image =
        files::read_image(&workspace_root(config)?, rel, MAX_IMAGE_BYTES).map_err(from_files)?;
    Ok(WorkspaceImage {
        path: image.rel,
        mime: image.mime,
        base64: image.base64,
    })
}

/// Atomically writes `content` to a workspace file, creating it if missing.
/// The parent directory must already exist, with one exception: a missing
/// parent that is itself a single new top-level directory is created (see
/// `axiomata_files::ensure_top_level_dir`) — anything deeper still requires
/// the parent to already exist. The write is blind (no expected version),
/// as it always was here.
pub fn write_file(config: &Config, rel: &str, content: &str) -> Result<(), AxiomataError> {
    let root = workspace_root(config)?;
    if content.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid(
            Path::new(rel),
            format!("content exceeds the {MAX_FILE_BYTES}-byte limit"),
        ));
    }
    files::ensure_top_level_dir(&root, rel).map_err(from_files)?;
    files::write_text(&root, rel, content, None, MAX_FILE_BYTES)
        .map(|_| ())
        .map_err(from_files)
}

/// Deletes a regular file from the workspace, guarded exactly like
/// [`read_file`]: no `..`, no symlink / hard link, must resolve inside
/// `workspace_root`, and must already exist as a regular file.
///
/// Errors:
///     [`AxiomataError::InvalidWorkspacePath`] for a guard failure;
///     [`AxiomataError::Io`] for a missing file or a failed `unlink`.
pub fn delete_file(config: &Config, rel: &str) -> Result<(), AxiomataError> {
    files::delete(&workspace_root(config)?, rel).map_err(from_files)
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
    fn deletes_a_file_and_guards_the_path() {
        let (root, config) = workspace();

        // Happy path — the file is gone, `read_file` now fails.
        delete_file(&config, "notes/inbox.md").unwrap();
        assert!(!root.join("notes/inbox.md").exists());
        assert!(read_file(&config, "notes/inbox.md").is_err());

        // A second delete of the same (now missing) path is a clean error,
        // not a panic (`resolve_existing` reports the missing metadata as Io)
        // -- specifically `from_files`'s `FilesError::NotFound` arm, which
        // maps onto `AxiomataError::Io`, not `InvalidWorkspacePath`: the path
        // itself was fine, the file just isn't there any more.
        assert!(matches!(
            delete_file(&config, "notes/inbox.md").unwrap_err(),
            AxiomataError::Io { .. }
        ));

        // A directory and a climb-out path are both refused.
        assert!(matches!(
            delete_file(&config, "notes").unwrap_err(),
            AxiomataError::InvalidWorkspacePath { .. }
        ));
        assert!(matches!(
            delete_file(&config, "../secret.md").unwrap_err(),
            AxiomataError::InvalidWorkspacePath { .. }
        ));

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
    fn writes_a_new_top_level_directory_but_not_a_nested_one() {
        let (root, config) = workspace();

        // "Mail" doesn't exist yet, but is a single new directory directly
        // under the (already-canonical) workspace root -- created.
        write_file(&config, "Mail/topics.md", "Development\n").unwrap();
        assert_eq!(
            fs::read_to_string(root.join("Mail/topics.md")).unwrap(),
            "Development\n"
        );
        // Writing a second file into the now-existing directory still works
        // (the "already exists" branch of ensure_top_level_dir).
        write_file(&config, "Mail/other.md", "y").unwrap();
        assert_eq!(fs::read_to_string(root.join("Mail/other.md")).unwrap(), "y");

        // A parent missing two levels deep is not auto-created.
        assert!(matches!(
            write_file(&config, "Brand/New/deep.md", "z").unwrap_err(),
            AxiomataError::Io { .. }
        ));
        assert!(!root.join("Brand").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ensure_top_level_dir_is_a_no_op_for_a_bare_top_level_filename() {
        let (root, config) = workspace();

        // "root.md" has no subdirectory component at all -- this exercises
        // `ensure_top_level_dir`'s early `rel_path.parent().filter(...)`
        // return, not the "already exists" branch (covered by
        // `writes_a_new_top_level_directory_but_not_a_nested_one`'s second
        // write) or the "create it" branch.
        write_file(&config, "root.md", "hello").unwrap();
        assert_eq!(fs::read_to_string(root.join("root.md")).unwrap(), "hello");
        assert!(fs::metadata(root.join("root.md")).unwrap().is_file());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn write_file_rejects_dotdot_and_absolute_paths_before_any_directory_creation() {
        let (root, config) = workspace();

        // Both of these would, if `validate_components` were skipped, name a
        // single new top-level directory ("Escape") for `create_dir` to make.
        // `ensure_top_level_dir` must reject them itself, via its own
        // `validate_components` call, *before* computing a parent path or
        // touching the filesystem at all -- not merely rely on the later
        // `resolve()` call inside `write_file` to catch it after the fact.
        for rel in ["../Escape/pwned.md", "/etc/Escape/pwned.md"] {
            let err = write_file(&config, rel, "x").unwrap_err();
            assert!(
                matches!(err, AxiomataError::InvalidWorkspacePath { .. }),
                "{rel}: {err}"
            );
        }
        assert!(!root.join("Escape").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(unix)]
    fn leaves_an_existing_plain_file_alone_when_it_occupies_the_parent_slot() {
        let (root, config) = workspace();

        // A plain file (not a directory, not a symlink) already sits at the
        // name `write_file` would otherwise treat as "a new top-level
        // directory to create". `ensure_top_level_dir`'s own doc
        // comment says a parent "already present as *anything*" is left
        // alone; this is the non-symlink case of that (the symlink case is
        // `refuses_to_create_a_top_level_directory_through_a_planted_symlink`
        // below) -- the follow-up `resolve()` call is what actually surfaces
        // the resulting error (a regular file can't have children).
        fs::write(root.join("NotADir"), "just a file").unwrap();
        let err = write_file(&config, "NotADir/inner.md", "x").unwrap_err();
        assert!(matches!(err, AxiomataError::Io { .. }), "{err}");
        assert_eq!(
            fs::read_to_string(root.join("NotADir")).unwrap(),
            "just a file"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(unix)]
    fn refuses_to_create_a_top_level_directory_through_a_planted_symlink() {
        let (root, config) = workspace();
        // A pre-planted symlink named "Escape" pointing outside the
        // workspace root entirely.
        let outside = unique_temp_dir("axiomata-test-workspace-outside");
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("Escape")).unwrap();

        // `ensure_top_level_dir` must not treat the symlink as "safe
        // to write through" just because *something* exists at that name —
        // `resolve`'s own containment check is what actually rejects this,
        // by canonicalising the parent and finding it outside the root.
        let err = write_file(&config, "Escape/pwned.md", "x").unwrap_err();
        assert!(matches!(err, AxiomataError::InvalidWorkspacePath { .. }));
        assert!(!outside.join("pwned.md").exists());

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
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
        // A parent missing *more than one* level deep is still refused —
        // only a single new top-level directory is ever auto-created (see
        // `writes_a_new_top_level_directory_but_not_a_nested_one` below).
        assert!(matches!(
            write_file(&config, "nope/deeper/new.md", "x").unwrap_err(),
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
    fn read_file_maps_binary_content_onto_invalid_workspace_path() {
        // Exercises `from_files`'s `FilesError::NotUtf8` arm, which nothing
        // else in this module's tests reaches (the size-cap test's `TooLarge`
        // and the symlink tests' `Refused` are covered separately, but a
        // plain binary file was not).
        let (root, config) = workspace();
        fs::write(root.join("notes/blob.bin"), [0xff, 0xfe, 0x00]).unwrap();
        let err = read_file(&config, "notes/blob.bin").unwrap_err();
        assert!(
            matches!(err, AxiomataError::InvalidWorkspacePath { .. }),
            "{err}"
        );
        let _ = fs::remove_dir_all(root);
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
        // Source files are indexed too now (curated vault) — this `.rs` has
        // three "ownership" lines, so it ranks first.
        fs::write(
            root.join("notes/example.rs"),
            "// ownership demo\nlet a = String::from(\"ownership\");\ndrop(a); // ownership moved\n",
        )
        .unwrap();
        let hits = search(&config, "OWNERSHIP", 10).unwrap();
        assert_eq!(
            hits.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
            vec!["notes/example.rs", "notes/rust.md", "Learning/l1.html"]
        );
        assert_eq!(hits[0].matches, 3);
        assert_eq!(hits[1].matches, 2);
        assert_eq!(hits[1].line, 3);
        assert_eq!(hits[2].snippet, "Lektion 1Ownership & erkl\u{e4}rt");
        // …but the binary `.png` (also containing the word) stays out.
        assert!(hits.iter().all(|h| h.path != "notes/skip.png"));
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

    #[test]
    fn reads_an_image_and_base64_round_trips_the_exact_bytes() {
        use base64::Engine as _;
        let (root, config) = workspace();
        let bytes: Vec<u8> = (0..=255).collect(); // exercises the full byte range
        fs::write(root.join("notes/photo.png"), &bytes).unwrap();

        let img = read_image(&config, "notes/photo.png").unwrap();
        assert_eq!(img.path, "notes/photo.png");
        assert_eq!(img.mime, "image/png");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(&img.base64)
                .unwrap(),
            bytes
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_image_recognises_every_supported_extension_case_insensitively() {
        let (root, config) = workspace();
        for (name, mime) in [
            ("a.png", "image/png"),
            ("b.jpg", "image/jpeg"),
            ("c.jpeg", "image/jpeg"),
            ("d.gif", "image/gif"),
            ("e.webp", "image/webp"),
            ("f.bmp", "image/bmp"),
            ("g.tif", "image/tiff"),
            ("h.tiff", "image/tiff"),
            ("i.heic", "image/heic"),
            ("j.heif", "image/heic"),
            ("k.avif", "image/avif"),
            ("F.PNG", "image/png"),
        ] {
            fs::write(root.join(format!("notes/{name}")), [0u8]).unwrap();
            assert_eq!(
                read_image(&config, &format!("notes/{name}")).unwrap().mime,
                mime,
                "{name}"
            );
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_image_rejects_an_unsupported_extension_without_touching_the_filesystem() {
        let (root, config) = workspace();
        fs::write(root.join("notes/vector.svg"), "<svg/>").unwrap();
        assert!(matches!(
            read_image(&config, "notes/vector.svg").unwrap_err(),
            AxiomataError::InvalidWorkspacePath { .. }
        ));
        // No extension at all is rejected the same way, not treated as an I/O miss.
        assert!(matches!(
            read_image(&config, "notes/noext").unwrap_err(),
            AxiomataError::InvalidWorkspacePath { .. }
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_image_enforces_its_own_larger_size_cap() {
        let (root, config) = workspace();
        let big = vec![0u8; MAX_IMAGE_BYTES as usize + 1];
        fs::write(root.join("notes/big.png"), &big).unwrap();
        assert!(matches!(
            read_image(&config, "notes/big.png").unwrap_err(),
            AxiomataError::InvalidWorkspacePath { .. }
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn read_image_refuses_a_symlinked_file_the_same_way_read_file_does() {
        let (root, config) = workspace();
        let outside = unique_temp_dir("axiomata-test-workspace-image-outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.png"), [0u8]).unwrap();
        std::os::unix::fs::symlink(outside.join("secret.png"), root.join("notes/link.png"))
            .unwrap();

        assert!(matches!(
            read_image(&config, "notes/link.png").unwrap_err(),
            AxiomataError::InvalidWorkspacePath { .. }
        ));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
}
