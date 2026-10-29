//! The net under A3c (`docs/plans/maintenance.md`): worktrees the studio made for agents that no longer exist.
//!
//! Deleting an agent removes its worktree since MT2; this finds the ones left from before, or from a delete whose
//! project folder was gone. A clean one goes with a clean-up, through git (`git worktree remove`, never forced). One
//! with changes only waits in Settings → Storage for the owner's click. Only `<worktrees>/<project>/<agent>-<id>`
//! folders are looked at: a plan's integration line (`<project>/.lines/<id>`) and anything else stays.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::fsops::{self, Entry};
use super::{Agents, Item};
use crate::ide::worktree;

/// A worktree whose agent is gone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanWorktree {
    /// Where it is (`~/Worktrees/axiomata-agents/<project>/<agent>-<id>`).
    pub path: PathBuf,
    /// Its size on disk.
    pub bytes: u64,
    /// `Some(true)`: uncommitted changes, it waits for the owner. `Some(false)`: clean, a clean-up removes it. `None`:
    /// git cannot say (its repository is gone or unreadable) — only listed, like one with changes.
    pub dirty: Option<bool>,
    /// The repository it belongs to, read from its `.git` file.
    pub repo_root: Option<PathBuf>,
    /// The clean-up removed it.
    pub removed: bool,
}

/// One orphan as found, with what is needed to remove it.
struct Found {
    orphan: OrphanWorktree,
    /// The project folder it was listed from, and its entry there.
    parent: PathBuf,
    entry: Entry,
}

/// The repository a worktree belongs to, from its `.git` file (`gitdir: <repo>/.git/worktrees/<name>`), read without
/// running anything inside the worktree — and only when that repository is one of the studio's projects: a `.git` file
/// is the agent's to write, so a repository it names is trusted only if the owner added it.
fn repo_of(worktree: &Path, projects: &HashSet<PathBuf>) -> Option<PathBuf> {
    let admin = worktree::admin_dir(worktree)?;
    let repo = admin.parent()?.parent()?.parent()?;
    let canonical = repo.canonicalize().ok()?;
    projects.contains(&canonical).then_some(canonical)
}

/// The agent id in an agent worktree's folder name (`<slug>-<id>`).
fn agent_id(name: &str) -> Option<i64> {
    let (_, id) = name.rsplit_once('-')?;
    id.parse().ok().filter(|id| *id > 0)
}

/// Every orphan under `root`, judged by `agents`.
/// `measure`: walk each orphan for its size — a report wants it, a single delete does not.
fn find(root: &Path, agents: &Agents, measure: bool) -> Vec<Found> {
    let recorded: HashSet<PathBuf> = agents
        .worktrees
        .iter()
        .map(|path| path.canonicalize().unwrap_or_else(|_| path.clone()))
        .collect();
    let projects: HashSet<PathBuf> = agents
        .projects
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect();
    let mut found = Vec::new();
    let Ok(folders) = fsops::entries(root) else {
        return found;
    };
    for project in folders
        .into_iter()
        .filter(|p| p.is_dir() && !p.name.starts_with('.'))
    {
        let Ok(checkouts) = fsops::entries(&project.path) else {
            continue;
        };
        for entry in checkouts.into_iter().filter(|e| e.is_dir()) {
            let Some(id) = agent_id(&entry.name) else {
                continue;
            };
            // An id above what the database had handed out belongs to an agent being created right now.
            if id > agents.issued || agents.ids.contains(&id) {
                continue;
            }
            let canonical = entry
                .path
                .canonicalize()
                .unwrap_or_else(|_| entry.path.clone());
            if recorded.contains(&canonical) {
                continue;
            }
            let repo_root = repo_of(&entry.path, &projects);
            // Git runs in the worktree only when that cannot make it start a program of the agent's choosing;
            // otherwise the state is unknown and the worktree waits for the owner, like one with changes.
            let dirty = repo_root
                .as_ref()
                .filter(|repo| worktree::safe_to_inspect(repo, &entry.path))
                .and_then(|_| worktree::has_uncommitted_changes(&entry.path).ok());
            found.push(Found {
                orphan: OrphanWorktree {
                    path: entry.path.clone(),
                    bytes: if measure {
                        fsops::tree_size(&entry.path).0
                    } else {
                        0
                    },
                    dirty,
                    repo_root,
                    removed: false,
                },
                parent: project.path.clone(),
                entry,
            });
        }
    }
    found
}

/// Measures the orphans under `root` and, unless `dry_run`, removes the clean ones through git.
pub(super) fn sweep(root: &Path, agents: &Agents, dry_run: bool) -> (Item, Vec<OrphanWorktree>) {
    let mut item = Item::new(
        "worktrees",
        "Orphaned worktrees",
        "worktrees",
        "removed with their agent; one with changes waits for you".to_owned(),
    );
    item.bytes = Some(0);
    item.freeable_bytes = Some(0);
    let mut orphans = Vec::new();
    for Found { mut orphan, .. } in find(root, agents, true) {
        item.count += 1;
        *item.bytes.get_or_insert(0) += orphan.bytes;
        if orphan.dirty == Some(false)
            && let Some(repo) = orphan.repo_root.clone()
        {
            if !dry_run {
                match worktree::remove(&repo, &orphan.path, false) {
                    Ok(true) => orphan.removed = true,
                    Ok(false) => {
                        item.note_error(format!(
                            "{} is not a worktree of {}",
                            orphan.path.display(),
                            repo.display()
                        ));
                    }
                    Err(err) => {
                        tracing::warn!(%err, path = %orphan.path.display(), "maintenance could not remove a worktree");
                        item.note_error(format!("{}: {err}", orphan.path.display()));
                    }
                }
            }
            if dry_run || orphan.removed {
                item.freeable_count += 1;
                *item.freeable_bytes.get_or_insert(0) += orphan.bytes;
            }
        }
        orphans.push(orphan);
    }
    (item, orphans)
}

/// Removes the orphan at `path` the owner chose in Settings → Storage, changes and all.
///
/// `path` must be one the sweep finds right now — a path the webview made up, or the worktree of an agent that exists,
/// is refused. With a known repository it goes through `git worktree remove --force`; one whose repository is gone is
/// deleted as a folder, which [`fsops::remove_entry`] allows only directly inside its project folder and never
/// through a link.
pub(super) fn remove_one(root: &Path, agents: &Agents, path: &Path) -> Result<(), String> {
    let Some(found) = find(root, agents, false)
        .into_iter()
        .find(|found| found.orphan.path == path)
    else {
        return Err(format!("{} is not an orphaned worktree", path.display()));
    };
    match &found.orphan.repo_root {
        Some(repo) if repo.is_dir() => match worktree::remove(repo, path, true) {
            Ok(true) => Ok(()),
            Ok(false) => fsops::remove_entry(root, &found.parent, &found.entry)
                .map_err(|err| err.to_string()),
            Err(err) => Err(err.to_string()),
        },
        _ => fsops::remove_entry(root, &found.parent, &found.entry).map_err(|err| err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_agent_folder_names_its_agent_and_a_line_does_not() {
        assert_eq!(agent_id("builder-12"), Some(12));
        assert_eq!(agent_id("qwen-3-8-27b-fp4-1"), Some(1));
        assert_eq!(agent_id("lines"), None);
        assert_eq!(agent_id("x-0"), None);
    }
}
