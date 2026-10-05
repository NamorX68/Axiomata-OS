//! The integration line of a plan (`docs/plans/a2a.md`, CP-A8).
//!
//! A plan that runs by itself has a branch of its own, `axiomata/line/<id>`, with a worktree of its own next to the
//! agents'. It is cut from the branch the project was on when the plan started. The reviewer's yes **integrates** a
//! card into it — one commit per card, squashed — and the cards that build on that card start from it, so each sees
//! what the ones before it did. The owner's one click at the end takes the line over into the project's own branch.
//!
//! Why a line and not stacked agent branches (what A16 first said): a card that needs two others would have to start
//! from a merge of two agent branches, a squash of the first into the main line would collide with the second's
//! history, and two cards that touch the same file would find out only at the owner's take-over. Here a conflict
//! between cards shows when the second one is integrated, to the studio, and not to the owner.
//!
//! Everything git does in a worktree here runs **without hooks** ([`crate::worktree::git_agent`]): the line holds what
//! agents wrote, and a hook script among it must not run when the studio commits. (The take-over into the owner's own
//! branch is the owner's click in the owner's repository and runs their hooks, as every take-over does.)

use std::path::{Path, PathBuf};

use crate::worktree::{self, git_agent, git_agent_with, slugify};
use crate::{IdeError, Result};

/// The start of every integration line's branch name. An agent's branch is `axiomata/<slug>-<id>` and a slug has no
/// `/`, so no agent can have a line's name.
const LINE_PREFIX: &str = "axiomata/line/";

/// The branch of plan `plan_id`'s line.
pub fn line_branch(plan_id: i64) -> String {
    format!("{LINE_PREFIX}{plan_id}")
}

/// Where the line's worktree lives: beside the agents' (`<base>/<project>/.lines/<id>`). An agent's directory is
/// `<slug>-<id>`, which cannot be `.lines`, whatever the agent is called; a project whose name has no letters or digits
/// is called `project`.
pub fn line_path(base: &Path, project_name: &str, plan_id: i64) -> PathBuf {
    let project = match slugify(project_name) {
        slug if slug.is_empty() => "project".to_owned(),
        slug => slug,
    };
    base.join(project).join(".lines").join(plan_id.to_string())
}

/// A plan's line: the worktree it is checked out in, its branch, and the branch it was cut from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub path: PathBuf,
    pub branch: String,
    pub base_branch: String,
    /// The line did not exist before this call. A line that did exist is only trusted when its tip is the one the
    /// studio recorded (`plans.line_tip`): a branch of that name is a branch like any other.
    pub created: bool,
}

/// The commit the line is at.
///
/// # Errors
///
/// [`IdeError::Git`] if git cannot say.
pub fn tip(line: &Path) -> Result<String> {
    Ok(git_agent(line, &["rev-parse", "HEAD"])?.trim().to_owned())
}

/// Whether everything on the line since `recorded` — the commit the studio last wrote down — is the studio's own: commits
/// whose subject is `#<card> <title>`, the message it integrates with. With no record (`None`) the line must have no
/// commit of its own at all, i.e. be contained in its base branch. Lets the studio take up again after a crash between
/// the commit and the record; a line that something else moved is not taken.
///
/// # Errors
///
/// [`IdeError::Git`] if git cannot say, or `base_branch` is not a safe ref name.
pub fn only_studio_commits_since(
    line: &Path,
    recorded: Option<&str>,
    base_branch: &str,
) -> Result<bool> {
    let is_ancestor = |older: &str, newer: &str| -> Result<bool> {
        Ok(git_agent_with(
            line,
            &["merge-base", "--is-ancestor", older, newer],
            // 128: a commit that does not exist is not an ancestor either.
            &[0, 1, 128],
        )?
        .0 == 0)
    };
    match recorded {
        None => {
            if !worktree::is_safe_ref(base_branch) {
                return Err(IdeError::Git {
                    command: "git merge-base".into(),
                    reason: format!("{base_branch:?} is not a branch name the studio uses"),
                });
            }
            is_ancestor("HEAD", base_branch)
        }
        Some(recorded) => {
            if !crate::agent_store::is_commit_id(recorded) || !is_ancestor(recorded, "HEAD")? {
                return Ok(false);
            }
            let subjects = git_agent(line, &["log", "--format=%s", &format!("{recorded}..HEAD")])?;
            Ok(subjects.lines().all(is_studio_subject))
        }
    }
}

/// `#<digits> <text>`: the subject the studio gives the commit of an integrated card.
fn is_studio_subject(subject: &str) -> bool {
    subject
        .strip_prefix('#')
        .and_then(|rest| rest.split_once(' '))
        .is_some_and(|(id, _)| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
}

/// What integrating a card did. A conflict is an ordinary outcome, like a take-over's: the line is as it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Integration {
    /// The card's work is a new commit on the line.
    Done { commit: String },
    /// The card's work does not fit what the line has by now; nothing was changed.
    Conflict { files: Vec<String> },
    /// The card changes nothing compared to the line (its work is on it already, or it did none).
    Empty,
}

/// Makes the line of plan `plan_id`, or finds it when it exists (a restart, a second card).
///
/// The line is cut from `recorded_base` when the plan has one, else from the branch the project folder has checked out;
/// a project on a detached `HEAD` has no branch to build on.
///
/// # Errors
///
/// [`IdeError::Git`] for a detached `HEAD`, a base that is not a safe branch name, or whatever git says.
pub fn ensure(
    repo_root: &Path,
    path: &Path,
    plan_id: i64,
    recorded_base: Option<&str>,
) -> Result<Line> {
    let base_branch = match recorded_base {
        Some(base) => base.to_owned(),
        None => worktree::current_branch(repo_root).ok_or_else(|| IdeError::Git {
            command: "git worktree add".into(),
            reason: "the project folder is on a detached HEAD, so there is no branch to build the plan on".into(),
        })?,
    };
    let branch = line_branch(plan_id);
    let created = !worktree::branch_exists(repo_root, &branch);
    let made = worktree::add_from(repo_root, path, &branch, Some(&base_branch))?;
    Ok(Line {
        path: made.path,
        branch,
        base_branch,
        created,
    })
}

/// Holds the line for one integration: a second one at the same moment (the app's tick and the CLI, two apps) would
/// find the first one's staged work and could throw it away.
struct LineLock {
    /// Held, not read: the lock lasts as long as the file is open.
    _file: std::fs::File,
}

impl LineLock {
    fn take(line: &Path) -> Result<Self> {
        let dir = git_agent(line, &["rev-parse", "--absolute-git-dir"])?;
        let path = Path::new(dir.trim()).join("axiomata-integrate.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|err| IdeError::Invalid {
                field: "integrate",
                reason: format!("could not open {}: {err}", path.display()),
            })?;
        file.lock().map_err(|err| IdeError::Invalid {
            field: "integrate",
            reason: format!("could not lock {}: {err}", path.display()),
        })?;
        Ok(LineLock { _file: file })
    }
}

/// Merges the work at `source` into the line as **one commit** with `message`, without hooks.
///
/// `source` is a full commit id (what the studio checked is what goes in — a branch name could have moved since) or a
/// branch of the studio's (`axiomata/…`); either becomes an argument of `git`. The card's worktree must be committed
/// and the line's clean; a conflict is undone. The line is refused while the repository's config has a program git
/// would run for a merge or a commit ([`worktree::unsafe_config`]), and the integration is taken one at a time per
/// line.
///
/// # Errors
///
/// [`IdeError::Invalid`] for a source that is neither, an empty message, a dirty line or an unsafe config;
/// [`IdeError::Git`] otherwise.
pub fn integrate(line: &Line, source: &str, message: &str) -> Result<Integration> {
    let path = line.path.as_path();
    let refuse = |reason: String| {
        Err(IdeError::Invalid {
            field: "integrate",
            reason,
        })
    };
    let studio_branch = source.starts_with("axiomata/") && worktree::is_safe_ref(source);
    if !studio_branch && !crate::agent_store::is_commit_id(source) {
        return refuse(format!(
            "{source:?} is neither a commit id nor a branch the studio made"
        ));
    }
    let message = message.trim();
    if message.is_empty() {
        return refuse("integrating work needs a commit message".into());
    }
    let unsafe_keys = worktree::unsafe_config(path)?;
    if !unsafe_keys.is_empty() {
        return refuse(format!(
            "the repository's config names a program git would run for a merge or a commit ({}); the studio does not \
             merge into a plan's line while it does",
            unsafe_keys.join(", ")
        ));
    }
    let _lock = LineLock::take(path)?;
    // Anything else on the line's worktree than its own branch would take the commit with it.
    if worktree::current_branch(path).as_deref() != Some(line.branch.as_str()) {
        return refuse(format!(
            "the plan's line worktree is not on {}; something moved it",
            line.branch
        ));
    }
    // Tracked changes only: an untracked file (`.DS_Store`) is no reason to refuse, and one that a merge would
    // overwrite makes git say so.
    if !git_agent(path, &["status", "--porcelain", "--untracked-files=no"])?
        .trim()
        .is_empty()
    {
        return refuse(
            "the plan's line has uncommitted changes; something else is working in it".into(),
        );
    }
    if let Err(err) = git_agent(path, &["merge", "--squash", source]) {
        let conflicted = git_agent_with(path, &["diff", "--name-only", "--diff-filter=U"], &[0])
            .map(|(_, out)| out.lines().map(str::to_owned).collect::<Vec<_>>())
            .unwrap_or_default();
        if conflicted.is_empty() {
            // Not a conflict: git did not get as far as staging anything of ours. Undoing here could undo somebody
            // else's staged work, so the failure is only reported.
            return Err(err);
        }
        git_agent(path, &["reset", "--merge"])?;
        return Ok(Integration::Conflict { files: conflicted });
    }
    // Nothing staged: the card's work is on the line already.
    if git_agent_with(path, &["diff", "--cached", "--quiet"], &[0, 1])?.0 == 0 {
        git_agent(path, &["reset", "--merge"])?;
        return Ok(Integration::Empty);
    }
    if let Err(err) = git_agent(path, &["commit", "--quiet", "--no-verify", "-m", message]) {
        git_agent(path, &["reset", "--merge"])?;
        return Err(err);
    }
    Ok(Integration::Done { commit: tip(path)? })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static N: AtomicU32 = AtomicU32::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-line-{label}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn repo() -> PathBuf {
        let dir = temp_dir("repo");
        git(&dir, &["init", "--initial-branch=main"]);
        git(&dir, &["config", "user.email", "t@example.com"]);
        git(&dir, &["config", "user.name", "T"]);
        fs::write(dir.join("a.txt"), "one\ntwo\nthree\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "first"]);
        dir
    }

    /// A card branch with its own worktree, cut from `start`, holding `file` = `content`, committed.
    fn card(repo: &Path, name: &str, start: &str, file: &str, content: &str) -> (PathBuf, String) {
        let path = temp_dir(name).join("w");
        let branch = format!("axiomata/{name}-1");
        worktree::add_from(repo, &path, &branch, Some(start)).unwrap();
        fs::write(path.join(file), content).unwrap();
        git(&path, &["add", "."]);
        git(&path, &["commit", "-q", "-m", name]);
        (path, branch)
    }

    #[test]
    fn a_line_is_cut_from_the_branch_the_project_is_on_and_found_again_not_made_twice() {
        let repo = repo();
        let path = temp_dir("line").join("plan-4");
        let line = ensure(&repo, &path, 4, None).unwrap();
        assert_eq!(
            (line.branch.as_str(), line.base_branch.as_str()),
            ("axiomata/line/4", "main")
        );
        assert_eq!(
            worktree::current_branch(&line.path).as_deref(),
            Some("axiomata/line/4")
        );
        let again = ensure(&repo, &path, 4, Some("main")).unwrap();
        assert_eq!(again.path, line.path);
        assert_eq!(
            line_path(Path::new("/w"), "My App", 4),
            Path::new("/w/my-app/.lines/4")
        );
    }

    #[test]
    fn a_project_on_a_detached_head_has_no_branch_to_build_a_plan_on() {
        let repo = repo();
        let head = git(&repo, &["rev-parse", "HEAD"]);
        git(&repo, &["checkout", "-q", "--detach", &head]);
        let err = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap_err();
        assert!(matches!(err, IdeError::Git { .. }), "{err:?}");
        // A recorded base is used even then.
        assert!(ensure(&repo, &temp_dir("line").join("plan-2"), 2, Some("main")).is_ok());
    }

    #[test]
    fn a_cards_work_becomes_one_commit_on_the_line_and_a_card_that_builds_on_it_starts_from_there()
    {
        let repo = repo();
        let line = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap();
        let (_, first) = card(&repo, "first", &line.branch, "a.txt", "one\nTWO\nthree\n");
        let done = integrate(&line, &first, "#1 first").unwrap();
        let Integration::Done { commit } = done else {
            panic!("{done:?}")
        };
        assert_eq!(git(&line.path, &["rev-parse", "HEAD"]), commit);
        assert_eq!(git(&line.path, &["log", "-1", "--format=%s"]), "#1 first");
        assert_eq!(git(&line.path, &["rev-list", "--count", "main..HEAD"]), "1");

        // The next card starts from the line, so it has the first one's change.
        let (second_tree, _) = card(&repo, "second", &line.branch, "b.txt", "new\n");
        assert_eq!(
            fs::read_to_string(second_tree.join("a.txt")).unwrap(),
            "one\nTWO\nthree\n"
        );
    }

    #[test]
    fn two_cards_that_change_the_same_line_conflict_on_the_second_and_the_line_stays_as_it_was() {
        let repo = repo();
        let line = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap();
        // Both start from the line before either is integrated: they run side by side.
        let (_, one) = card(&repo, "one", &line.branch, "a.txt", "one\nUNO\nthree\n");
        let (_, two) = card(&repo, "two", &line.branch, "a.txt", "one\nDUE\nthree\n");
        assert!(matches!(
            integrate(&line, &one, "#1").unwrap(),
            Integration::Done { .. }
        ));
        let tip = git(&line.path, &["rev-parse", "HEAD"]);
        let conflict = integrate(&line, &two, "#2").unwrap();
        assert_eq!(
            conflict,
            Integration::Conflict {
                files: vec!["a.txt".into()]
            }
        );
        assert_eq!(git(&line.path, &["rev-parse", "HEAD"]), tip);
        assert_eq!(git(&line.path, &["status", "--porcelain"]), "");
    }

    #[test]
    fn work_that_is_on_the_line_already_integrates_as_nothing() {
        let repo = repo();
        let line = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap();
        let (_, one) = card(&repo, "one", &line.branch, "a.txt", "one\nUNO\nthree\n");
        integrate(&line, &one, "#1").unwrap();
        assert_eq!(
            integrate(&line, &one, "#1 again").unwrap(),
            Integration::Empty
        );
    }

    #[test]
    fn only_a_branch_of_the_studio_is_integrated_and_a_message_is_needed() {
        let repo = repo();
        let line = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap();
        for bad in ["main", "--output=/tmp/x", "axiomata/../main", "", "HEAD~1"] {
            assert!(
                matches!(integrate(&line, bad, "m"), Err(IdeError::Invalid { .. })),
                "{bad}"
            );
        }
        let (_, one) = card(&repo, "one", &line.branch, "a.txt", "x\n");
        assert!(matches!(
            integrate(&line, &one, "  "),
            Err(IdeError::Invalid { .. })
        ));
    }

    #[test]
    fn a_commit_id_is_integrated_as_well_as_a_branch_so_what_was_checked_is_what_goes_in() {
        let repo = repo();
        let line = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap();
        let (tree, branch) = card(&repo, "one", &line.branch, "a.txt", "one\nUNO\nthree\n");
        let reviewed = git(&tree, &["rev-parse", "HEAD"]);
        // The branch moves after the check; the commit that was checked is what is merged.
        fs::write(tree.join("a.txt"), "one\nSNEAKY\nthree\n").unwrap();
        git(&tree, &["add", "."]);
        git(&tree, &["commit", "-q", "-m", "later"]);
        assert_ne!(git(&repo, &["rev-parse", &branch]), reviewed);
        assert!(matches!(
            integrate(&line, &reviewed, "#1").unwrap(),
            Integration::Done { .. }
        ));
        assert_eq!(git(&line.path, &["show", "HEAD:a.txt"]), "one\nUNO\nthree");
        assert!(matches!(
            integrate(&line, "abc123", "#1"),
            Err(IdeError::Invalid { .. })
        ));
    }

    #[test]
    fn a_line_is_known_to_be_new_only_the_first_time_and_its_tip_is_what_the_studio_made() {
        let repo = repo();
        let path = temp_dir("line").join("plan-1");
        let first = ensure(&repo, &path, 1, None).unwrap();
        assert!(first.created);
        assert_eq!(
            tip(&first.path).unwrap(),
            git(&repo, &["rev-parse", "main"])
        );
        assert!(!ensure(&repo, &path, 1, Some("main")).unwrap().created);
        // A branch of that name that somebody made before is not "created" by the studio.
        git(&repo, &["branch", "axiomata/line/2", "main"]);
        assert!(
            !ensure(&repo, &temp_dir("line").join("plan-2"), 2, None)
                .unwrap()
                .created
        );
    }

    #[test]
    fn a_repository_that_names_a_merge_driver_or_a_filter_is_not_merged_into() {
        let repo = repo();
        let line = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap();
        let (_, one) = card(&repo, "one", &line.branch, "a.txt", "one\nUNO\nthree\n");
        assert!(worktree::unsafe_config(&line.path).unwrap().is_empty());
        for (key, value) in [
            ("merge.x.driver", "touch /tmp/never %A"),
            ("filter.x.clean", "cat"),
            ("diff.x.textconv", "cat"),
            ("gpg.program", "/bin/false"),
        ] {
            git(&repo, &["config", key, value]);
            let keys = worktree::unsafe_config(&line.path).unwrap();
            assert_eq!(keys, vec![key.to_owned()], "{key}");
            let refused = integrate(&line, &one, "#1").unwrap_err();
            assert!(
                matches!(refused, IdeError::Invalid { .. }),
                "{key}: {refused:?}"
            );
            git(&repo, &["config", "--unset", key]);
        }
        assert!(matches!(
            integrate(&line, &one, "#1").unwrap(),
            Integration::Done { .. }
        ));
    }

    #[test]
    fn a_line_worktree_that_was_moved_off_its_branch_takes_no_commit_and_an_untracked_file_is_no_reason_to_refuse()
     {
        let repo = repo();
        let line = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap();
        let (_, one) = card(&repo, "one", &line.branch, "a.txt", "one\nUNO\nthree\n");
        fs::write(line.path.join(".DS_Store"), "x").unwrap();
        let head = git(&line.path, &["rev-parse", "HEAD"]);
        git(&line.path, &["checkout", "-q", "--detach", &head]);
        assert!(matches!(
            integrate(&line, &one, "#1"),
            Err(IdeError::Invalid { .. })
        ));
        git(&line.path, &["checkout", "-q", &line.branch]);
        assert!(matches!(
            integrate(&line, &one, "#1").unwrap(),
            Integration::Done { .. }
        ));
    }

    #[test]
    fn a_line_that_is_ahead_of_its_record_only_by_studio_commits_is_taken_up_again() {
        let repo = repo();
        let line = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap();
        let start = tip(&line.path).unwrap();
        // Nothing was written down and nothing is on the line: as made.
        assert!(only_studio_commits_since(&line.path, None, "main").unwrap());

        // The studio committed and the record was lost (a crash in between): its own commit is taken.
        let (_, first) = card(&repo, "first", &line.branch, "a.txt", "one\nTWO\nthree\n");
        integrate(&line, &first, "#7 first").unwrap();
        assert!(only_studio_commits_since(&line.path, Some(&start), "main").unwrap());
        // With no record at all a line that has a commit of its own is not taken.
        assert!(!only_studio_commits_since(&line.path, None, "main").unwrap());

        // Something else's commit on the line is not.
        let recorded = tip(&line.path).unwrap();
        fs::write(line.path.join("b.txt"), "b\n").unwrap();
        git(&line.path, &["add", "."]);
        git(&line.path, &["commit", "-q", "-m", "sneaked"]);
        assert!(!only_studio_commits_since(&line.path, Some(&recorded), "main").unwrap());
        // And a record that is not an ancestor, or not a commit id, is not.
        assert!(
            !only_studio_commits_since(&line.path, Some("0".repeat(40).as_str()), "main").unwrap()
        );
        assert!(!only_studio_commits_since(&line.path, Some("main"), "main").unwrap());
    }

    #[test]
    fn a_line_is_made_again_after_its_worktree_was_deleted_by_hand() {
        let repo = repo();
        let path = temp_dir("line").join("plan-1");
        let line = ensure(&repo, &path, 1, None).unwrap();
        fs::remove_dir_all(&line.path).unwrap();
        let again = ensure(&repo, &path, 1, Some("main")).unwrap();
        assert_eq!(
            worktree::current_branch(&again.path).as_deref(),
            Some(line.branch.as_str())
        );
    }

    #[test]
    fn no_agent_can_have_the_name_or_the_directory_of_a_line() {
        // An agent called "line" or "plan": its branch and its directory are `<slug>-<id>`.
        assert_ne!(worktree::branch_name("line", 4), line_branch(4));
        assert_ne!(worktree::branch_name("plan", 4), line_branch(4));
        let base = Path::new("/w");
        assert_ne!(
            worktree::worktree_path(base, "P", "line", 4),
            line_path(base, "P", 4)
        );
        assert_ne!(
            worktree::worktree_path(base, "P", ".lines", 4),
            line_path(base, "P", 4)
        );
    }

    #[test]
    fn two_integrations_at_the_same_moment_take_turns() {
        let repo = repo();
        let line = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap();
        let (_, one) = card(&repo, "one", &line.branch, "a.txt", "one\nUNO\nthree\n");
        let (_, two) = card(&repo, "two", &line.branch, "b.txt", "two\n");
        let (line_a, line_b) = (line.clone(), line.clone());
        let (first, second) = (one.clone(), two.clone());
        let a = std::thread::spawn(move || integrate(&line_a, &first, "#1").unwrap());
        let b = std::thread::spawn(move || integrate(&line_b, &second, "#2").unwrap());
        let (a, b) = (a.join().unwrap(), b.join().unwrap());
        assert!(
            matches!(a, Integration::Done { .. }) && matches!(b, Integration::Done { .. }),
            "{a:?} {b:?}"
        );
        assert_eq!(git(&line.path, &["rev-list", "--count", "main..HEAD"]), "2");
        assert_eq!(git(&line.path, &["status", "--porcelain"]), "");
    }

    #[test]
    fn a_hook_of_the_line_does_not_run_when_the_studio_commits() {
        let repo = repo();
        let line = ensure(&repo, &temp_dir("line").join("plan-1"), 1, None).unwrap();
        let (_, one) = card(&repo, "one", &line.branch, "a.txt", "one\nUNO\nthree\n");
        // A hook the project (or an agent) put where git looks for them.
        let hooks = repo.join(".git/hooks");
        fs::create_dir_all(&hooks).unwrap();
        let marker = temp_dir("marker").join("ran");
        let hook = hooks.join("pre-commit");
        fs::write(&hook, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(matches!(
            integrate(&line, &one, "#1").unwrap(),
            Integration::Done { .. }
        ));
        assert!(!marker.exists(), "no hook runs in the studio's own commits");
    }
}
