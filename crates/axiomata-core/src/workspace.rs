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
//! followed) and renamed into place. [`write_file`] will create *at most one*
//! new top-level directory (see [`ensure_immediate_parent_dir`]) — anything
//! deeper is still left alone, so a write can never conjure a multi-level
//! chain of directories, symlinked or not.

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

/// Hard cap for a single image read via [`read_image`] — larger than
/// [`MAX_FILE_BYTES`] since photos routinely exceed 1 MiB. Base64-encoding
/// inflates this by about a third over the Tauri IPC bridge, still small for
/// a local call.
pub const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;

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
    /// One of `image/png`, `image/jpeg`, `image/gif`, `image/webp` —
    /// inferred from the file extension by [`image_mime`]; anything else is
    /// rejected before a `WorkspaceImage` is ever constructed.
    pub mime: &'static str,
    /// Base64-encoded file content — the caller wraps this into a
    /// `data:<mime>;base64,<...>` URI itself.
    pub base64: String,
}

/// Maps a file extension onto the raster MIME types the dashboard's Markdown
/// renderer allows inline (`core/markdown.ts`'s `DATA_IMAGE_RE`) — keep the
/// two lists in lockstep; extend both together if a format is ever added.
/// SVG is deliberately never included: it can carry `<script>`, unlike a
/// raster format.
fn image_mime(rel: &str) -> Option<&'static str> {
    let ext = Path::new(rel).extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return None,
    })
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

/// Rejects an absolute path, a `..` component, or a Windows drive prefix.
/// Shared by [`resolve`] and [`ensure_immediate_parent_dir`] — the latter
/// runs *before* a parent directory necessarily exists, so it can't lean on
/// `resolve`'s own canonicalisation to catch these first.
fn validate_components(rel_path: &Path) -> Result<(), AxiomataError> {
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
    Ok(())
}

/// If `rel`'s immediate parent directory doesn't exist yet, creates *exactly
/// that one* directory level — never more, and never through a symlink — so
/// [`write_file`] can write into a brand-new top-level area (a connector
/// module's own notes folder, say) without a separate "create this folder
/// first" step.
///
/// Deliberately narrow: `resolve`'s own containment guard needs a directory
/// to already exist before it can canonicalise and check it, so naively
/// `create_dir_all`-ing an arbitrary `rel`'s parent chain would create (or
/// walk through) directories *before* any of them have been proven safe —
/// exactly the kind of symlinked-ancestor escape the rest of this module
/// guards against. This function instead only ever creates a directory that
/// is:
/// - not already present as *anything* (checked via `symlink_metadata`,
///   which does not follow a symlink to decide "present"), and
/// - a single path segment directly under the workspace root, which
///   [`guarded_root`] has already canonicalised — so there is no unproven
///   intermediate segment to walk through in the first place.
///
/// A `rel` whose parent's *own* parent is also missing (i.e. creating it
/// would take more than one new directory) is left alone; the follow-up
/// [`resolve`] call surfaces the real "no such file or directory" error
/// rather than this function silently building a multi-level chain.
///
/// Returns `Err` if `rel` itself fails [`validate_components`] (an absolute
/// path or a `..` component), if [`guarded_root`] can't resolve the
/// workspace root, or — the only case expected in practice, since both of
/// the above would also make the follow-up [`resolve`] call fail the same
/// way — if the OS-level `create_dir` call itself fails (e.g. a permissions
/// error). It never returns `Ok` for a `rel` this function declined to
/// create a directory for; those cases fall through to `Ok(())` and rely on
/// [`resolve`] to report the real problem.
fn ensure_immediate_parent_dir(config: &Config, rel: &str) -> Result<(), AxiomataError> {
    let rel_path = Path::new(rel);
    validate_components(rel_path)?;

    let Some(parent_rel) = rel_path.parent().filter(|p| !p.as_os_str().is_empty()) else {
        return Ok(()); // `rel` has no subdirectory at all — nothing to ensure.
    };
    // A single path segment has an empty parent (`Path::new("Mail").parent()
    // == Some(Path::new(""))`) — anything deeper is refused, per the doc
    // comment above.
    let is_single_segment = parent_rel
        .parent()
        .is_some_and(|gp| gp.as_os_str().is_empty());
    if !is_single_segment {
        return Ok(());
    }

    let root = guarded_root(config)?;
    let parent_full = root.join(parent_rel);
    if fs::symlink_metadata(&parent_full).is_ok() {
        return Ok(()); // already exists as *something* — let `resolve` judge it.
    }
    fs::create_dir(&parent_full).map_err(io(&parent_full))
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
    validate_components(rel_path)?;

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

/// Reads a raster image file from the workspace, base64-encoded — the
/// binary counterpart to [`read_file`]. Used by the Markdown viewer to
/// inline a note's own relatively-referenced images (`![alt](photo.jpg)`) as
/// `data:` URIs, since this app has no other image-serving mechanism: an
/// `asset://` + `<img src=…>` design was considered instead (there's already
/// a dormant `assetFileUrl` helper on the frontend for exactly this), but
/// re-adding the Rust-side scope-granting it needs risks the same class of
/// Tauri asset-protocol failure that broke the HTML lesson viewer's original
/// design (see `md-file.svelte`'s doc comment) — inlining is simpler and
/// already proven (DOMPurify's Markdown sanitiser already allow-lists
/// exactly this `data:image/…;base64,` shape).
///
/// Errors:
///     [`AxiomataError::InvalidWorkspacePath`] for a path outside the
///     workspace, a symlink/hard link, a file over [`MAX_IMAGE_BYTES`], or
///     an extension [`image_mime`] doesn't recognise.
///     [`AxiomataError::Io`] on any other read failure.
pub fn read_image(config: &Config, rel: &str) -> Result<WorkspaceImage, AxiomataError> {
    let mime = image_mime(rel).ok_or_else(|| {
        invalid(
            Path::new(rel),
            "not a supported image type (png/jpeg/gif/webp)",
        )
    })?;
    let full = resolve(config, rel)?;
    let meta = fs::metadata(&full).map_err(io(&full))?;
    if meta.len() > MAX_IMAGE_BYTES {
        return Err(invalid(
            Path::new(rel),
            format!("larger than the {MAX_IMAGE_BYTES}-byte limit"),
        ));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    fs::File::open(&full)
        .and_then(|f| f.take(MAX_IMAGE_BYTES).read_to_end(&mut bytes))
        .map_err(io(&full))?;
    Ok(WorkspaceImage {
        path: rel.replace('\\', "/"),
        mime,
        base64: base64_encode(&bytes),
    })
}

/// `base64` 0.22 dropped the old free-function API in favour of an explicit
/// `Engine` — this is the standard (not URL-safe) alphabet with padding,
/// what every `data:` URI expects.
fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Atomically writes `content` to a workspace file, creating it if missing.
/// The parent directory must already exist, with one exception: a missing
/// parent that is itself a single new top-level directory is created (see
/// [`ensure_immediate_parent_dir`]) — anything deeper still requires the
/// parent to already exist.
pub fn write_file(config: &Config, rel: &str, content: &str) -> Result<(), AxiomataError> {
    if content.len() as u64 > MAX_FILE_BYTES {
        return Err(invalid(
            Path::new(rel),
            format!("content exceeds the {MAX_FILE_BYTES}-byte limit"),
        ));
    }
    ensure_immediate_parent_dir(config, rel)?;
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
        // (the "already exists" branch of ensure_immediate_parent_dir).
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
    fn ensure_immediate_parent_dir_is_a_no_op_for_a_bare_top_level_filename() {
        let (root, config) = workspace();

        // "root.md" has no subdirectory component at all -- this exercises
        // `ensure_immediate_parent_dir`'s early `rel_path.parent().filter(...)`
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
        // `ensure_immediate_parent_dir` must reject them itself, via its own
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
        // directory to create". `ensure_immediate_parent_dir`'s own doc
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

        // `ensure_immediate_parent_dir` must not treat the symlink as "safe
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
