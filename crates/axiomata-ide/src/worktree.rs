//! Git worktrees, one per agent (milestone M7.2 CP5).
//!
//! ⚠️ **This module changes the file system.** `store`'s promise — "looks at
//! the file system, never changes it" — is per module and does not extend
//! here: creating a worktree makes a real directory, and removing one deletes
//! it. What it will never touch is the user's own repository working copy; it
//! only ever adds and removes directories under the base path it is handed.
//!
//! **Driven by the `git` command line, not `git2`/libgit2** (plan question
//! F3, answered here because CP5 needs worktrees before M7.3 needs a diff):
//!
//! * `git worktree` is the reference implementation of a feature libgit2 only
//!   partially models; matching its behaviour by hand is a bug farm.
//! * The user's own configuration applies for free — credential helpers,
//!   `includeIf`, hooks, `core.hooksPath`. That matters the moment an agent
//!   commits, which is what M7.3 is for.
//! * No C dependency and no build-time cost in a crate that is meant to stay
//!   easy to extract.
//!
//! The price is a runtime dependency on `git` being installed. On macOS with
//! the developer tools it always is; [`ensure_available`] turns its absence
//! into a sentence rather than a mystery.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{IdeError, Result};

/// A worktree as git reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    /// The checked-out branch, without `refs/heads/`. `None` for a detached HEAD.
    pub branch: Option<String>,
}

/// Runs `git` in `repo`, returning stdout on success.
///
/// Every git call in the crate goes through here so that a failure always
/// carries the command that failed and git's own stderr — the two things that
/// make a git error readable.
fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|err| IdeError::Git {
            command: format!("git {}", args.join(" ")),
            reason: format!("could not run git: {err}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(IdeError::Git {
            command: format!("git {}", args.join(" ")),
            reason: if stderr.is_empty() {
                format!("exited with {}", output.status)
            } else {
                stderr
            },
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Compares two paths as the file system sees them, not as they are spelled.
///
/// Necessary because git answers with resolved paths: on macOS a worktree
/// created under `/var/folders/…` (a symlink) comes back as
/// `/private/var/folders/…`, and a plain `==` then says the worktree that was
/// just created does not exist. The same trap `store::normalize_root` avoids
/// for a project folder, one directory deeper.
fn same_path(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Checks that `git` can be run at all, so a missing tool is reported once and
/// clearly rather than as the first worktree failure.
pub fn ensure_available() -> Result<String> {
    let output = Command::new("git")
        .arg("--version")
        .output()
        .map_err(|err| IdeError::Git {
            command: "git --version".into(),
            reason: format!("git does not appear to be installed: {err}"),
        })?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// True when `path` is inside a git repository's working tree.
///
/// A project folder that is not a repository is an ordinary situation, not an
/// error: its agents simply run in the folder itself, sharing it, the way they
/// did before worktrees existed.
pub fn is_repo(path: &Path) -> bool {
    git(path, &["rev-parse", "--is-inside-work-tree"])
        .map(|out| out.trim() == "true")
        .unwrap_or(false)
}

/// Turns a name into something safe to put in a path or a branch.
///
/// Lowercase, runs of anything unusual collapsed to a single `-`, trimmed.
/// Agent names are free text — "Qwen 3.8 27b FP4" is a real one — and this has
/// to survive being both a directory component and a git ref.
pub fn slugify(name: &str) -> String {
    let mut slug = String::with_capacity(name.len());
    let mut pending_dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.extend(ch.to_lowercase());
        } else {
            pending_dash = true;
        }
    }
    slug
}

/// Where an agent's worktree goes: `<base>/<project>/<agent>-<id>`.
///
/// The id is appended because a slug is not unique the way a name is: "A B"
/// and "A-B" are two different agents that would otherwise share one
/// directory, and a shared worktree is two agents overwriting each other.
pub fn worktree_path(base: &Path, project_name: &str, agent_name: &str, agent_id: i64) -> PathBuf {
    base.join(slugify(project_name))
        .join(format!("{}-{agent_id}", slugify(agent_name)))
}

/// The branch an agent works on. Namespaced so it is obvious who made it.
pub fn branch_name(agent_name: &str, agent_id: i64) -> String {
    format!("axiomata/{}-{agent_id}", slugify(agent_name))
}

/// Creates a worktree for an agent, on a new branch off the current HEAD.
///
/// Returns the existing one unchanged if the path is already a worktree, so a
/// repeated call — a restart, a re-created profile — is not an error.
///
/// The returned path is **git's**, which is the resolved one: ask for
/// `/var/…` on macOS and get `/private/var/…` back. Store that, so later
/// comparisons are against what git will say next time too.
pub fn add(repo_root: &Path, path: &Path, branch: &str) -> Result<Worktree> {
    if path.exists() {
        if let Some(existing) = list(repo_root)?
            .into_iter()
            .find(|w| same_path(&w.path, path))
        {
            return Ok(existing);
        }
        return Err(IdeError::Git {
            command: "git worktree add".into(),
            reason: format!("{} already exists and is not a worktree", path.display()),
        });
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| IdeError::Git {
            command: "git worktree add".into(),
            reason: format!("could not create {}: {err}", parent.display()),
        })?;
    }

    let path_arg = path.to_str().ok_or_else(|| IdeError::Git {
        command: "git worktree add".into(),
        reason: format!("path is not valid UTF-8: {}", path.display()),
    })?;

    // An existing branch is *checked out*, never reset.
    //
    // This used to pass `-B`, on the reasoning that a leftover branch must not
    // make a restart fail forever — but `-B` moves that branch to HEAD, and a
    // branch left over from a discarded worktree is exactly where an agent's
    // committed work lives. Discarding a worktree only ever asks about
    // *uncommitted* changes, so the sequence "agent commits, worktree is
    // discarded, agent is started again" silently threw the commits away
    // (security audit, CP5). Re-attaching to the branch gives the agent its
    // work back instead.
    //
    // If that branch is checked out in another worktree, git refuses — which
    // is right: two worktrees on one branch is how they diverge.
    if branch_exists(repo_root, branch) {
        git(repo_root, &["worktree", "add", path_arg, branch])?;
    } else {
        git(repo_root, &["worktree", "add", "-b", branch, path_arg])?;
    }

    list(repo_root)?
        .into_iter()
        .find(|w| same_path(&w.path, path))
        .ok_or_else(|| IdeError::Git {
            command: "git worktree add".into(),
            reason: "git reported success but the worktree is not listed".into(),
        })
}

/// Whether a local branch of that name exists.
pub fn branch_exists(repo_root: &Path, branch: &str) -> bool {
    git(
        repo_root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok()
}

/// Removes a worktree and its directory. `false` if it was not there.
///
/// `force` lets it go even with uncommitted changes in it — which is why the
/// caller has to decide, and why the UI asks before it passes `true`.
pub fn remove(repo_root: &Path, path: &Path, force: bool) -> Result<bool> {
    if !list(repo_root)?.iter().any(|w| same_path(&w.path, path)) {
        return Ok(false);
    }
    let path_arg = path.to_str().ok_or_else(|| IdeError::Git {
        command: "git worktree remove".into(),
        reason: format!("path is not valid UTF-8: {}", path.display()),
    })?;

    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(path_arg);
    git(repo_root, &args)?;
    Ok(true)
}

/// Every worktree of a repository, the main one first.
pub fn list(repo_root: &Path) -> Result<Vec<Worktree>> {
    let out = git(repo_root, &["worktree", "list", "--porcelain"])?;
    let mut worktrees = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;

    // Porcelain format: records separated by blank lines, `worktree <path>`
    // first, then optional `HEAD`/`branch`/`detached` lines.
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            if let Some(previous) = path.take() {
                worktrees.push(Worktree {
                    path: previous,
                    branch: branch.take(),
                });
            }
            path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("branch ") {
            branch = Some(rest.trim_start_matches("refs/heads/").to_string());
        }
    }
    if let Some(last) = path {
        worktrees.push(Worktree { path: last, branch });
    }
    Ok(worktrees)
}

/// Whether a worktree has changes that would be lost by removing it.
pub fn has_uncommitted_changes(worktree_path: &Path) -> Result<bool> {
    Ok(!git(worktree_path, &["status", "--porcelain"])?
        .trim()
        .is_empty())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-worktree-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A repository with one commit — `git worktree add` needs a HEAD to
    /// branch from, so an empty `git init` is not enough.
    fn repo() -> PathBuf {
        let dir = temp_dir("repo");
        git(&dir, &["init", "--initial-branch=main"]).unwrap();
        git(&dir, &["config", "user.email", "test@example.com"]).unwrap();
        git(&dir, &["config", "user.name", "Test"]).unwrap();
        std::fs::write(dir.join("README.md"), "hello\n").unwrap();
        git(&dir, &["add", "."]).unwrap();
        git(&dir, &["commit", "-m", "first"]).unwrap();
        dir
    }

    #[test]
    fn git_is_available() {
        assert!(ensure_available().unwrap().starts_with("git version"));
    }

    #[test]
    fn a_repository_is_recognised_and_a_plain_folder_is_not() {
        assert!(is_repo(&repo()));
        assert!(!is_repo(&temp_dir("plain")));
    }

    #[test]
    fn adding_a_worktree_creates_it_on_its_own_branch() {
        let repo = repo();
        let base = temp_dir("base");
        let path = worktree_path(&base, "Axiomata OS", "Qwen 3.8 27b FP4", 7);
        let branch = branch_name("Qwen 3.8 27b FP4", 7);

        let created = add(&repo, &path, &branch).unwrap();
        // Git answers with the resolved path, which may differ in spelling.
        assert!(
            same_path(&created.path, &path),
            "{:?} vs {:?}",
            created.path,
            path
        );
        assert_eq!(created.branch.as_deref(), Some(branch.as_str()));
        assert!(
            path.join("README.md").exists(),
            "the checkout should be populated"
        );

        let listed = list(&repo).unwrap();
        assert_eq!(listed.len(), 2, "the main worktree and the agent's");
    }

    #[test]
    fn adding_the_same_worktree_twice_returns_the_existing_one() {
        let repo = repo();
        let base = temp_dir("base");
        let path = worktree_path(&base, "P", "Agent", 1);
        let branch = branch_name("Agent", 1);

        let first = add(&repo, &path, &branch).unwrap();
        // A restart, or a profile recreated with the same id, must not fail.
        let second = add(&repo, &path, &branch).unwrap();
        assert_eq!(first, second);
        assert_eq!(list(&repo).unwrap().len(), 2);
    }

    #[test]
    fn re_creating_a_worktree_keeps_the_commits_on_its_branch() {
        let repo = repo();
        let base = temp_dir("base");
        let path = worktree_path(&base, "P", "Agent", 5);
        let branch = branch_name("Agent", 5);

        // An agent does some work and commits it.
        add(&repo, &path, &branch).unwrap();
        std::fs::write(path.join("work.txt"), "finished\n").unwrap();
        git(&path, &["add", "."]).unwrap();
        git(&path, &["commit", "-m", "the agent's work"]).unwrap();
        let commit = git(&path, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_string();

        // Its worktree is discarded — allowed without force, the tree is clean.
        assert!(remove(&repo, &path, false).unwrap());

        // Starting it again must give the work back, not reset the branch.
        let again = add(&repo, &path, &branch).unwrap();
        assert_eq!(again.branch.as_deref(), Some(branch.as_str()));
        assert!(
            again.path.join("work.txt").exists(),
            "the committed file is back"
        );
        assert_eq!(
            git(&again.path, &["rev-parse", "HEAD"]).unwrap().trim(),
            commit,
            "the branch must still point at the agent's commit"
        );
    }

    #[test]
    fn a_branch_checked_out_elsewhere_is_refused_rather_than_duplicated() {
        let repo = repo();
        let base = temp_dir("base");
        let first = worktree_path(&base, "P", "Agent", 6);
        let branch = branch_name("Agent", 6);
        add(&repo, &first, &branch).unwrap();

        // Two worktrees on one branch is how they diverge; git says no.
        let second = worktree_path(&base, "P", "Other", 7);
        assert!(add(&repo, &second, &branch).is_err());
    }

    #[test]
    fn a_directory_that_is_not_a_worktree_is_refused_rather_than_taken_over() {
        let repo = repo();
        let base = temp_dir("base");
        let path = worktree_path(&base, "P", "Agent", 1);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("something.txt"), "the user's file").unwrap();

        let err = add(&repo, &path, "axiomata/agent-1").unwrap_err();
        assert!(err.to_string().contains("already exists"), "{err}");
        assert!(
            path.join("something.txt").exists(),
            "must not have touched it"
        );
    }

    #[test]
    fn removing_a_worktree_takes_its_directory_with_it() {
        let repo = repo();
        let base = temp_dir("base");
        let path = worktree_path(&base, "P", "Agent", 2);
        add(&repo, &path, &branch_name("Agent", 2)).unwrap();

        assert!(remove(&repo, &path, false).unwrap());
        assert!(!path.exists());
        assert_eq!(list(&repo).unwrap().len(), 1);
        // Removing again is not an error — it is already gone.
        assert!(!remove(&repo, &path, false).unwrap());
    }

    #[test]
    fn a_worktree_with_changes_says_so_and_needs_force() {
        let repo = repo();
        let base = temp_dir("base");
        let path = worktree_path(&base, "P", "Agent", 3);
        add(&repo, &path, &branch_name("Agent", 3)).unwrap();

        assert!(!has_uncommitted_changes(&path).unwrap());
        std::fs::write(path.join("work.txt"), "in progress").unwrap();
        assert!(has_uncommitted_changes(&path).unwrap());

        assert!(
            remove(&repo, &path, false).is_err(),
            "unforced removal should refuse"
        );
        assert!(remove(&repo, &path, true).unwrap());
    }

    #[test]
    fn a_failure_carries_the_command_and_gits_own_words() {
        let plain = temp_dir("plain");
        let err = list(&plain).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("git worktree list"),
            "should name the command: {message}"
        );
        assert!(
            message.contains("repository") || message.contains("repo"),
            "should carry git's own explanation: {message}"
        );
    }

    #[test]
    fn slugs_survive_the_names_people_actually_use() {
        assert_eq!(slugify("Qwen 3.8 27b FP4"), "qwen-3-8-27b-fp4");
        assert_eq!(slugify("DeepSeek V4 Flash"), "deepseek-v4-flash");
        assert_eq!(slugify("  spaced  out  "), "spaced-out");
        assert_eq!(slugify("Ümläut"), "ml-ut");
        assert_eq!(slugify("///"), "");
    }

    #[test]
    fn two_agents_whose_names_slug_alike_still_get_their_own_directory() {
        let base = Path::new("/base");
        // Without the id these would be one directory, i.e. two agents
        // overwriting each other's work.
        assert_ne!(
            worktree_path(base, "P", "A B", 1),
            worktree_path(base, "P", "A-B", 2)
        );
    }
}
