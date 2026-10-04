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

use std::path::{Path, PathBuf};

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

const AGENT_COLS: &str = "id, project_id, name, harness, command, model, env, created_at, \
                          updated_at, worktree_path, branch, port, base_branch, \
                          opencode_session, engine_id, agent_role, card_id, card_review, start_ref";

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
    worktree_path: Option<String>,
    branch: Option<String>,
    port: Option<i64>,
    base_branch: Option<String>,
    opencode_session: Option<String>,
    engine_id: Option<String>,
    agent_role: String,
    card_id: Option<i64>,
    card_review: bool,
    start_ref: Option<String>,
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
        worktree_path: row.get(9)?,
        branch: row.get(10)?,
        port: row.get(11)?,
        base_branch: row.get(12)?,
        opencode_session: row.get(13)?,
        engine_id: row.get(14)?,
        agent_role: row.get(15)?,
        card_id: row.get(16)?,
        card_review: row.get::<_, i64>(17)? != 0,
        start_ref: row.get(18)?,
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
        let worktree_path = self.worktree_path.map(PathBuf::from);
        // A port outside u16 cannot come from this crate; a hand-edited row is
        // not worth failing the whole read over, so it is dropped instead.
        let port = self.port.and_then(|p| u16::try_from(p).ok());
        let effective_env = Agent::resolve_env(
            &self.env,
            self.id,
            &self.name,
            worktree_path.as_deref(),
            self.branch.as_deref(),
            port,
        );
        Ok(Agent {
            id: self.id,
            project_id: self.project_id,
            effective_command: Agent::resolve_command(
                &self.command,
                harness,
                self.model.as_deref(),
            ),
            effective_env,
            name: self.name,
            harness,
            command: self.command,
            model: self.model,
            env: self.env,
            created_at: parse_ts(&self.created_at, self.id, "created_at")?,
            updated_at: parse_ts(&self.updated_at, self.id, "updated_at")?,
            worktree_path,
            branch: self.branch,
            port,
            base_branch: self.base_branch,
            opencode_session: self.opencode_session,
            engine_id: self.engine_id,
            agent_role: self.agent_role,
            card_id: self.card_id,
            card_review: self.card_review,
            start_ref: self.start_ref,
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
        // A changed profile no longer describes the engine it was derived from, so the assignment is dropped and
        // derived again (the right-hand sides below read the row as it was before this update).
        "UPDATE ide_agents SET name = ?2, harness = ?3, command = ?4, model = ?5, env = ?6, \
         engine_id = CASE WHEN harness IS NOT ?3 OR command IS NOT ?4 OR model IS NOT ?5 OR env IS NOT ?6 \
                          THEN NULL ELSE engine_id END, \
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

/// Records the worktree an agent works in, and the branch checked out there.
///
/// Separate from `update_agent` on purpose: that one is the editing form's
/// full replace of what a *person* typed, and a worktree is not something a
/// person types. Mixing them would mean the form could clear a worktree path
/// by not knowing about it.
pub fn set_worktree(
    db: &Connection,
    id: i64,
    path: Option<&Path>,
    branch: Option<&str>,
) -> Result<bool> {
    let path_text = match path {
        Some(path) => Some(path.to_str().ok_or_else(|| IdeError::Invalid {
            field: "worktree_path",
            reason: "path is not valid UTF-8".into(),
        })?),
        None => None,
    };
    let changed = db.execute(
        "UPDATE ide_agents SET worktree_path = ?2, branch = ?3, updated_at = ?4 WHERE id = ?1",
        params![id, path_text, branch, now()],
    )?;
    Ok(changed == 1)
}

/// Records the branch an agent's worktree was cut from (M7.3, G1).
///
/// Written once, when the worktree's branch is first created, and never by
/// the editing form — like [`set_worktree`], it is not something a person
/// types.
pub fn set_base_branch(db: &Connection, id: i64, base_branch: Option<&str>) -> Result<bool> {
    let changed = db.execute(
        "UPDATE ide_agents SET base_branch = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, base_branch, now()],
    )?;
    Ok(changed == 1)
}

/// The Opencode sessions the IDE keeps for its agents: those of Opencode
/// agents on the generated command (an own command gets none, E13).
pub fn opencode_sessions(db: &Connection) -> Result<Vec<String>> {
    let mut stmt = db.prepare(
        "SELECT opencode_session FROM ide_agents \
         WHERE harness = 'opencode' AND trim(command) = '' AND opencode_session IS NOT NULL \
         ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Records the Opencode session an agent runs in; `None` forgets it, so the
/// next start creates a fresh one ("New session").
pub fn set_opencode_session(db: &Connection, id: i64, session: Option<&str>) -> Result<bool> {
    if let Some(session) = session {
        check_len(session, "opencode_session", 128)?;
        // The id ends up typed into a shell; only what Opencode hands out.
        if !session
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(IdeError::Invalid {
                field: "opencode_session",
                reason: "not a session id".into(),
            });
        }
    }
    let changed = db.execute(
        "UPDATE ide_agents SET opencode_session = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, session, now()],
    )?;
    Ok(changed == 1)
}

/// Every agent without an engine yet, across all projects — what the start-up derivation of the engine catalog
/// works through (CP-A1).
pub fn unassigned_agents(db: &Connection) -> Result<Vec<Agent>> {
    let mut stmt = db.prepare(&format!(
        "SELECT {AGENT_COLS} FROM ide_agents WHERE engine_id IS NULL ORDER BY id"
    ))?;
    let raws = stmt
        .query_map([], row_to_raw)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raws.into_iter().map(RawAgent::into_agent).collect()
}

/// Points an agent session at an engine of the owner's catalog; `None` clears it.
///
/// Whether the engine exists is the catalog's business (it lives in the config, not here); only the shape is
/// checked, so a row can never carry something that is not an id.
pub fn set_engine(db: &Connection, id: i64, engine_id: Option<&str>) -> Result<bool> {
    if let Some(engine_id) = engine_id {
        axiomata_roster::check_slug("engine_id", engine_id).map_err(|err| IdeError::Invalid {
            field: "engine_id",
            reason: err.to_string(),
        })?;
    }
    let changed = db.execute(
        "UPDATE ide_agents SET engine_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, engine_id, now()],
    )?;
    Ok(changed == 1)
}

/// Sets the role an agent session plays (a name under `~/.axiomata/agents/`).
pub fn set_role(db: &Connection, id: i64, role: &str) -> Result<bool> {
    axiomata_roster::check_slug("agent_role", role).map_err(|err| IdeError::Invalid {
        field: "agent_role",
        reason: err.to_string(),
    })?;
    let changed = db.execute(
        "UPDATE ide_agents SET agent_role = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, role, now()],
    )?;
    Ok(changed == 1)
}

/// Records the card the studio started this session for, whether the session reviews it (instead of working on it) and,
/// for a reviewer, the commit its worktree is cut from.
///
/// # Errors
///
/// [`IdeError::Invalid`] for a `start_ref` that is not a full commit id: it ends up as an argument of `git`.
pub fn set_card(
    db: &Connection,
    id: i64,
    card_id: Option<i64>,
    review: bool,
    start_ref: Option<&str>,
) -> Result<bool> {
    if let Some(reference) = start_ref
        && !is_commit_id(reference)
    {
        return Err(IdeError::Invalid {
            field: "start_ref",
            reason: "must be a full commit id".into(),
        });
    }
    let changed = db.execute(
        "UPDATE ide_agents SET card_id = ?2, card_review = ?3, start_ref = ?4, updated_at = ?5 WHERE id = ?1",
        params![id, card_id, i64::from(review), start_ref, now()],
    )?;
    Ok(changed == 1)
}

/// Whether `text` is a full commit id: 40 lower-case hex digits (a SHA-256 repository would have 64).
pub(crate) fn is_commit_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// How many sessions play `role` — a role may only be deleted when this is 0.
pub fn count_with_role(db: &Connection, role: &str) -> Result<usize> {
    let n: i64 = db.query_row(
        "SELECT COUNT(*) FROM ide_agents WHERE agent_role = ?1",
        params![role],
        |row| row.get(0),
    )?;
    Ok(usize::try_from(n).unwrap_or(0))
}

/// Reserves a port for an agent. The UNIQUE index refuses one already taken.
pub fn set_port(db: &Connection, id: i64, port: Option<u16>) -> Result<bool> {
    let changed = db
        .execute(
            "UPDATE ide_agents SET port = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, port.map(i64::from), now()],
        )
        .map_err(|err| IdeError::Invalid {
            field: "port",
            reason: format!("{port:?} is already reserved by another agent ({err})"),
        })?;
    Ok(changed == 1)
}

/// Every port currently reserved, across all projects — they share a machine.
pub fn reserved_ports(db: &Connection) -> Result<Vec<u16>> {
    let mut stmt =
        db.prepare("SELECT port FROM ide_agents WHERE port IS NOT NULL ORDER BY port")?;
    let ports = stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ports
        .into_iter()
        .filter_map(|p| u16::try_from(p).ok())
        .collect())
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
        crate::apply_all_schemas(&db);

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
    fn an_opencode_model_stays_off_the_command_line() {
        // Opencode 2's terminal UI refuses `--model`; the model goes onto the
        // session the IDE creates (OC2), and the start adds `--session`.
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
        assert_eq!(agent.effective_command, "opencode");
        assert_eq!(
            agent.model.as_deref(),
            Some("openrouter/deepseek/deepseek-v4-flash-0731")
        );
    }

    #[test]
    fn set_opencode_session_records_and_forgets_it() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "Sess")).unwrap();
        assert!(agent.opencode_session.is_none());
        assert!(set_opencode_session(&db, agent.id, Some("ses_abc")).unwrap());
        assert_eq!(
            get_agent(&db, agent.id)
                .unwrap()
                .unwrap()
                .opencode_session
                .as_deref(),
            Some("ses_abc")
        );
        assert!(set_opencode_session(&db, agent.id, None).unwrap());
        assert!(
            get_agent(&db, agent.id)
                .unwrap()
                .unwrap()
                .opencode_session
                .is_none()
        );
        assert!(!set_opencode_session(&db, 4242, Some("ses_x")).unwrap());
        assert!(set_opencode_session(&db, agent.id, Some(&"s".repeat(129))).is_err());
        assert!(set_opencode_session(&db, agent.id, Some("ses_x; rm -rf ~")).is_err());

        // Only Opencode agents on the generated command are listed.
        let mut own = fields("Own");
        own.command = "opencode --agent plan".into();
        let own = create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: own,
            },
        )
        .unwrap();
        set_opencode_session(&db, own.id, Some("ses_own")).unwrap();
        set_opencode_session(&db, agent.id, Some("ses_mine")).unwrap();
        assert_eq!(
            opencode_sessions(&db).unwrap(),
            vec!["ses_mine".to_string()]
        );
    }

    #[test]
    fn a_model_reaches_the_command_line() {
        let (db, project) = fixture();
        let mut with_model = fields("Deep");
        with_model.harness = Harness::ClaudeCode;
        with_model.model = Some("claude-sonnet-5".into());
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
        assert_eq!(agent.effective_command, "claude --model 'claude-sonnet-5'");
    }

    #[test]
    fn a_model_is_quoted_so_a_shell_cannot_read_it_as_syntax() {
        let (db, project) = fixture();
        let mut odd = fields("Odd");
        odd.harness = Harness::ClaudeCode;
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
            "claude --model 'weird (model) name'"
        );

        let mut quoted = fields("Quoted");
        quoted.harness = Harness::ClaudeCode;
        quoted.model = Some("it's".into());
        let agent = create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: quoted,
            },
        )
        .unwrap();
        assert_eq!(agent.effective_command, r"claude --model 'it'\''s'");
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
    fn set_base_branch_records_it_and_a_missing_agent_reports_not_found() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "Builder")).unwrap();
        assert!(agent.base_branch.is_none());

        let before = get_agent(&db, agent.id).unwrap().unwrap().updated_at;
        assert!(set_base_branch(&db, agent.id, Some("main")).unwrap());
        let updated = get_agent(&db, agent.id).unwrap().unwrap();
        assert_eq!(updated.base_branch.as_deref(), Some("main"));
        assert!(updated.updated_at >= before);

        // Clearing it back to None is a legitimate write, not a no-op skip.
        assert!(set_base_branch(&db, agent.id, None).unwrap());
        assert!(
            get_agent(&db, agent.id)
                .unwrap()
                .unwrap()
                .base_branch
                .is_none()
        );

        assert!(!set_base_branch(&db, 4242, Some("main")).unwrap());
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

    #[test]
    fn a_new_agent_is_an_unassigned_allrounder() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "Builder")).unwrap();
        assert_eq!(agent.engine_id, None);
        assert_eq!(agent.agent_role, "allrounder");
        let unassigned = unassigned_agents(&db).unwrap();
        assert_eq!(unassigned.len(), 1);
        assert_eq!(unassigned[0].id, agent.id);
    }

    #[test]
    fn assigning_an_engine_takes_the_agent_off_the_unassigned_list_and_clearing_puts_it_back() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "Builder")).unwrap();

        assert!(set_engine(&db, agent.id, Some("claude-opus")).unwrap());
        assert_eq!(
            get_agent(&db, agent.id)
                .unwrap()
                .unwrap()
                .engine_id
                .as_deref(),
            Some("claude-opus")
        );
        assert!(unassigned_agents(&db).unwrap().is_empty());

        assert!(set_engine(&db, agent.id, None).unwrap());
        assert_eq!(unassigned_agents(&db).unwrap().len(), 1);
        assert!(!set_engine(&db, 9999, Some("x")).unwrap(), "no such agent");
    }

    #[test]
    fn engine_and_role_must_look_like_ids() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "Builder")).unwrap();
        for bad in ["", "Has Space", "../x", "UPPER"] {
            assert!(
                matches!(
                    set_engine(&db, agent.id, Some(bad)),
                    Err(IdeError::Invalid {
                        field: "engine_id",
                        ..
                    })
                ),
                "{bad:?}"
            );
            assert!(
                matches!(
                    set_role(&db, agent.id, bad),
                    Err(IdeError::Invalid {
                        field: "agent_role",
                        ..
                    })
                ),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn roles_are_counted_so_that_a_used_one_is_not_deleted() {
        let (db, project) = fixture();
        let a = create_agent(&db, new_agent(project, "A")).unwrap();
        create_agent(&db, new_agent(project, "B")).unwrap();
        assert_eq!(count_with_role(&db, "allrounder").unwrap(), 2);
        assert!(set_role(&db, a.id, "reviewer").unwrap());
        assert_eq!(count_with_role(&db, "allrounder").unwrap(), 1);
        assert_eq!(count_with_role(&db, "reviewer").unwrap(), 1);
        assert_eq!(count_with_role(&db, "planner").unwrap(), 0);
    }

    #[test]
    fn rows_from_before_the_migration_become_allrounders_without_an_engine() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        for schema in [
            crate::SCHEMA_SQL_V1,
            crate::SCHEMA_SQL_V2,
            crate::SCHEMA_SQL_V3,
            crate::SCHEMA_SQL_V4,
            crate::SCHEMA_SQL_V5,
        ] {
            db.execute_batch(schema).unwrap();
        }
        db.execute(
            "INSERT INTO projects (name, repo_root, created_at) VALUES ('P', '/tmp/p', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO ide_agents (project_id, name, harness, created_at, updated_at) \
             VALUES (1, 'old', 'claude_code', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        db.execute_batch(crate::SCHEMA_SQL_V6).unwrap();
        db.execute_batch(crate::SCHEMA_SQL_V8).unwrap();

        let agent = get_agent(&db, 1).unwrap().unwrap();
        assert_eq!(agent.agent_role, "allrounder");
        assert_eq!(agent.engine_id, None);
        // A row from before the card columns is a session of the owner's own making.
        assert_eq!(
            (agent.card_id, agent.card_review, agent.start_ref),
            (None, false, None)
        );
        assert_eq!(agent.harness, Harness::ClaudeCode);
    }

    #[test]
    fn changing_the_profile_drops_the_engine_but_a_rename_keeps_it() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "Builder")).unwrap();
        set_engine(&db, agent.id, Some("opencode")).unwrap();

        let mut renamed = fields("Builder 2");
        let kept = update_agent(&db, agent.id, renamed.clone())
            .unwrap()
            .unwrap();
        assert_eq!(kept.engine_id.as_deref(), Some("opencode"));

        renamed.model = Some("other/model".into());
        let dropped = update_agent(&db, agent.id, renamed).unwrap().unwrap();
        assert_eq!(
            dropped.engine_id, None,
            "a different model is a different engine"
        );
    }

    #[test]
    fn the_card_of_a_session_is_kept_and_a_reference_must_be_a_commit_id() {
        let (db, project) = fixture();
        let agent = create_agent(&db, new_agent(project, "builder-3"))
            .unwrap()
            .id;
        let fresh = get_agent(&db, agent).unwrap().unwrap();
        assert_eq!(
            (fresh.card_id, fresh.card_review, fresh.start_ref),
            (None, false, None)
        );

        let commit = "a".repeat(40);
        assert!(set_card(&db, agent, Some(3), true, Some(&commit)).unwrap());
        let got = get_agent(&db, agent).unwrap().unwrap();
        assert_eq!(got.card_id, Some(3));
        assert!(got.card_review);
        assert_eq!(got.start_ref.as_deref(), Some(commit.as_str()));

        // The reference ends up as a `git` argument: nothing else is taken, and nothing changes then.
        for bad in ["HEAD", "--detach", "abc", &"A".repeat(40)] {
            assert!(
                set_card(&db, agent, Some(9), false, Some(bad)).is_err(),
                "{bad:?}"
            );
        }
        assert_eq!(get_agent(&db, agent).unwrap().unwrap().card_id, Some(3));
        assert!(!set_card(&db, 999, Some(1), false, None).unwrap());
    }
}
