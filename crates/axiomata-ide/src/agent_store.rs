//! Every SQL statement about agent profiles.
//!
//! Same conventions as [`crate::store`]: free functions over a borrowed
//! `&Connection`, `create` returns the stored struct, `update` returns
//! `Option<T>` (`None` = no such id), `delete` returns a found-flag. "Missing"
//! is never an error.
//!
//! Unlike the projects store, nothing here touches the file system at all —
//! not even to look. An agent profile is rows. CP5 adds the worktree module
//! beside this one, and *that* one will create and remove real directories;
//! the contract is per module, so do not carry this one's over to it.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{Agent, AgentFields, Harness, NewAgent};
use crate::{IdeError, Result};

/// Counted in characters, like a project's name and for the same reason: this
/// cap is about how much of a tab bar a name eats, and that is a question
/// about characters, not bytes.
const MAX_NAME_LEN: usize = 60;

/// Generous for a command line, small enough that a runaway value is caught
/// before it becomes a row nobody can display.
const MAX_COMMAND_LEN: usize = 2000;

/// Enough for a page of `KEY=value` lines.
const MAX_ENV_LEN: usize = 8000;

const AGENT_COLS: &str =
    "id, project_id, name, harness, command, model, env, created_at, updated_at";

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn parse_ts(raw: &str, id: i64, field: &'static str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|err| IdeError::CorruptRow {
            table: "ide_agents",
            id,
            reason: format!("{field} is not RFC 3339: {err}"),
        })
}

/// An agent row with its enum and timestamps still as text.
struct RawAgent {
    id: i64,
    project_id: i64,
    name: String,
    harness: String,
    command: String,
    model: Option<String>,
    env: String,
    created_at: String,
    updated_at: String,
}

fn row_to_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawAgent> {
    Ok(RawAgent {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        harness: row.get(3)?,
        command: row.get(4)?,
        model: row.get(5)?,
        env: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

impl RawAgent {
    fn into_agent(self) -> Result<Agent> {
        // An unrecognised harness is a corrupt row, not a default. Starting a
        // row we cannot read with "whatever `opencode` does" would run the
        // wrong program against the user's repository.
        let harness = Harness::parse(&self.harness).ok_or_else(|| IdeError::CorruptRow {
            table: "ide_agents",
            id: self.id,
            reason: format!("unknown harness {:?}", self.harness),
        })?;
        Ok(Agent {
            id: self.id,
            project_id: self.project_id,
            name: self.name,
            harness,
            effective_command: Agent::resolve_command(
                &self.command,
                harness,
                self.model.as_deref(),
            ),
            command: self.command,
            model: self.model,
            env: self.env,
            created_at: parse_ts(&self.created_at, self.id, "created_at")?,
            updated_at: parse_ts(&self.updated_at, self.id, "updated_at")?,
        })
    }
}

fn check_len(value: &str, field: &'static str, max: usize) -> Result<()> {
    if value.chars().count() > max {
        return Err(IdeError::Invalid {
            field,
            reason: format!("longer than {max} characters"),
        });
    }
    Ok(())
}

/// Trims and checks the fields every write shares.
fn normalize(fields: &AgentFields) -> Result<AgentFields> {
    let name = fields.name.trim().to_string();
    if name.is_empty() {
        return Err(IdeError::Invalid {
            field: "name",
            reason: "must not be empty".into(),
        });
    }
    check_len(&name, "name", MAX_NAME_LEN)?;
    check_len(&fields.command, "command", MAX_COMMAND_LEN)?;
    check_len(&fields.env, "env", MAX_ENV_LEN)?;

    let model = fields
        .model
        .as_ref()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty());

    Ok(AgentFields {
        name,
        harness: fields.harness,
        command: fields.command.trim().to_string(),
        model,
        env: fields.env.clone(),
    })
}

/// Turns the UNIQUE(project_id, name) violation into a message that names who
/// is already there, the way the projects store does for a folder clash.
fn explain_name_clash(
    db: &Connection,
    project_id: i64,
    name: &str,
    err: rusqlite::Error,
) -> IdeError {
    let is_unique_violation = matches!(
        err,
        rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::ConstraintViolation,
                ..
            },
            _
        )
    );
    if !is_unique_violation {
        return IdeError::Database(err);
    }
    // A foreign-key failure lands in the same error code; tell them apart by
    // asking whether the project exists at all, rather than parsing a message.
    let project_exists = db
        .query_row(
            "SELECT 1 FROM projects WHERE id = ?1",
            params![project_id],
            |_| Ok(()),
        )
        .optional()
        .unwrap_or(None)
        .is_some();
    if !project_exists {
        return IdeError::Invalid {
            field: "project_id",
            reason: format!("no project {project_id}"),
        };
    }
    IdeError::Invalid {
        field: "name",
        reason: format!("this project already has an agent called {name:?}"),
    }
}

pub fn list_agents(db: &Connection, project_id: i64) -> Result<Vec<Agent>> {
    let mut stmt = db.prepare(&format!(
        "SELECT {AGENT_COLS} FROM ide_agents WHERE project_id = ?1 \
         ORDER BY name COLLATE NOCASE, id"
    ))?;
    let raws = stmt
        .query_map(params![project_id], row_to_raw)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raws.into_iter().map(RawAgent::into_agent).collect()
}

pub fn get_agent(db: &Connection, id: i64) -> Result<Option<Agent>> {
    let raw = db
        .query_row(
            &format!("SELECT {AGENT_COLS} FROM ide_agents WHERE id = ?1"),
            params![id],
            row_to_raw,
        )
        .optional()?;
    raw.map(RawAgent::into_agent).transpose()
}

pub fn create_agent(db: &Connection, new: NewAgent) -> Result<Agent> {
    let fields = normalize(&new.fields)?;
    let stamp = now();

    db.execute(
        "INSERT INTO ide_agents (project_id, name, harness, command, model, env, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![
            new.project_id,
            fields.name,
            fields.harness.as_str(),
            fields.command,
            fields.model,
            fields.env,
            stamp,
        ],
    )
    .map_err(|err| explain_name_clash(db, new.project_id, &fields.name, err))?;

    let id = db.last_insert_rowid();
    get_agent(db, id)?.ok_or_else(|| IdeError::CorruptRow {
        table: "ide_agents",
        id,
        reason: "row vanished between insert and read".into(),
    })
}

/// Replaces every field. `None` if there is no such agent.
pub fn update_agent(db: &Connection, id: i64, fields: AgentFields) -> Result<Option<Agent>> {
    let fields = normalize(&fields)?;
    let project_id: Option<i64> = db
        .query_row(
            "SELECT project_id FROM ide_agents WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(project_id) = project_id else {
        return Ok(None);
    };

    db.execute(
        "UPDATE ide_agents SET name = ?2, harness = ?3, command = ?4, model = ?5, env = ?6, \
         updated_at = ?7 WHERE id = ?1",
        params![
            id,
            fields.name,
            fields.harness.as_str(),
            fields.command,
            fields.model,
            fields.env,
            now(),
        ],
    )
    .map_err(|err| explain_name_clash(db, project_id, &fields.name, err))?;

    get_agent(db, id)
}

/// Removes the profile. From CP5 the caller removes its worktree first — this
/// function will never do it, for the same reason deleting a project never
/// removes a folder.
pub fn delete_agent(db: &Connection, id: i64) -> Result<bool> {
    let changed = db.execute("DELETE FROM ide_agents WHERE id = ?1", params![id])?;
    Ok(changed == 1)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::store;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// An in-memory database with both IDE migrations applied, plus one
    /// project to hang agents off.
    fn fixture() -> (Connection, i64) {
        let db = Connection::open_in_memory().expect("open in-memory db");
        db.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        db.execute_batch(crate::SCHEMA_SQL_V1).unwrap();
        db.execute_batch(crate::SCHEMA_SQL_V2).unwrap();

        let dir = std::env::temp_dir().join(format!(
            "axiomata-agent-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let project = store::create_project(
            &db,
            crate::NewProject {
                name: "Test".into(),
                repo_root: dir,
            },
        )
        .unwrap();
        (db, project.id)
    }

    fn fields(name: &str) -> AgentFields {
        AgentFields {
            name: name.into(),
            harness: Harness::Opencode,
            command: String::new(),
            model: None,
            env: String::new(),
        }
    }

    fn new_agent(project_id: i64, name: &str) -> NewAgent {
        NewAgent {
            project_id,
            fields: fields(name),
        }
    }

    #[test]
    fn a_created_agent_comes_back_with_what_was_stored() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "Builder")).unwrap();

        assert_eq!(agent.name, "Builder");
        assert_eq!(agent.harness, Harness::Opencode);
        assert_eq!(agent.project_id, project);
        assert_eq!(agent.created_at, agent.updated_at);
        assert_eq!(get_agent(&db, agent.id).unwrap().unwrap().name, "Builder");
    }

    #[test]
    fn a_model_reaches_the_command_line() {
        let (db, project) = fixture();
        let mut with_model = fields("Deep");
        with_model.model = Some("openrouter/deepseek/deepseek-v4-flash-0731".into());
        let agent = create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: with_model,
            },
        )
        .unwrap();

        // Without this the harness starts on whatever its own config says is
        // the default, and the model on the profile is decoration.
        assert_eq!(
            agent.effective_command,
            "opencode --model 'openrouter/deepseek/deepseek-v4-flash-0731'"
        );
    }

    #[test]
    fn a_model_is_quoted_so_a_shell_cannot_read_it_as_syntax() {
        let (db, project) = fixture();
        let mut odd = fields("Odd");
        // Nobody should name a model like this; the point is that it cannot
        // become shell syntax if they do. `(` is a glob character in zsh.
        odd.model = Some("weird (model) name".into());
        let agent = create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: odd,
            },
        )
        .unwrap();
        assert_eq!(
            agent.effective_command,
            "opencode --model 'weird (model) name'"
        );

        let mut quoted = fields("Quoted");
        quoted.model = Some("it's".into());
        let agent = create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: quoted,
            },
        )
        .unwrap();
        assert_eq!(agent.effective_command, r"opencode --model 'it'\''s'");
    }

    #[test]
    fn a_model_is_left_out_of_a_command_somebody_wrote_themselves() {
        let (db, project) = fixture();
        let mut own = fields("Own");
        own.command = "opencode --agent build --model already/chosen".into();
        own.model = Some("something/else".into());
        let agent = create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: own,
            },
        )
        .unwrap();

        assert_eq!(
            agent.effective_command,
            "opencode --agent build --model already/chosen"
        );
    }

    #[test]
    fn an_empty_command_resolves_to_the_harness_default() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "Builder")).unwrap();
        assert_eq!(agent.effective_command, "opencode");

        let mut own = fields("Builder");
        own.command = "  opencode run --agent build  ".into();
        let updated = update_agent(&db, agent.id, own).unwrap().unwrap();
        assert_eq!(updated.effective_command, "opencode run --agent build");
    }

    #[test]
    fn two_agents_in_one_project_cannot_share_a_name() {
        let (db, project) = fixture();
        create_agent(&db, new_agent(project, "Builder")).unwrap();

        // CP5 derives a worktree directory from this name; a duplicate would
        // be two agents in one directory.
        let err = create_agent(&db, new_agent(project, "Builder")).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("Builder"),
            "should name the clash: {message}"
        );
    }

    #[test]
    fn a_name_differing_only_in_case_is_the_same_name() {
        let (db, project) = fixture();
        create_agent(&db, new_agent(project, "Builder")).unwrap();

        // macOS filesystems are case-insensitive, and CP5 makes this name a
        // directory — so these two would share one worktree.
        assert!(create_agent(&db, new_agent(project, "builder")).is_err());
    }

    #[test]
    fn the_same_name_in_another_project_is_fine() {
        let (db, first) = fixture();
        let dir = std::env::temp_dir().join(format!(
            "axiomata-agent-test-other-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let second = store::create_project(
            &db,
            crate::NewProject {
                name: "Other".into(),
                repo_root: dir,
            },
        )
        .unwrap();

        create_agent(&db, new_agent(first, "Builder")).unwrap();
        create_agent(&db, new_agent(second.id, "Builder")).unwrap();
        assert_eq!(list_agents(&db, first).unwrap().len(), 1);
        assert_eq!(list_agents(&db, second.id).unwrap().len(), 1);
    }

    #[test]
    fn an_agent_for_a_project_that_does_not_exist_says_so() {
        let (db, _) = fixture();
        let err = create_agent(&db, new_agent(9999, "Builder")).unwrap_err();
        assert!(
            err.to_string().contains("no project 9999"),
            "should blame the project, not the name: {err}"
        );
    }

    #[test]
    fn a_nameless_agent_is_refused() {
        let (db, project) = fixture();
        let mut blank = new_agent(project, "   ");
        blank.fields.name = "   ".into();
        assert!(create_agent(&db, blank).is_err());
    }

    #[test]
    fn a_name_at_the_maximum_is_kept_and_one_over_is_refused() {
        let (db, project) = fixture();
        let at_max = "ä".repeat(MAX_NAME_LEN);
        assert!(create_agent(&db, new_agent(project, &at_max)).is_ok());

        let over = "ä".repeat(MAX_NAME_LEN + 1);
        assert!(create_agent(&db, new_agent(project, &over)).is_err());
    }

    #[test]
    fn an_empty_model_is_stored_as_none_not_as_an_empty_string() {
        let (db, project) = fixture();
        let mut blank_model = fields("Builder");
        blank_model.model = Some("   ".into());
        let agent = create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: blank_model,
            },
        )
        .unwrap();
        assert_eq!(agent.model, None);
    }

    #[test]
    fn updating_replaces_every_field_and_moves_updated_at() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "Builder")).unwrap();

        let replaced = update_agent(
            &db,
            agent.id,
            AgentFields {
                name: "Reviewer".into(),
                harness: Harness::ClaudeCode,
                command: "claude --print".into(),
                model: Some("claude-sonnet-5".into()),
                env: "FOO=bar".into(),
            },
        )
        .unwrap()
        .unwrap();

        assert_eq!(replaced.name, "Reviewer");
        assert_eq!(replaced.harness, Harness::ClaudeCode);
        assert_eq!(replaced.model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(replaced.env, "FOO=bar");
        assert!(replaced.updated_at >= replaced.created_at);
    }

    #[test]
    fn updating_or_deleting_something_that_is_not_there_is_not_an_error() {
        let (db, _) = fixture();
        assert!(update_agent(&db, 4242, fields("Ghost")).unwrap().is_none());
        assert!(!delete_agent(&db, 4242).unwrap());
    }

    #[test]
    fn deleting_a_project_takes_its_agents_with_it() {
        let (db, project) = fixture();
        create_agent(&db, new_agent(project, "Builder")).unwrap();
        create_agent(&db, new_agent(project, "Reviewer")).unwrap();

        assert!(store::delete_project(&db, project).unwrap());
        assert!(list_agents(&db, project).unwrap().is_empty());
    }

    #[test]
    fn a_row_with_an_unreadable_harness_is_an_error_not_a_guess() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "Builder")).unwrap();
        db.execute(
            "UPDATE ide_agents SET harness = 'something_else' WHERE id = ?1",
            params![agent.id],
        )
        .unwrap();

        // Defaulting here would start the wrong program against a repository.
        let err = get_agent(&db, agent.id).unwrap_err();
        assert!(err.to_string().contains("something_else"), "{err}");
    }

    #[test]
    fn agents_are_listed_by_name_regardless_of_case() {
        let (db, project) = fixture();
        for name in ["zeta", "Alpha", "beta"] {
            create_agent(&db, new_agent(project, name)).unwrap();
        }
        let names: Vec<_> = list_agents(&db, project)
            .unwrap()
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names, vec!["Alpha", "beta", "zeta"]);
    }
}
