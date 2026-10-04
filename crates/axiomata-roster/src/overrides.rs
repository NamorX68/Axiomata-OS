//! A project's own roles: `<project>/.axiomata/agents/<name>/AGENT.md`.
//!
//! A cloned repository can bring role files, and instructions are text an agent obeys, so they apply only after
//! the owner looked at them and said yes — for **exactly that content**: the confirmation stores the SHA-256 of
//! everything that was read, and any change asks again. The confirmation reuses `axiomata-tasks`' store (a file
//! under `~/.axiomata`, written from Rust only, keyed by the overrides directory so it cannot collide with the
//! project's `tasks.json`).
//!
//! What a project role *cannot* do is start anything: it names engines by id, and engines are the owner's.

use std::path::{Path, PathBuf};

use axiomata_tasks::trust::{self, TrustStore};
use serde::Serialize;

use crate::error::{Result, RosterError};
use crate::role::{Role, Source};
use crate::store::{Skipped, scan};

/// What a project brings, and whether the owner has confirmed it.
#[derive(Debug, Clone, Serialize)]
pub struct Overrides {
    /// `<project>/.axiomata/agents` — also the key of the confirmation.
    pub dir: PathBuf,
    /// Whether the project has any role files at all. Without them there is nothing to confirm.
    pub present: bool,
    /// SHA-256 over every file read (name and bytes), lower-case hex; empty when not [`present`](Self::present).
    pub hash: String,
    /// Roles that parsed, stamped [`Source::Project`]. Shown to the owner for the confirmation whether or not
    /// they apply yet.
    pub roles: Vec<Role>,
    pub skipped: Vec<Skipped>,
    /// Why these files can never be confirmed (a symlinked directory, more files than are read), else `None`.
    /// Nothing safe could be shown as complete, so no confirmation is accepted.
    pub blocked: Option<String>,
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// Reads the project's role files. Never follows a symlinked `.axiomata` or `agents` directory.
///
/// # Errors
///
/// [`RosterError::Io`] for a failure listing the directory.
pub fn scan_project(project: &Path) -> Result<Overrides> {
    let dir = project.join(".axiomata").join("agents");
    let mut result = Overrides {
        dir: dir.clone(),
        present: false,
        hash: String::new(),
        roles: Vec::new(),
        skipped: Vec::new(),
        blocked: None,
    };
    if is_symlink(&project.join(".axiomata")) || is_symlink(&dir) {
        result.present = true;
        result.blocked = Some("a symlinked .axiomata or agents directory is not followed".into());
        return Ok(result);
    }
    let scanned = scan(&dir, Source::Project)?;
    let mut hashed = Vec::new();
    for entry in scanned.entries {
        result.present = true;
        // Fixed-width digests of the name and of the content, so no choice of names and bytes can make two
        // different sets of files produce the same input ("a" + "bc" against "ab" + "c", or a name that contains
        // the separator). A file that could not be read is told apart from an empty one.
        hashed.push(if entry.bytes.is_some() { b'R' } else { b'U' });
        hashed.extend_from_slice(trust::hash(entry.name.as_bytes()).as_bytes());
        hashed.extend_from_slice(trust::hash(entry.bytes.as_deref().unwrap_or(&[])).as_bytes());
        match entry.outcome {
            Ok(role) => result.roles.push(role),
            Err(reason) => result.skipped.push(Skipped {
                name: entry.name,
                reason,
            }),
        }
    }
    if scanned.truncated {
        result.blocked = Some(
            "the project has more role files than are read, so what is shown is not all of it"
                .into(),
        );
    }
    if result.present {
        result.hash = trust::hash(&hashed);
    }
    Ok(result)
}

/// Whether the owner confirmed exactly what the project brings now.
pub fn is_confirmed(store: &TrustStore, overrides: &Overrides) -> bool {
    overrides.present
        && overrides.blocked.is_none()
        && !overrides.hash.is_empty()
        && store.is_trusted(&overrides.dir, &overrides.hash)
}

/// Records the owner's confirmation of what was shown (`shown_hash`), if the files are still the same.
///
/// # Errors
///
/// [`RosterError::Changed`] if the files differ from what was shown or there is nothing to confirm;
/// [`RosterError::Io`] if the confirmation could not be written.
pub fn confirm(store: &mut TrustStore, project: &Path, shown_hash: &str) -> Result<()> {
    let now = scan_project(project)?;
    if !now.present || now.blocked.is_some() || now.hash != shown_hash {
        return Err(RosterError::Changed);
    }
    store
        .trust(&now.dir, &now.hash)
        .map_err(|source| RosterError::Io {
            path: now.dir.clone(),
            source,
        })
}

/// The roles that apply: the owner's, with a confirmed project's replacing same-named ones and adding the rest.
/// Sorted by name. An unconfirmed project changes nothing.
pub fn effective(user: Vec<Role>, overrides: &Overrides, confirmed: bool) -> Vec<Role> {
    let mut roles = user;
    if confirmed {
        for project_role in &overrides.roles {
            match roles.iter_mut().find(|r| r.name == project_role.name) {
                Some(slot) => *slot = project_role.clone(),
                None => roles.push(project_role.clone()),
            }
        }
    }
    roles.sort_by(|a, b| a.name.cmp(&b.name));
    roles
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::store::{ROLE_FILE, default_role, save_role, tests::Tmp};

    fn project_with(tmp: &Tmp, description: &str) -> PathBuf {
        let mut role = default_role();
        role.description = description.into();
        save_role(&tmp.0.join(".axiomata").join("agents"), &role).unwrap();
        tmp.0.clone()
    }

    #[test]
    fn a_project_without_role_files_has_nothing_to_confirm() {
        let tmp = Tmp::new();
        let found = scan_project(&tmp.0).unwrap();
        assert!(!found.present && found.hash.is_empty() && found.roles.is_empty());
        assert!(!is_confirmed(&TrustStore::in_memory(), &found));
        assert!(matches!(
            confirm(&mut TrustStore::in_memory(), &tmp.0, ""),
            Err(RosterError::Changed)
        ));
    }

    #[test]
    fn roles_apply_only_after_the_exact_content_was_confirmed() {
        let tmp = Tmp::new();
        let project = project_with(&tmp, "from the repo");
        let mut store = TrustStore::in_memory();

        let shown = scan_project(&project).unwrap();
        assert!(shown.present && shown.hash.len() == 64);
        assert!(!is_confirmed(&store, &shown));

        let mine = vec![default_role()];
        assert_eq!(
            effective(mine.clone(), &shown, false),
            mine,
            "unconfirmed changes nothing"
        );

        confirm(&mut store, &project, &shown.hash).unwrap();
        assert!(is_confirmed(&store, &scan_project(&project).unwrap()));
        let applied = effective(mine, &shown, true);
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].description, "from the repo");
        assert_eq!(applied[0].source, Source::Project);

        // Any change to the files asks again.
        let file = project.join(".axiomata/agents/allrounder").join(ROLE_FILE);
        fs::write(
            &file,
            fs::read_to_string(&file).unwrap() + "\nOne more instruction.\n",
        )
        .unwrap();
        assert!(!is_confirmed(&store, &scan_project(&project).unwrap()));
    }

    #[test]
    fn confirming_something_other_than_what_was_shown_is_refused() {
        let tmp = Tmp::new();
        let project = project_with(&tmp, "first");
        let shown = scan_project(&project).unwrap();
        project_with(&tmp, "swapped after the owner looked");
        let mut store = TrustStore::in_memory();
        assert!(matches!(
            confirm(&mut store, &project, &shown.hash),
            Err(RosterError::Changed)
        ));
        assert!(!is_confirmed(&store, &scan_project(&project).unwrap()));
    }

    #[test]
    fn project_roles_add_to_and_replace_the_owners() {
        let tmp = Tmp::new();
        let project = project_with(&tmp, "replaced");
        let mut extra = default_role();
        extra.name = "extra".into();
        save_role(&project.join(".axiomata/agents"), &extra).unwrap();
        let found = scan_project(&project).unwrap();

        let mut zed = default_role();
        zed.name = "zed".into();
        let merged = effective(vec![default_role(), zed], &found, true);
        let names: Vec<_> = merged.iter().map(|r| (r.name.as_str(), r.source)).collect();
        assert_eq!(
            names,
            [
                ("allrounder", Source::Project),
                ("extra", Source::Project),
                ("zed", Source::User)
            ]
        );
    }

    #[test]
    fn the_hash_separates_names_from_contents() {
        let tmp_a = Tmp::new();
        let tmp_b = Tmp::new();
        let agents_a = tmp_a.0.join(".axiomata/agents");
        let agents_b = tmp_b.0.join(".axiomata/agents");
        for (agents, name) in [(&agents_a, "aa"), (&agents_b, "a")] {
            fs::create_dir_all(agents.join(name)).unwrap();
            fs::write(agents.join(name).join(ROLE_FILE), "same bytes").unwrap();
        }
        assert_ne!(
            scan_project(&tmp_a.0).unwrap().hash,
            scan_project(&tmp_b.0).unwrap().hash
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_agents_directory_is_never_followed_or_confirmable() {
        let tmp = Tmp::new();
        let outside = Tmp::new();
        save_role(&outside.0, &default_role()).unwrap();
        fs::create_dir_all(tmp.0.join(".axiomata")).unwrap();
        std::os::unix::fs::symlink(&outside.0, tmp.0.join(".axiomata/agents")).unwrap();
        let found = scan_project(&tmp.0).unwrap();
        assert!(found.roles.is_empty() && found.hash.is_empty());
        let mut store = TrustStore::in_memory();
        assert!(!is_confirmed(&store, &found));
        assert!(matches!(
            confirm(&mut store, &tmp.0, ""),
            Err(RosterError::Changed)
        ));
    }

    /// The collision the audit constructed: with a space-separated `name len bytes` framing, a directory `a` holding
    /// `q 1 Z` and a directory `a 5 q` holding `Z` produced the same input.
    #[test]
    fn crafted_names_cannot_make_two_file_sets_hash_alike() {
        let first = Tmp::new();
        let second = Tmp::new();
        for (tmp, name, content) in [(&first, "a", "q 1 Z"), (&second, "a 5 q", "Z")] {
            let dir = tmp.0.join(".axiomata/agents").join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(ROLE_FILE), content).unwrap();
        }
        assert_ne!(
            scan_project(&first.0).unwrap().hash,
            scan_project(&second.0).unwrap().hash
        );
    }

    #[test]
    fn a_project_with_too_many_role_files_is_read_in_part_and_can_never_be_confirmed() {
        let tmp = Tmp::new();
        let agents = tmp.0.join(".axiomata/agents");
        for n in 0..70 {
            let dir = agents.join(format!("r{n:03}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(ROLE_FILE), "not a role").unwrap();
        }
        let found = scan_project(&tmp.0).unwrap();
        assert!(found.blocked.is_some());
        assert_eq!(
            found.skipped.len(),
            64,
            "only the first 64 directories are read, in name order"
        );
        let mut store = TrustStore::in_memory();
        assert!(matches!(
            confirm(&mut store, &tmp.0, &found.hash),
            Err(RosterError::Changed)
        ));
        assert!(!is_confirmed(&store, &found));
    }

    #[test]
    fn a_blocked_project_is_not_confirmable_even_with_the_right_hash() {
        let tmp = Tmp::new();
        let outside = Tmp::new();
        save_role(&outside.0, &default_role()).unwrap();
        fs::create_dir_all(tmp.0.join(".axiomata")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside.0, tmp.0.join(".axiomata/agents")).unwrap();
        let found = scan_project(&tmp.0).unwrap();
        #[cfg(unix)]
        assert!(found.blocked.is_some() && found.present);
        let mut store = TrustStore::in_memory();
        assert!(matches!(
            confirm(&mut store, &tmp.0, &found.hash),
            Err(RosterError::Changed)
        ));
    }
}
