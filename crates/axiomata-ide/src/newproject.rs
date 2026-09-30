//! Making the folder of a **new** project (editor plan, "Projekt neu").
//!
//! Kept apart from [`crate::store`] on purpose: the store promises that it
//! looks at the file system and never changes it, which is what makes
//! removing a project safe. Creating a folder is the one thing here that
//! does change it, so it lives in a module that says so.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{IdeError, Result};

const MAX_FOLDER_NAME: usize = 100;

/// A folder name is one path component: no separators, not `.`/`..`, no NUL,
/// not hidden (a project that vanishes from the Finder is a surprise).
/// Whether `name` would do as a new project's folder — exposed so the caller
/// can refuse before it asks the user for a parent folder.
pub fn check_folder_name(name: &str) -> Result<()> {
    folder_name(name).map(|_| ())
}

fn folder_name(name: &str) -> Result<&str> {
    let name = name.trim();
    let bad = |reason: &str| IdeError::Invalid {
        field: "name",
        reason: reason.to_string(),
    };
    if name.is_empty() {
        return Err(bad("must not be empty"));
    }
    if name.chars().count() > MAX_FOLDER_NAME {
        return Err(bad("is too long for a folder name"));
    }
    if name.starts_with('.') {
        return Err(bad("must not start with a dot"));
    }
    if name.contains(['/', '\\', '\0', ':']) {
        return Err(bad("must not contain / \\ : or control characters"));
    }
    Ok(name)
}

/// Creates `parent/name` (which must not exist yet) and, if asked,
/// `git init`s it. Returns the new folder.
///
/// If `git init` fails the folder is removed again — it is ours, nothing else
/// is in it yet — so a failed "new project" leaves nothing behind.
pub fn create_project_folder(parent: &Path, name: &str, git_init: bool) -> Result<PathBuf> {
    let name = folder_name(name)?;
    let parent = parent.canonicalize().map_err(|source| IdeError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    if !parent.is_dir() {
        return Err(IdeError::Invalid {
            field: "parent",
            reason: "is not a directory".to_string(),
        });
    }
    let folder = parent.join(name);
    // `create_dir` (not `_all`) fails when the name is taken: never adopt a
    // folder that already exists under the guise of making a new one.
    std::fs::create_dir(&folder).map_err(|source| IdeError::Io {
        path: folder.clone(),
        source,
    })?;
    if git_init {
        let done = Command::new("git")
            .arg("init")
            .current_dir(&folder)
            .output()
            .map_err(|err| err.to_string())
            .and_then(|out| {
                if out.status.success() {
                    Ok(())
                } else {
                    Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
                }
            });
        if let Err(reason) = done {
            let _ = std::fs::remove_dir_all(&folder);
            return Err(IdeError::Git {
                command: "git init".to_string(),
                reason,
            });
        }
    }
    Ok(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-ide-newproject-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    #[test]
    fn makes_the_folder_and_a_git_repository_when_asked() {
        let parent = scratch("git");
        let folder = create_project_folder(&parent, " demo ", true).unwrap();
        assert_eq!(folder, parent.join("demo"));
        assert!(folder.join(".git").is_dir());
        let plain = create_project_folder(&parent, "plain", false).unwrap();
        assert!(plain.is_dir() && !plain.join(".git").exists());
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn refuses_a_taken_name_and_names_that_are_not_one_folder() {
        let parent = scratch("names");
        create_project_folder(&parent, "taken", false).unwrap();
        assert!(create_project_folder(&parent, "taken", false).is_err());
        for bad in ["", "  ", ".hidden", "a/b", "..", "x\\y", "a:b", "nul\0"] {
            assert!(
                create_project_folder(&parent, bad, false).is_err(),
                "{bad:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&parent);
    }
}
