//! What an agent has changed, and what can be done about it (M7.3 CP7).
//!
//! Every agent works on its own branch in its own worktree (CP5). This module
//! answers "what has this agent changed?" against the branch it was cut from,
//! and carries out the few things the IDE lets you do about it: throw a file's
//! changes away, commit what the agent left uncommitted, and take the agent's
//! work over into the project's own working copy. Decisions G1–G13 in
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

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::worktree::{self, git, git_with};
use crate::{IdeError, Result};

/// The most diff output read for one file. Beyond it the diff is cut off and
/// marked `truncated` — a generated lock file is not something to scroll.
pub const MAX_DIFF_BYTES: usize = 2 * 1024 * 1024;
/// The most diff lines returned for one file.
pub const MAX_DIFF_LINES: usize = 10_000;
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

/// How a file differs from the base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// Not in the base — either committed on the agent's branch or untracked.
    Added,
    /// In both, with different content.
    Modified,
    /// In the base, gone from the worktree.
    Deleted,
    /// The same content (or close to it) under a different path.
    Renamed,
    /// File became a symlink or the other way round.
    TypeChanged,
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

/// The kind of one line in a diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    /// Unchanged, shown for orientation only.
    Context,
    /// Present on the agent's side, not on the base's.
    Add,
    /// Present on the base's side, not on the agent's.
    Remove,
    /// `\ No newline at end of file`, attached to the line above it.
    NoNewline,
}

/// One line of a hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    /// Context, an addition, a removal, or a no-newline marker.
    pub kind: LineKind,
    /// Line number on the base side; `None` for an added line.
    pub old_line: Option<u32>,
    /// Line number on the agent's side; `None` for a removed line.
    pub new_line: Option<u32>,
    /// Without the leading `+`/`-`/space.
    pub text: String,
}

/// One `@@` hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hunk {
    /// The whole `@@ -a,b +c,d @@ context` line.
    pub header: String,
    /// The hunk's lines, in the order git printed them.
    pub lines: Vec<DiffLine>,
}

/// One file's diff, parsed (CP7) so the UI never has to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    /// Relative to the worktree root, `/`-separated.
    pub path: String,
    /// True when git reported this as binary content; `hunks` is then empty.
    pub binary: bool,
    /// Empty for a binary file, a pure rename, or a file too large to diff.
    pub hunks: Vec<Hunk>,
    /// Cut off at [`MAX_DIFF_BYTES`] / [`MAX_DIFF_LINES`].
    pub truncated: bool,
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
        let (lines, binary) = count_lines(&worktree.join(&path));
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
        });
    }
    let too_large = |size: u64| size > MAX_DIFF_BYTES as u64;
    let on_disk = fs::symlink_metadata(worktree.join(path))
        .map(|m| m.len())
        .unwrap_or(0);
    let in_base = base_blob_size(worktree, base, old_path.unwrap_or(path))?.unwrap_or(0);
    if too_large(on_disk) || too_large(in_base) {
        return Ok(FileDiff {
            path: path.to_string(),
            binary: false,
            hunks: Vec::new(),
            truncated: true,
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
    Ok(parse_diff(path, &raw))
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
        let relative = checked_path(path)?;
        if base_blob_size(worktree, base, path)?.is_some() {
            git(
                worktree,
                &[
                    "restore",
                    "--source",
                    base,
                    "--staged",
                    "--worktree",
                    "--",
                    path,
                ],
            )?;
            continue;
        }
        // Not in the base: the agent added it. Drop it from the index if it
        // is there, then from the disk — but only a file that really lives
        // inside the worktree, so a symlinked directory cannot turn this into
        // a deletion somewhere else.
        git(
            worktree,
            &["rm", "--cached", "--quiet", "--ignore-unmatch", "--", path],
        )?;
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

/// The size of `path` in `base`, or `None` when the base has no such file.
///
/// `ls-tree` rather than `cat-file -e`: the latter exits 128 for "no such
/// path" — the same code as any fatal error, so a real failure would have
/// read as "the agent added this" and led to a deletion (architecture review).
/// `ls-tree` exits 0 either way and answers with an empty line for "absent".
fn base_blob_size(worktree: &Path, base: &str, path: &str) -> Result<Option<u64>> {
    let listed = git(worktree, &["ls-tree", "-z", "--long", base, "--", path])?;
    Ok(listed
        .split('\0')
        .find(|entry| !entry.is_empty())
        .and_then(|entry| entry.split('\t').next())
        .map(|meta| {
            meta.split_whitespace()
                .nth(3)
                .and_then(|size| size.parse().ok())
                .unwrap_or(0)
        }))
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

/// Checks a path that came from outside: relative, no `..`, not empty.
pub fn checked_path(path: &str) -> Result<PathBuf> {
    let refuse = |why: &str| IdeError::Invalid {
        field: "path",
        reason: format!("{path:?} {why}"),
    };
    if path.is_empty() || path.contains('\0') {
        return Err(refuse("is not a usable path"));
    }
    let candidate = Path::new(path);
    let mut clean = PathBuf::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return Err(refuse("must be relative to the worktree, without `..`")),
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(refuse("is not a usable path"));
    }
    Ok(clean)
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

/// Parses `git diff` output for one file into hunks.
fn parse_diff(path: &str, raw: &str) -> FileDiff {
    let mut diff = FileDiff {
        path: path.to_string(),
        binary: false,
        hunks: Vec::new(),
        truncated: false,
    };
    let raw = if raw.len() > MAX_DIFF_BYTES {
        diff.truncated = true;
        // Cut at a line boundary at or before the limit.
        let cut = raw[..raw.floor_char_boundary(MAX_DIFF_BYTES)]
            .rfind('\n')
            .unwrap_or(0);
        &raw[..cut]
    } else {
        raw
    };

    let (mut old_line, mut new_line) = (0u32, 0u32);
    let mut lines_seen = 0usize;
    for line in raw.lines() {
        if line.starts_with("Binary files ") || line == "GIT binary patch" {
            diff.binary = true;
            continue;
        }
        if let Some((old_start, new_start)) = parse_hunk_header(line) {
            old_line = old_start;
            new_line = new_start;
            diff.hunks.push(Hunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = diff.hunks.last_mut() else {
            // Still in the file header (`diff --git`, `index`, `---`, `+++`).
            continue;
        };
        if lines_seen >= MAX_DIFF_LINES {
            diff.truncated = true;
            break;
        }
        let (kind, text) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Add, &line[1..]),
            Some(b'-') => (LineKind::Remove, &line[1..]),
            Some(b' ') => (LineKind::Context, &line[1..]),
            Some(b'\\') => (LineKind::NoNewline, line),
            // An empty context line whose leading space an editor stripped.
            None => (LineKind::Context, ""),
            _ => continue,
        };
        let (old, new) = match kind {
            LineKind::Add => {
                new_line += 1;
                (None, Some(new_line))
            }
            LineKind::Remove => {
                old_line += 1;
                (Some(old_line), None)
            }
            LineKind::Context => {
                old_line += 1;
                new_line += 1;
                (Some(old_line), Some(new_line))
            }
            LineKind::NoNewline => (None, None),
        };
        hunk.lines.push(DiffLine {
            kind,
            old_line: old,
            new_line: new,
            text: text.to_string(),
        });
        lines_seen += 1;
    }
    diff
}

/// `@@ -12,7 +12,9 @@ …` → the line *before* each side's first line, so the
/// caller can increment before every line it counts.
fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let rest = line.strip_prefix("@@ -")?;
    let (old, rest) = rest.split_once(" +")?;
    let (new, _) = rest.split_once(" @@")?;
    let start = |range: &str| -> Option<u32> {
        let first: u32 = range.split(',').next()?.parse().ok()?;
        Some(first.saturating_sub(1))
    };
    Some((start(old)?, start(new)?))
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
    fn an_untracked_file_diffs_against_nothing() {
        let f = fixture();
        fs::write(f.worktree.join("notes.md"), "hello\nworld").unwrap();
        let diff = file_diff(&f.worktree, &f.base().commit, "notes.md", None).unwrap();
        let kinds: Vec<_> = diff.hunks[0].lines.iter().map(|l| l.kind).collect();
        assert_eq!(kinds, [LineKind::Add, LineKind::Add, LineKind::NoNewline]);
    }

    #[test]
    fn the_parser_marks_a_cut_off_diff() {
        let body: String = (0..MAX_DIFF_LINES + 10)
            .map(|i| format!("+line {i}\n"))
            .collect();
        let raw = format!(
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -0,0 +1,{} @@\n{body}",
            MAX_DIFF_LINES + 10
        );
        let diff = parse_diff("x", &raw);
        assert!(diff.truncated);
        assert_eq!(diff.hunks[0].lines.len(), MAX_DIFF_LINES);
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
    fn the_parser_reads_a_hunk_header_without_explicit_counts() {
        // `@@ -1 +1 @@` — git omits the count when it is 1.
        let raw = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-old\n+new\n";
        let diff = parse_diff("x", raw);
        assert_eq!(diff.hunks.len(), 1);
        let lines: Vec<_> = diff.hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.old_line, l.new_line))
            .collect();
        assert_eq!(
            lines,
            [
                (LineKind::Remove, Some(1), None),
                (LineKind::Add, None, Some(1)),
            ]
        );
    }

    #[test]
    fn the_parser_handles_a_no_newline_marker_mid_hunk_and_several_hunks() {
        let raw = "diff --git a/x b/x\n--- a/x\n+++ b/x\n\
                    @@ -1,2 +1,2 @@\n-old1\n\\ No newline at end of file\n+new1\n context\n\
                    @@ -10,1 +10,2 @@\n context2\n+added2\n";
        let diff = parse_diff("x", raw);
        assert_eq!(diff.hunks.len(), 2);

        let first: Vec<_> = diff.hunks[0]
            .lines
            .iter()
            .map(|l| (l.kind, l.old_line, l.new_line))
            .collect();
        assert_eq!(
            first,
            [
                (LineKind::Remove, Some(1), None),
                (LineKind::NoNewline, None, None),
                (LineKind::Add, None, Some(1)),
                (LineKind::Context, Some(2), Some(2)),
            ]
        );

        let second: Vec<_> = diff.hunks[1]
            .lines
            .iter()
            .map(|l| (l.kind, l.old_line, l.new_line))
            .collect();
        assert_eq!(
            second,
            [
                (LineKind::Context, Some(10), Some(10)),
                (LineKind::Add, None, Some(11)),
            ]
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
}
