//! What an agent has changed, and what can be done about it (M7.3 CP7).
//!
//! Every agent works on its own branch in its own worktree (CP5). This module
//! answers "what has this agent changed?" against the branch it was cut from,
//! reads either side of one file for the Diffs tab — the worktree's own content
//! via [`file_diff`], the base's via [`base_blob`] (H2, H8) — and carries out
//! the few things the IDE lets you do about it: throw a file's changes away —
//! whole ([`discard`]) or one hunk at a time ([`discard_hunk`], H6) — commit
//! what the agent left uncommitted, and take the agent's work over into the
//! project's own working copy. Decisions G1–G13 and H1–H16 in
//! `docs/plans/git-layer.md`.
//!
//! Driven by the `git` command line like [`crate::worktree`] (question F3), and
//! for the same reasons — above all because a commit made here runs the
//! user's own hooks with the user's own identity. Everything git hands back is
//! read in its machine format (`-z`, `--numstat`), never guessed from what a
//! human would see.
//!
//! ⚠️ Two promises worth knowing before changing anything:
//!
//! * **Paths from outside are checked** ([`checked_path`]): relative, no `..`,
//!   and a file that is deleted must really live inside the worktree. The UI
//!   sends paths back that it once got from here, but "once got from here" is
//!   not something this module can verify.
//! * **[`take_over`] is the only function that touches the user's own working
//!   copy**, and it refuses rather than guesses: wrong branch checked out,
//!   something staged, the agent's work not committed, no message given, or
//!   nothing on the agent's branch beyond what the base already has. A
//!   conflict is undone before it returns, so the working copy is never left
//!   half-merged. Whether the agent is mid-turn is a separate refusal, one
//!   level up in `provision::TakeOverTarget::run` (G12) — `AgentRepo::take_over`
//!   itself is crate-private, so nothing can reach it without going through
//!   that check first.
//!
//! Nothing here ever pushes.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};

use crate::worktree::{self, git, git_bytes, git_with, git_with_input};
use crate::{IdeError, Result};

// The diff types and parser are shared with the editor's git panel and live in `axiomata-git`;
// they stay reachable here under their old names.
pub use axiomata_git::diff::{
    ChangeKind, DiffLine, FileDiff, Hunk, LineKind, MAX_DIFF_BYTES, MAX_DIFF_LINES,
};
use axiomata_git::diff::{hunk_patch, parse_diff};

/// Checks a path that came from outside: relative, no `..`, not empty.
pub fn checked_path(path: &str) -> Result<PathBuf> {
    Ok(axiomata_git::diff::checked_path(path)?)
}

/// How much of an untracked file is read to count its lines and to tell text
/// from binary.
const MAX_UNTRACKED_BYTES: u64 = 2 * 1024 * 1024;

/// What an agent's changes are measured against (G1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Base {
    /// The branch, e.g. `main`.
    pub branch: String,
    /// The merge base of that branch and the agent's `HEAD` — the commit the
    /// diff starts from. Not the branch's tip: work that landed on `main`
    /// after the agent started is not the agent's, and against the tip it
    /// would show up as the agent reverting it.
    pub commit: String,
    /// True when `branch` was not recorded for this agent but is simply what
    /// the project folder has checked out (an agent from before M7.3).
    pub fallback: bool,
}

/// One changed file, as the Diffs tab lists it (G2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    /// Relative to the worktree root, `/`-separated.
    pub path: String,
    /// The old path of a rename.
    pub old_path: Option<String>,
    /// How the file differs from the base.
    pub kind: ChangeKind,
    /// `None` for a binary file, where git does not count lines.
    pub additions: Option<u32>,
    /// `None` for a binary file, where git does not count lines.
    pub deletions: Option<u32>,
    /// Content git cannot diff as text — no line counts, hunks read as opaque.
    pub binary: bool,
    /// Some of this file's change is not committed yet (G2).
    pub uncommitted: bool,
}

/// How to take an agent's work over (G7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakeOverMode {
    /// One new commit with the user's message; the default.
    Squash,
    /// A merge commit that keeps the agent's commits.
    NoFf,
}

/// What [`take_over`] did. A conflict is an ordinary outcome, not an error —
/// the same stance as the crate's error type takes on "not found".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum TakeOver {
    /// Taken over; `commit` is the new commit on the base branch.
    Done { commit: String },
    /// Conflicted and was undone; the working copy is as it was (G9).
    Conflict { files: Vec<String> },
}

/// Everything this module needs to know about one agent's repository — built
/// from the database once (`provision::agent_repo`), then used without it, so
/// no database connection is held while git runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRepo {
    /// The project folder — the user's own working copy.
    pub repo_root: PathBuf,
    /// Where this agent's own checkout lives (CP5).
    pub worktree: PathBuf,
    /// The branch this agent works on, in its own worktree (CP5).
    pub agent_branch: String,
    /// The recorded base (G1); `None` falls back to the project folder's branch.
    pub base_branch: Option<String>,
}

/// What the Diffs tab shows for one agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentChanges {
    /// What `files` is measured against.
    pub base: Base,
    /// Every file that differs from the base, sorted by path (G2).
    pub files: Vec<FileChange>,
}

/// One file as the base has it — the left side of the diff view (H2, H8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseBlob {
    /// The base has no such file: the agent added it.
    Absent,
    /// Larger than the limit the caller gave; not read.
    TooLarge {
        /// In bytes, as the base's tree records it.
        size: u64,
    },
    /// The file's content, byte for byte.
    Bytes(Vec<u8>),
}

impl AgentRepo {
    /// The base this agent's work is measured against (G1).
    pub fn base(&self) -> Result<Base> {
        resolve_base(&self.repo_root, &self.worktree, self.base_branch.as_deref())
    }

    /// Everything the agent has changed since its base (G2).
    pub fn changes(&self) -> Result<AgentChanges> {
        let base = self.base()?;
        let files = changes(&self.worktree, &base.commit)?;
        Ok(AgentChanges { base, files })
    }

    /// One file's diff against the base.
    pub fn file_diff(&self, path: &str, old_path: Option<&str>) -> Result<FileDiff> {
        file_diff(&self.worktree, &self.base()?.commit, path, old_path)
    }

    /// `path` as the base has it, if it is at most `max_bytes` (H2).
    pub fn base_blob(&self, path: &str, max_bytes: u64) -> Result<BaseBlob> {
        base_blob(&self.worktree, &self.base()?.commit, path, max_bytes)
    }

    /// Puts files back to the base (G13).
    ///
    /// Rename-aware: discarding a renamed file's new name also brings back
    /// its old one. On its own, the new name is simply absent from the base
    /// and would be deleted, and the file would vanish instead of reverting
    /// (found by the test pass, CP7) — the caller should not have to know.
    pub fn discard(&self, paths: &[String]) -> Result<()> {
        let base = self.base()?;
        let mut targets: Vec<String> = paths.to_vec();
        for change in changes(&self.worktree, &base.commit)? {
            if let (ChangeKind::Renamed, Some(old)) = (change.kind, &change.old_path)
                && paths.contains(&change.path)
                && !targets.contains(old)
            {
                targets.push(old.clone());
            }
        }
        discard(&self.worktree, &base.commit, &targets)
    }

    /// Puts one hunk back to the base (H6); see [`discard_hunk`].
    pub fn discard_hunk(
        &self,
        path: &str,
        old_path: Option<&str>,
        index: usize,
        header: &str,
    ) -> Result<()> {
        discard_hunk(
            &self.worktree,
            &self.base()?.commit,
            path,
            old_path,
            index,
            header,
        )
    }

    /// The subject of the agent's latest own commit (H10); see [`last_subject`].
    pub fn last_subject(&self) -> Result<Option<String>> {
        last_subject(&self.worktree, &self.base()?.commit)
    }

    /// Commits everything uncommitted in the worktree (G3).
    pub fn commit_all(&self, message: &str) -> Result<String> {
        commit_all(&self.worktree, message)
    }

    /// Takes the agent's committed work over into the project folder (G7–G12).
    ///
    /// Crate-private on purpose: from outside, the only way in is
    /// `provision::TakeOverTarget::run`, which checks first that the agent is
    /// not in the middle of a turn (G12). A caller cannot forget that check
    /// if it cannot reach this without it (architecture review, CP7).
    pub(crate) fn take_over(&self, mode: TakeOverMode, message: &str) -> Result<TakeOver> {
        let base = self.base()?;
        take_over(TakeOverRequest {
            repo_root: &self.repo_root,
            worktree: &self.worktree,
            agent_branch: &self.agent_branch,
            base_branch: &base.branch,
            mode,
            message,
        })
    }
}

/// Works out the base of an agent's worktree (G1).
///
/// `recorded` is the agent's stored `base_branch`; without it the branch the
/// project folder has checked out stands in, and the result says so.
pub fn resolve_base(repo_root: &Path, worktree: &Path, recorded: Option<&str>) -> Result<Base> {
    let (branch, fallback) = match recorded {
        Some(branch) => (branch.to_string(), false),
        None => (
            worktree::current_branch(repo_root).ok_or_else(|| IdeError::Invalid {
                field: "base_branch",
                reason: "the project folder is on a detached HEAD, so there is no branch to \
                         compare this agent's work against"
                    .into(),
            })?,
            true,
        ),
    };
    let commit = git(worktree, &["merge-base", &branch, "HEAD"])?
        .trim()
        .to_string();
    Ok(Base {
        branch,
        commit,
        fallback,
    })
}

/// Everything that differs between the base and the worktree as it stands —
/// committed on the agent's branch or not (G2), untracked files included.
///
/// Two git calls per refresh, not five (performance review, CP7): one
/// `git status` gives both "what is uncommitted" and the untracked files, and
/// one `git diff --raw --numstat` gives the kinds and the line counts. The
/// Diffs tab polls this while an agent works, per open agent.
pub fn changes(worktree: &Path, base: &str) -> Result<Vec<FileChange>> {
    let status = worktree_status(worktree)?;
    let mut changes = tracked_changes(worktree, base, &status.uncommitted)?;

    // `git diff` does not see untracked files; list them as added.
    for path in status.untracked {
        let (lines, binary) = counted_lines(&worktree.join(&path));
        changes.push(FileChange {
            path,
            old_path: None,
            kind: ChangeKind::Added,
            additions: (!binary).then_some(lines),
            deletions: (!binary).then_some(0),
            binary,
            uncommitted: true,
        });
    }

    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

/// The tracked part of [`changes`], from one `git diff -z --raw --numstat`.
///
/// Its output is the raw section (entries starting with `:`, the status as
/// the last token, then one path — two for a rename or copy), followed by the
/// numstat section (`added\tdeleted\tpath`, or `added\tdeleted\t` and then the
/// old and the new path for a rename). Checked against git 2.55's actual
/// output rather than assumed.
fn tracked_changes(
    worktree: &Path,
    base: &str,
    uncommitted: &HashSet<String>,
) -> Result<Vec<FileChange>> {
    let raw = git(
        worktree,
        &[
            "diff",
            "-z",
            "--no-ext-diff",
            "--no-textconv",
            "-M",
            "--raw",
            "--numstat",
            base,
        ],
    )?;
    let mut entries: Vec<(ChangeKind, Option<String>, String)> = Vec::new();
    let mut counts: std::collections::HashMap<String, (Option<u32>, Option<u32>)> =
        std::collections::HashMap::new();

    let mut fields = raw.split('\0').filter(|f| !f.is_empty());
    while let Some(field) = fields.next() {
        if let Some(header) = field.strip_prefix(':') {
            let code = header.split_whitespace().last().unwrap_or_default();
            let two_paths = code.starts_with('R') || code.starts_with('C');
            let first = fields.next().unwrap_or_default().to_string();
            let (old_path, path) = if two_paths {
                (Some(first), fields.next().unwrap_or_default().to_string())
            } else {
                (None, first)
            };
            if let Some(kind) = parse_status_code(code)
                && !path.is_empty()
            {
                entries.push((kind, old_path, path));
            }
            continue;
        }
        let mut parts = field.splitn(3, '\t');
        let (Some(add), Some(del), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        // A rename has an empty path here, then the old and the new path.
        let path = if path.is_empty() {
            fields.next();
            fields.next().unwrap_or_default().to_string()
        } else {
            path.to_string()
        };
        counts.insert(path, (add.parse().ok(), del.parse().ok()));
    }

    Ok(entries
        .into_iter()
        .map(|(kind, old_path, path)| {
            let (additions, deletions) = counts.get(&path).copied().unwrap_or((None, None));
            FileChange {
                uncommitted: uncommitted.contains(&path)
                    || old_path
                        .as_ref()
                        .is_some_and(|old| uncommitted.contains(old)),
                binary: additions.is_none() && kind != ChangeKind::Deleted,
                path,
                old_path,
                kind,
                additions,
                deletions,
            }
        })
        .collect())
}

/// One file's diff against the base, parsed.
///
/// `old_path` is the other side of a rename, so git can pair the two;
/// without it a renamed file would read as one deletion and one addition.
/// A file larger than [`MAX_DIFF_BYTES`] on either side is not diffed at all
/// but reported as cut off — git would otherwise produce the whole diff, and
/// it would be read into memory before any cap applied (security review).
pub fn file_diff(
    worktree: &Path,
    base: &str,
    path: &str,
    old_path: Option<&str>,
) -> Result<FileDiff> {
    checked_path(path)?;
    if let Some(old) = old_path {
        checked_path(old)?;
    }
    if is_special_file(&worktree.join(path)) {
        // A FIFO, socket or device: never handed to git, which would block
        // reading it just like `count_lines` would.
        return Ok(FileDiff {
            path: path.to_string(),
            binary: true,
            hunks: Vec::new(),
            truncated: false,
            old_size: None,
            new_size: None,
        });
    }
    let too_large = |size: Option<u64>| size.is_some_and(|size| size > MAX_DIFF_BYTES as u64);
    let on_disk = fs::symlink_metadata(worktree.join(path))
        .ok()
        .map(|m| m.len());
    let in_base = base_blob_size(worktree, base, old_path.unwrap_or(path))?;
    if too_large(on_disk) || too_large(in_base) {
        return Ok(FileDiff {
            path: path.to_string(),
            binary: false,
            hunks: Vec::new(),
            truncated: true,
            old_size: in_base,
            new_size: on_disk,
        });
    }

    let raw = if is_untracked(worktree, path)? {
        // Untracked: git only diffs it against nothing with `--no-index`,
        // which exits 1 when there is a difference — the normal case here.
        git_with(
            worktree,
            &[
                "diff",
                "--no-index",
                "--no-color",
                "--no-ext-diff",
                "--no-textconv",
                "--",
                "/dev/null",
                path,
            ],
            &[0, 1],
        )?
        .1
    } else {
        let mut args = vec![
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "-M",
            "-U3",
            base,
            "--",
            path,
        ];
        if let Some(old) = old_path {
            args.push(old);
        }
        git(worktree, &args)?
    };
    Ok(FileDiff {
        old_size: in_base,
        new_size: on_disk,
        ..parse_diff(path, &raw)
    })
}

/// Puts files back to how they are in the base (G13): changed files are
/// restored, files the agent added are deleted. Committed changes included —
/// the result is an uncommitted change, visible and reversible until someone
/// commits it.
///
/// Every path is either done or reported: a file this cannot place safely
/// inside the worktree is an error, never silently skipped (architecture
/// review) — the UI would otherwise show it as discarded while it is not.
pub fn discard(worktree: &Path, base: &str, paths: &[String]) -> Result<()> {
    let root = worktree.canonicalize().map_err(|source| IdeError::Io {
        path: worktree.to_path_buf(),
        source,
    })?;
    for path in paths {
        checked_path(path)?;
    }
    // One lookup, one restore and one index removal for all paths together
    // rather than two git calls per path (performance review, CP8).
    let in_base = base_paths(worktree, base, paths)?;
    let (restore, added): (Vec<&String>, Vec<&String>) =
        paths.iter().partition(|path| in_base.contains(*path));
    if !restore.is_empty() {
        let mut args = vec!["restore", "--source", base, "--staged", "--worktree", "--"];
        args.extend(restore.iter().map(|path| path.as_str()));
        git(worktree, &args)?;
    }
    if added.is_empty() {
        return Ok(());
    }
    // Not in the base: the agent added them. Drop them from the index if they
    // are there, then from the disk — but only files that really live inside
    // the worktree, so a symlinked directory cannot turn this into a deletion
    // somewhere else.
    let mut args = vec!["rm", "--cached", "--quiet", "--ignore-unmatch", "--"];
    args.extend(added.iter().map(|path| path.as_str()));
    git(worktree, &args)?;
    for path in added {
        let relative = checked_path(path)?;
        let target = worktree.join(&relative);
        if fs::symlink_metadata(&target).is_err() {
            // Already gone from the disk (it was only in the index): done.
            continue;
        }
        let parent = target
            .parent()
            .and_then(|p| p.canonicalize().ok())
            .filter(|parent| parent.starts_with(&root))
            .ok_or_else(|| IdeError::Invalid {
                field: "path",
                reason: format!("{path} is not inside the agent's worktree"),
            })?;
        let file = parent.join(target.file_name().unwrap_or_default());
        if fs::symlink_metadata(&file).is_ok_and(|meta| meta.is_dir()) {
            return Err(IdeError::Invalid {
                field: "path",
                reason: format!("{path} is a directory, not a file"),
            });
        }
        fs::remove_file(&file).map_err(|source| IdeError::Io { path: file, source })?;
    }
    Ok(())
}

/// `path` as `base` has it (H2): absent, too large, or its bytes.
///
/// The blob is looked up by `ls-tree` (a literal path, like every path here)
/// and then read by its object id — never as `<commit>:<path>`, whose path
/// part git would interpret (`./`, `:/`) instead of taking it as spelled.
/// A directory or submodule under that name counts as absent: there is no
/// file to show.
pub fn base_blob(worktree: &Path, base: &str, path: &str, max_bytes: u64) -> Result<BaseBlob> {
    checked_path(path)?;
    let Some(entry) = base_entry(worktree, base, path)? else {
        return Ok(BaseBlob::Absent);
    };
    if entry.kind != "blob" {
        return Ok(BaseBlob::Absent);
    }
    if entry.size > max_bytes {
        return Ok(BaseBlob::TooLarge { size: entry.size });
    }
    git_bytes(worktree, &["cat-file", "blob", &entry.oid]).map(BaseBlob::Bytes)
}

/// One entry of the base's tree, as `ls-tree --long` lists it.
struct TreeEntry {
    /// `blob`, `tree` or `commit` (a submodule).
    kind: String,
    /// The object id `cat-file blob` reads the content from.
    oid: String,
    /// `0` for a tree, which has no size.
    size: u64,
}

/// `path`'s entry in `base`, or `None` when the base has no such path.
///
/// `ls-tree` rather than `cat-file -e`: the latter exits 128 for "no such
/// path" — the same code as any fatal error, so a real failure would have
/// read as "the agent added this" and led to a deletion (architecture review).
/// `ls-tree` exits 0 either way and answers with an empty line for "absent".
fn base_entry(worktree: &Path, base: &str, path: &str) -> Result<Option<TreeEntry>> {
    let listed = git(worktree, &["ls-tree", "-z", "--long", base, "--", path])?;
    Ok(listed
        .split('\0')
        .find(|entry| !entry.is_empty())
        .and_then(|entry| entry.split('\t').next())
        .and_then(|meta| {
            // `<mode> <type> <object> <size>`, the size right-aligned.
            let mut fields = meta.split_whitespace().skip(1);
            let kind = fields.next()?.to_string();
            let oid = fields.next()?.to_string();
            let size = fields
                .next()
                .and_then(|size| size.parse().ok())
                .unwrap_or(0);
            Some(TreeEntry { kind, oid, size })
        }))
}

/// Which of `paths` the base has an entry for — one `ls-tree` for all of them.
///
/// Not type-aware: a path that is a tree (directory) or a submodule in the
/// base still counts as "in the base", so [`discard`] restores it rather than
/// deleting it. Left that way on purpose for now — noted, not fixed, in
/// `docs/plans/git-layer.md`'s CP9 section.
fn base_paths(worktree: &Path, base: &str, paths: &[String]) -> Result<HashSet<String>> {
    let mut args = vec!["ls-tree", "-z", "--long", "--full-tree", base, "--"];
    args.extend(paths.iter().map(|path| path.as_str()));
    let listed = git(worktree, &args)?;
    Ok(listed
        .split('\0')
        .filter_map(|entry| entry.split_once('\t').map(|(_, path)| path.to_string()))
        .collect())
}

/// Puts one hunk of a file back to the base (H6): the hunk at `index` of the
/// file's diff as it is *now*, which must still read `header` — what the
/// user saw and confirmed. If the agent has changed the file since, the hunk
/// is refused rather than a different one taken back (H13).
///
/// Reverse-applies just that hunk to the worktree (`git apply -R`), so the
/// result is an uncommitted change like any discard (G13), committed work
/// included. A new or deleted file is one hunk that is the whole file; it
/// goes back the way [`discard`] puts a whole file back.
pub fn discard_hunk(
    worktree: &Path,
    base: &str,
    path: &str,
    old_path: Option<&str>,
    index: usize,
    header: &str,
) -> Result<()> {
    let diff = file_diff(worktree, base, path, old_path)?;
    if diff.binary || diff.truncated {
        return Err(IdeError::Invalid {
            field: "hunk",
            reason: format!(
                "{path} has no hunks to take back one by one; discard the whole file instead"
            ),
        });
    }
    let hunk = diff
        .hunks
        .get(index)
        .filter(|hunk| hunk.header == header)
        .ok_or_else(|| IdeError::Invalid {
            field: "hunk",
            reason: format!("{path} changed since its diff was shown; reload it and try again"),
        })?;
    if diff.old_size.is_none() || diff.new_size.is_none() {
        // Added or deleted as a whole: the one hunk is the file.
        let mut targets = vec![path.to_string()];
        targets.extend(old_path.map(str::to_string));
        return discard(worktree, base, &targets);
    }
    let patch = hunk_patch(path, hunk);
    git_with_input(
        worktree,
        &["apply", "-R", "--whitespace=nowarn", "-"],
        patch.as_bytes(),
    )
    .map(drop)
}

/// The subject of the agent's latest commit beyond `base`, or `None` when it
/// has none — what "take over" suggests as its message when there is no plan
/// title (H10).
pub fn last_subject(worktree: &Path, base: &str) -> Result<Option<String>> {
    let range = format!("{base}..HEAD");
    let subject = git(worktree, &["log", "-1", "--format=%s", &range, "--"])?;
    let subject = subject.trim();
    Ok((!subject.is_empty()).then(|| subject.to_string()))
}

/// The size of `path` in `base`, or `None` when the base has no such file.
fn base_blob_size(worktree: &Path, base: &str, path: &str) -> Result<Option<u64>> {
    Ok(base_entry(worktree, base, path)?.map(|entry| entry.size))
}
/// Commits everything uncommitted in the worktree (G3) — what the agent left
/// lying around. Returns the new commit.
pub fn commit_all(worktree: &Path, message: &str) -> Result<String> {
    let message = message.trim();
    if message.is_empty() {
        return Err(IdeError::Invalid {
            field: "message",
            reason: "a commit needs a message".into(),
        });
    }
    git(worktree, &["add", "-A"])?;
    if git_with(worktree, &["diff", "--cached", "--quiet"], &[0, 1])?.0 == 0 {
        return Err(IdeError::Invalid {
            field: "worktree",
            reason: "there is nothing uncommitted to commit".into(),
        });
    }
    git(worktree, &["commit", "--quiet", "-m", message])?;
    Ok(git(worktree, &["rev-parse", "HEAD"])?.trim().to_string())
}

/// What [`take_over`] needs to know about the agent.
#[derive(Debug, Clone, Copy)]
pub struct TakeOverRequest<'a> {
    /// The user's own working copy — the project folder.
    pub repo_root: &'a Path,
    /// The agent's own worktree, where its committed work is read from.
    pub worktree: &'a Path,
    /// The branch in `worktree` whose work is taken over.
    pub agent_branch: &'a str,
    /// The branch in `repo_root` the work is taken over into.
    pub base_branch: &'a str,
    /// Squash into one commit, or a `--no-ff` merge that keeps the agent's own.
    pub mode: TakeOverMode,
    /// The commit or merge message, from the user.
    pub message: &'a str,
}

/// Takes an agent's committed work over into the project folder (G7–G12).
///
/// Refuses — with a sentence, never by guessing — when the project folder has
/// another branch checked out (G10), has something staged (G8: a squash
/// commit would take it along), or the agent has uncommitted work (G12: only
/// committed work is taken over). A conflict is undone before this returns
/// (G9). Afterwards the agent's branch is moved to the new base, so its diff
/// is empty and it carries on from what was just taken over (G11).
pub fn take_over(request: TakeOverRequest<'_>) -> Result<TakeOver> {
    let TakeOverRequest {
        repo_root,
        worktree,
        agent_branch,
        base_branch,
        mode,
        message,
    } = request;
    let refuse = |reason: String| {
        Err(IdeError::Invalid {
            field: "take_over",
            reason,
        })
    };

    let message = message.trim();
    if message.is_empty() {
        return refuse("taking work over needs a commit message".into());
    }
    match worktree::current_branch(repo_root) {
        Some(branch) if branch == base_branch => {}
        Some(branch) => {
            return refuse(format!(
                "the project folder has {branch} checked out; check out {base_branch} there to take this agent's work over"
            ));
        }
        None => {
            return refuse(format!(
                "the project folder is on a detached HEAD; check out {base_branch} there first"
            ));
        }
    }
    if worktree::has_uncommitted_changes(worktree)? {
        return refuse("the agent has uncommitted work; commit it first".into());
    }
    let ahead: u32 = git(
        worktree,
        &[
            "rev-list",
            "--count",
            &format!("{base_branch}..{agent_branch}"),
        ],
    )?
    .trim()
    .parse()
    .unwrap_or(0);
    if ahead == 0 {
        return refuse(
            "there is nothing on the agent's branch that is not already on the base".into(),
        );
    }
    if git_with(repo_root, &["diff", "--cached", "--quiet"], &[0, 1])?.0 == 1 {
        return refuse(
            "the project folder has staged changes, which the take-over commit would include; \
             commit or unstage them first"
                .into(),
        );
    }

    match mode {
        TakeOverMode::Squash => {
            if let Err(err) = git(repo_root, &["merge", "--squash", agent_branch]) {
                return undo_or_fail(repo_root, &["reset", "--merge"], err);
            }
            if git_with(repo_root, &["diff", "--cached", "--quiet"], &[0, 1])?.0 == 0 {
                return refuse("the agent's branch changes nothing compared to the base".into());
            }
            if let Err(err) = git(repo_root, &["commit", "--quiet", "-m", message]) {
                // A hook said no. Put the working copy back the way it was.
                git(repo_root, &["reset", "--merge"])?;
                return Err(err);
            }
            // The squash commit is new, so the agent's branch is not part of
            // the base's history; set it to the base (G11). Safe: the worktree
            // was checked clean above.
            git(worktree, &["reset", "--quiet", "--hard", base_branch])?;
        }
        TakeOverMode::NoFf => {
            if let Err(err) = git(
                repo_root,
                &["merge", "--no-ff", "--quiet", "-m", message, agent_branch],
            ) {
                return undo_or_fail(repo_root, &["merge", "--abort"], err);
            }
            git(worktree, &["merge", "--quiet", "--ff-only", base_branch])?;
        }
    }
    Ok(TakeOver::Done {
        commit: git(repo_root, &["rev-parse", "HEAD"])?.trim().to_string(),
    })
}

/// After a failed merge: undo whatever git left behind, then report.
///
/// Two ways a merge fails with state left behind, and both are undone:
/// a content conflict (reported as [`TakeOver::Conflict`]), and a merge that
/// git resolved but a hook (`pre-merge-commit`) rejected — no conflict
/// markers, but `MERGE_HEAD` and a staged result stay behind, and the user's
/// next unrelated `git commit` would silently complete the merge the hook had
/// refused (security review, CP7). A failure that left nothing behind (git
/// refusing because a local change would be overwritten) is passed on as the
/// error it is.
fn undo_or_fail(repo_root: &Path, undo: &[&str], err: IdeError) -> Result<TakeOver> {
    let conflicts: Vec<String> = git(repo_root, &["diff", "--name-only", "-z", "--diff-filter=U"])
        .unwrap_or_default()
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    let merging = git_with(
        repo_root,
        &["rev-parse", "-q", "--verify", "MERGE_HEAD"],
        &[0, 1],
    )
    .is_ok_and(|(code, _)| code == 0);
    if conflicts.is_empty() && !merging {
        return Err(err);
    }
    git(repo_root, undo)?;
    if conflicts.is_empty() {
        return Err(err);
    }
    Ok(TakeOver::Conflict { files: conflicts })
}

fn parse_status_code(code: &str) -> Option<ChangeKind> {
    match code.chars().next()? {
        'A' | 'C' => Some(ChangeKind::Added),
        'M' => Some(ChangeKind::Modified),
        'D' => Some(ChangeKind::Deleted),
        'R' => Some(ChangeKind::Renamed),
        'T' => Some(ChangeKind::TypeChanged),
        _ => None,
    }
}

/// What `git status` says about a worktree, in one call.
struct WorktreeStatus {
    /// Every path with an uncommitted change: staged, unstaged or untracked.
    uncommitted: HashSet<String>,
    /// The untracked ones, which `git diff` does not see.
    untracked: Vec<String>,
}

fn worktree_status(worktree: &Path) -> Result<WorktreeStatus> {
    let status = git(
        worktree,
        &["status", "-z", "--porcelain=v1", "--untracked-files=all"],
    )?;
    let mut uncommitted = HashSet::new();
    let mut untracked = Vec::new();
    let mut fields = status.split('\0').filter(|f| !f.is_empty());
    while let Some(entry) = fields.next() {
        let Some(path) = entry.get(3..) else { continue };
        if entry.starts_with("??") {
            untracked.push(path.to_string());
        }
        uncommitted.insert(path.to_string());
        // A rename carries its old path as the next field.
        if (entry.starts_with('R') || entry.starts_with('C'))
            && let Some(old) = fields.next()
        {
            uncommitted.insert(old.to_string());
        }
    }
    Ok(WorktreeStatus {
        uncommitted,
        untracked,
    })
}

fn is_untracked(worktree: &Path, path: &str) -> Result<bool> {
    let listed = git(
        worktree,
        &[
            "ls-files",
            "-z",
            "--others",
            "--exclude-standard",
            "--",
            path,
        ],
    )?;
    Ok(listed.split('\0').any(|p| p == path))
}

/// Counts lines of an untracked file and tells whether it is binary, the way
/// git does: a NUL byte in the first 8000 bytes.
///
/// Only a regular file is ever opened. An agent can `mkfifo` in its worktree,
/// and opening a FIFO for reading blocks until a writer appears — forever,
/// taking a blocking-pool thread with it on every poll (security review).
/// Anything that is not a regular file counts as binary, unopened.
/// The most untracked files whose line counts are remembered; past it the
/// memory is simply dropped and rebuilt.
const LINE_COUNT_CACHE_LIMIT: usize = 4096;

/// What a file looked like when its lines were counted: size and mtime.
type Stamp = (u64, std::time::SystemTime);

/// A file's line count and binary flag, as [`count_lines`] returns them.
type Counted = (u32, bool);

/// Line counts of untracked files, remembered by what the file looked like
/// then. The Diffs tab asks every 5 s while an agent works (G4), and an
/// untracked file is otherwise read whole each time to count its lines —
/// up to 2 MiB per file per poll for a badge that has not changed
/// (performance review, CP8).
static LINE_COUNTS: LazyLock<Mutex<HashMap<PathBuf, (Stamp, Counted)>>> =
    LazyLock::new(Default::default);

/// [`count_lines`], skipped when the file's size and mtime are unchanged
/// since it was last counted.
fn counted_lines(path: &Path) -> (u32, bool) {
    let stamp = fs::symlink_metadata(path)
        .ok()
        .filter(|meta| meta.file_type().is_file())
        .and_then(|meta| Some((meta.len(), meta.modified().ok()?)));
    let Some(stamp) = stamp else {
        return count_lines(path);
    };
    if let Some((seen, counted)) = LINE_COUNTS
        .lock()
        .ok()
        .and_then(|cache| cache.get(path).copied())
        && seen == stamp
    {
        return counted;
    }
    // Counted without the lock held: reading the file is the slow part.
    let counted = count_lines(path);
    if let Ok(mut cache) = LINE_COUNTS.lock() {
        if cache.len() >= LINE_COUNT_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), (stamp, counted));
    }
    counted
}

fn count_lines(path: &Path) -> (u32, bool) {
    if !is_regular_file(path) {
        return (0, true);
    }
    let mut bytes = Vec::new();
    if fs::File::open(path)
        .and_then(|file| file.take(MAX_UNTRACKED_BYTES).read_to_end(&mut bytes))
        .is_err()
    {
        return (0, false);
    }
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return (0, true);
    }
    let newlines = bytes.iter().filter(|b| **b == b'\n').count();
    let trailing = u32::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
    (
        u32::try_from(newlines)
            .unwrap_or(u32::MAX)
            .saturating_add(trailing),
        false,
    )
}

/// A regular file, not following a symlink — so never a FIFO, socket or device.
fn is_regular_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_file())
}

/// A FIFO, socket or device — something that exists but is neither a file,
/// a directory nor a symlink (which git tracks and diffs normally).
fn is_special_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| {
        let kind = meta.file_type();
        !kind.is_file() && !kind.is_dir() && !kind.is_symlink()
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-git-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn run(dir: &Path, args: &[&str]) {
        git(dir, args).unwrap_or_else(|err| panic!("git {args:?}: {err}"));
    }

    /// A repository on `main` with two files, and an agent worktree cut from
    /// it on its own branch — the shape CP5 leaves behind.
    struct Fixture {
        repo: PathBuf,
        worktree: PathBuf,
    }

    const AGENT_BRANCH: &str = "axiomata/builder-1";

    fn fixture() -> Fixture {
        let repo = temp_dir("repo");
        run(&repo, &["init", "--quiet", "--initial-branch=main"]);
        run(&repo, &["config", "user.email", "t@example.com"]);
        run(&repo, &["config", "user.name", "T"]);
        run(&repo, &["config", "commit.gpgsign", "false"]);
        fs::write(repo.join("README.md"), "one\ntwo\nthree\n").unwrap();
        fs::write(repo.join("keep.txt"), "keep\n").unwrap();
        run(&repo, &["add", "."]);
        run(&repo, &["commit", "--quiet", "-m", "first"]);
        let worktree = temp_dir("wt").join("builder");
        let created = worktree::add(&repo, &worktree, AGENT_BRANCH).unwrap();
        Fixture {
            repo,
            worktree: created.path,
        }
    }

    impl Fixture {
        fn base(&self) -> Base {
            resolve_base(&self.repo, &self.worktree, Some("main")).unwrap()
        }

        fn changes(&self) -> Vec<FileChange> {
            changes(&self.worktree, &self.base().commit).unwrap()
        }

        fn agent_commits(&self, file: &str, content: &str) {
            fs::write(self.worktree.join(file), content).unwrap();
            run(&self.worktree, &["add", "-A"]);
            run(&self.worktree, &["commit", "--quiet", "-m", "agent work"]);
        }

        fn request(&self, mode: TakeOverMode) -> TakeOverRequest<'_> {
            TakeOverRequest {
                repo_root: &self.repo,
                worktree: &self.worktree,
                agent_branch: AGENT_BRANCH,
                base_branch: "main",
                mode,
                message: "Take over the agent's work",
            }
        }
    }

    #[test]
    fn a_fresh_worktree_has_no_changes() {
        let f = fixture();
        assert!(f.changes().is_empty());
    }

    #[test]
    fn committed_and_uncommitted_work_show_up_together_marked_apart() {
        let f = fixture();
        f.agent_commits("README.md", "one\nTWO\nthree\nfour\n");
        fs::write(f.worktree.join("keep.txt"), "kept, but edited\n").unwrap();
        fs::write(f.worktree.join("new file ü.txt"), "a\nb").unwrap();

        let changes = f.changes();
        let by_path = |p: &str| changes.iter().find(|c| c.path == p).unwrap();
        let readme = by_path("README.md");
        assert_eq!(
            (readme.kind, readme.additions, readme.deletions),
            (ChangeKind::Modified, Some(2), Some(1))
        );
        assert!(!readme.uncommitted, "committed on the agent's branch");
        assert!(by_path("keep.txt").uncommitted);
        let added = by_path("new file ü.txt");
        assert_eq!(
            (added.kind, added.additions, added.uncommitted),
            (ChangeKind::Added, Some(2), true)
        );
    }

    #[test]
    fn renames_deletions_and_binaries_are_told_apart() {
        let f = fixture();
        run(&f.worktree, &["mv", "keep.txt", "kept.txt"]);
        run(&f.worktree, &["rm", "--quiet", "README.md"]);
        fs::write(f.worktree.join("image.bin"), [0u8, 1, 2, 0, 255]).unwrap();
        run(&f.worktree, &["add", "-A"]);
        run(&f.worktree, &["commit", "--quiet", "-m", "shuffle"]);

        let changes = f.changes();
        let renamed = changes
            .iter()
            .find(|c| c.kind == ChangeKind::Renamed)
            .unwrap();
        assert_eq!(
            (renamed.path.as_str(), renamed.old_path.as_deref()),
            ("kept.txt", Some("keep.txt"))
        );
        let deleted = changes.iter().find(|c| c.path == "README.md").unwrap();
        assert_eq!((deleted.kind, deleted.binary), (ChangeKind::Deleted, false));
        let binary = changes.iter().find(|c| c.path == "image.bin").unwrap();
        assert!(binary.binary && binary.additions.is_none());

        let diff = file_diff(&f.worktree, &f.base().commit, "kept.txt", Some("keep.txt")).unwrap();
        assert!(
            diff.hunks.is_empty(),
            "a pure rename has no content change: {diff:?}"
        );
        let diff = file_diff(&f.worktree, &f.base().commit, "image.bin", None).unwrap();
        assert!(diff.binary);
    }

    #[test]
    fn work_that_landed_on_main_is_not_shown_as_the_agents() {
        let f = fixture();
        f.agent_commits("README.md", "one\ntwo\nthree\nagent\n");
        // Somebody else changes main after the agent started.
        fs::write(f.repo.join("keep.txt"), "changed on main\n").unwrap();
        run(&f.repo, &["commit", "--quiet", "-am", "main moves on"]);

        let paths: Vec<_> = f.changes().into_iter().map(|c| c.path).collect();
        assert_eq!(
            paths,
            ["README.md"],
            "measured from the merge base, not main's tip"
        );
    }

    #[test]
    fn a_diff_is_parsed_with_line_numbers_on_both_sides() {
        let f = fixture();
        f.agent_commits("README.md", "one\nTWO\nthree\n");
        let diff = file_diff(&f.worktree, &f.base().commit, "README.md", None).unwrap();
        assert_eq!(diff.hunks.len(), 1);
        let lines: Vec<_> = diff.hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.old_line, l.new_line, l.text.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (LineKind::Context, Some(1), Some(1), "one"),
                (LineKind::Remove, Some(2), None, "two"),
                (LineKind::Add, None, Some(2), "TWO"),
                (LineKind::Context, Some(3), Some(3), "three"),
            ]
        );
    }

    #[test]
    fn the_base_side_of_a_file_is_read_as_the_base_has_it() {
        let f = fixture();
        f.agent_commits("README.md", "changed\n");
        let commit = f.base().commit;
        assert_eq!(
            base_blob(&f.worktree, &commit, "README.md", 1024).unwrap(),
            BaseBlob::Bytes(b"one\ntwo\nthree\n".to_vec())
        );
        fs::write(f.worktree.join("new.md"), "new").unwrap();
        assert_eq!(
            base_blob(&f.worktree, &commit, "new.md", 1024).unwrap(),
            BaseBlob::Absent
        );
        assert_eq!(
            base_blob(&f.worktree, &commit, "README.md", 4).unwrap(),
            BaseBlob::TooLarge { size: 14 }
        );
        assert!(base_blob(&f.worktree, &commit, "../README.md", 1024).is_err());
    }

    #[test]
    fn the_base_side_takes_a_path_literally_and_skips_directories() {
        let f = fixture();
        fs::create_dir_all(f.repo.join("src")).unwrap();
        fs::write(f.repo.join("src/lib.rs"), "fn a() {}\n").unwrap();
        fs::write(f.repo.join(":colon.txt"), "odd\n").unwrap();
        fs::write(f.repo.join("image.png"), [0x89, b'P', b'N', b'G', 0, 1, 2]).unwrap();
        run(&f.repo, &["add", "."]);
        run(&f.repo, &["commit", "--quiet", "-m", "more"]);
        let commit = git(&f.repo, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        assert_eq!(
            base_blob(&f.worktree, &commit, "src/lib.rs", 1024).unwrap(),
            BaseBlob::Bytes(b"fn a() {}\n".to_vec())
        );
        assert_eq!(
            base_blob(&f.worktree, &commit, "src", 1024).unwrap(),
            BaseBlob::Absent
        );
        assert_eq!(
            base_blob(&f.worktree, &commit, ":colon.txt", 1024).unwrap(),
            BaseBlob::Bytes(b"odd\n".to_vec())
        );
        assert_eq!(
            base_blob(&f.worktree, &commit, "image.png", 1024).unwrap(),
            BaseBlob::Bytes(vec![0x89, b'P', b'N', b'G', 0, 1, 2])
        );
    }

    #[test]
    fn a_diff_carries_the_size_of_both_sides() {
        let f = fixture();
        f.agent_commits("README.md", "one\n");
        let diff = file_diff(&f.worktree, &f.base().commit, "README.md", None).unwrap();
        assert_eq!((diff.old_size, diff.new_size), (Some(14), Some(4)));
        fs::write(f.worktree.join("new.md"), "abc").unwrap();
        let added = file_diff(&f.worktree, &f.base().commit, "new.md", None).unwrap();
        assert_eq!((added.old_size, added.new_size), (None, Some(3)));
    }

    #[test]
    fn an_untracked_file_is_counted_again_once_it_changes() {
        let f = fixture();
        let file = f.worktree.join("draft.md");
        fs::write(&file, "a\nb\n").unwrap();
        assert_eq!(counted_lines(&file), (2, false));
        assert_eq!(counted_lines(&file), (2, false));
        // A different size is a different stamp, whatever the clock's resolution.
        fs::write(&file, "a\nb\nc\n").unwrap();
        assert_eq!(counted_lines(&file), (3, false));
    }

    /// The fixture's README with twenty lines, and the agent's copy changed
    /// at line 2 and line 18 — far enough apart for two hunks.
    fn two_hunk_fixture() -> (Fixture, String) {
        let f = fixture();
        let base: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        fs::write(f.repo.join("README.md"), &base).unwrap();
        run(&f.repo, &["commit", "--quiet", "-am", "twenty lines"]);
        run(&f.worktree, &["merge", "--quiet", "--ff-only", "main"]);
        let changed = base
            .replace("line 2\n", "LINE TWO\n")
            .replace("line 18\n", "LINE EIGHTEEN\n");
        fs::write(f.worktree.join("README.md"), &changed).unwrap();
        (f, base)
    }

    #[test]
    fn discarding_one_hunk_leaves_the_other() {
        let (f, _) = two_hunk_fixture();
        let commit = f.base().commit;
        let diff = file_diff(&f.worktree, &commit, "README.md", None).unwrap();
        assert_eq!(diff.hunks.len(), 2);
        discard_hunk(
            &f.worktree,
            &commit,
            "README.md",
            None,
            0,
            &diff.hunks[0].header,
        )
        .unwrap();
        let text = fs::read_to_string(f.worktree.join("README.md")).unwrap();
        assert!(text.contains("line 2\n") && text.contains("LINE EIGHTEEN\n"));
    }

    #[test]
    fn a_hunk_that_changed_since_it_was_shown_is_refused() {
        let (f, _) = two_hunk_fixture();
        let commit = f.base().commit;
        let err = discard_hunk(
            &f.worktree,
            &commit,
            "README.md",
            None,
            0,
            "@@ -1,5 +1,5 @@ stale",
        )
        .unwrap_err();
        assert!(err.to_string().contains("changed since"), "{err}");
        assert!(discard_hunk(&f.worktree, &commit, "README.md", None, 7, "@@ x @@").is_err());
        let text = fs::read_to_string(f.worktree.join("README.md")).unwrap();
        assert!(
            text.contains("LINE TWO"),
            "nothing may have been taken back"
        );
    }

    #[test]
    fn a_committed_hunk_comes_back_as_an_uncommitted_change() {
        let (f, _) = two_hunk_fixture();
        run(&f.worktree, &["commit", "--quiet", "-am", "agent work"]);
        let commit = f.base().commit;
        let diff = file_diff(&f.worktree, &commit, "README.md", None).unwrap();
        discard_hunk(
            &f.worktree,
            &commit,
            "README.md",
            None,
            1,
            &diff.hunks[1].header,
        )
        .unwrap();
        let file = f
            .changes()
            .into_iter()
            .find(|c| c.path == "README.md")
            .unwrap();
        assert!(file.uncommitted);
        assert_eq!((file.additions, file.deletions), (Some(1), Some(1)));
    }

    #[test]
    fn a_hunk_of_a_crlf_file_with_an_odd_name_goes_back_too() {
        let f = fixture();
        let name = "notes \"odd\" -name.txt";
        fs::write(f.repo.join(name), "one\r\ntwo\r\nthree\r\n").unwrap();
        run(&f.repo, &["add", "."]);
        run(&f.repo, &["commit", "--quiet", "-m", "crlf"]);
        run(&f.worktree, &["merge", "--quiet", "--ff-only", "main"]);
        fs::write(f.worktree.join(name), "one\r\nTWO\r\nthree\r\n").unwrap();
        let commit = f.base().commit;
        let diff = file_diff(&f.worktree, &commit, name, None).unwrap();
        discard_hunk(&f.worktree, &commit, name, None, 0, &diff.hunks[0].header).unwrap();
        assert_eq!(
            fs::read(f.worktree.join(name)).unwrap(),
            b"one\r\ntwo\r\nthree\r\n"
        );
    }

    #[test]
    fn discarding_the_hunk_of_an_added_file_removes_the_file() {
        let f = fixture();
        fs::write(f.worktree.join("new.md"), "fresh\n").unwrap();
        let commit = f.base().commit;
        let diff = file_diff(&f.worktree, &commit, "new.md", None).unwrap();
        discard_hunk(
            &f.worktree,
            &commit,
            "new.md",
            None,
            0,
            &diff.hunks[0].header,
        )
        .unwrap();
        assert!(!f.worktree.join("new.md").exists());
    }

    #[test]
    fn discarding_several_files_at_once_restores_and_deletes_as_needed() {
        let f = fixture();
        fs::write(f.worktree.join("README.md"), "changed\n").unwrap();
        fs::write(f.worktree.join("keep.txt"), "changed too\n").unwrap();
        fs::write(f.worktree.join("added.md"), "new\n").unwrap();
        let paths = ["README.md", "keep.txt", "added.md"].map(String::from);
        discard(&f.worktree, &f.base().commit, &paths).unwrap();
        assert!(f.changes().is_empty());
    }

    #[test]
    fn the_last_subject_is_the_agents_own_latest_commit() {
        let f = fixture();
        assert_eq!(last_subject(&f.worktree, &f.base().commit).unwrap(), None);
        f.agent_commits("README.md", "x\n");
        assert_eq!(
            last_subject(&f.worktree, &f.base().commit)
                .unwrap()
                .as_deref(),
            Some("agent work")
        );
    }

    #[test]
    fn an_untracked_file_diffs_against_nothing() {
        let f = fixture();
        fs::write(f.worktree.join("notes.md"), "hello\nworld").unwrap();
        let diff = file_diff(&f.worktree, &f.base().commit, "notes.md", None).unwrap();
        let kinds: Vec<_> = diff.hunks[0].lines.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, [LineKind::Add, LineKind::Add, LineKind::NoNewline]);
    }

    #[test]
    fn paths_that_leave_the_worktree_are_refused() {
        for bad in ["", "../outside", "/etc/passwd", "a/../../b", "a\0b"] {
            assert!(checked_path(bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            checked_path("./src/lib.rs").unwrap(),
            PathBuf::from("src/lib.rs")
        );
        let f = fixture();
        assert!(file_diff(&f.worktree, &f.base().commit, "../README.md", None).is_err());
        assert!(discard(&f.worktree, &f.base().commit, &["../x".into()]).is_err());
    }

    #[test]
    fn discarding_puts_a_file_back_to_the_base_even_after_a_commit() {
        let f = fixture();
        f.agent_commits("README.md", "rewritten by the agent\n");
        fs::write(f.worktree.join("added.txt"), "new").unwrap();
        run(&f.worktree, &["add", "added.txt"]);
        fs::write(f.worktree.join("loose.txt"), "untracked").unwrap();

        let base = f.base().commit;
        discard(
            &f.worktree,
            &base,
            &["README.md".into(), "added.txt".into(), "loose.txt".into()],
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(f.worktree.join("README.md")).unwrap(),
            "one\ntwo\nthree\n"
        );
        assert!(!f.worktree.join("added.txt").exists());
        assert!(!f.worktree.join("loose.txt").exists());
        assert!(
            f.changes().is_empty(),
            "back to the base: {:?}",
            f.changes()
        );
        // …but only as an uncommitted change: the agent's commit is still there.
        assert!(worktree::has_uncommitted_changes(&f.worktree).unwrap());
    }

    #[test]
    fn commit_all_commits_what_the_agent_left_and_refuses_an_empty_commit() {
        let f = fixture();
        fs::write(f.worktree.join("left.txt"), "left behind").unwrap();
        commit_all(&f.worktree, "Commit what the agent left").unwrap();
        assert!(!worktree::has_uncommitted_changes(&f.worktree).unwrap());
        assert!(commit_all(&f.worktree, "again").is_err(), "nothing left");
        assert!(commit_all(&f.worktree, "  ").is_err(), "needs a message");
    }

    #[test]
    fn a_squash_take_over_lands_one_commit_and_resets_the_agent() {
        let f = fixture();
        f.agent_commits("README.md", "one\ntwo\nthree\nfour\n");
        f.agent_commits("feature.txt", "feature\n");
        // The user has an unrelated change of their own lying around.
        fs::write(f.repo.join("keep.txt"), "user's own edit\n").unwrap();

        let outcome = take_over(f.request(TakeOverMode::Squash)).unwrap();
        let TakeOver::Done { commit } = outcome else {
            panic!("{outcome:?}")
        };

        assert_eq!(
            git(&f.repo, &["log", "-1", "--format=%s", &commit])
                .unwrap()
                .trim(),
            "Take over the agent's work"
        );
        assert_eq!(
            git(&f.repo, &["rev-list", "--count", "HEAD"])
                .unwrap()
                .trim(),
            "2",
            "one squash commit"
        );
        assert!(f.repo.join("feature.txt").exists());
        assert_eq!(
            fs::read_to_string(f.repo.join("keep.txt")).unwrap(),
            "user's own edit\n",
            "untouched"
        );
        assert!(
            f.changes().is_empty(),
            "the agent now starts from what was taken over"
        );
    }

    #[test]
    fn a_no_ff_take_over_keeps_the_agents_commits() {
        let f = fixture();
        f.agent_commits("feature.txt", "feature\n");
        let TakeOver::Done { .. } = take_over(f.request(TakeOverMode::NoFf)).unwrap() else {
            panic!()
        };
        let subjects = git(&f.repo, &["log", "--format=%s"]).unwrap();
        assert!(subjects.contains("agent work") && subjects.contains("Take over the agent's work"));
        assert!(f.changes().is_empty());
    }

    #[test]
    fn a_conflict_is_undone_and_reported() {
        for mode in [TakeOverMode::Squash, TakeOverMode::NoFf] {
            let f = fixture();
            f.agent_commits("README.md", "agent's version\n");
            fs::write(f.repo.join("README.md"), "main's version\n").unwrap();
            run(
                &f.repo,
                &["commit", "--quiet", "-am", "main edits the same line"],
            );
            let head = git(&f.repo, &["rev-parse", "HEAD"]).unwrap();

            let outcome = take_over(f.request(mode)).unwrap();
            assert_eq!(
                outcome,
                TakeOver::Conflict {
                    files: vec!["README.md".into()]
                },
                "{mode:?}"
            );
            assert_eq!(git(&f.repo, &["rev-parse", "HEAD"]).unwrap(), head);
            assert!(
                !worktree::has_uncommitted_changes(&f.repo).unwrap(),
                "{mode:?}: nothing half-merged"
            );
            assert!(
                !f.changes().is_empty(),
                "{mode:?}: the agent keeps its work"
            );
        }
    }

    #[test]
    fn take_over_refuses_instead_of_guessing() {
        let f = fixture();
        f.agent_commits("feature.txt", "feature\n");

        // Uncommitted agent work.
        fs::write(f.worktree.join("loose.txt"), "x").unwrap();
        assert!(take_over(f.request(TakeOverMode::Squash)).is_err());
        fs::remove_file(f.worktree.join("loose.txt")).unwrap();

        // Something staged in the project folder.
        fs::write(f.repo.join("keep.txt"), "staged by the user\n").unwrap();
        run(&f.repo, &["add", "keep.txt"]);
        let err = take_over(f.request(TakeOverMode::Squash))
            .unwrap_err()
            .to_string();
        assert!(err.contains("staged"), "{err}");
        run(&f.repo, &["restore", "--staged", "keep.txt"]);

        // Another branch checked out in the project folder.
        run(&f.repo, &["switch", "--quiet", "-c", "elsewhere"]);
        let err = take_over(f.request(TakeOverMode::Squash))
            .unwrap_err()
            .to_string();
        assert!(err.contains("check out main"), "{err}");
        run(&f.repo, &["switch", "--quiet", "main"]);

        // And the happy path still works afterwards, so nothing was broken.
        assert!(matches!(
            take_over(f.request(TakeOverMode::Squash)).unwrap(),
            TakeOver::Done { .. }
        ));
        // Nothing left to take over.
        assert!(take_over(f.request(TakeOverMode::Squash)).is_err());
    }

    #[test]
    fn the_base_falls_back_to_what_the_project_folder_has_checked_out() {
        let f = fixture();
        let base = resolve_base(&f.repo, &f.worktree, None).unwrap();
        assert_eq!((base.branch.as_str(), base.fallback), ("main", true));
        assert_eq!(base.commit, f.base().commit);
    }

    #[test]
    fn the_base_is_an_error_on_a_detached_head_with_nothing_recorded() {
        let f = fixture();
        let head = git(&f.repo, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        run(&f.repo, &["checkout", "--quiet", &head]);

        let err = resolve_base(&f.repo, &f.worktree, None).unwrap_err();
        assert!(
            matches!(
                &err,
                IdeError::Invalid {
                    field: "base_branch",
                    ..
                }
            ),
            "{err}"
        );
    }

    /// Writes an executable hook that always refuses, so a `git commit` in
    /// `repo` fails the way a real `pre-commit`/`commit-msg` hook can.
    fn install_rejecting_hook(repo: &Path, name: &str) {
        let hooks = repo.join(".git/hooks");
        fs::create_dir_all(&hooks).unwrap();
        let hook = hooks.join(name);
        fs::write(&hook, "#!/bin/sh\necho 'no thanks' 1>&2\nexit 1\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&hook).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook, perms).unwrap();
    }

    #[test]
    fn a_rejecting_commit_hook_leaves_the_working_copy_exactly_as_it_was() {
        let f = fixture();
        f.agent_commits("feature.txt", "feature\n");
        // The user's own, unrelated uncommitted edit sits in the working copy.
        fs::write(f.repo.join("keep.txt"), "user's own edit\n").unwrap();
        install_rejecting_hook(&f.repo, "pre-commit");
        let head_before = git(&f.repo, &["rev-parse", "HEAD"]).unwrap();

        let err = take_over(f.request(TakeOverMode::Squash)).unwrap_err();
        assert!(err.to_string().contains("no thanks"), "{err}");

        assert_eq!(
            git(&f.repo, &["rev-parse", "HEAD"]).unwrap(),
            head_before,
            "the rejected commit must not have landed"
        );
        assert!(
            !f.repo.join("feature.txt").exists(),
            "the squash's staged content must have been undone"
        );
        assert_eq!(
            git(&f.repo, &["status", "--porcelain"]).unwrap(),
            " M keep.txt\n",
            "only the user's own edit is left, and it is still unstaged"
        );
        assert_eq!(
            fs::read_to_string(f.repo.join("keep.txt")).unwrap(),
            "user's own edit\n",
            "the user's unrelated edit must survive untouched"
        );
        assert!(
            !f.changes().is_empty(),
            "the agent keeps its work; nothing was taken over"
        );
    }

    #[test]
    fn take_over_refuses_when_an_unstaged_local_edit_collides_with_the_agents_change() {
        let f = fixture();
        f.agent_commits("keep.txt", "the agent's version\n");
        // Not staged, just sitting in the working copy — and on the very file
        // the agent changed, so merging it in would overwrite it.
        fs::write(f.repo.join("keep.txt"), "the user's unstaged edit\n").unwrap();
        let head_before = git(&f.repo, &["rev-parse", "HEAD"]).unwrap();

        let outcome = take_over(f.request(TakeOverMode::Squash));
        assert!(outcome.is_err(), "{outcome:?}");

        assert_eq!(
            git(&f.repo, &["rev-parse", "HEAD"]).unwrap(),
            head_before,
            "nothing was committed"
        );
        assert_eq!(
            fs::read_to_string(f.repo.join("keep.txt")).unwrap(),
            "the user's unstaged edit\n",
            "the user's edit must be untouched"
        );
        assert!(
            !f.changes().is_empty(),
            "the agent keeps its work; nothing was taken over"
        );
    }

    #[test]
    fn discard_reaches_a_file_inside_a_directory_the_agent_added() {
        let f = fixture();
        fs::create_dir_all(f.worktree.join("sub/dir")).unwrap();
        fs::write(f.worktree.join("sub/dir/new.txt"), "added").unwrap();
        run(&f.worktree, &["add", "-A"]);
        run(
            &f.worktree,
            &["commit", "--quiet", "-m", "add a nested file"],
        );

        let base = f.base().commit;
        discard(&f.worktree, &base, &["sub/dir/new.txt".into()]).unwrap();

        assert!(!f.worktree.join("sub/dir/new.txt").exists());
        assert!(f.changes().is_empty());
    }

    #[test]
    fn discarding_only_a_renames_new_name_does_not_bring_the_old_name_back() {
        // `discard` restores a path to what the base has at that path; a
        // rename has no path in the base under its *new* name, so it is
        // treated as an added file and simply deleted — the old name is a
        // separate path that nobody asked to discard. Callers that want a
        // rename fully undone must discard both `path` and `old_path` from
        // the `FileChange`.
        let f = fixture();
        run(&f.worktree, &["mv", "keep.txt", "kept.txt"]);
        run(&f.worktree, &["commit", "--quiet", "-m", "rename"]);
        let base = f.base().commit;

        discard(&f.worktree, &base, &["kept.txt".into()]).unwrap();

        assert!(!f.worktree.join("kept.txt").exists());
        assert!(
            !f.worktree.join("keep.txt").exists(),
            "the old name is not restored by discarding only the new one"
        );
    }

    #[test]
    fn discarding_a_renames_old_and_new_path_together_restores_it() {
        let f = fixture();
        run(&f.worktree, &["mv", "keep.txt", "kept.txt"]);
        run(&f.worktree, &["commit", "--quiet", "-m", "rename"]);
        let base = f.base().commit;

        discard(&f.worktree, &base, &["kept.txt".into(), "keep.txt".into()]).unwrap();

        assert!(!f.worktree.join("kept.txt").exists());
        assert_eq!(
            fs::read_to_string(f.worktree.join("keep.txt")).unwrap(),
            "keep\n"
        );
        assert!(f.changes().is_empty());
    }

    #[test]
    fn changes_handles_file_names_with_tabs_newlines_and_a_leading_dash() {
        let f = fixture();
        let names = ["weird\tname.txt", "line\nbreak.txt", "-leading-dash.txt"];
        for name in names {
            fs::write(f.worktree.join(name), "content").unwrap();
        }

        let mut paths: Vec<_> = f.changes().into_iter().map(|c| c.path).collect();
        paths.sort();
        let mut expected: Vec<_> = names.iter().map(|s| s.to_string()).collect();
        expected.sort();
        assert_eq!(paths, expected);

        // A leading dash must not be read as a flag by the diff machinery.
        let diff = file_diff(&f.worktree, &f.base().commit, "-leading-dash.txt", None).unwrap();
        assert!(!diff.hunks.is_empty());
    }

    #[test]
    fn an_untracked_file_inside_an_ignored_directory_does_not_show_up() {
        let f = fixture();
        fs::write(f.worktree.join(".gitignore"), "ignored/\n").unwrap();
        fs::create_dir_all(f.worktree.join("ignored")).unwrap();
        fs::write(f.worktree.join("ignored/file.txt"), "secret").unwrap();
        fs::write(f.worktree.join("visible.txt"), "shown").unwrap();

        let paths: Vec<_> = f.changes().into_iter().map(|c| c.path).collect();
        assert!(paths.contains(&"visible.txt".to_string()), "{paths:?}");
        assert!(paths.contains(&".gitignore".to_string()), "{paths:?}");
        assert!(
            !paths.iter().any(|p| p.starts_with("ignored/")),
            "an ignored directory's contents must stay invisible: {paths:?}"
        );
    }

    #[test]
    fn a_hook_that_rejects_a_no_ff_merge_leaves_no_half_merge_behind() {
        let f = fixture();
        f.agent_commits("feature.txt", "feature\n");
        fs::write(f.repo.join("keep.txt"), "user's own edit\n").unwrap();
        install_rejecting_hook(&f.repo, "pre-merge-commit");

        assert!(
            take_over(f.request(TakeOverMode::NoFf)).is_err(),
            "the hook's refusal is reported"
        );
        let merging = git_with(
            &f.repo,
            &["rev-parse", "-q", "--verify", "MERGE_HEAD"],
            &[0, 1],
        )
        .unwrap()
        .0;
        assert_eq!(
            merging, 1,
            "no MERGE_HEAD left for the next unrelated commit to complete"
        );
        assert_eq!(
            git(&f.repo, &["status", "--porcelain"]).unwrap(),
            " M keep.txt\n",
            "nothing staged; only the user's own edit remains"
        );
    }

    #[test]
    fn a_file_named_like_a_pathspec_is_just_that_file() {
        let f = fixture();
        // An agent names a file after pathspec magic that would match both
        // tracked files if git read it as a pattern.
        let sneaky = ":(glob)*.txt";
        fs::write(f.worktree.join(sneaky), "x\n").unwrap();
        fs::write(f.worktree.join("keep.txt"), "the agent's real edit\n").unwrap();
        let base = f.base().commit;

        let diff = file_diff(&f.worktree, &base, sneaky, None).unwrap();
        assert_eq!(
            diff.hunks[0].lines.len(),
            1,
            "only its own single line: {diff:?}"
        );

        discard(&f.worktree, &base, &[sneaky.to_string()]).unwrap();
        assert!(!f.worktree.join(sneaky).exists());
        assert_eq!(
            fs::read_to_string(f.worktree.join("keep.txt")).unwrap(),
            "the agent's real edit\n",
            "a file the user did not select must not be discarded"
        );
    }

    #[test]
    fn a_fifo_in_the_worktree_is_listed_but_never_opened() {
        let f = fixture();
        let fifo = f.worktree.join("pipe");
        let made = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap();
        assert!(made.success());

        // Would block forever if either call opened the FIFO for reading.
        // `git status` does not list a FIFO at all; should it ever, it must
        // read as binary rather than be opened.
        let changes = f.changes();
        if let Some(pipe) = changes.iter().find(|c| c.path == "pipe") {
            assert!(pipe.binary);
        }
        // Asked for directly (a stale UI entry), it is still never opened.
        assert!(count_lines(&fifo).1);
        let diff = file_diff(&f.worktree, &f.base().commit, "pipe", None).unwrap();
        assert!(diff.binary && diff.hunks.is_empty());
    }

    #[test]
    fn a_file_too_large_to_diff_is_reported_without_diffing_it() {
        let f = fixture();
        fs::write(f.worktree.join("huge.txt"), "x".repeat(MAX_DIFF_BYTES + 1)).unwrap();
        let diff = file_diff(&f.worktree, &f.base().commit, "huge.txt", None).unwrap();
        assert!(diff.truncated && diff.hunks.is_empty());
    }

    #[test]
    fn discarding_a_renamed_file_through_the_agent_repo_brings_the_old_name_back() {
        let f = fixture();
        run(&f.worktree, &["mv", "keep.txt", "kept.txt"]);
        run(&f.worktree, &["commit", "--quiet", "-m", "rename"]);
        let repo = AgentRepo {
            repo_root: f.repo.clone(),
            worktree: f.worktree.clone(),
            agent_branch: AGENT_BRANCH.into(),
            base_branch: Some("main".into()),
        };

        repo.discard(&["kept.txt".into()]).unwrap();
        assert!(!f.worktree.join("kept.txt").exists());
        assert_eq!(
            fs::read_to_string(f.worktree.join("keep.txt")).unwrap(),
            "keep\n"
        );
        assert!(f.changes().is_empty(), "{:?}", f.changes());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_in_the_base_reads_as_its_link_target() {
        use std::os::unix::fs::symlink;
        let f = fixture();
        symlink("README.md", f.repo.join("link.md")).unwrap();
        run(&f.repo, &["add", "link.md"]);
        run(&f.repo, &["commit", "--quiet", "-m", "add a symlink"]);
        let commit = git(&f.repo, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        assert_eq!(
            base_blob(&f.worktree, &commit, "link.md", 1024).unwrap(),
            BaseBlob::Bytes(b"README.md".to_vec()),
            "a symlink is a blob whose content is its target path, mode 120000"
        );
    }

    #[test]
    fn a_submodule_entry_in_the_base_counts_as_absent() {
        let f = fixture();
        let head = git(&f.repo, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();
        // A gitlink entry (mode 160000, type `commit`) is what `ls-tree` reports
        // for a submodule reference — built directly via the index rather than a
        // real submodule checkout (cheap, and `ls-tree` cannot tell the
        // difference from a real one).
        run(
            &f.repo,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                "160000",
                &head,
                "sub",
            ],
        );
        run(
            &f.repo,
            &["commit", "--quiet", "-m", "add a submodule reference"],
        );
        let commit = git(&f.repo, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        assert_eq!(
            base_blob(&f.worktree, &commit, "sub", 1024).unwrap(),
            BaseBlob::Absent,
            "a submodule has no file content to show"
        );
    }

    #[test]
    fn the_base_side_of_a_rename_is_read_under_its_old_path_only() {
        let f = fixture();
        run(&f.worktree, &["mv", "keep.txt", "kept.txt"]);
        run(&f.worktree, &["commit", "--quiet", "-m", "rename"]);
        let commit = f.base().commit;

        assert_eq!(
            base_blob(&f.worktree, &commit, "keep.txt", 1024).unwrap(),
            BaseBlob::Bytes(b"keep\n".to_vec()),
            "the old name still resolves in the base"
        );
        assert_eq!(
            base_blob(&f.worktree, &commit, "kept.txt", 1024).unwrap(),
            BaseBlob::Absent,
            "the new name does not exist in the base"
        );
    }

    #[test]
    fn base_blob_at_exactly_the_size_limit_is_still_read() {
        let f = fixture();
        // README.md is 14 bytes in the base (see
        // `the_base_side_of_a_file_is_read_as_the_base_has_it`).
        let commit = f.base().commit;
        assert_eq!(
            base_blob(&f.worktree, &commit, "README.md", 14).unwrap(),
            BaseBlob::Bytes(b"one\ntwo\nthree\n".to_vec()),
            "the limit itself must not be treated as too large"
        );
        assert_eq!(
            base_blob(&f.worktree, &commit, "README.md", 13).unwrap(),
            BaseBlob::TooLarge { size: 14 },
            "one byte over the limit is too large"
        );
    }

    #[test]
    fn the_base_side_of_a_file_with_spaces_and_umlauts_in_its_name_is_read() {
        let f = fixture();
        fs::write(f.repo.join("new file ü.txt"), "a\nb").unwrap();
        run(&f.repo, &["add", "."]);
        run(
            &f.repo,
            &["commit", "--quiet", "-m", "add a file with an odd name"],
        );
        let commit = git(&f.repo, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        assert_eq!(
            base_blob(&f.worktree, &commit, "new file ü.txt", 1024).unwrap(),
            BaseBlob::Bytes(b"a\nb".to_vec())
        );
    }

    #[test]
    fn discard_hunk_on_a_renamed_and_edited_file_uses_the_old_path() {
        let f = fixture();
        // A big enough file that a one-line edit still keeps the similarity
        // git's `-M` rename detection needs (default threshold 50%) — unlike
        // a tiny file, where a small edit can drop below it (see the pinning
        // test below for what happens then).
        let original: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        fs::write(f.repo.join("notes.txt"), &original).unwrap();
        run(&f.repo, &["add", "."]);
        run(&f.repo, &["commit", "--quiet", "-m", "add notes.txt"]);
        run(&f.worktree, &["merge", "--quiet", "--ff-only", "main"]);

        run(&f.worktree, &["mv", "notes.txt", "renamed-notes.txt"]);
        let edited = original.replace("line 10\n", "LINE TEN\n");
        fs::write(f.worktree.join("renamed-notes.txt"), &edited).unwrap();
        run(&f.worktree, &["add", "-A"]);
        run(
            &f.worktree,
            &["commit", "--quiet", "-m", "rename and edit notes.txt"],
        );

        let commit = f.base().commit;
        // Confirm this really is one rename, not a delete and an add — the
        // situation discard_hunk's old_path parameter exists for.
        let change = changes(&f.worktree, &commit)
            .unwrap()
            .into_iter()
            .find(|c| c.path == "renamed-notes.txt")
            .unwrap();
        assert_eq!(
            (change.kind, change.old_path.as_deref()),
            (ChangeKind::Renamed, Some("notes.txt"))
        );

        let diff = file_diff(&f.worktree, &commit, "renamed-notes.txt", Some("notes.txt")).unwrap();
        assert_eq!(diff.hunks.len(), 1, "{diff:?}");
        discard_hunk(
            &f.worktree,
            &commit,
            "renamed-notes.txt",
            Some("notes.txt"),
            0,
            &diff.hunks[0].header,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(f.worktree.join("renamed-notes.txt")).unwrap(),
            original,
            "the edit must have been taken back, under the renamed file's own name"
        );
    }

    /// **Suspected production bug, pinned rather than fixed (test-engineer
    /// constraint: production code is not to be touched).**
    ///
    /// `file_diff`'s docs promise that passing `old_path` alongside `path`
    /// lets git "pair the two" as one rename. That only holds while git's own
    /// `-M` rename detection agrees the pair is similar enough (default
    /// threshold 50%) — the same threshold `tracked_changes` uses to decide
    /// whether to hand out an `old_path` at all, so in the ordinary flow the
    /// two agree. But `discard_hunk`'s `old_path` argument is not re-verified
    /// against that threshold — it comes straight from the webview
    /// (`ide_agent_discard_hunk` in `commands.rs`) — and if the file changes
    /// again after the UI last listed it, `changes()` and a *stale* `old_path`
    /// can disagree with what a fresh `file_diff` call would pair. When they
    /// disagree, git's `diff -- path old_path` call emits **two independent**
    /// `diff --git` sections (a deletion of `old_path`, an addition of
    /// `path`) instead of one rename section, and `parse_diff` — which never
    /// looks at `diff --git`/`---`/`+++` lines to notice a new file has
    /// started — merges both sections' hunks into one `FileDiff`, misreading
    /// the second section's own `--- /dev/null` / `+++ b/…` header lines as
    /// ordinary content lines. The result is a `FileDiff` that not only mixes
    /// two unrelated files' hunks together but also inserts real diff-header
    /// text into `DiffLine::text` — data that would go straight into a
    /// `git apply -R` patch (`hunk_patch`) if a hunk from it were discarded.
    #[test]
    fn discard_hunk_pins_a_parser_gap_when_the_rename_pair_falls_below_the_similarity_threshold() {
        let f = fixture();
        // keep.txt is 5 bytes; adding one more line drops well under the 50%
        // similarity `-M` needs, so git does NOT pair this as a rename.
        run(&f.worktree, &["mv", "keep.txt", "kept.txt"]);
        fs::write(f.worktree.join("kept.txt"), "keep\nadded line\n").unwrap();
        run(&f.worktree, &["add", "-A"]);
        run(
            &f.worktree,
            &[
                "commit",
                "--quiet",
                "-m",
                "rename and edit, below the threshold",
            ],
        );
        let commit = f.base().commit;

        // `changes()` agrees: this is not a rename, so a real caller would
        // never have an `old_path` to pass here in the first place.
        let change = changes(&f.worktree, &commit)
            .unwrap()
            .into_iter()
            .find(|c| c.path == "kept.txt")
            .unwrap();
        assert_ne!(
            change.kind,
            ChangeKind::Renamed,
            "not a rename by git's own count"
        );

        // A caller that (through a stale UI state) still passes the old
        // `old_path` anyway gets back a `FileDiff` with hunks bled together
        // from two unrelated `diff --git` sections — pinned here as today's
        // actual behaviour, not as the intended one.
        let diff = file_diff(&f.worktree, &commit, "kept.txt", Some("keep.txt")).unwrap();
        assert_eq!(
            diff.hunks.len(),
            2,
            "one file's worth of change parsed as two unrelated hunks: {diff:?}"
        );
        let second_hunk_texts: Vec<_> = diff.hunks[0]
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect();
        assert!(
            second_hunk_texts.contains(&"-- /dev/null"),
            "the second file section's own `--- /dev/null` header line was misread as content: \
             {second_hunk_texts:?}"
        );
    }

    #[test]
    fn discard_hunk_handles_no_newline_markers_on_both_sides() {
        let f = fixture();
        // Neither the base nor the agent's copy ends in a newline.
        fs::write(f.repo.join("no-newline.txt"), "one\ntwo").unwrap();
        run(&f.repo, &["add", "."]);
        run(
            &f.repo,
            &[
                "commit",
                "--quiet",
                "-m",
                "add a file without a trailing newline",
            ],
        );
        run(&f.worktree, &["merge", "--quiet", "--ff-only", "main"]);
        fs::write(f.worktree.join("no-newline.txt"), "one\nTWO").unwrap();
        run(
            &f.worktree,
            &[
                "commit",
                "--quiet",
                "-am",
                "agent edit, still no trailing newline",
            ],
        );

        let commit = f.base().commit;
        let diff = file_diff(&f.worktree, &commit, "no-newline.txt", None).unwrap();
        assert_eq!(diff.hunks.len(), 1, "{diff:?}");
        let kinds: Vec<_> = diff.hunks[0].lines.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds.iter().filter(|k| **k == LineKind::NoNewline).count(),
            2,
            "one marker on the base side, one on the agent's side: {kinds:?}"
        );

        discard_hunk(
            &f.worktree,
            &commit,
            "no-newline.txt",
            None,
            0,
            &diff.hunks[0].header,
        )
        .unwrap();
        assert_eq!(
            fs::read(f.worktree.join("no-newline.txt")).unwrap(),
            b"one\ntwo".to_vec(),
            "back to the base, still without a trailing newline"
        );
    }

    #[test]
    fn discard_hunk_on_a_deleted_file_brings_it_back() {
        let f = fixture();
        run(&f.worktree, &["rm", "--quiet", "keep.txt"]);
        run(&f.worktree, &["commit", "--quiet", "-m", "delete keep.txt"]);

        let commit = f.base().commit;
        let diff = file_diff(&f.worktree, &commit, "keep.txt", None).unwrap();
        assert_eq!(diff.hunks.len(), 1, "{diff:?}");
        assert!(diff.new_size.is_none(), "gone from the worktree");
        discard_hunk(
            &f.worktree,
            &commit,
            "keep.txt",
            None,
            0,
            &diff.hunks[0].header,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(f.worktree.join("keep.txt")).unwrap(),
            "keep\n",
            "the whole-file hunk of a deleted file routes through discard"
        );
    }

    #[test]
    fn discard_hunk_refuses_an_out_of_range_index_even_with_a_real_header() {
        let (f, _) = two_hunk_fixture();
        let commit = f.base().commit;
        let diff = file_diff(&f.worktree, &commit, "README.md", None).unwrap();
        // A header that is byte-for-byte one of the diff's real headers, but
        // at an index past the end — the bounds check must run before the
        // header is ever compared, so this must not panic.
        let real_header = diff.hunks[0].header.clone();
        let err =
            discard_hunk(&f.worktree, &commit, "README.md", None, 99, &real_header).unwrap_err();
        assert!(err.to_string().contains("changed since"), "{err}");
        let text = fs::read_to_string(f.worktree.join("README.md")).unwrap();
        assert!(text.contains("LINE TWO"), "nothing was taken back");
    }

    #[test]
    fn discard_with_a_directory_path_reaches_everything_under_it() {
        let f = fixture();
        fs::create_dir_all(f.repo.join("docs")).unwrap();
        fs::write(f.repo.join("docs/a.md"), "a\n").unwrap();
        fs::write(f.repo.join("docs/b.md"), "b\n").unwrap();
        run(&f.repo, &["add", "."]);
        run(
            &f.repo,
            &["commit", "--quiet", "-m", "add a docs directory"],
        );
        run(&f.worktree, &["merge", "--quiet", "--ff-only", "main"]);

        fs::write(f.worktree.join("docs/a.md"), "changed a\n").unwrap();
        fs::write(f.worktree.join("docs/b.md"), "changed b\n").unwrap();
        run(
            &f.worktree,
            &["commit", "--quiet", "-am", "agent edits the docs directory"],
        );

        let base = f.base().commit;
        discard(&f.worktree, &base, &["docs".to_string()]).unwrap();

        assert_eq!(
            fs::read_to_string(f.worktree.join("docs/a.md")).unwrap(),
            "a\n"
        );
        assert_eq!(
            fs::read_to_string(f.worktree.join("docs/b.md")).unwrap(),
            "b\n"
        );
    }

    #[test]
    fn git_with_input_fails_cleanly_when_git_rejects_the_patch() {
        let f = fixture();
        let err =
            git_with_input(&f.worktree, &["apply", "--check", "-"], b"not a patch\n").unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("apply"),
            "should name the command: {message}"
        );
    }

    #[test]
    fn the_last_subject_is_only_the_most_recent_of_several_commits() {
        let f = fixture();
        f.agent_commits("README.md", "first change\n");
        f.agent_commits("README.md", "second change\n");
        assert_eq!(
            last_subject(&f.worktree, &f.base().commit)
                .unwrap()
                .as_deref(),
            Some("agent work"),
            "there is only one most-recent subject to report"
        );
        // A distinct message on the latest commit must be the one reported.
        fs::write(f.worktree.join("README.md"), "third change\n").unwrap();
        run(&f.worktree, &["add", "-A"]);
        run(&f.worktree, &["commit", "--quiet", "-m", "final touch-up"]);
        assert_eq!(
            last_subject(&f.worktree, &f.base().commit)
                .unwrap()
                .as_deref(),
            Some("final touch-up")
        );
    }
}
