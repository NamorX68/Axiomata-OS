//! The file-system side of the maintenance: measuring and deleting, never following a link.
//!
//! Every deletion here goes through [`remove_entry`], which only removes something that lies **directly** in the
//! folder it was listed from and is not a link — the same care as for the background image (`docs/plans/glass.md`).
//! Folders are reached through [`real_dir`], so a link planted in place of `logs/` or `claude/projects/` cannot steer a
//! clean-up to somewhere outside `~/.axiomata`.

use std::fs::{self, Metadata};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

/// One thing directly inside a folder, as `lstat` saw it.
#[derive(Debug)]
pub(super) struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub meta: Metadata,
}

impl Entry {
    /// A plain file (never a link).
    pub fn is_file(&self) -> bool {
        self.meta.file_type().is_file()
    }

    /// A real folder (never a link to one).
    pub fn is_dir(&self) -> bool {
        self.meta.file_type().is_dir()
    }

    /// When it was last written; the epoch when the file system cannot say, so an unknown age never reads as fresh.
    pub fn modified(&self) -> SystemTime {
        self.meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)
    }
}

/// `base/<parts…>` if every step below `base` is a real folder, not a link; `None` when one is missing or a link.
///
/// `base` itself is taken as it is: `~/.axiomata` (or `$AXIOMATA_HOME`) is the owner's choice of place.
pub(super) fn real_dir(base: &Path, parts: &[&str]) -> Option<PathBuf> {
    let mut dir = base.to_path_buf();
    if !fs::metadata(&dir).is_ok_and(|meta| meta.is_dir()) {
        return None;
    }
    for part in parts {
        dir.push(part);
        if !fs::symlink_metadata(&dir).is_ok_and(|meta| meta.file_type().is_dir()) {
            return None;
        }
    }
    Some(dir)
}

/// What lies directly in `dir`, links left out. A name that is not UTF-8 is left out too: nothing this module cleans
/// is ever named that way, so such an entry is not Axiomata's.
pub(super) fn entries(dir: &Path) -> io::Result<Vec<Entry>> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let path = entry.path();
        // Gone between the listing and the look (a rotating log, a temporary file): simply not there.
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err),
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        found.push(Entry { path, name, meta });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// Whether every folder from `base` down to `dir` is a real folder right now, none of them a link.
fn chain_is_real(base: &Path, dir: &Path) -> bool {
    let Ok(rest) = dir.strip_prefix(base) else {
        return false;
    };
    let mut at = base.to_path_buf();
    for part in rest.components() {
        let Component::Normal(part) = part else {
            return false;
        };
        at.push(part);
        if !fs::symlink_metadata(&at).is_ok_and(|meta| meta.file_type().is_dir()) {
            return false;
        }
    }
    true
}

/// Removes `entry`, which [`entries`] listed from `dir`, a folder below `base` (`~/.axiomata`, or the agents'
/// worktree folder).
///
/// Checks again right before deleting: every folder from `base` down to `dir` must still be a real folder (a link
/// swapped in for one of them would point the delete elsewhere), and the entry must still lie directly in `dir`,
/// still be what it was (a file or a folder) and still not be a link. A folder goes with everything in it;
/// `remove_dir_all` does not follow links inside it either, it removes the link itself. What is left is the moment
/// between this check and the delete; closing that too would need a pinned directory descriptor per folder.
pub(super) fn remove_entry(base: &Path, dir: &Path, entry: &Entry) -> io::Result<()> {
    if !chain_is_real(base, dir) {
        return Err(io::Error::other(format!(
            "{} is not a real folder below {}",
            dir.display(),
            base.display()
        )));
    }
    let inside = entry.path.parent() == Some(dir)
        && !entry.name.is_empty()
        && entry.name != "."
        && entry.name != ".."
        && !entry.name.contains('/');
    if !inside {
        return Err(io::Error::other(format!(
            "{} does not lie directly in {}",
            entry.path.display(),
            dir.display()
        )));
    }
    let now = fs::symlink_metadata(&entry.path)?;
    let kind = now.file_type();
    if kind.is_symlink() || kind.is_dir() != entry.is_dir() || kind.is_file() != entry.is_file() {
        return Err(io::Error::other(format!(
            "{} changed while it was being cleaned up",
            entry.path.display()
        )));
    }
    if kind.is_dir() {
        fs::remove_dir_all(&entry.path)
    } else {
        fs::remove_file(&entry.path)
    }
}

/// The size of a file or folder in bytes and the newest modification inside it, links neither followed nor counted.
///
/// A part that cannot be read counts as nothing: a size is a hint for the owner, not a promise.
pub(super) fn tree_size(path: &Path) -> (u64, SystemTime) {
    let mut bytes = 0;
    let mut newest = SystemTime::UNIX_EPOCH;
    let mut stack = vec![path.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(meta) = fs::symlink_metadata(&next) else {
            continue;
        };
        if let Ok(modified) = meta.modified() {
            newest = newest.max(modified);
        }
        let kind = meta.file_type();
        if kind.is_file() {
            bytes += meta.len();
        } else if kind.is_dir()
            && let Ok(children) = fs::read_dir(&next)
        {
            stack.extend(children.flatten().map(|child| child.path()));
        }
    }
    (bytes, newest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;

    #[test]
    fn a_linked_folder_is_not_a_real_dir_and_a_link_is_never_listed() {
        let base = unique_temp_dir("axiomata-test-fsops-links");
        let outside = unique_temp_dir("axiomata-test-fsops-outside");
        fs::create_dir_all(base.join("real")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), "the owner's").unwrap();
        std::os::unix::fs::symlink(&outside, base.join("linked")).unwrap();
        std::os::unix::fs::symlink(outside.join("keep.txt"), base.join("real").join("link.txt"))
            .unwrap();
        fs::write(base.join("real").join("file.txt"), "x").unwrap();

        assert!(real_dir(&base, &["real"]).is_some());
        assert!(real_dir(&base, &["linked"]).is_none());
        assert!(real_dir(&base, &["missing"]).is_none());

        let listed = entries(&base.join("real")).unwrap();
        assert_eq!(listed.len(), 1, "the link is left out");
        assert_eq!(listed[0].name, "file.txt");

        let _ = fs::remove_dir_all(&base);
        assert!(outside.join("keep.txt").exists());
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn remove_entry_refuses_an_entry_that_turned_into_a_link() {
        let base = unique_temp_dir("axiomata-test-fsops-swap");
        let outside = unique_temp_dir("axiomata-test-fsops-swap-outside");
        fs::create_dir_all(base.join("old")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("keep.txt"), "the owner's").unwrap();
        let listed = entries(&base).unwrap();
        assert_eq!(listed.len(), 1);

        // Swapped for a link between listing and deleting.
        fs::remove_dir(base.join("old")).unwrap();
        std::os::unix::fs::symlink(&outside, base.join("old")).unwrap();
        assert!(remove_entry(&base, &base, &listed[0]).is_err());
        assert!(outside.join("keep.txt").exists());

        // And an entry of another folder is refused outright.
        let elsewhere = entries(&outside).unwrap();
        assert!(remove_entry(&base, &base, &elsewhere[0]).is_err());
        assert!(outside.join("keep.txt").exists());

        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn tree_size_counts_files_but_not_what_a_link_points_at() {
        let base = unique_temp_dir("axiomata-test-fsops-size");
        let outside = unique_temp_dir("axiomata-test-fsops-size-outside");
        fs::create_dir_all(base.join("a/b")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(base.join("a/one"), [0u8; 10]).unwrap();
        fs::write(base.join("a/b/two"), [0u8; 5]).unwrap();
        fs::write(outside.join("big"), [0u8; 1000]).unwrap();
        std::os::unix::fs::symlink(&outside, base.join("a/link")).unwrap();
        assert_eq!(tree_size(&base).0, 15);
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn remove_entry_refuses_when_a_folder_above_turned_into_a_link() {
        let base = unique_temp_dir("axiomata-test-fsops-chain");
        let outside = unique_temp_dir("axiomata-test-fsops-chain-outside");
        fs::create_dir_all(base.join("logs")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(base.join("logs").join("old"), "x").unwrap();
        fs::write(outside.join("old"), "the owner's").unwrap();
        let dir = base.join("logs");
        let listed = entries(&dir).unwrap();

        fs::remove_dir_all(&dir).unwrap();
        std::os::unix::fs::symlink(&outside, &dir).unwrap();
        assert!(remove_entry(&base, &dir, &listed[0]).is_err());
        assert!(outside.join("old").exists());
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&outside);
    }
}
