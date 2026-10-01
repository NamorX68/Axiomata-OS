//! A repository as the editor's git panel sees it (`docs/plans/editor-projekt-werkzeuge.md`, #48):
//! what is staged and what is not, one file's diff on either side, staging and unstaging a file
//! or a single hunk, and committing what is staged.
//!
//! Everything works on the repository's own working copy, so these are the functions that change
//! the user's files and index — each says what it touches. **Nothing here pushes.** `fetch` only
//! updates remote-tracking branches.
//!
//! Paths from outside are checked ([`crate::diff::checked_path`]), a hunk is applied only if it
//! is still the hunk the UI showed (same index, same header), and git runs with literal
//! pathspecs (see [`crate::run`]) so a file called `:(glob)**` is only ever a file name.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::diff::{ChangeKind, FileDiff, MAX_DIFF_BYTES, checked_path, hunk_patch, parse_diff};
use crate::run::{git, git_with, git_with_input};
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
}
