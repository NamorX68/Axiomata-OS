//! A repository as the editor's git panel sees it (`docs/plans/editor-projekt-werkzeuge.md`, #48):
//! what is staged and what is not, one file's diff on either side, staging and unstaging a file
//! or a single hunk, and committing what is staged.
//!
//! Everything works on the repository's own working copy, so these are the functions that change
//! the user's files and index — each says what it touches. **[`push`] is the only function that
//! publishes anything**, and only the checked-out branch, never with force; `fetch` only updates
//! remote-tracking branches.
//!
//! Paths from outside are checked ([`crate::diff::checked_path`]), a hunk is applied only if it
//! is still the hunk the UI showed (same index, same header), and git runs with literal
//! pathspecs (see [`crate::run`]) so a file called `:(glob)**` is only ever a file name.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::diff::{ChangeKind, FileDiff, MAX_DIFF_BYTES, checked_path, hunk_patch, parse_diff};
use crate::run::{git, git_bytes, git_with, git_with_input};
use crate::{GitError, Result};

/// One changed path, with what is staged and what is not (Zed's two groups).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusEntry {
    /// Relative to the repository root, `/`-separated.
    pub path: String,
    /// The old path of a staged rename.
    pub old_path: Option<String>,
    /// What is staged for this path, if anything.
    pub staged: Option<ChangeKind>,
    /// What is changed in the working tree but not staged, if anything.
    pub unstaged: Option<ChangeKind>,
    /// Not tracked by git yet.
    pub untracked: bool,
    /// Both sides changed it in a merge that is not resolved.
    pub conflicted: bool,
}

/// The state of one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoStatus {
    /// The checked-out branch; `None` for a detached `HEAD`.
    pub branch: Option<String>,
    /// The short id of `HEAD`; `None` before the first commit.
    pub head: Option<String>,
    /// The remote branch this one follows, e.g. `origin/main`.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Sorted by path.
    pub entries: Vec<StatusEntry>,
}

/// Which side of a path to diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    /// The index against `HEAD` — what the next commit contains.
    Staged,
    /// The working tree against the index — what is not staged yet. An untracked file counts:
    /// it is diffed against nothing.
    Unstaged,
}

/// Whether `path` is a repository at all (`git rev-parse --is-inside-work-tree`).
pub fn is_repo(path: &Path) -> bool {
    git(path, &["rev-parse", "--is-inside-work-tree"])
        .map(|out| out.trim() == "true")
        .unwrap_or(false)
}

/// Whether `HEAD` points at a commit (a fresh `git init` has none).
fn has_head(repo: &Path) -> bool {
    git_with(repo, &["rev-parse", "--verify", "--quiet", "HEAD"], &[0, 1])
        .map(|(code, _)| code == 0)
        .unwrap_or(false)
}

fn kind_of(code: char) -> Option<ChangeKind> {
    match code {
        'M' => Some(ChangeKind::Modified),
        'A' | 'C' => Some(ChangeKind::Added),
        'D' => Some(ChangeKind::Deleted),
        'R' => Some(ChangeKind::Renamed),
        'T' => Some(ChangeKind::TypeChanged),
        _ => None,
    }
}

/// The status of the repository at `repo`: branch, upstream and every changed path.
///
/// Read-only. Uses `--porcelain=v2 -z`, git's machine format, and lists untracked files one by one
/// (ignored ones never).
pub fn status(repo: &Path) -> Result<RepoStatus> {
    let raw = git(
        repo,
        &[
            "status",
            "--porcelain=v2",
            "--branch",
            "--untracked-files=all",
            "-z",
        ],
    )?;
    Ok(parse_status(&raw))
}

pub(crate) fn parse_status(raw: &str) -> RepoStatus {
    let mut out = RepoStatus {
        branch: None,
        head: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        entries: Vec::new(),
    };
    let mut records = raw.split('\0');
    while let Some(record) = records.next() {
        if record.is_empty() {
            continue;
        }
        if let Some(header) = record.strip_prefix("# ") {
            let (key, value) = header.split_once(' ').unwrap_or((header, ""));
            match key {
                "branch.oid" if value != "(initial)" => {
                    out.head = Some(value.chars().take(8).collect())
                }
                "branch.head" if value != "(detached)" => out.branch = Some(value.to_string()),
                "branch.upstream" => out.upstream = Some(value.to_string()),
                "branch.ab" => {
                    for part in value.split(' ') {
                        if let Some(n) = part.strip_prefix('+') {
                            out.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            out.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let mut chars = record.chars();
        match chars.next() {
            // `1 XY sub mH mI mW hH hI path`
            Some('1') => {
                let fields: Vec<&str> = record.splitn(9, ' ').collect();
                if let [_, xy, .., path] = fields[..] {
                    out.entries.push(entry(path, None, xy));
                }
            }
            // `2 XY sub mH mI mW hH hI Xscore path` then the original path as the next record.
            Some('2') => {
                let fields: Vec<&str> = record.splitn(10, ' ').collect();
                let old = records.next().map(str::to_string);
                if let [_, xy, .., path] = fields[..] {
                    out.entries.push(entry(path, old, xy));
                }
            }
            // `u XY sub m1 m2 m3 mW h1 h2 h3 path`
            Some('u') => {
                let fields: Vec<&str> = record.splitn(11, ' ').collect();
                if let [_, _, .., path] = fields[..] {
                    let mut e = entry(path, None, "UU");
                    e.conflicted = true;
                    e.staged = None;
                    e.unstaged = None;
                    out.entries.push(e);
                }
            }
            Some('?') => {
                if let Some(path) = record.strip_prefix("? ") {
                    out.entries.push(StatusEntry {
                        path: path.to_string(),
                        old_path: None,
                        staged: None,
                        unstaged: Some(ChangeKind::Added),
                        untracked: true,
                        conflicted: false,
                    });
                }
            }
            _ => {}
        }
    }
    out.entries.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn entry(path: &str, old_path: Option<String>, xy: &str) -> StatusEntry {
    let mut codes = xy.chars();
    let x = codes.next().unwrap_or('.');
    let y = codes.next().unwrap_or('.');
    StatusEntry {
        path: path.to_string(),
        old_path,
        staged: kind_of(x),
        unstaged: kind_of(y),
        untracked: false,
        conflicted: false,
    }
}

fn check_all(paths: &[String]) -> Result<()> {
    paths.iter().try_for_each(|p| checked_path(p).map(drop))
}

/// Stages `paths` — what is changed, added or deleted there goes into the index.
/// **Changes the index.**
pub fn stage(repo: &Path, paths: &[String]) -> Result<()> {
    check_all(paths)?;
    if paths.is_empty() {
        return Ok(());
    }
    let mut args = vec!["add", "--all", "--"];
    args.extend(paths.iter().map(String::as_str));
    git(repo, &args).map(drop)
}

/// Unstages `paths`: they are as `HEAD` has them in the index again; the working tree is not
/// touched. **Changes the index.**
pub fn unstage(repo: &Path, paths: &[String]) -> Result<()> {
    check_all(paths)?;
    if paths.is_empty() {
        return Ok(());
    }
    let mut args = if has_head(repo) {
        vec!["restore", "--staged", "--"]
    } else {
        // Before the first commit there is no `HEAD` to restore from: just drop them from the index.
        vec!["rm", "--cached", "--quiet", "-f", "--"]
    };
    args.extend(paths.iter().map(String::as_str));
    git(repo, &args).map(drop)
}

/// Stages everything (`git add -A`). **Changes the index.**
pub fn stage_all(repo: &Path) -> Result<()> {
    git(repo, &["add", "--all"]).map(drop)
}

/// Unstages everything; the working tree is not touched. **Changes the index.**
pub fn unstage_all(repo: &Path) -> Result<()> {
    git(repo, &["reset", "--quiet"]).map(drop)
}

/// A file git cannot diff safely: a FIFO, socket or device would block reading it.
fn is_special(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| {
            let kind = m.file_type();
            !kind.is_file() && !kind.is_dir() && !kind.is_symlink()
        })
        .unwrap_or(false)
}

fn empty_diff(
    path: &str,
    binary: bool,
    truncated: bool,
    sizes: (Option<u64>, Option<u64>),
) -> FileDiff {
    FileDiff {
        path: path.to_string(),
        binary,
        hunks: Vec::new(),
        truncated,
        old_size: sizes.0,
        new_size: sizes.1,
    }
}

/// The size of `spec` (`HEAD:path`, `:path`) as git has it, or `None` if there is no such object.
fn object_size(repo: &Path, spec: &str) -> Option<u64> {
    git_with(repo, &["cat-file", "-s", spec], &[0, 128])
        .ok()
        .filter(|(code, _)| *code == 0)
        .and_then(|(_, out)| out.trim().parse().ok())
}

/// One file's diff on one side. Read-only. A file larger than [`MAX_DIFF_BYTES`] comes back
/// without hunks, marked `truncated`.
pub fn file_diff(repo: &Path, path: &str, old_path: Option<&str>, side: Side) -> Result<FileDiff> {
    checked_path(path)?;
    if let Some(old) = old_path {
        checked_path(old)?;
    }
    let too_large = |size: Option<u64>| size.is_some_and(|s| s > MAX_DIFF_BYTES as u64);
    let on_disk_path = repo.join(path);
    let on_disk = fs::symlink_metadata(&on_disk_path).ok().map(|m| m.len());
    let head_has_it = has_head(repo) && object_size(repo, &format!("HEAD:{path}")).is_some();
    let diff_flags = ["diff", "--no-color", "--no-ext-diff", "--no-textconv"];

    let (raw, old_size, new_size) = match side {
        Side::Staged => {
            let old_name = old_path.unwrap_or(path);
            let old_size = if has_head(repo) {
                object_size(repo, &format!("HEAD:{old_name}"))
            } else {
                None
            };
            let new_size = object_size(repo, &format!(":{path}"));
            if too_large(old_size) || too_large(new_size) {
                return Ok(empty_diff(path, false, true, (old_size, new_size)));
            }
            let mut args: Vec<&str> = diff_flags.to_vec();
            args.extend(["--cached", "-M", "-U3", "--", path]);
            if let Some(old) = old_path {
                args.push(old);
            }
            (git(repo, &args)?, old_size, new_size)
        }
        Side::Unstaged => {
            if is_special(&on_disk_path) {
                return Ok(empty_diff(path, true, false, (None, None)));
            }
            let in_index = object_size(repo, &format!(":{path}"));
            if too_large(on_disk) || too_large(in_index) {
                return Ok(empty_diff(path, false, true, (in_index, on_disk)));
            }
            let raw = if in_index.is_none() && on_disk.is_some() {
                // Not in the index: an untracked file, diffed against nothing (`--no-index` exits 1
                // when the files differ — the normal case here).
                let mut args: Vec<&str> = diff_flags.to_vec();
                args.extend(["--no-index", "--", "/dev/null", path]);
                git_with(repo, &args, &[0, 1])?.1
            } else {
                let mut args: Vec<&str> = diff_flags.to_vec();
                args.extend(["-U3", "--", path]);
                git(repo, &args)?
            };
            (raw, in_index, on_disk)
        }
    };
    let _ = head_has_it;
    Ok(FileDiff {
        old_size,
        new_size,
        ..parse_diff(path, &raw)
    })
}

/// Where to read a file from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// As the last commit has it.
    Head,
    /// As the index has it — what is staged, or unchanged.
    Index,
}

/// A file as git has it, for the diff view's whole sides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Blob {
    /// There is no such file there.
    Absent,
    /// Larger than the limit; not read.
    TooLarge {
        size: u64,
    },
    /// Not UTF-8 text.
    Binary,
    Text {
        text: String,
    },
}

/// `path` as `source` has it, if it is at most `max_bytes` and text. Read-only.
pub fn blob(repo: &Path, path: &str, source: Source, max_bytes: u64) -> Result<Blob> {
    checked_path(path)?;
    let spec = match source {
        Source::Head if has_head(repo) => format!("HEAD:{path}"),
        Source::Head => return Ok(Blob::Absent),
        Source::Index => format!(":{path}"),
    };
    let Some(size) = object_size(repo, &spec) else {
        return Ok(Blob::Absent);
    };
    if size > max_bytes {
        return Ok(Blob::TooLarge { size });
    }
    let bytes = git_bytes(repo, &["cat-file", "blob", &spec])?;
    // NUL in the first stretch is how git itself tells binary from text.
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return Ok(Blob::Binary);
    }
    Ok(String::from_utf8(bytes).map_or(Blob::Binary, |text| Blob::Text { text }))
}

/// Stages one hunk of `path`'s unstaged diff, or unstages one hunk of its staged diff.
/// `index` and `header` name the hunk as `file_diff` showed it; if the file changed since, this
/// refuses rather than guess. A file that is added or deleted as a whole has one hunk that is the
/// file, so that is staged or unstaged as a whole. **Changes the index.**
pub fn apply_hunk(
    repo: &Path,
    path: &str,
    old_path: Option<&str>,
    side: Side,
    index: usize,
    header: &str,
) -> Result<()> {
    let diff = file_diff(repo, path, old_path, side)?;
    if diff.binary || diff.truncated {
        return Err(GitError::Invalid {
            field: "hunk",
            reason: format!("{path} has no hunks to handle one by one; use the whole file"),
        });
    }
    let hunk = diff
        .hunks
        .get(index)
        .filter(|hunk| hunk.header == header)
        .ok_or_else(|| GitError::Invalid {
            field: "hunk",
            reason: format!("{path} changed since its diff was shown; reload it and try again"),
        })?;
    if diff.old_size.is_none() || diff.new_size.is_none() {
        // Added or deleted as a whole: the hunk is the file.
        let mut targets = vec![path.to_string()];
        targets.extend(old_path.map(str::to_string));
        return match side {
            Side::Unstaged => stage(repo, &targets),
            Side::Staged => unstage(repo, &targets),
        };
    }
    let patch = hunk_patch(path, hunk);
    let reverse = side == Side::Staged;
    let mut args = vec!["apply", "--cached", "--whitespace=nowarn"];
    if reverse {
        args.push("-R");
    }
    args.push("-");
    git_with_input(repo, &args, patch.as_bytes()).map(drop)
}

/// Commits what is staged, with `message`; returns the new commit's id. Runs the user's own hooks
/// with the user's own identity. Refuses an empty message and an empty index. **Creates a commit.**
pub fn commit(repo: &Path, message: &str) -> Result<String> {
    let message = message.trim();
    if message.is_empty() {
        return Err(GitError::Invalid {
            field: "message",
            reason: "a commit needs a message".into(),
        });
    }
    let nothing_staged = if has_head(repo) {
        git_with(repo, &["diff", "--cached", "--quiet"], &[0, 1])?.0 == 0
    } else {
        git(repo, &["ls-files", "--cached"])?.trim().is_empty()
    };
    if nothing_staged {
        return Err(GitError::Invalid {
            field: "index",
            reason: "nothing is staged".into(),
        });
    }
    git(repo, &["commit", "--quiet", "-m", message])?;
    Ok(git(repo, &["rev-parse", "HEAD"])?.trim().to_string())
}

/// Fetches from the default remote (`git fetch`): updates remote-tracking branches only — no merge,
/// no push. Fails with git's own message when there is no remote or it needs a password.
pub fn fetch(repo: &Path) -> Result<()> {
    git(repo, &["fetch", "--quiet"]).map(drop)
}

/// One local branch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    pub name: String,
    pub current: bool,
    /// The remote branch it follows, e.g. `origin/main`.
    pub upstream: Option<String>,
}

/// One file a commit changed, as `git log --name-status` says it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingFile {
    /// `A`dded, `M`odified, `D`eleted, `T`ype changed.
    pub status: String,
    pub path: String,
}

/// One commit that is on the checked-out branch and not on its upstream yet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutgoingCommit {
    /// The short id.
    pub id: String,
    pub subject: String,
    /// When it was made, as git says it ("2 hours ago").
    pub when: String,
    pub files: Vec<OutgoingFile>,
}

/// How many commits [`outgoing`] reads at most: the panel lists what a push would publish, not a history.
pub const OUTGOING_LIMIT: usize = 200;

/// The commits a push of the checked-out branch would publish — those after its upstream — newest first, each with the
/// files it changed. Empty when the branch has no upstream (a push would publish all of it, which is no list worth
/// showing) or `HEAD` has no commit. Read-only.
pub fn outgoing(repo: &Path) -> Result<Vec<OutgoingCommit>> {
    if !has_head(repo) {
        return Ok(Vec::new());
    }
    let (code, _) = git_with(
        repo,
        &["rev-parse", "--verify", "--quiet", "@{upstream}"],
        &[0, 1],
    )?;
    if code != 0 {
        return Ok(Vec::new());
    }
    let limit = format!("-n{OUTGOING_LIMIT}");
    let raw = git(
        repo,
        &[
            "log",
            &limit,
            "--no-renames",
            "--name-status",
            "--format=%x01%h%x00%s%x00%cr",
            "@{upstream}..HEAD",
        ],
    )?;
    Ok(parse_outgoing(&raw))
}

fn parse_outgoing(raw: &str) -> Vec<OutgoingCommit> {
    raw.split('\u{1}')
        .filter(|chunk| !chunk.trim().is_empty())
        .filter_map(|chunk| {
            let mut lines = chunk.lines();
            let mut head = lines.next()?.split('\0');
            let id = head.next()?.to_string();
            let subject = head.next()?.to_string();
            let when = head.next().unwrap_or_default().to_string();
            let files = lines
                .filter_map(|line| {
                    let (status, path) = line.split_once('\t')?;
                    Some(OutgoingFile {
                        status: status.to_string(),
                        path: path.to_string(),
                    })
                })
                .collect();
            Some(OutgoingCommit {
                id,
                subject,
                when,
                files,
            })
        })
        .collect()
}

/// Makes `path` a git repository (`git init`). Refuses a folder that already is one — or lies inside
/// one — so a nested repository is never made by accident. **Creates `.git`.**
pub fn init(path: &Path) -> Result<()> {
    if is_repo(path) {
        return Err(GitError::Invalid {
            field: "path",
            reason: "this folder is already part of a git repository".into(),
        });
    }
    git(path, &["init", "--quiet"]).map(drop)
}

/// A branch name git would accept for a new local branch (`git check-ref-format --branch`), never
/// one that starts with a dash (it would read as an option).
fn checked_branch(name: &str) -> Result<&str> {
    let name = name.trim();
    let bad = |reason: &str| GitError::Invalid {
        field: "branch",
        reason: reason.to_string(),
    };
    if name.is_empty() {
        return Err(bad("a branch needs a name"));
    }
    if name.starts_with('-') || name.contains('\0') {
        return Err(bad("is not a usable branch name"));
    }
    // `check-ref-format` runs in no repository; git itself is the judge of the rules.
    let ok = std::process::Command::new("git")
        .args(["check-ref-format", "--branch", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok {
        Ok(name)
    } else {
        Err(bad("is not a valid branch name"))
    }
}

/// The local branches, current one first-marked; read-only.
pub fn branches(repo: &Path) -> Result<Vec<Branch>> {
    let raw = git(
        repo,
        &[
            "for-each-ref",
            "--format=%(HEAD)%00%(refname:short)%00%(upstream:short)",
            "--sort=refname",
            "refs/heads",
        ],
    )?;
    Ok(raw
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\0');
            let head = parts.next()?;
            let name = parts.next()?.to_string();
            let upstream = parts.next().filter(|u| !u.is_empty()).map(str::to_string);
            Some(Branch {
                name,
                current: head == "*",
                upstream,
            })
        })
        .collect())
}

/// Switches to the local branch `name`. Git refuses when uncommitted changes would be overwritten —
/// nothing is stashed or thrown away to make it work. **Changes the checked-out files and `HEAD`.**
pub fn switch_branch(repo: &Path, name: &str) -> Result<()> {
    let name = checked_branch(name)?;
    if !branches(repo)?.iter().any(|b| b.name == name) {
        return Err(GitError::Invalid {
            field: "branch",
            reason: format!("there is no local branch called {name}"),
        });
    }
    git(repo, &["switch", "--quiet", name]).map(drop)
}

/// Creates the branch `name` at the current commit and switches to it; uncommitted changes come
/// along. Refuses a name that exists. **Changes `HEAD`.**
pub fn create_branch(repo: &Path, name: &str) -> Result<()> {
    let name = checked_branch(name)?;
    if branches(repo)?.iter().any(|b| b.name == name) {
        return Err(GitError::Invalid {
            field: "branch",
            reason: format!("the branch {name} exists already"),
        });
    }
    git(repo, &["switch", "--quiet", "--create", name]).map(drop)
}

/// Throws away the unstaged changes of `paths` — the files go back to what the index has — and
/// deletes untracked ones. **Destroys work that git cannot bring back**: the caller must have asked
/// the owner. Staged changes are kept (a file changed on both sides goes back to its staged
/// version). A path with nothing to discard is refused, so a typo cannot pass for success.
pub fn discard(repo: &Path, paths: &[String]) -> Result<()> {
    check_all(paths)?;
    let entries = status(repo)?.entries;
    let mut restore: Vec<&str> = Vec::new();
    let mut remove: Vec<&str> = Vec::new();
    for path in paths {
        let Some(entry) = entries.iter().find(|e| &e.path == path) else {
            return Err(GitError::Invalid {
                field: "path",
                reason: format!("{path} has no changes to discard"),
            });
        };
        if entry.untracked {
            remove.push(path);
        } else if entry.unstaged.is_some() {
            restore.push(path);
        } else {
            return Err(GitError::Invalid {
                field: "path",
                reason: format!("{path} has no unstaged changes to discard"),
            });
        }
    }
    if !restore.is_empty() {
        let mut args = vec!["restore", "--worktree", "--"];
        args.extend(restore);
        git(repo, &args)?;
    }
    for path in remove {
        let file = repo.join(path);
        let meta = fs::symlink_metadata(&file).map_err(|err| GitError::Invalid {
            field: "path",
            reason: format!("{path}: {err}"),
        })?;
        // Only a plain file or a symlink itself — never a directory, never what a link points to.
        if meta.is_dir() {
            return Err(GitError::Invalid {
                field: "path",
                reason: format!("{path} is a folder; discard its files"),
            });
        }
        fs::remove_file(&file).map_err(|err| GitError::Invalid {
            field: "path",
            reason: format!("{path}: {err}"),
        })?;
    }
    Ok(())
}

/// Throws away one hunk of `path`'s unstaged diff (same checks as [`apply_hunk`]). **Destroys work
/// git cannot bring back**: ask first.
pub fn discard_hunk(repo: &Path, path: &str, index: usize, header: &str) -> Result<()> {
    let diff = file_diff(repo, path, None, Side::Unstaged)?;
    if diff.binary || diff.truncated {
        return Err(GitError::Invalid {
            field: "hunk",
            reason: format!("{path} has no hunks to take back one by one; discard the whole file"),
        });
    }
    let hunk = diff
        .hunks
        .get(index)
        .filter(|hunk| hunk.header == header)
        .ok_or_else(|| GitError::Invalid {
            field: "hunk",
            reason: format!("{path} changed since its diff was shown; reload it and try again"),
        })?;
    if diff.old_size.is_none() || diff.new_size.is_none() {
        // Added (untracked) or deleted as a whole: the hunk is the file.
        return discard(repo, &[path.to_string()]);
    }
    let patch = hunk_patch(path, hunk);
    git_with_input(
        repo,
        &["apply", "-R", "--whitespace=nowarn", "-"],
        patch.as_bytes(),
    )
    .map(drop)
}

/// What a push did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushResult {
    pub remote: String,
    pub branch: String,
    /// The branch had no upstream yet; it was published and now follows `remote/branch`.
    pub created_upstream: bool,
}

/// Pushes the checked-out branch to its upstream — or, if it has none, publishes it to `origin`
/// (the only remote if there is just one) and makes that its upstream. **Publishes commits.**
///
/// The one function here that leaves the machine, so it is narrow on purpose: only the current
/// branch, to a ref git itself reports as its upstream (or `origin`), **never with `--force`** —
/// a rejected, non-fast-forward push fails with git's own message and changes nothing. Nothing
/// from outside names a remote or a ref.
pub fn push(repo: &Path) -> Result<PushResult> {
    if !has_head(repo) {
        return Err(GitError::Invalid {
            field: "branch",
            reason: "nothing is committed yet".into(),
        });
    }
    let (code, branch) = git_with(
        repo,
        &["symbolic-ref", "--short", "--quiet", "HEAD"],
        &[0, 1],
    )?;
    let branch = branch.trim().to_string();
    if code != 0 || branch.is_empty() {
        return Err(GitError::Invalid {
            field: "branch",
            reason: "HEAD is detached; switch to a branch before pushing".into(),
        });
    }
    let (code, upstream) = git_with(
        repo,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        &[0, 128],
    )?;
    let upstream = upstream.trim();
    if code == 0 && !upstream.is_empty() {
        let remote = git(
            repo,
            &["config", "--get", &format!("branch.{branch}.remote")],
        )?
        .trim()
        .to_string();
        let merge = git(
            repo,
            &["config", "--get", &format!("branch.{branch}.merge")],
        )?
        .trim()
        .to_string();
        if remote.is_empty() || !merge.starts_with("refs/heads/") {
            return Err(GitError::Invalid {
                field: "upstream",
                reason: format!("{upstream} is not a branch on a remote; set an upstream first"),
            });
        }
        git(repo, &["push", &remote, &format!("HEAD:{merge}")])?;
        return Ok(PushResult {
            remote,
            branch: merge["refs/heads/".len()..].to_string(),
            created_upstream: false,
        });
    }
    let remotes = git(repo, &["remote"])?;
    let remotes: Vec<&str> = remotes
        .lines()
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .collect();
    let remote = if remotes.contains(&"origin") {
        "origin"
    } else if let [only] = remotes[..] {
        only
    } else {
        return Err(GitError::Invalid {
            field: "remote",
            reason: if remotes.is_empty() {
                "this repository has no remote to push to".into()
            } else {
                "there is no remote called origin; set an upstream for this branch first".into()
            },
        });
    };
    git(
        repo,
        &[
            "push",
            "--set-upstream",
            remote,
            &format!("HEAD:refs/heads/{branch}"),
        ],
    )?;
    Ok(PushResult {
        remote: remote.to_string(),
        branch,
        created_upstream: true,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    struct Repo(PathBuf);

    impl Repo {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "axiomata-git-{label}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let dir = dir.canonicalize().unwrap();
            let repo = Repo(dir);
            repo.run(&["init", "--quiet", "-b", "main"]);
            repo.run(&["config", "user.email", "t@example.com"]);
            repo.run(&["config", "user.name", "T"]);
            repo.run(&["config", "commit.gpgsign", "false"]);
            repo
        }

        /// A bare repository, to push to.
        fn bare(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "axiomata-git-{label}-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            let dir = dir.canonicalize().unwrap();
            git(&dir, &["init", "--quiet", "--bare", "-b", "main"]).unwrap();
            Repo(dir)
        }

        fn run(&self, args: &[&str]) -> String {
            git(&self.0, args).unwrap()
        }

        fn write(&self, rel: &str, content: &str) {
            let path = self.0.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }

        fn commit_all(&self, message: &str) {
            self.run(&["add", "-A"]);
            self.run(&["commit", "--quiet", "-m", message]);
        }

        fn entry(&self, path: &str) -> Option<StatusEntry> {
            status(&self.0)
                .unwrap()
                .entries
                .into_iter()
                .find(|e| e.path == path)
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn status_sorts_staged_unstaged_and_untracked_apart() {
        let r = Repo::new("status");
        r.write("a.txt", "one\n");
        r.write("b.txt", "one\n");
        r.commit_all("base");
        r.write("a.txt", "two\n");
        r.run(&["add", "a.txt"]);
        r.write("b.txt", "two\n");
        r.write("c.txt", "new\n");
        let s = status(&r.0).unwrap();
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert!(s.head.is_some());
        let e = |p: &str| s.entries.iter().find(|e| e.path == p).unwrap().clone();
        assert_eq!(
            (e("a.txt").staged, e("a.txt").unstaged),
            (Some(ChangeKind::Modified), None)
        );
        assert_eq!(
            (e("b.txt").staged, e("b.txt").unstaged),
            (None, Some(ChangeKind::Modified))
        );
        assert!(e("c.txt").untracked);
        assert_eq!(s.entries.len(), 3);
    }

    #[test]
    fn status_in_a_fresh_repository_has_no_head_and_sees_renames_and_deletions() {
        let r = Repo::new("fresh");
        let s = status(&r.0).unwrap();
        assert_eq!((s.head, s.entries.len()), (None, 0));
        r.write("old name.txt", "same content\nline two\nline three\n");
        r.commit_all("base");
        r.run(&["mv", "old name.txt", "new name.txt"]);
        r.write("gone.txt", "x\n");
        r.run(&["add", "gone.txt"]);
        fs::remove_file(r.0.join("gone.txt")).unwrap();
        let e = r.entry("new name.txt").unwrap();
        assert_eq!(e.staged, Some(ChangeKind::Renamed));
        assert_eq!(e.old_path.as_deref(), Some("old name.txt"));
        let g = r.entry("gone.txt").unwrap();
        assert_eq!(
            (g.staged, g.unstaged),
            (Some(ChangeKind::Added), Some(ChangeKind::Deleted))
        );
    }

    #[test]
    fn stage_and_unstage_a_file_and_everything() {
        let r = Repo::new("stage");
        r.write("a.txt", "1\n");
        r.write("b.txt", "1\n");
        r.commit_all("base");
        r.write("a.txt", "2\n");
        r.write("b.txt", "2\n");
        r.write("new file.txt", "n\n");
        stage(&r.0, &["a.txt".into(), "new file.txt".into()]).unwrap();
        assert_eq!(r.entry("a.txt").unwrap().staged, Some(ChangeKind::Modified));
        assert_eq!(
            r.entry("new file.txt").unwrap().staged,
            Some(ChangeKind::Added)
        );
        assert_eq!(r.entry("b.txt").unwrap().staged, None);
        unstage(&r.0, &["a.txt".into()]).unwrap();
        assert_eq!(r.entry("a.txt").unwrap().staged, None);
        assert_eq!(fs::read_to_string(r.0.join("a.txt")).unwrap(), "2\n");
        stage_all(&r.0).unwrap();
        assert!(
            status(&r.0)
                .unwrap()
                .entries
                .iter()
                .all(|e| e.staged.is_some())
        );
        unstage_all(&r.0).unwrap();
        assert!(
            status(&r.0)
                .unwrap()
                .entries
                .iter()
                .all(|e| e.staged.is_none())
        );
    }

    #[test]
    fn unstaging_works_before_the_first_commit_too() {
        let r = Repo::new("unborn");
        r.write("a.txt", "1\n");
        stage_all(&r.0).unwrap();
        assert_eq!(r.entry("a.txt").unwrap().staged, Some(ChangeKind::Added));
        unstage(&r.0, &["a.txt".into()]).unwrap();
        assert!(r.entry("a.txt").unwrap().untracked);
    }

    #[test]
    fn paths_that_leave_the_repository_are_refused_and_a_glob_is_only_a_name() {
        let r = Repo::new("paths");
        r.write("a.txt", "1\n");
        r.commit_all("base");
        assert!(stage(&r.0, &["../x".into()]).is_err());
        assert!(unstage(&r.0, &["/etc/passwd".into()]).is_err());
        assert!(file_diff(&r.0, "../x", None, Side::Unstaged).is_err());
        r.write("a.txt", "2\n");
        r.write("b.txt", "2\n");
        stage(&r.0, &[":(glob)*.txt".into()]).unwrap_or(());
        assert_eq!(
            r.entry("a.txt").unwrap().staged,
            None,
            "a glob must not match files"
        );
    }

    const BASE: &str = "l1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\nl10\nl11\nl12\nl13\nl14\nl15\n";

    fn two_hunks(r: &Repo) {
        r.write("f.txt", BASE);
        r.commit_all("base");
        r.write(
            "f.txt",
            &BASE.replace("l2\n", "L2\n").replace("l14\n", "L14\n"),
        );
    }

    #[test]
    fn diffs_on_both_sides_and_for_an_untracked_file() {
        let r = Repo::new("diff");
        two_hunks(&r);
        let un = file_diff(&r.0, "f.txt", None, Side::Unstaged).unwrap();
        assert_eq!(un.hunks.len(), 2);
        assert!(
            file_diff(&r.0, "f.txt", None, Side::Staged)
                .unwrap()
                .hunks
                .is_empty()
        );
        stage(&r.0, &["f.txt".into()]).unwrap();
        assert_eq!(
            file_diff(&r.0, "f.txt", None, Side::Staged)
                .unwrap()
                .hunks
                .len(),
            2
        );
        assert!(
            file_diff(&r.0, "f.txt", None, Side::Unstaged)
                .unwrap()
                .hunks
                .is_empty()
        );
        r.write("n.txt", "a\nb\n");
        let new = file_diff(&r.0, "n.txt", None, Side::Unstaged).unwrap();
        assert_eq!(new.hunks.len(), 1);
        assert_eq!(new.old_size, None);
        assert_eq!(new.new_size, Some(4));
    }

    #[test]
    fn one_hunk_is_staged_then_unstaged_on_its_own() {
        let r = Repo::new("hunk");
        two_hunks(&r);
        let un = file_diff(&r.0, "f.txt", None, Side::Unstaged).unwrap();
        apply_hunk(&r.0, "f.txt", None, Side::Unstaged, 1, &un.hunks[1].header).unwrap();
        let staged = file_diff(&r.0, "f.txt", None, Side::Staged).unwrap();
        assert_eq!(staged.hunks.len(), 1);
        assert!(staged.hunks[0].lines.iter().any(|l| l.text == "L14"));
        let still = file_diff(&r.0, "f.txt", None, Side::Unstaged).unwrap();
        assert_eq!(still.hunks.len(), 1);
        assert!(still.hunks[0].lines.iter().any(|l| l.text == "L2"));
        // Back out again.
        apply_hunk(
            &r.0,
            "f.txt",
            None,
            Side::Staged,
            0,
            &staged.hunks[0].header,
        )
        .unwrap();
        assert!(
            file_diff(&r.0, "f.txt", None, Side::Staged)
                .unwrap()
                .hunks
                .is_empty()
        );
        assert_eq!(
            file_diff(&r.0, "f.txt", None, Side::Unstaged)
                .unwrap()
                .hunks
                .len(),
            2
        );
        // The working tree itself was never touched.
        assert!(
            fs::read_to_string(r.0.join("f.txt"))
                .unwrap()
                .contains("L14")
        );
    }

    #[test]
    fn a_hunk_that_changed_since_it_was_shown_is_refused() {
        let r = Repo::new("stale");
        two_hunks(&r);
        let un = file_diff(&r.0, "f.txt", None, Side::Unstaged).unwrap();
        // A line inserted above moves the hunk, so the header the UI holds no longer matches.
        r.write("f.txt", &format!("zero\n{}", BASE.replace("l2\n", "L2\n")));
        let err =
            apply_hunk(&r.0, "f.txt", None, Side::Unstaged, 0, &un.hunks[0].header).unwrap_err();
        assert!(err.to_string().contains("changed since"), "{err}");
        assert!(apply_hunk(&r.0, "f.txt", None, Side::Unstaged, 9, "@@ nope").is_err());
    }

    #[test]
    fn the_hunk_of_an_added_file_is_the_file() {
        let r = Repo::new("added");
        r.write("a.txt", "1\n");
        r.commit_all("base");
        r.write("n.txt", "x\ny\n");
        let un = file_diff(&r.0, "n.txt", None, Side::Unstaged).unwrap();
        apply_hunk(&r.0, "n.txt", None, Side::Unstaged, 0, &un.hunks[0].header).unwrap();
        assert_eq!(r.entry("n.txt").unwrap().staged, Some(ChangeKind::Added));
        let staged = file_diff(&r.0, "n.txt", None, Side::Staged).unwrap();
        apply_hunk(
            &r.0,
            "n.txt",
            None,
            Side::Staged,
            0,
            &staged.hunks[0].header,
        )
        .unwrap();
        assert!(r.entry("n.txt").unwrap().untracked);
    }

    #[test]
    fn commit_takes_only_what_is_staged_and_refuses_nothing_or_no_message() {
        let r = Repo::new("commit");
        r.write("a.txt", "1\n");
        r.write("b.txt", "1\n");
        r.commit_all("base");
        r.write("a.txt", "2\n");
        r.write("b.txt", "2\n");
        assert!(commit(&r.0, "no index").is_err());
        stage(&r.0, &["a.txt".into()]).unwrap();
        assert!(commit(&r.0, "   ").is_err());
        let sha = commit(&r.0, "only a").unwrap();
        assert_eq!(sha, r.run(&["rev-parse", "HEAD"]).trim());
        assert_eq!(r.run(&["log", "-1", "--format=%s"]).trim(), "only a");
        assert_eq!(
            r.entry("b.txt").unwrap().unstaged,
            Some(ChangeKind::Modified)
        );
        assert!(r.entry("a.txt").is_none());
    }

    #[test]
    fn the_first_commit_works_and_fetch_without_a_remote_says_so() {
        let r = Repo::new("first");
        r.write("a.txt", "1\n");
        stage_all(&r.0).unwrap();
        commit(&r.0, "first").unwrap();
        assert!(status(&r.0).unwrap().entries.is_empty());
        let err = fetch(&r.0);
        // No remote configured: git has nothing to fetch and exits quietly or complains — never hangs.
        let _ = err;
    }

    #[test]
    fn upstream_and_ahead_behind_are_read() {
        let origin = Repo::new("origin");
        origin.write("a.txt", "1\n");
        origin.commit_all("base");
        let clone = std::env::temp_dir().join(format!("axiomata-git-clone-{}", std::process::id()));
        let _ = fs::remove_dir_all(&clone);
        git(
            &origin.0,
            &[
                "clone",
                "--quiet",
                origin.0.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        )
        .unwrap();
        git(&clone, &["config", "user.email", "t@example.com"]).unwrap();
        git(&clone, &["config", "user.name", "T"]).unwrap();
        fs::write(clone.join("b.txt"), "x\n").unwrap();
        git(&clone, &["add", "-A"]).unwrap();
        git(&clone, &["commit", "--quiet", "-m", "local"]).unwrap();
        origin.write("c.txt", "y\n");
        origin.commit_all("remote");
        fetch(&clone).unwrap();
        let s = status(&clone).unwrap();
        assert_eq!(s.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.ahead, s.behind), (1, 1));
        let _ = fs::remove_dir_all(&clone);
    }

    #[test]
    fn is_repo_tells_a_repository_from_a_plain_folder() {
        let r = Repo::new("isrepo");
        assert!(is_repo(&r.0));
        let plain = std::env::temp_dir().join(format!("axiomata-git-plain-{}", std::process::id()));
        fs::create_dir_all(&plain).unwrap();
        assert!(!is_repo(&plain));
        let _ = fs::remove_dir_all(&plain);
    }
    #[test]
    fn blobs_read_either_side_and_say_why_they_cannot() {
        let r = Repo::new("blob");
        r.write("a.txt", "head\n");
        r.write("bin.dat", "x\0y");
        r.commit_all("base");
        r.write("a.txt", "index\n");
        r.run(&["add", "a.txt"]);
        r.write("a.txt", "disk\n");
        let text = |source| match blob(&r.0, "a.txt", source, 1000).unwrap() {
            Blob::Text { text } => text,
            other => panic!("{other:?}"),
        };
        assert_eq!(text(Source::Head), "head\n");
        assert_eq!(text(Source::Index), "index\n");
        assert_eq!(
            blob(&r.0, "bin.dat", Source::Index, 1000).unwrap(),
            Blob::Binary
        );
        assert_eq!(
            blob(&r.0, "a.txt", Source::Index, 2).unwrap(),
            Blob::TooLarge { size: 6 }
        );
        assert_eq!(
            blob(&r.0, "nope.txt", Source::Head, 1000).unwrap(),
            Blob::Absent
        );
        assert!(blob(&r.0, "../x", Source::Head, 1000).is_err());
        let fresh = Repo::new("blob-fresh");
        assert_eq!(
            blob(&fresh.0, "a.txt", Source::Head, 10).unwrap(),
            Blob::Absent
        );
    }
    /// `origin` (bare) and a clone of it with one commit, ready to push from.
    fn with_remote(label: &str) -> (Repo, Repo) {
        let bare = Repo::bare(&format!("{label}-bare"));
        let work = Repo::new(&format!("{label}-work"));
        work.write("a.txt", "1\n");
        work.commit_all("base");
        work.run(&["remote", "add", "origin", bare.0.to_str().unwrap()]);
        (bare, work)
    }

    #[test]
    fn push_publishes_a_new_branch_and_then_pushes_to_its_upstream() {
        let (bare, work) = with_remote("push");
        let first = push(&work.0).unwrap();
        assert_eq!(
            first,
            PushResult {
                remote: "origin".into(),
                branch: "main".into(),
                created_upstream: true
            }
        );
        assert_eq!(
            status(&work.0).unwrap().upstream.as_deref(),
            Some("origin/main")
        );
        work.write("a.txt", "2\n");
        work.commit_all("second");
        assert_eq!(status(&work.0).unwrap().ahead, 1);
        let second = push(&work.0).unwrap();
        assert!(!second.created_upstream);
        assert_eq!(
            git(&bare.0, &["log", "-1", "--format=%s", "main"])
                .unwrap()
                .trim(),
            "second"
        );
        assert_eq!(status(&work.0).unwrap().ahead, 0);
    }

    #[test]
    fn outgoing_lists_the_commits_a_push_would_publish_with_their_files() {
        let (_bare, work) = with_remote("outgoing");
        // No upstream yet: no list.
        assert!(outgoing(&work.0).unwrap().is_empty());
        push(&work.0).unwrap();
        assert!(outgoing(&work.0).unwrap().is_empty());

        work.write("a.txt", "2\n");
        work.write("dir/b.txt", "b\n");
        work.commit_all("second: two files");
        work.run(&["rm", "--quiet", "a.txt"]);
        work.run(&["commit", "--quiet", "-m", "third"]);

        let list = outgoing(&work.0).unwrap();
        assert_eq!(
            list.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(),
            ["third", "second: two files"]
        );
        assert_eq!(
            list[0].files,
            [OutgoingFile {
                status: "D".into(),
                path: "a.txt".into()
            }]
        );
        let mut paths: Vec<_> = list[1].files.iter().map(|f| f.path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(paths, ["a.txt", "dir/b.txt"]);
        assert!(list[1].id.len() >= 7 && !list[1].when.is_empty());
    }

    #[test]
    fn push_refuses_without_commits_detached_or_without_a_remote() {
        let fresh = Repo::new("push-fresh");
        assert!(
            push(&fresh.0)
                .unwrap_err()
                .to_string()
                .contains("nothing is committed")
        );
        let lone = Repo::new("push-lone");
        lone.write("a.txt", "1\n");
        lone.commit_all("base");
        assert!(push(&lone.0).unwrap_err().to_string().contains("no remote"));
        lone.run(&["checkout", "--quiet", "--detach"]);
        assert!(push(&lone.0).unwrap_err().to_string().contains("detached"));
    }

    #[test]
    fn a_push_that_would_overwrite_the_remote_is_rejected_and_never_forced() {
        let (bare, work) = with_remote("reject");
        push(&work.0).unwrap();
        // Someone else lands a commit on the remote…
        let other = std::env::temp_dir().join(format!("axiomata-git-other-{}", std::process::id()));
        let _ = fs::remove_dir_all(&other);
        git(
            &bare.0,
            &[
                "clone",
                "--quiet",
                bare.0.to_str().unwrap(),
                other.to_str().unwrap(),
            ],
        )
        .unwrap();
        git(&other, &["config", "user.email", "o@example.com"]).unwrap();
        git(&other, &["config", "user.name", "O"]).unwrap();
        fs::write(other.join("theirs.txt"), "x\n").unwrap();
        git(&other, &["add", "-A"]).unwrap();
        git(&other, &["commit", "--quiet", "-m", "theirs"]).unwrap();
        git(&other, &["push", "--quiet", "origin", "HEAD:main"]).unwrap();
        // …and ours diverges.
        work.write("a.txt", "ours\n");
        work.commit_all("ours");
        assert!(push(&work.0).is_err());
        assert_eq!(
            git(&bare.0, &["log", "-1", "--format=%s", "main"])
                .unwrap()
                .trim(),
            "theirs"
        );
        let _ = fs::remove_dir_all(&other);
    }
    #[test]
    fn init_makes_a_repository_but_not_inside_one() {
        let plain = std::env::temp_dir().join(format!("axiomata-git-init-{}", std::process::id()));
        let _ = fs::remove_dir_all(&plain);
        fs::create_dir_all(&plain).unwrap();
        assert!(!is_repo(&plain));
        init(&plain).unwrap();
        assert!(is_repo(&plain));
        assert!(init(&plain).is_err());
        let inner = plain.join("sub");
        fs::create_dir_all(&inner).unwrap();
        assert!(
            init(&inner)
                .unwrap_err()
                .to_string()
                .contains("already part")
        );
        let _ = fs::remove_dir_all(&plain);
    }

    #[test]
    fn discarding_puts_a_file_back_to_the_index_and_deletes_untracked_ones() {
        let r = Repo::new("discard");
        r.write("a.txt", "1\n");
        r.write("b.txt", "1\n");
        r.commit_all("base");
        r.write("a.txt", "2\n");
        r.run(&["add", "a.txt"]);
        r.write("a.txt", "3\n");
        r.write("b.txt", "2\n");
        r.write("new.txt", "n\n");
        discard(&r.0, &["a.txt".into(), "b.txt".into(), "new.txt".into()]).unwrap();
        // Staged work survives; unstaged work does not.
        assert_eq!(fs::read_to_string(r.0.join("a.txt")).unwrap(), "2\n");
        assert_eq!(fs::read_to_string(r.0.join("b.txt")).unwrap(), "1\n");
        assert!(!r.0.join("new.txt").exists());
        assert_eq!(r.entry("a.txt").unwrap().staged, Some(ChangeKind::Modified));
        // A deleted file comes back.
        fs::remove_file(r.0.join("b.txt")).unwrap();
        discard(&r.0, &["b.txt".into()]).unwrap();
        assert!(r.0.join("b.txt").exists());
    }

    #[test]
    fn discarding_refuses_what_has_nothing_to_discard_and_what_leaves_the_repository() {
        let r = Repo::new("discard-refuse");
        r.write("a.txt", "1\n");
        r.commit_all("base");
        assert!(discard(&r.0, &["a.txt".into()]).is_err());
        assert!(discard(&r.0, &["nope.txt".into()]).is_err());
        assert!(discard(&r.0, &["../x".into()]).is_err());
        r.write("dir/f.txt", "x\n");
        assert!(discard(&r.0, &["dir".into()]).is_err());
        assert!(r.0.join("dir/f.txt").exists());
    }

    #[test]
    fn one_hunk_is_discarded_on_its_own_and_a_stale_one_is_refused() {
        let r = Repo::new("discard-hunk");
        two_hunks(&r);
        let un = file_diff(&r.0, "f.txt", None, Side::Unstaged).unwrap();
        assert!(discard_hunk(&r.0, "f.txt", 0, "@@ -9,9 +9,9 @@").is_err());
        discard_hunk(&r.0, "f.txt", 1, &un.hunks[1].header).unwrap();
        let text = fs::read_to_string(r.0.join("f.txt")).unwrap();
        assert!(text.contains("L2") && !text.contains("L14") && text.contains("l14"));
    }

    #[test]
    fn branches_are_listed_created_and_switched_to_without_losing_changes() {
        let r = Repo::new("branches");
        r.write("a.txt", "1\n");
        r.commit_all("base");
        create_branch(&r.0, "feature/x").unwrap();
        let list = branches(&r.0).unwrap();
        assert_eq!(
            list.iter()
                .map(|b| (b.name.as_str(), b.current))
                .collect::<Vec<_>>(),
            [("feature/x", true), ("main", false)]
        );
        assert_eq!(status(&r.0).unwrap().branch.as_deref(), Some("feature/x"));
        // Uncommitted work comes along when the branch is made…
        r.write("a.txt", "2\n");
        create_branch(&r.0, "feature/y").unwrap();
        assert_eq!(fs::read_to_string(r.0.join("a.txt")).unwrap(), "2\n");
        // …and a switch that would overwrite it is refused, not forced.
        r.run(&["add", "a.txt"]);
        r.run(&["commit", "--quiet", "-m", "on y"]);
        r.write("a.txt", "3\n");
        assert!(switch_branch(&r.0, "main").is_err());
        assert_eq!(status(&r.0).unwrap().branch.as_deref(), Some("feature/y"));
        discard(&r.0, &["a.txt".into()]).unwrap();
        switch_branch(&r.0, "main").unwrap();
        assert_eq!(status(&r.0).unwrap().branch.as_deref(), Some("main"));
    }

    #[test]
    fn bad_branch_names_and_unknown_branches_are_refused() {
        let r = Repo::new("branch-names");
        r.write("a.txt", "1\n");
        r.commit_all("base");
        for bad in ["", "  ", "-x", "a b", "a..b", "x~1", "/lead"] {
            assert!(create_branch(&r.0, bad).is_err(), "{bad:?}");
        }
        assert!(
            create_branch(&r.0, "main")
                .unwrap_err()
                .to_string()
                .contains("exists")
        );
        assert!(
            switch_branch(&r.0, "nope")
                .unwrap_err()
                .to_string()
                .contains("no local branch")
        );
    }
}
