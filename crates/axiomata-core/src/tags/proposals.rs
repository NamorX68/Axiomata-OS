//! Tags waiting for the owner (`docs/plans/tags-links.md` Q6, Q10): an agent that finds no fitting tag on the list
//! proposes one, with a line of description; the owner accepts it — it goes onto `TAGS.md` and onto the notes that
//! wanted it — or turns it down. Only the app's own command answers, acting as the owner.

use std::sync::Mutex;

use chrono::{Duration, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use super::list::{self, one_line};
use super::{detect::normalise, note, note_path, refused, workspace};
use crate::config::Config;
use crate::error::AxiomataError;

/// Proposals that may wait at once: an agent run gone wrong must not bury the owner's list.
pub const MAX_OPEN: usize = 200;

/// Notes one proposal remembers; beyond that the tag is still proposed, the further notes are not kept.
pub const MAX_NOTES: usize = 100;

/// How long a turned-down tag is not proposed again: long enough that one vault run does not ask twice.
const REJECTED_DAYS: i64 = 30;

/// A proposal as the app shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TagProposal {
    pub id: i64,
    pub tag: String,
    pub description: String,
    /// The notes that asked for the tag; an accepted tag is set there.
    pub notes: Vec<String>,
    pub proposed_by: String,
    pub proposed_at: String,
}

/// What [`propose`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proposed {
    /// The tag waits for the owner (newly, or a note joined an open proposal).
    Waiting,
    /// The owner turned the tag down within the last [`REJECTED_DAYS`].
    RejectedBefore,
    /// [`MAX_OPEN`] proposals wait already.
    Full,
}

/// What [`answer`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Answered {
    pub tag: String,
    pub accepted: bool,
    /// Notes the accepted tag was set on now.
    pub notes_tagged: usize,
    /// Notes it could not be set on (gone, changed, no longer a note), one line each.
    pub notes_failed: Vec<String>,
}

/// Proposes `tag` (normalised) with `description` for `note`. An open proposal of the same tag takes the note (and a
/// description, if it had none).
///
/// # Errors
///
/// `tag` is no tag; the database fails.
pub fn propose(
    db: &Connection,
    tag: &str,
    description: &str,
    note: Option<&str>,
    by: &str,
) -> Result<Proposed, AxiomataError> {
    let Some(tag) = normalise(tag) else {
        return Err(refused(format!("{tag:?} is no tag")));
    };
    let description = one_line(description);
    let now = Utc::now();
    let since = (now - Duration::days(REJECTED_DAYS)).to_rfc3339();
    let turned_down: Option<i64> = db
        .query_row(
            "SELECT id FROM tag_proposals WHERE tag = ?1 AND decision = 'rejected' AND decided_at >= ?2 LIMIT 1",
            params![tag, since],
            |row| row.get(0),
        )
        .optional()?;
    if turned_down.is_some() {
        return Ok(Proposed::RejectedBefore);
    }
    let open: Option<(i64, String, String)> = db
        .query_row(
            "SELECT id, description, notes FROM tag_proposals WHERE tag = ?1 AND decision IS NULL",
            [&tag],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((id, old_description, notes)) = open {
        let mut notes = parse_notes(&notes);
        if let Some(note) = note
            && !notes.iter().any(|known| known == note)
            && notes.len() < MAX_NOTES
        {
            notes.push(note.to_owned());
        }
        let description = if old_description.is_empty() {
            description
        } else {
            old_description
        };
        db.execute(
            "UPDATE tag_proposals SET notes = ?1, description = ?2 WHERE id = ?3",
            params![
                serde_json::to_string(&notes).unwrap_or_default(),
                description,
                id
            ],
        )?;
        return Ok(Proposed::Waiting);
    }
    if count_open(db)? >= MAX_OPEN {
        return Ok(Proposed::Full);
    }
    let notes: Vec<&str> = note.into_iter().collect();
    db.execute(
        "INSERT INTO tag_proposals (tag, description, notes, proposed_by, proposed_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            tag,
            description,
            serde_json::to_string(&notes).unwrap_or_default(),
            one_line(by),
            now.to_rfc3339()
        ],
    )?;
    Ok(Proposed::Waiting)
}

fn parse_notes(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

/// The proposals waiting for the owner, oldest first.
///
/// # Errors
///
/// The database fails.
pub fn open(db: &Connection) -> Result<Vec<TagProposal>, AxiomataError> {
    let mut stmt = db.prepare(
        "SELECT id, tag, description, notes, proposed_by, proposed_at FROM tag_proposals \
         WHERE decision IS NULL ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(TagProposal {
            id: row.get(0)?,
            tag: row.get(1)?,
            description: row.get(2)?,
            notes: parse_notes(&row.get::<_, String>(3)?),
            proposed_by: row.get(4)?,
            proposed_at: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// How many proposals wait: the number on the waiting badge.
///
/// # Errors
///
/// The database fails.
pub fn count_open(db: &Connection) -> Result<usize, AxiomataError> {
    let count: i64 = db.query_row(
        "SELECT COUNT(*) FROM tag_proposals WHERE decision IS NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(count).unwrap_or(0))
}

/// The owner's answer to proposal `id`. Accepted, the tag goes onto the list — with `description` when the owner
/// wrote one, else the proposal's — and is set on the notes that wanted it, each through the guard (a note that is
/// gone or no longer a note is listed, not an error).
///
/// # Errors
///
/// No open proposal `id`; the list cannot be written; the database fails.
pub fn answer(
    db: &Mutex<Connection>,
    config: &Config,
    id: i64,
    accept: bool,
    description: Option<&str>,
) -> Result<Answered, AxiomataError> {
    let lock = || db.lock().unwrap_or_else(|poison| poison.into_inner());
    // The decision is claimed first, under the lock, so two answers cannot both act; the files are written after the
    // lock is let go (a vault iCloud is busy with must not hold the whole app's database).
    let (tag, proposed_description, notes) = {
        let db = lock();
        let row: Option<(String, String, String)> = db
            .query_row(
                "SELECT tag, description, notes FROM tag_proposals WHERE id = ?1 AND decision IS NULL",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let Some(row) = row else {
            return Err(refused(format!("there is no open tag proposal #{id}")));
        };
        db.execute(
            "UPDATE tag_proposals SET decision = ?1, decided_at = ?2 WHERE id = ?3 AND decision IS NULL",
            params![
                if accept { "accepted" } else { "rejected" },
                Utc::now().to_rfc3339(),
                id
            ],
        )?;
        row
    };
    let mut answered = Answered {
        tag: tag.clone(),
        accepted: accept,
        notes_tagged: 0,
        notes_failed: Vec::new(),
    };
    if !accept {
        return Ok(answered);
    }
    let description = description
        .map(one_line)
        .filter(|text| !text.is_empty())
        .unwrap_or(proposed_description);
    if let Err(err) = list::add(config, &tag, &description) {
        // Not on the list: the proposal waits again rather than being lost.
        lock().execute(
            "UPDATE tag_proposals SET decision = NULL, decided_at = NULL WHERE id = ?1",
            [id],
        )?;
        return Err(err);
    }
    for rel in parse_notes(&notes) {
        match set_tag(config, &rel, &tag) {
            Ok(()) => answered.notes_tagged += 1,
            Err(err) => answered.notes_failed.push(format!("{rel}: {err}")),
        }
    }
    Ok(answered)
}

/// Sets one tag on one note, nothing else.
fn set_tag(config: &Config, rel: &str, tag: &str) -> Result<(), AxiomataError> {
    let rel = note_path(rel)?;
    let Some(file) = workspace::read(config, &rel)? else {
        return Err(refused("the note is gone"));
    };
    let (text, added) = note::with_tags(&file.content, &[tag.to_owned()])?;
    if !added.is_empty() {
        workspace::write(config, &rel, &text, Some(&file.version))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::unique_temp_dir;
    use std::fs;

    fn fixture() -> (std::path::PathBuf, Config, Mutex<Connection>) {
        let root = unique_temp_dir("axiomata-tags-proposals");
        fs::create_dir_all(root.join("A")).unwrap();
        fs::write(root.join("A/one.md"), "# One\n").unwrap();
        fs::write(root.join("A/two.md"), "---\ntags: [x]\n---\n# Two\n").unwrap();
        let config = Config {
            workspace_root: root.clone(),
            ..Config::default()
        };
        let db = crate::db::open_and_migrate_at(&root.join(".t.db")).unwrap();
        (root, config, Mutex::new(db))
    }

    #[test]
    fn a_second_proposal_of_a_tag_joins_the_open_one() {
        let (root, _config, db) = fixture();
        let db = db.lock().unwrap();
        assert_eq!(
            propose(&db, "#Neu", "", Some("A/one.md"), "skill").unwrap(),
            Proposed::Waiting
        );
        assert_eq!(
            propose(&db, "neu", "New things", Some("A/two.md"), "chat").unwrap(),
            Proposed::Waiting
        );
        assert_eq!(
            propose(&db, "neu", "Ignored", Some("A/two.md"), "chat").unwrap(),
            Proposed::Waiting
        );
        let open = open(&db).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].description, "New things");
        assert_eq!(
            open[0].notes,
            vec!["A/one.md".to_string(), "A/two.md".to_string()]
        );
        assert!(propose(&db, "no tag", "", None, "x").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn accepting_lists_the_tag_and_sets_it_on_its_notes() {
        let (root, config, db_mutex) = fixture();
        let db = db_mutex.lock().unwrap();
        propose(&db, "neu", "New", Some("A/one.md"), "skill").unwrap();
        propose(&db, "neu", "", Some("A/two.md"), "skill").unwrap();
        propose(&db, "neu", "", Some("A/gone.md"), "skill").unwrap();
        let id = open(&db).unwrap()[0].id;
        drop(db);
        let answered = answer(&db_mutex, &config, id, true, Some("Brand new")).unwrap();
        assert_eq!(answered.notes_tagged, 2);
        assert_eq!(answered.notes_failed.len(), 1);
        assert_eq!(
            fs::read_to_string(root.join("A/two.md")).unwrap(),
            "---\ntags:\n  - x\n  - neu\n---\n# Two\n"
        );
        let listed = list::load(&config).unwrap();
        assert!(
            listed
                .iter()
                .any(|e| e.tag == "neu" && e.description == "Brand new")
        );
        assert!(count_open(&db_mutex.lock().unwrap()).unwrap() == 0);
        assert!(
            answer(&db_mutex, &config, id, true, None).is_err(),
            "answered once"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_turned_down_tag_is_not_proposed_again_soon() {
        let (root, config, db_mutex) = fixture();
        let db = db_mutex.lock().unwrap();
        propose(&db, "meh", "", Some("A/one.md"), "skill").unwrap();
        let id = open(&db).unwrap()[0].id;
        drop(db);
        let answered = answer(&db_mutex, &config, id, false, None).unwrap();
        let db = db_mutex.lock().unwrap();
        assert!(!answered.accepted && answered.notes_tagged == 0);
        assert_eq!(
            fs::read_to_string(root.join("A/one.md")).unwrap(),
            "# One\n"
        );
        assert_eq!(
            propose(&db, "meh", "", None, "skill").unwrap(),
            Proposed::RejectedBefore
        );
        let _ = fs::remove_dir_all(root);
    }
}
