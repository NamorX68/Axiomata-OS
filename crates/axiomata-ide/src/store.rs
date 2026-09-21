//! Every SQL statement the IDE core issues.
//!
//! Same two conventions as `axiomata_board::store`, so the two read alike:
//!
//! * free functions taking `db: &Connection` — the store owns no connection,
//!   locking is the caller's business;
//! * `create` returns the stored struct, `update` returns `Option<T>` (`None`
//!   = no such id), `delete` returns a `bool` found-flag. "Missing" is never
//!   an error.
//!
//! Unlike the board, this store does touch the file system — but only to
//! *look*: it canonicalises a caller-supplied path on the way in and asks
//! whether it still exists on the way out. It never creates, moves or removes
//! anything. Deleting a project removes a row, never a folder; that is a rule
//! about what this crate is allowed to do, not an omission.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{NewProject, Project};
use crate::{IdeError, Result};

/// Counted in characters, not bytes — deliberately unlike the board's
/// byte-based `check_len`. This cap exists to keep a name printable in a list,
/// and "how much of the list does it eat" is a question about characters; a
/// byte cap would cut an umlaut-heavy name at half the visible length.
const MAX_NAME_LEN: usize = 200;

/// Ceiling on a stored dock layout.
///
/// A pane's module writes its own config straight through into the layout
/// (`ctx.config`, see `core/registry.ts`), so a module with a bug — or one
/// that decides to cache something large — could grow this without anybody
/// deciding to. A megabyte is far beyond any honest dock tree and far below
/// anything that would hurt; the point is that there *is* a ceiling.
const MAX_LAYOUT_LEN: usize = 1024 * 1024;

const PROJECT_COLS: &str = "id, name, repo_root, layout_json, created_at, last_opened_at";

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn parse_ts(raw: &str, id: i64, field: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| IdeError::CorruptRow {
            table: "projects",
            id,
            reason: format!("{field} is not RFC 3339: {err}"),
        })
}

fn parse_opt_ts(raw: Option<String>, id: i64, field: &str) -> Result<Option<DateTime<Utc>>> {
    raw.map(|value| parse_ts(&value, id, field)).transpose()
}

/// A project row, still with its timestamps as text.
///
/// Two-stage row mapping, as in the board: `rusqlite`'s closure cannot return
/// our own error type, so it collects the raw columns and the fallible parsing
/// happens outside it, where a bad row can name itself.
struct RawProject {
    id: i64,
    name: String,
    repo_root: String,
    layout_json: Option<String>,
    created_at: String,
    last_opened_at: Option<String>,
}

impl RawProject {
    fn into_project(self) -> Result<Project> {
        let created_at = parse_ts(&self.created_at, self.id, "created_at")?;
        let last_opened_at = parse_opt_ts(self.last_opened_at, self.id, "last_opened_at")?;
        let repo_root = PathBuf::from(self.repo_root);
        Ok(Project {
            id: self.id,
            name: self.name,
            root_exists: repo_root.is_dir(),
            repo_root,
            layout_json: self.layout_json,
            created_at,
            last_opened_at,
        })
    }
}

fn row_to_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawProject> {
    Ok(RawProject {
        id: row.get(0)?,
        name: row.get(1)?,
        repo_root: row.get(2)?,
        layout_json: row.get(3)?,
        created_at: row.get(4)?,
        last_opened_at: row.get(5)?,
    })
}

/* --------------------------------------------------------------- input --- */

fn normalize_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(IdeError::Invalid {
            field: "name",
            reason: "must not be empty".to_string(),
        });
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(IdeError::Invalid {
            field: "name",
            reason: format!("longer than {MAX_NAME_LEN} characters"),
        });
    }
    Ok(trimmed.to_string())
}

/// Turns a caller-supplied folder into the one spelling that gets stored.
///
/// `canonicalize` does three jobs at once here, which is why there is no
/// separate existence check: it resolves `.`/`..` and symlinks, it makes the
/// path absolute, and it **fails if the folder is not there** — so "does this
/// exist?" and "what is its real name?" are one question with one answer, and
/// no gap between them for the answer to change in.
///
/// Whether the folder is a *git repository* is deliberately not checked. That
/// belongs to M7.3, and a folder can become one after it is added here.
fn normalize_root(root: &Path) -> Result<PathBuf> {
    let resolved = root.canonicalize().map_err(|err| IdeError::Invalid {
        field: "repo_root",
        reason: format!("cannot be resolved: {err}"),
    })?;
    if !resolved.is_dir() {
        return Err(IdeError::Invalid {
            field: "repo_root",
            reason: "is not a directory".to_string(),
        });
    }
    Ok(resolved)
}

fn root_as_text(root: &Path) -> Result<String> {
    root.to_str()
        .map(str::to_string)
        .ok_or_else(|| IdeError::Invalid {
            field: "repo_root",
            reason: "is not valid UTF-8".to_string(),
        })
}

/// Turns SQLite's UNIQUE complaint into something a user can act on.
///
/// The constraint is the real guard — this only replaces "UNIQUE constraint
/// failed: projects.repo_root" with the name of the project already sitting on
/// that folder. Reading afterwards cannot reopen a race: the write was already
/// refused.
///
/// It insists on *that* constraint by name rather than accepting any
/// constraint violation. `repo_root` is the only one this table can fail on
/// today, so the narrow check costs nothing now — but the schema grows (M7.2
/// puts agents and worktrees beside this table, and `projects` itself may gain
/// a `CHECK`), and a future violation reported as "already used by …" would
/// send the user to fix the wrong thing entirely. Anything else stays the
/// database error it really was.
fn explain_root_clash(db: &Connection, root: &str, err: rusqlite::Error) -> IdeError {
    let is_root_clash = match &err {
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                ..
            },
            message,
        ) => message
            .as_deref()
            .is_some_and(|text| text.contains("projects.repo_root")),
        _ => false,
    };
    if !is_root_clash {
        return IdeError::Database(err);
    }
    let holder: Option<String> = db
        .query_row(
            "SELECT name FROM projects WHERE repo_root = ?1",
            params![root],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    IdeError::Invalid {
        field: "repo_root",
        reason: match holder {
            Some(name) => format!("already used by the project \"{name}\""),
            None => "already used by another project".to_string(),
        },
    }
}

/* ------------------------------------------------------------ reading --- */

/// Every project, most recently opened first.
///
/// Never-opened projects sort last rather than first: a brand new project is
/// the one you just created and are about to open anyway, whereas the top of
/// the list should answer "what was I working on?". Ties break by name so the
/// order is stable rather than whatever SQLite feels like.
pub fn list_projects(db: &Connection) -> Result<Vec<Project>> {
    let mut stmt = db.prepare(&format!(
        "SELECT {PROJECT_COLS} FROM projects \
         ORDER BY last_opened_at IS NULL, last_opened_at DESC, name COLLATE NOCASE, id"
    ))?;
    let raws = stmt
        .query_map([], row_to_raw)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raws.into_iter().map(RawProject::into_project).collect()
}

pub fn get_project(db: &Connection, id: i64) -> Result<Option<Project>> {
    let raw = db
        .query_row(
            &format!("SELECT {PROJECT_COLS} FROM projects WHERE id = ?1"),
            params![id],
            row_to_raw,
        )
        .optional()?;
    raw.map(RawProject::into_project).transpose()
}

/* ------------------------------------------------------------ writing --- */

pub fn create_project(db: &Connection, new: NewProject) -> Result<Project> {
    let name = normalize_name(&new.name)?;
    let root = normalize_root(&new.repo_root)?;
    let root_text = root_as_text(&root)?;

    db.execute(
        "INSERT INTO projects (name, repo_root, layout_json, created_at, last_opened_at) \
         VALUES (?1, ?2, NULL, ?3, NULL)",
        params![name, root_text, now()],
    )
    .map_err(|err| explain_root_clash(db, &root_text, err))?;

    let id = db.last_insert_rowid();
    get_project(db, id)?.ok_or_else(|| IdeError::CorruptRow {
        table: "projects",
        id,
        reason: "vanished immediately after being inserted".to_string(),
    })
}

pub fn rename_project(db: &Connection, id: i64, name: &str) -> Result<Option<Project>> {
    let name = normalize_name(name)?;
    let changed = db.execute(
        "UPDATE projects SET name = ?2 WHERE id = ?1",
        params![id, name],
    )?;
    if changed == 0 {
        return Ok(None);
    }
    get_project(db, id)
}

/// Repoints a project at a different folder, keeping its id and its layout.
///
/// This is what "Pfad ändern…" uses when a folder has moved or its disk was
/// not mounted. Deliberately not "delete and re-add": the layout, and from
/// M7.2 the agents, hang off the id.
pub fn set_repo_root(db: &Connection, id: i64, root: &Path) -> Result<Option<Project>> {
    let root = normalize_root(root)?;
    let root_text = root_as_text(&root)?;
    let changed = db
        .execute(
            "UPDATE projects SET repo_root = ?2 WHERE id = ?1",
            params![id, root_text],
        )
        .map_err(|err| explain_root_clash(db, &root_text, err))?;
    if changed == 0 {
        return Ok(None);
    }
    get_project(db, id)
}

/// Stores the frontend's dock tree verbatim. `None` clears it, which puts the
/// project back to "opens with the starting layout".
pub fn set_layout(db: &Connection, id: i64, layout: Option<&str>) -> Result<bool> {
    if layout.is_some_and(|text| text.len() > MAX_LAYOUT_LEN) {
        return Err(IdeError::Invalid {
            field: "layout_json",
            reason: format!("larger than {MAX_LAYOUT_LEN} bytes"),
        });
    }
    let changed = db.execute(
        "UPDATE projects SET layout_json = ?2 WHERE id = ?1",
        params![id, layout],
    )?;
    Ok(changed == 1)
}

/// Records that the project was just opened — what `list_projects` sorts by.
pub fn touch_opened(db: &Connection, id: i64) -> Result<bool> {
    let changed = db.execute(
        "UPDATE projects SET last_opened_at = ?2 WHERE id = ?1",
        params![id, now()],
    )?;
    Ok(changed == 1)
}

/// Removes the project **row**. Never the folder.
///
/// Worth stating in code because the opposite is a plausible reading of
/// "delete project": the folder is the user's, the row is ours. The UI calls
/// this "remove from the list" for the same reason. From M7.2 the app's *own*
/// data for the project (git worktrees under `~/.axiomata/worktrees/`) may be
/// cleaned up here too — but only after checking that no unmerged branch is
/// hanging off one.
pub fn delete_project(db: &Connection, id: i64) -> Result<bool> {
    let changed = db.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
    Ok(changed == 1)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A fresh in-memory database with the shipped schema applied.
    fn temp_db() -> Connection {
        let db = Connection::open_in_memory().expect("in-memory database");
        db.execute_batch(crate::SCHEMA_SQL_V1).expect("schema");
        db
    }

    /// A real directory to point a project at.
    ///
    /// Hand-rolled rather than via `tempfile`, matching `axiomata-board`'s own
    /// test helper and `axiomata_core::test_support` — the workspace has
    /// deliberately not taken that dependency.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "axiomata-ide-test-{}-{}-{}",
                label,
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).expect("temp dir");
            // Canonicalised here too: on macOS `/var` is a symlink to
            // `/private/var`, so the store's own canonicalisation would
            // otherwise make every comparison in these tests fail.
            TempDir(path.canonicalize().expect("canonical temp dir"))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn new_project(name: &str, dir: &TempDir) -> NewProject {
        NewProject {
            name: name.to_string(),
            repo_root: dir.path().to_path_buf(),
        }
    }

    #[test]
    fn a_created_project_comes_back_with_what_was_stored() {
        let db = temp_db();
        let dir = TempDir::new("create");

        let project = create_project(&db, new_project("Axiomata-OS", &dir)).expect("created");

        assert_eq!(project.name, "Axiomata-OS");
        assert_eq!(project.repo_root, dir.path());
        assert!(project.layout_json.is_none(), "never opened yet");
        assert!(project.last_opened_at.is_none());
        assert!(project.root_exists);
    }

    #[test]
    fn a_name_is_trimmed_and_an_empty_one_refused() {
        let db = temp_db();
        let dir = TempDir::new("name");

        let project = create_project(&db, new_project("  Mit Rand  ", &dir)).expect("created");
        assert_eq!(project.name, "Mit Rand");

        let other = TempDir::new("name-empty");
        let err = create_project(&db, new_project("   ", &other)).expect_err("refused");
        assert!(matches!(err, IdeError::Invalid { field: "name", .. }));
    }

    /// The whole point of `repo_root` being UNIQUE — and the message has to
    /// name the project already sitting there, or the user cannot act on it.
    #[test]
    fn two_projects_cannot_share_one_folder() {
        let db = temp_db();
        let dir = TempDir::new("clash");
        create_project(&db, new_project("Erstes", &dir)).expect("created");

        let err = create_project(&db, new_project("Zweites", &dir)).expect_err("refused");
        match err {
            IdeError::Invalid { field, reason } => {
                assert_eq!(field, "repo_root");
                assert!(reason.contains("Erstes"), "must name the holder: {reason}");
            }
            other => panic!("expected an Invalid, got {other:?}"),
        }
    }

    /// `~/x`, `./x` and `/Users/me/x` must not become three projects. The
    /// store canonicalises on the way in, so the UNIQUE constraint compares
    /// like with like.
    #[test]
    fn a_differently_spelled_path_is_still_the_same_folder() {
        let db = temp_db();
        let dir = TempDir::new("spelling");
        create_project(&db, new_project("Erstes", &dir)).expect("created");

        let roundabout = dir.path().join("..").join(
            dir.path()
                .file_name()
                .expect("the temp dir has a final component"),
        );
        let err = create_project(
            &db,
            NewProject {
                name: "Zweites".to_string(),
                repo_root: roundabout,
            },
        )
        .expect_err("refused");
        assert!(matches!(
            err,
            IdeError::Invalid {
                field: "repo_root",
                ..
            }
        ));
    }

    /// The narrowing in `explain_root_clash`: some *other* constraint failing
    /// must stay the database error it is, instead of being relabelled as a
    /// folder clash and sending the user to fix the wrong thing. Constructed
    /// by hand because `projects` has no second failable constraint today —
    /// which is exactly why this would otherwise rot unnoticed until M7.2
    /// adds one.
    #[test]
    fn a_constraint_that_is_not_the_folder_stays_a_database_error() {
        let db = temp_db();
        let not_ours = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                extended_code: 1299, // SQLITE_CONSTRAINT_NOTNULL
            },
            Some("NOT NULL constraint failed: projects.name".to_string()),
        );

        let mapped = explain_root_clash(&db, "/somewhere", not_ours);
        assert!(
            matches!(mapped, IdeError::Database(_)),
            "expected the original database error, got {mapped:?}"
        );
    }

    #[test]
    fn a_folder_that_is_not_there_is_refused() {
        let db = temp_db();
        let err = create_project(
            &db,
            NewProject {
                name: "Weg".to_string(),
                repo_root: PathBuf::from("/definitely/not/here/at/all"),
            },
        )
        .expect_err("refused");
        assert!(matches!(
            err,
            IdeError::Invalid {
                field: "repo_root",
                ..
            }
        ));
    }

    #[test]
    fn a_file_is_not_a_project_folder() {
        let db = temp_db();
        let dir = TempDir::new("file");
        let file = dir.path().join("not-a-folder.txt");
        std::fs::write(&file, "x").expect("write");

        let err = create_project(
            &db,
            NewProject {
                name: "Datei".to_string(),
                repo_root: file,
            },
        )
        .expect_err("refused");
        assert!(matches!(
            err,
            IdeError::Invalid {
                field: "repo_root",
                ..
            }
        ));
    }

    /// The database and the file system drift, and the list is where that has
    /// to become visible — noticing at open time means the user already
    /// clicked something that then failed.
    #[test]
    fn a_vanished_folder_shows_up_as_missing_rather_than_disappearing() {
        let db = temp_db();
        let dir = TempDir::new("vanish");
        let project = create_project(&db, new_project("Verschwunden", &dir)).expect("created");
        assert!(project.root_exists);

        std::fs::remove_dir_all(dir.path()).expect("removed");

        let listed = list_projects(&db).expect("listed");
        assert_eq!(listed.len(), 1, "the row stays");
        assert!(!listed[0].root_exists);
    }

    #[test]
    fn changing_the_path_keeps_the_id_and_the_layout() {
        let db = temp_db();
        let old = TempDir::new("moved-from");
        let new = TempDir::new("moved-to");
        let project = create_project(&db, new_project("Umgezogen", &old)).expect("created");
        set_layout(&db, project.id, Some(r#"{"v":1}"#)).expect("layout stored");

        let moved = set_repo_root(&db, project.id, new.path())
            .expect("repointed")
            .expect("still there");

        assert_eq!(moved.id, project.id);
        assert_eq!(moved.repo_root, new.path());
        assert_eq!(moved.layout_json.as_deref(), Some(r#"{"v":1}"#));
    }

    #[test]
    fn a_path_change_onto_an_occupied_folder_is_refused() {
        let db = temp_db();
        let a = TempDir::new("occupied-a");
        let b = TempDir::new("occupied-b");
        create_project(&db, new_project("A", &a)).expect("created");
        let second = create_project(&db, new_project("B", &b)).expect("created");

        let err = set_repo_root(&db, second.id, a.path()).expect_err("refused");
        assert!(matches!(
            err,
            IdeError::Invalid {
                field: "repo_root",
                ..
            }
        ));
    }

    #[test]
    fn a_layout_survives_the_round_trip_and_can_be_cleared() {
        let db = temp_db();
        let dir = TempDir::new("layout");
        let project = create_project(&db, new_project("Layout", &dir)).expect("created");

        assert!(set_layout(&db, project.id, Some("{\"tabs\":[]}")).expect("stored"));
        let reread = get_project(&db, project.id).expect("read").expect("there");
        assert_eq!(reread.layout_json.as_deref(), Some("{\"tabs\":[]}"));

        assert!(set_layout(&db, project.id, None).expect("cleared"));
        let cleared = get_project(&db, project.id).expect("read").expect("there");
        assert!(cleared.layout_json.is_none(), "back to the starting layout");
    }

    #[test]
    fn an_absurdly_large_layout_is_refused_rather_than_stored() {
        let db = temp_db();
        let dir = TempDir::new("huge");
        let project = create_project(&db, new_project("Gross", &dir)).expect("created");

        let huge = "x".repeat(MAX_LAYOUT_LEN + 1);
        let err = set_layout(&db, project.id, Some(&huge)).expect_err("refused");
        assert!(matches!(
            err,
            IdeError::Invalid {
                field: "layout_json",
                ..
            }
        ));
    }

    #[test]
    fn the_list_puts_the_most_recently_opened_first_and_the_unopened_last() {
        let db = temp_db();
        let a = TempDir::new("order-a");
        let b = TempDir::new("order-b");
        let c = TempDir::new("order-c");
        let first = create_project(&db, new_project("Zuerst", &a)).expect("created");
        let second = create_project(&db, new_project("Danach", &b)).expect("created");
        create_project(&db, new_project("Nie", &c)).expect("created");

        // Written directly rather than via `touch_opened`, so the two stamps
        // are far apart instead of within the same millisecond.
        db.execute(
            "UPDATE projects SET last_opened_at = ?2 WHERE id = ?1",
            params![first.id, "2026-09-01T10:00:00Z"],
        )
        .expect("stamped");
        db.execute(
            "UPDATE projects SET last_opened_at = ?2 WHERE id = ?1",
            params![second.id, "2026-09-20T10:00:00Z"],
        )
        .expect("stamped");

        let names: Vec<String> = list_projects(&db)
            .expect("listed")
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["Danach", "Zuerst", "Nie"]);
    }

    #[test]
    fn opening_a_project_records_when() {
        let db = temp_db();
        let dir = TempDir::new("touch");
        let project = create_project(&db, new_project("Offen", &dir)).expect("created");
        assert!(project.last_opened_at.is_none());

        assert!(touch_opened(&db, project.id).expect("touched"));
        let reread = get_project(&db, project.id).expect("read").expect("there");
        assert!(reread.last_opened_at.is_some());
    }

    /// House convention: a missing id is an ordinary outcome, not an error.
    #[test]
    fn operations_on_an_unknown_id_report_missing_rather_than_failing() {
        let db = temp_db();
        assert!(get_project(&db, 999).expect("read").is_none());
        assert!(rename_project(&db, 999, "x").expect("renamed").is_none());
        assert!(!set_layout(&db, 999, Some("{}")).expect("layout"));
        assert!(!touch_opened(&db, 999).expect("touched"));
        assert!(!delete_project(&db, 999).expect("deleted"));
    }

    /// The folder is the user's; the row is ours.
    #[test]
    fn deleting_a_project_leaves_the_folder_alone() {
        let db = temp_db();
        let dir = TempDir::new("delete");
        let project = create_project(&db, new_project("Weg damit", &dir)).expect("created");

        assert!(delete_project(&db, project.id).expect("deleted"));
        assert!(get_project(&db, project.id).expect("read").is_none());
        assert!(dir.path().is_dir(), "the folder must still be there");
    }

    #[test]
    fn the_folder_is_free_again_once_its_project_is_gone() {
        let db = temp_db();
        let dir = TempDir::new("reuse");
        let project = create_project(&db, new_project("Erstes", &dir)).expect("created");
        delete_project(&db, project.id).expect("deleted");

        create_project(&db, new_project("Zweites", &dir)).expect("the folder is free again");
    }

    #[test]
    fn a_name_at_the_maximum_length_is_kept_one_over_is_refused() {
        let db = temp_db();
        let dir = TempDir::new("name-max");
        let exactly_max = "x".repeat(MAX_NAME_LEN);
        let project = create_project(
            &db,
            NewProject {
                name: exactly_max.clone(),
                repo_root: dir.path().to_path_buf(),
            },
        )
        .expect("exactly the maximum is fine");
        assert_eq!(project.name, exactly_max);

        let other = TempDir::new("name-over");
        let err = create_project(
            &db,
            NewProject {
                name: "x".repeat(MAX_NAME_LEN + 1),
                repo_root: other.path().to_path_buf(),
            },
        )
        .expect_err("refused");
        assert!(matches!(err, IdeError::Invalid { field: "name", .. }));
    }

    /// `rename_project` runs the same `normalize_name` as `create_project` —
    /// worth pinning down separately since it is a different call site into
    /// the same validation, not a shared test.
    #[test]
    fn renaming_trims_and_refuses_an_empty_name_just_like_creating_does() {
        let db = temp_db();
        let dir = TempDir::new("rename-validate");
        let project = create_project(&db, new_project("Original", &dir)).expect("created");

        let renamed = rename_project(&db, project.id, "  Mit Rand  ")
            .expect("renamed")
            .expect("still there");
        assert_eq!(renamed.name, "Mit Rand");

        let err = rename_project(&db, project.id, "   ").expect_err("refused");
        assert!(matches!(err, IdeError::Invalid { field: "name", .. }));
    }

    /// `set_repo_root` runs the same `normalize_root` as `create_project` —
    /// a folder that does not exist must be refused there too, not just at
    /// creation time.
    #[test]
    fn set_repo_root_refuses_a_folder_that_is_not_there() {
        let db = temp_db();
        let dir = TempDir::new("set-root-validate");
        let project = create_project(&db, new_project("Original", &dir)).expect("created");

        let err = set_repo_root(&db, project.id, Path::new("/definitely/not/here/at/all"))
            .expect_err("refused");
        assert!(matches!(
            err,
            IdeError::Invalid {
                field: "repo_root",
                ..
            }
        ));
    }

    /// The UNIQUE index must not treat a row as clashing with *itself*: a
    /// project pointed back at the folder it already occupies is a no-op, not
    /// a refusal.
    #[test]
    fn set_repo_root_onto_its_own_current_folder_is_a_no_op() {
        let db = temp_db();
        let dir = TempDir::new("set-root-self");
        let project = create_project(&db, new_project("Original", &dir)).expect("created");

        let same = set_repo_root(&db, project.id, dir.path())
            .expect("not a clash with itself")
            .expect("still there");
        assert_eq!(same.id, project.id);
        assert_eq!(same.repo_root, dir.path());
    }

    /// Mirrors `two_projects_cannot_share_one_folder`'s check that the message
    /// names the holder, but for the `set_repo_root` path rather than create.
    #[test]
    fn a_path_change_onto_an_occupied_folder_names_the_holder() {
        let db = temp_db();
        let a = TempDir::new("occupied-named-a");
        let b = TempDir::new("occupied-named-b");
        create_project(&db, new_project("Erstes", &a)).expect("created");
        let second = create_project(&db, new_project("Zweites", &b)).expect("created");

        let err = set_repo_root(&db, second.id, a.path()).expect_err("refused");
        match err {
            IdeError::Invalid { field, reason } => {
                assert_eq!(field, "repo_root");
                assert!(reason.contains("Erstes"), "must name the holder: {reason}");
            }
            other => panic!("expected an Invalid, got {other:?}"),
        }
    }

    #[test]
    fn a_layout_exactly_at_the_ceiling_is_stored() {
        let db = temp_db();
        let dir = TempDir::new("layout-max");
        let project = create_project(&db, new_project("Layout", &dir)).expect("created");

        let exactly_max = "x".repeat(MAX_LAYOUT_LEN);
        assert!(set_layout(&db, project.id, Some(&exactly_max)).expect("stored at the ceiling"));
        let reread = get_project(&db, project.id).expect("read").expect("there");
        assert_eq!(reread.layout_json.as_deref(), Some(exactly_max.as_str()));
    }

    /// Two never-opened projects both sort last, and break their tie by name
    /// rather than insertion order — `the_list_puts_the_most_recently_opened…`
    /// only ever has one never-opened project, so this is the tie itself.
    #[test]
    fn never_opened_projects_tie_break_by_name() {
        let db = temp_db();
        let a = TempDir::new("tie-a");
        let b = TempDir::new("tie-b");
        create_project(&db, new_project("Zebra", &a)).expect("created");
        create_project(&db, new_project("Apfel", &b)).expect("created");

        let names: Vec<String> = list_projects(&db)
            .expect("listed")
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(
            names,
            vec!["Apfel", "Zebra"],
            "alphabetical, not insertion order"
        );
    }

    /// `touch_opened` is what the sort key is meant to react to — a later
    /// touch must move a project ahead of one touched earlier, independent of
    /// creation order.
    #[test]
    fn touching_a_project_moves_it_ahead_of_one_touched_earlier() {
        let db = temp_db();
        let a = TempDir::new("touch-order-a");
        let b = TempDir::new("touch-order-b");
        let first = create_project(&db, new_project("Zuerst", &a)).expect("created");
        let second = create_project(&db, new_project("Danach", &b)).expect("created");

        assert!(touch_opened(&db, first.id).expect("touched"));
        assert!(touch_opened(&db, second.id).expect("touched"));

        let names: Vec<String> = list_projects(&db)
            .expect("listed")
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, vec!["Danach", "Zuerst"]);
    }

    /// `parse_opt_ts` is a distinct code path from `parse_ts` (it also has to
    /// handle `None`) — worth its own corrupt-row case rather than trusting
    /// that `created_at`'s coverage stands in for it.
    #[test]
    fn a_row_with_a_broken_last_opened_timestamp_names_itself() {
        let db = temp_db();
        let dir = TempDir::new("corrupt-opened");
        let project = create_project(&db, new_project("Kaputt", &dir)).expect("created");
        touch_opened(&db, project.id).expect("touched");
        db.execute(
            "UPDATE projects SET last_opened_at = 'gestern' WHERE id = ?1",
            params![project.id],
        )
        .expect("mangled");

        match get_project(&db, project.id).expect_err("refused") {
            IdeError::CorruptRow { table, id, reason } => {
                assert_eq!(table, "projects");
                assert_eq!(id, project.id);
                assert!(reason.contains("last_opened_at"), "{reason}");
            }
            other => panic!("expected a CorruptRow, got {other:?}"),
        }
    }

    /// `list_projects` maps every row through the same fallible conversion as
    /// `get_project` — a corrupt row must fail the whole listing loudly
    /// rather than being silently dropped or panicking.
    #[test]
    fn a_corrupt_row_fails_the_whole_listing_rather_than_being_dropped() {
        let db = temp_db();
        let dir = TempDir::new("corrupt-list");
        let project = create_project(&db, new_project("Kaputt", &dir)).expect("created");
        db.execute(
            "UPDATE projects SET created_at = 'gestern' WHERE id = ?1",
            params![project.id],
        )
        .expect("mangled");

        let err = list_projects(&db).expect_err("refused");
        assert!(matches!(err, IdeError::CorruptRow { .. }));
    }

    #[test]
    fn a_row_with_a_broken_timestamp_names_itself() {
        let db = temp_db();
        let dir = TempDir::new("corrupt");
        let project = create_project(&db, new_project("Kaputt", &dir)).expect("created");
        db.execute(
            "UPDATE projects SET created_at = 'gestern' WHERE id = ?1",
            params![project.id],
        )
        .expect("mangled");

        match get_project(&db, project.id).expect_err("refused") {
            IdeError::CorruptRow { table, id, reason } => {
                assert_eq!(table, "projects");
                assert_eq!(id, project.id);
                assert!(reason.contains("created_at"), "{reason}");
            }
            other => panic!("expected a CorruptRow, got {other:?}"),
        }
    }
}
