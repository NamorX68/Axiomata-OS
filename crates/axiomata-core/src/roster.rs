//! The Studio's roster inside the app (`docs/plans/a2a.md`, CP-A1): which engines the owner has, which roles, and
//! how the agents that existed before the catalog get an engine.
//!
//! The types and the file handling live in the pure crate `axiomata-roster`; this module adds what needs the app's
//! config and database: deriving engines from the existing agent profiles, refusing to delete an engine or a role
//! something still uses, and the paths under `~/.axiomata`.

use std::collections::BTreeMap;
use std::sync::RwLock;

use axiomata_roster::{
    Billing, Engine, Harness, Loaded, MAX_IDENT_LEN, MAX_LABEL_CHARS, Role, RosterError, Source,
};
use rusqlite::Connection;

use crate::AxiomataError;
use crate::config::Config;
use crate::ide::agent_store;
use crate::ide::model::{Agent, AgentFields, Harness as AgentHarness};
use crate::paths;

type Result<T> = std::result::Result<T, AxiomataError>;

/// A refusal for the owner to read: `field` names what was asked for, `reason` why not.
pub fn refusal(field: &'static str, reason: String) -> AxiomataError {
    RosterError::Invalid { field, reason }.into()
}

/// Lower-case slug of free text: letters and digits, single dashes between them.
fn slugify(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

fn harness_slug(harness: Harness) -> &'static str {
    match harness {
        Harness::ClaudeCode => "claude-code",
        Harness::Opencode => "opencode",
        Harness::Glut => "glut",
    }
}

fn harness_label(harness: Harness) -> &'static str {
    match harness {
        Harness::ClaudeCode => "Claude Code",
        Harness::Opencode => "Opencode",
        Harness::Glut => "Glut",
    }
}

/// Whether `engine` is what `agent`'s profile describes.
fn describes(engine: &Engine, agent: &Agent) -> bool {
    engine.harness == agent.harness
        && engine.command.trim() == agent.command.trim()
        && engine.model == agent.model
        && engine.env.trim_end() == agent.env.trim_end()
}

/// A free id for a new engine built from `agent`'s profile.
fn fresh_id(engines: &BTreeMap<String, Engine>, agent: &Agent) -> String {
    let mut base = harness_slug(agent.harness).to_string();
    if let Some(model) = agent
        .model
        .as_deref()
        .map(slugify)
        .filter(|m| !m.is_empty())
    {
        base = format!("{base}-{model}");
    }
    // Room for a "-NN" suffix inside the id limit.
    base.truncate(MAX_IDENT_LEN - 4);
    let base = base.trim_end_matches(['-', '_']).to_string();
    let mut id = base.clone();
    let mut n = 2;
    while engines.contains_key(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    id
}

/// The file config, freshly read: the base every engine change is applied to.
///
/// Engines are written into **this** and never by saving the live config: that one still holds the workspace root
/// of this session, while the file may carry a change queued for the next start (see `save_config`) — saving it
/// back would silently undo it. Likewise the live engine map may be stale (the CLI changed the file meanwhile).
fn disk_config() -> Result<Config> {
    Config::load()
}

/// Gives every agent without an engine one: reuses an engine that describes its profile, else adds one.
///
/// Returns the assignments to write (`agent id → engine id`). Touches only `engines`; saving the config and
/// writing the rows is [`sync_agents`]' job, so this stays testable without the owner's home directory.
///
/// A profile that cannot make a valid engine (agent profiles are checked far more loosely than engines: any text
/// is a valid `env`) is **left unassigned** with a warning, never written as an invalid engine — that would make
/// every later save of the config fail validation.
pub fn derive_engines(
    agents: &[Agent],
    engines: &mut BTreeMap<String, Engine>,
) -> Vec<(i64, String)> {
    let mut assignments = Vec::new();
    for agent in agents {
        if let Some(existing) = engines.values().find(|e| describes(e, agent)) {
            assignments.push((agent.id, existing.id.clone()));
            continue;
        }
        let id = fresh_id(engines, agent);
        let mut label = harness_label(agent.harness).to_string();
        if let Some(model) = &agent.model {
            label = format!("{label} · {model}");
        }
        if !agent.command.trim().is_empty() {
            label.push_str(" · own command");
        }
        // An engine label is one short line; a long model id must not make the derived engine invalid.
        if label.chars().count() > MAX_LABEL_CHARS {
            label = label.chars().take(MAX_LABEL_CHARS - 1).collect::<String>() + "…";
        }
        let engine = Engine {
            id: id.clone(),
            label,
            harness: agent.harness,
            command: agent.command.trim().to_string(),
            model: agent.model.clone(),
            env: agent.env.clone(),
            // Claude Code runs on the owner's account; everything else is paid per token.
            billing: if agent.harness == Harness::ClaudeCode {
                Billing::Subscription
            } else {
                Billing::Metered
            },
            tier: None,
        };
        if let Err(err) = engine.validate() {
            tracing::warn!(agent = agent.id, %err, "the agent profile does not make a valid engine; left unassigned");
            continue;
        }
        engines.insert(id.clone(), engine);
        assignments.push((agent.id, id));
    }
    assignments
}

/// Assigns an engine to every agent that has none (CP-A1 migration, also run after an agent is created or edited).
///
/// Works on the freshly read file config, saves it first when engines were added, then writes the rows — all under
/// one lock — so a failure leaves an unused engine at worst, never a row pointing at a missing one. `config` is
/// updated with the resulting catalog only after everything succeeded. Returns how many agents were assigned.
///
/// # Errors
///
/// A failing config read or save, or a failing database write.
pub fn sync_agents(db: &Connection, config: &mut Config) -> Result<usize> {
    // The plain Opencode tile can have no engine: an Opencode engine is refused (opencode-exit.md, O1), and it runs
    // its harness's own command anyway — asking again on every sync would only warn every time.
    let agents: Vec<_> = agent_store::unassigned_agents(db)?
        .into_iter()
        .filter(|agent| !(agent.is_master() && agent.harness == Harness::Opencode))
        .collect();
    if agents.is_empty() {
        return Ok(0);
    }
    let _guard = crate::config::file_edit_lock();
    let mut disk = disk_config()?;
    let mut engines = disk.agents.engines.clone();
    let assignments = derive_engines(&agents, &mut engines);
    if engines != disk.agents.engines {
        disk.agents.engines = engines.clone();
        disk.save()?;
    }
    for (agent_id, engine_id) in &assignments {
        agent_store::set_engine(db, *agent_id, Some(engine_id))?;
    }
    config.agents.engines = engines;
    Ok(assignments.len())
}

/// [`sync_agents`] against the live config: the entry point for the dashboard and the CLI after an agent was
/// created or edited.
///
/// # Errors
///
/// As [`sync_agents`].
pub fn sync_live(db: &Connection, config: &RwLock<Config>) -> Result<usize> {
    let mut copy = config.read().unwrap_or_else(|p| p.into_inner()).clone();
    let assigned = sync_agents(db, &mut copy)?;
    if assigned > 0 {
        config
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .agents
            .engines = copy.agents.engines;
    }
    Ok(assigned)
}

/// An engine with how many agent sessions run on it — what the engine list shows.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineEntry {
    #[serde(flatten)]
    pub info: crate::engines::EngineInfo,
    pub sessions: usize,
}

/// Every engine — the owner's own, then the automatic ones ([`crate::engines::all`]) — with its session count.
///
/// # Errors
///
/// A failing database read.
pub fn engine_overview(
    db: &Connection,
    config: &Config,
    sources: &crate::model_tiers::ModelSources,
) -> Result<Vec<EngineEntry>> {
    let mut stmt = db.prepare(
        "SELECT engine_id, COUNT(*) FROM ide_agents WHERE engine_id IS NOT NULL GROUP BY engine_id",
    )?;
    let counts: BTreeMap<String, usize> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(id, n)| (id, usize::try_from(n).unwrap_or(0)))
        .collect();
    Ok(crate::engines::all(config, sources)
        .into_iter()
        .map(|info| EngineEntry {
            sessions: counts.get(&info.engine.id).copied().unwrap_or(0),
            info,
        })
        .collect())
}

/// The refusal for changing automatic engine `id`.
fn auto_refusal(id: &str) -> AxiomataError {
    refusal(
        "id",
        format!(
            "“{id}” is an automatic engine, made from a model of the catalog; it cannot be edited or removed — switch \
             its model on or off in the Studio's Models settings, or give an own engine another id"
        ),
    )
}

/// Adds or replaces an own engine and saves it into the config file. `config` is updated with the file's catalog.
///
/// # Errors
///
/// [`RosterError::Invalid`] if the engine is not valid or takes the id of an automatic engine; a failing read or save
/// (then `config` is unchanged).
pub fn save_engine(config: &mut Config, engine: Engine) -> Result<()> {
    engine.validate()?;
    let _guard = crate::config::file_edit_lock();
    let mut disk = disk_config()?;
    // An own engine of the same id may be edited (it already wins over the automatic one); a new one may not take it.
    if crate::engines::is_auto(&disk, &crate::model_tiers::ModelSources::load(), &engine.id) {
        return Err(auto_refusal(&engine.id));
    }
    disk.agents.engines.insert(engine.id.clone(), engine);
    disk.save()?;
    config.agents.engines = disk.agents.engines;
    Ok(())
}

/// Who still uses engine `id`: sessions, then roles. `None` if nobody.
///
/// Roles are the owner's that parsed; a role file that is currently broken, and the roles of projects that are not
/// open, cannot be asked.
fn engine_user(db: &Connection, roles: &[Role], id: &str) -> Result<Option<String>> {
    let sessions: i64 = db.query_row(
        "SELECT COUNT(*) FROM ide_agents WHERE engine_id = ?1",
        [id],
        |row| row.get(0),
    )?;
    if sessions > 0 {
        return Ok(Some(format!("{sessions} agent session(s) run on it")));
    }
    let role = roles
        .iter()
        .find(|r| r.engine.as_deref() == Some(id) || r.fallback_engines.iter().any(|f| f == id));
    Ok(role.map(|r| format!("the role “{}” uses it", r.name)))
}

/// The roles in force for `project_id`: the owner's, with the project's own once the owner confirmed them
/// ([`project_roles`]). The owner's alone when the project cannot be resolved, nothing when even that cannot be read.
pub fn roles_for_project(db: &Connection, config: &Config, project_id: i64) -> Vec<Role> {
    let root = crate::ide::store::get_project(db, project_id)
        .ok()
        .flatten()
        .map(|project| project.repo_root);
    match root.and_then(|root| project_roles(&root, config).ok()) {
        Some(project) => project.effective,
        None => list_roles().map(|loaded| loaded.roles).unwrap_or_default(),
    }
}

/// The profile columns of an agent session, copied from its engine. The columns stay the fallback that starting reads
/// until CP-A6 starts from the engine itself; copying them here means a session made on an engine runs exactly what the
/// engine says, and the engine is the only place that is edited.
fn fields_from_engine(engine: &Engine, name: &str) -> AgentFields {
    AgentFields {
        name: name.to_string(),
        harness: engine.harness,
        command: engine.command.clone(),
        model: engine.model.clone(),
        env: engine.env.clone(),
    }
}

/// Engine `id`, own or automatic.
fn known_engine(config: &Config, id: &str) -> Result<Engine> {
    crate::engines::Lookup::new(config)
        .engine(id)
        .ok_or_else(|| {
            RosterError::Invalid {
                field: "engine",
                reason: format!(
                    "there is no engine “{id}”; switch its model on in the Studio's Models settings, or add an own \
                     engine"
                ),
            }
            .into()
        })
}

fn known_role(roles: &[Role], name: &str) -> Result<()> {
    // Reserved: a role file of that name would turn an ordinary session into a project's master, in its real folder.
    if name == crate::ide::model::MASTER_ROLE {
        return Err(RosterError::Invalid {
            field: "role",
            reason: format!(
                "“{name}” is the project's master session, opened from the Agents panel, not a role"
            ),
        }
        .into());
    }
    if roles.iter().any(|role| role.name == name) {
        return Ok(());
    }
    Err(RosterError::Invalid {
        field: "role",
        reason: format!("there is no role “{name}” for this project"),
    }
    .into())
}

/// A master session of `project_id` on `harness` (`docs/plans/agent-isolation.md`, D4, CP3): Claude Code, Opencode or
/// Glut with its own default command — not an engine of the catalog, whose flags would be the studio's — in the real
/// project folder, playing [`MASTER_ROLE`](crate::ide::model::MASTER_ROLE).
///
/// A project may have several on one engine, as the owner may start the engine twice in a folder outside: the first
/// one whose id is not in `open` (the masters already on screen) is given, and only when every one is open a new one
/// is made — `master-glut`, then `master-glut-2`, `master-glut-3`, … So the first click opens the same session as
/// before, and a second click beside it starts a second one.
///
/// # Errors
///
/// The agent store's refusals; a failing database write. Nothing is created then.
pub fn master_session(
    db: &Connection,
    project_id: i64,
    harness: AgentHarness,
    open: &[i64],
) -> Result<Agent> {
    let base = match harness {
        AgentHarness::ClaudeCode => "master-claude-code",
        AgentHarness::Opencode => "master-opencode",
        AgentHarness::Glut => "master-glut",
    };
    let agents = agent_store::list_agents(db, project_id)?;
    let mut masters: Vec<&Agent> = agents
        .iter()
        .filter(|agent| agent.is_master() && agent.harness == harness)
        .collect();
    masters.sort_by_key(|agent| agent.id);
    if let Some(master) = masters.iter().find(|agent| !open.contains(&agent.id)) {
        return Ok((*master).clone());
    }
    let taken = |name: &str| agents.iter().any(|agent| agent.name == name);
    let name = std::iter::once(base.to_owned())
        .chain((2..).map(|n| format!("{base}-{n}")))
        .find(|name| !taken(name))
        .expect("an endless sequence of names always has a free one");
    let tx = db.unchecked_transaction()?;
    let created = agent_store::create_agent(
        &tx,
        crate::ide::model::NewAgent {
            project_id,
            fields: AgentFields {
                name,
                harness,
                command: String::new(),
                model: None,
                env: String::new(),
            },
        },
    )?;
    agent_store::set_role(&tx, created.id, crate::ide::model::MASTER_ROLE)?;
    tx.commit()?;
    agent_store::get_agent(db, created.id)?.ok_or_else(|| {
        crate::ide::IdeError::CorruptRow {
            table: "ide_agents",
            id: created.id,
            reason: "row vanished after its creation".into(),
        }
        .into()
    })
}

/// Creates an agent session in `project_id` that runs on an engine of the owner's catalog and plays `role`.
///
/// This is the one way the Studio's Agents panel makes an agent: it **chooses** an engine, it does not describe one —
/// engines are made only in the settings. `roles` are the roles in force for the project ([`roles_for_project`]).
///
/// # Errors
///
/// [`RosterError::Invalid`] for an engine or role that does not exist; the agent store's refusals (name clash, bad
/// name); a failing database write. Nothing is created then.
pub fn create_agent_on_engine(
    db: &Connection,
    config: &Config,
    roles: &[Role],
    project_id: i64,
    name: &str,
    engine_id: &str,
    role: &str,
) -> Result<Agent> {
    let engine = known_engine(config, engine_id)?;
    known_role(roles, role)?;
    let tx = db.unchecked_transaction()?;
    let created = agent_store::create_agent(
        &tx,
        crate::ide::model::NewAgent {
            project_id,
            fields: fields_from_engine(&engine, name),
        },
    )?;
    agent_store::set_engine(&tx, created.id, Some(engine_id))?;
    agent_store::set_role(&tx, created.id, role)?;
    tx.commit()?;
    agent_store::get_agent(db, created.id)?.ok_or_else(|| {
        crate::ide::IdeError::CorruptRow {
            table: "ide_agents",
            id: created.id,
            reason: "row vanished after its creation".into(),
        }
        .into()
    })
}

/// Renames an agent session and moves it to another engine and role. `None` if there is no such agent.
///
/// # Errors
///
/// As [`create_agent_on_engine`]; nothing changes then.
pub fn update_agent_on_engine(
    db: &Connection,
    config: &Config,
    roles: &[Role],
    id: i64,
    name: &str,
    engine_id: &str,
    role: &str,
) -> Result<Option<Agent>> {
    // A master is the engine as the owner uses it, not a session on a catalog engine: it is not turned into one.
    if agent_store::get_agent(db, id)?.is_some_and(|agent| agent.is_master()) {
        return Err(RosterError::Invalid {
            field: "role",
            reason: "the project's master session keeps its engine and role".into(),
        }
        .into());
    }
    let engine = known_engine(config, engine_id)?;
    known_role(roles, role)?;
    let tx = db.unchecked_transaction()?;
    if agent_store::update_agent(&tx, id, fields_from_engine(&engine, name))?.is_none() {
        return Ok(None);
    }
    // Changing the profile columns dropped the engine assignment; it is set again, to the engine they came from.
    agent_store::set_engine(&tx, id, Some(engine_id))?;
    agent_store::set_role(&tx, id, role)?;
    tx.commit()?;
    Ok(agent_store::get_agent(db, id)?)
}

/// Removes an own engine from the config file. `false` if there was no such engine.
///
/// The check and the removal happen under the same lock that serialises assignments, so a session cannot be given
/// the engine between the two.
///
/// # Errors
///
/// [`RosterError::Invalid`] while a session or a role still uses it, or for an automatic engine; a failing read or
/// save.
pub fn delete_engine(db: &Connection, config: &mut Config, id: &str) -> Result<bool> {
    let _guard = crate::config::file_edit_lock();
    let mut disk = disk_config()?;
    if !disk.agents.engines.contains_key(id) {
        let auto = crate::engines::is_auto(&disk, &crate::model_tiers::ModelSources::load(), id);
        config.agents.engines = disk.agents.engines;
        if auto {
            return Err(auto_refusal(id));
        }
        return Ok(false);
    }
    let roles = list_roles()?.roles;
    if let Some(user) = engine_user(db, &roles, id)? {
        return Err(RosterError::Invalid {
            field: "id",
            reason: format!("engine “{id}” cannot be removed: {user}"),
        }
        .into());
    }
    disk.agents.engines.remove(id);
    disk.save()?;
    config.agents.engines = disk.agents.engines;
    Ok(true)
}

/// The owner's roles (`~/.axiomata/agents/`), with the reasons for any that were skipped.
///
/// # Errors
///
/// Only a failure listing the directory.
pub fn list_roles() -> Result<Loaded> {
    Ok(axiomata_roster::load_roles(
        &paths::agent_roles_dir(),
        Source::User,
    )?)
}

/// Saves a role of the owner's. Checks that the engines it names exist.
///
/// # Errors
///
/// [`RosterError::Invalid`] for a bad role or an unknown engine.
pub fn save_role(config: &Config, role: Role) -> Result<()> {
    // Reserved (see `known_role`): a role file of that name is refused where it is made, not only where it is used.
    if role.name == crate::ide::model::MASTER_ROLE {
        return Err(RosterError::Invalid {
            field: "name",
            reason: format!(
                "“{}” is reserved for the project's master session",
                role.name
            ),
        }
        .into());
    }
    role.validate()?;
    let engines = crate::engines::Lookup::new(config);
    role.check_engines(|id| engines.exists(id))?;
    axiomata_roster::save_role(&paths::agent_roles_dir(), &role)?;
    Ok(())
}

/// Deletes a role of the owner's unless a session plays it or it is the default role. `false` if there was no such
/// role.
///
/// The default role (`allrounder`) stays: it is what the `agent_role` column of every new session defaults to, and
/// the owner's edits to it are kept (it is only ever seeded when absent).
///
/// # Errors
///
/// [`RosterError::Invalid`] for the default role, or while a session still plays the role.
pub fn delete_role(db: &Connection, name: &str) -> Result<bool> {
    if name == axiomata_roster::default_role().name {
        return Err(RosterError::Invalid {
            field: "name",
            reason: format!("the default role “{name}” cannot be deleted (new sessions start as it); edit it instead"),
        }
        .into());
    }
    let playing = agent_store::count_with_role(db, name)?;
    if playing > 0 {
        return Err(RosterError::Invalid {
            field: "name",
            reason: format!("role “{name}” cannot be deleted: {playing} agent session(s) play it"),
        }
        .into());
    }
    Ok(axiomata_roster::delete_role(
        &paths::agent_roles_dir(),
        name,
    )?)
}

/// What a project brings in roles, and what applies because of it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ProjectRoles {
    /// The project's own files (`<project>/.axiomata/agents/`), shown to the owner before they confirm.
    pub overrides: axiomata_roster::overrides::Overrides,
    /// Whether the owner confirmed exactly these files.
    pub confirmed: bool,
    /// The roles in force for this project: the owner's, with a confirmed project's replacing and adding.
    pub effective: Vec<Role>,
    /// The owner's own roles that could not be read, with the reason.
    pub skipped: Vec<axiomata_roster::Skipped>,
    /// Names of the owner's roles that the project's files would replace — shown before the confirmation, since
    /// replacing `allrounder` changes what every new agent is told.
    pub replaces: Vec<String>,
    /// Engine ids the project's roles name that the owner's catalog does not have. Harmless until sessions start
    /// from roles (CP-A6), but the owner should see a role that points nowhere before agreeing to it.
    pub unknown_engines: Vec<String>,
}

/// The roles in force for `project`.
///
/// # Errors
///
/// A failure listing one of the directories.
pub fn project_roles(project: &std::path::Path, config: &Config) -> Result<ProjectRoles> {
    let user = list_roles()?;
    let overrides = axiomata_roster::overrides::scan_project(project)?;
    let store = axiomata_tasks::trust::TrustStore::load(&paths::agent_roles_trust_path());
    let confirmed = axiomata_roster::overrides::is_confirmed(&store, &overrides);

    let replaces = overrides
        .roles
        .iter()
        .filter(|brought| user.roles.iter().any(|mine| mine.name == brought.name))
        .map(|role| role.name.clone())
        .collect();
    let engines = crate::engines::Lookup::new(config);
    let mut unknown_engines: Vec<String> = overrides
        .roles
        .iter()
        .flat_map(|role| role.engine.iter().chain(role.fallback_engines.iter()))
        .filter(|id| !engines.exists(id))
        .cloned()
        .collect();
    unknown_engines.sort();
    unknown_engines.dedup();

    let effective = axiomata_roster::overrides::effective(user.roles, &overrides, confirmed);
    Ok(ProjectRoles {
        overrides,
        confirmed,
        effective,
        skipped: user.skipped,
        replaces,
        unknown_engines,
    })
}

/// Records the owner's confirmation of the project's role files as shown (`shown_hash`).
///
/// # Errors
///
/// [`RosterError::Changed`] if the files differ from what was shown; a failing write.
pub fn confirm_project_roles(project: &std::path::Path, shown_hash: &str) -> Result<()> {
    let mut store = axiomata_tasks::trust::TrustStore::load(&paths::agent_roles_trust_path());
    axiomata_roster::overrides::confirm(&mut store, project, shown_hash)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::db;
    use crate::ide::model::{AgentFields, NewAgent, NewProject};
    use crate::ide::store;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};

    static N: AtomicU32 = AtomicU32::new(0);

    fn fixture() -> (Connection, i64, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-core-roster-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let conn = db::open_and_migrate_at(&dir.join("test.db")).unwrap();
        let project = store::create_project(
            &conn,
            NewProject {
                name: "P".into(),
                repo_root: dir.clone(),
            },
        )
        .unwrap();
        (conn, project.id, dir)
    }

    #[test]
    fn a_project_has_one_master_per_engine_and_no_role_can_pretend_to_be_it() {
        let (db, project, dir) = fixture();
        let claude = master_session(&db, project, AgentHarness::ClaudeCode, &[]).unwrap();
        let opencode = master_session(&db, project, AgentHarness::Opencode, &[]).unwrap();
        assert!(claude.is_master() && opencode.is_master());
        assert_ne!(claude.id, opencode.id);
        assert!(
            claude.command.is_empty(),
            "the engine's own default command, no catalog flags"
        );
        assert_eq!(
            master_session(&db, project, AgentHarness::ClaudeCode, &[])
                .unwrap()
                .id,
            claude.id,
            "asked again: the same"
        );
        let glut = master_session(&db, project, AgentHarness::Glut, &[]).unwrap();
        assert!(glut.is_master() && glut.id != claude.id && glut.id != opencode.id);
        assert!(known_role(&[], crate::ide::model::MASTER_ROLE).is_err());
        let edited = update_agent_on_engine(
            &db,
            &Config::default(),
            &[],
            claude.id,
            "x",
            "e",
            "allrounder",
        );
        assert!(
            edited.unwrap_err().to_string().contains("master"),
            "a master is not turned into a session"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_second_master_on_one_engine_is_made_only_while_the_first_is_open() {
        let (db, project, dir) = fixture();
        let first = master_session(&db, project, AgentHarness::Glut, &[]).unwrap();
        let second = master_session(&db, project, AgentHarness::Glut, &[first.id]).unwrap();
        assert_ne!(second.id, first.id);
        assert_eq!(
            (first.name.as_str(), second.name.as_str()),
            ("master-glut", "master-glut-2")
        );
        assert!(second.is_master() && second.harness == AgentHarness::Glut);
        let third =
            master_session(&db, project, AgentHarness::Glut, &[first.id, second.id]).unwrap();
        assert_eq!(third.name, "master-glut-3");
        assert_eq!(
            master_session(&db, project, AgentHarness::Glut, &[first.id])
                .unwrap()
                .id,
            second.id,
            "a closed master is opened again before a new one is made"
        );
        assert_eq!(
            master_session(
                &db,
                project,
                AgentHarness::ClaudeCode,
                &[first.id, second.id]
            )
            .unwrap()
            .name,
            "master-claude-code",
            "the other engines count apart"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn add(
        db: &Connection,
        project: i64,
        name: &str,
        harness: Harness,
        model: Option<&str>,
        command: &str,
    ) -> Agent {
        agent_store::create_agent(
            db,
            NewAgent {
                project_id: project,
                fields: AgentFields {
                    name: name.into(),
                    harness,
                    command: command.into(),
                    model: model.map(String::from),
                    env: String::new(),
                },
            },
        )
        .unwrap()
    }

    #[test]
    fn equal_profiles_share_an_engine_and_different_ones_get_their_own() {
        let (conn, project, _dir) = fixture();
        add(
            &conn,
            project,
            "a",
            Harness::ClaudeCode,
            Some("claude-opus-5-5"),
            "",
        );
        add(
            &conn,
            project,
            "b",
            Harness::ClaudeCode,
            Some("claude-opus-5-5"),
            "",
        );
        add(
            &conn,
            project,
            "c",
            Harness::Glut,
            Some("openrouter/deepseek/v4"),
            "",
        );
        add(&conn, project, "d", Harness::Glut, None, "opencode --foo");

        let agents = agent_store::unassigned_agents(&conn).unwrap();
        let mut engines = BTreeMap::new();
        let assigned = derive_engines(&agents, &mut engines);

        assert_eq!(engines.len(), 3);
        assert_eq!(assigned[0].1, assigned[1].1, "same profile, same engine");
        assert_ne!(assigned[0].1, assigned[2].1);
        assert_eq!(assigned[0].1, "claude-code-claude-opus-5-5");
        assert_eq!(assigned[2].1, "glut-openrouter-deepseek-v4");
        assert_eq!(engines[&assigned[0].1].billing, Billing::Subscription);
        assert_eq!(engines[&assigned[2].1].billing, Billing::Metered);
        assert!(engines[&assigned[3].1].label.ends_with("own command"));
        for engine in engines.values() {
            engine.validate().unwrap();
        }
    }

    #[test]
    fn deriving_twice_changes_nothing_and_a_clashing_id_gets_a_suffix() {
        let (conn, project, _dir) = fixture();
        let a = add(&conn, project, "a", Harness::Glut, None, "");
        let b = add(&conn, project, "b", Harness::Glut, None, "glut --x");
        let mut engines = BTreeMap::new();
        let first = derive_engines(&[a.clone(), b.clone()], &mut engines);
        assert_eq!(first[0].1, "glut");
        assert_eq!(first[1].1, "glut-2");
        let again = derive_engines(&[a, b], &mut engines);
        assert_eq!(again, first);
        assert_eq!(engines.len(), 2);
    }

    #[test]
    fn a_very_long_model_name_still_makes_a_valid_id() {
        let (conn, project, _dir) = fixture();
        let model = "provider/".to_string() + &"very-long-model-name-".repeat(6);
        let agent = add(&conn, project, "a", Harness::Glut, Some(&model), "");
        let mut engines = BTreeMap::new();
        derive_engines(&[agent], &mut engines);
        let engine = engines.values().next().unwrap();
        engine.validate().unwrap();
        assert!(engine.id.len() <= MAX_IDENT_LEN);
    }

    #[test]
    fn an_engine_in_use_by_a_session_cannot_be_deleted() {
        let (conn, project, _dir) = fixture();
        let agent = add(&conn, project, "a", Harness::Glut, None, "");
        agent_store::set_engine(&conn, agent.id, Some("glut")).unwrap();
        let user = engine_user(&conn, &[], "glut").unwrap();
        assert!(user.unwrap().contains("session"));
        assert!(engine_user(&conn, &[], "unused").unwrap().is_none());
    }

    #[test]
    fn an_engine_named_by_a_role_cannot_be_deleted() {
        let (conn, _project, _dir) = fixture();
        let mut role = axiomata_roster::default_role();
        role.fallback_engines = vec!["spare".into()];
        let user = engine_user(&conn, &[role], "spare").unwrap();
        assert!(user.unwrap().contains("allrounder"));
    }

    #[test]
    fn slugs_drop_everything_but_letters_and_digits() {
        assert_eq!(
            slugify("openrouter/deepseek/deepseek-v4-flash-0731"),
            "openrouter-deepseek-deepseek-v4-flash-0731"
        );
        assert_eq!(slugify("  --Weird__Name!!  "), "weird-name");
        assert_eq!(slugify("///"), "");
    }

    /// Runs `body` with `AXIOMATA_HOME` pointing at a fresh directory.
    fn with_temp_home(body: impl FnOnce(&std::path::Path)) {
        let _guard = ENV_MUTEX.lock().unwrap();
        let home = unique_temp_dir("axiomata-test-roster");
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            std::env::set_var(paths::AXIOMATA_HOME_ENV, &home);
        }
        body(&home);
        unsafe {
            std::env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    fn engine(id: &str) -> Engine {
        Engine {
            id: id.into(),
            label: id.into(),
            harness: Harness::Glut,
            command: String::new(),
            model: None,
            env: String::new(),
            billing: Billing::Metered,
            tier: None,
        }
    }

    #[test]
    fn saving_an_engine_keeps_a_workspace_change_that_waits_for_the_next_start() {
        with_temp_home(|home| {
            // On disk: the owner already switched the workspace; live: this session still runs on the old root.
            let queued = Config {
                workspace_root: home.join("QueuedWorkspace"),
                ..Config::default()
            };
            queued.save().unwrap();
            let mut live = Config {
                workspace_root: home.join("LiveWorkspace"),
                ..Config::default()
            };

            save_engine(&mut live, engine("spare")).unwrap();

            let on_disk = Config::load().unwrap();
            assert_eq!(on_disk.workspace_root, home.join("QueuedWorkspace"));
            assert!(on_disk.agents.engines.contains_key("spare"));
            assert!(live.agents.engines.contains_key("spare"));
        });
    }

    #[test]
    fn an_invalid_engine_is_refused_before_anything_is_written() {
        with_temp_home(|_| {
            let mut live = Config::default();
            let mut bad = engine("spare");
            bad.label = String::new();
            assert!(matches!(
                save_engine(&mut live, bad),
                Err(AxiomataError::Roster(_))
            ));
            assert!(live.agents.engines.is_empty());
            assert!(!paths::config_path().exists());
        });
    }

    /// The automatic engine of Claude Code on Sonnet 5.5, which the built-in Anthropic list always yields.
    const AUTO_SONNET: &str = "claude-code-claude-sonnet-5-5";

    #[test]
    fn an_automatic_engine_is_read_only_but_starts_sessions_like_an_own_one() {
        with_temp_home(|home| {
            let mut live = Config {
                workspace_root: home.join("workspace"),
                ..Config::default()
            };
            live.save().unwrap();
            let mut taken = engine(AUTO_SONNET);
            taken.harness = Harness::ClaudeCode;
            let refused = save_engine(&mut live, taken).unwrap_err().to_string();
            assert!(refused.contains("automatic engine"), "{refused}");
            let (conn, project, _dir) = fixture();
            let refused = delete_engine(&conn, &mut live, AUTO_SONNET)
                .unwrap_err()
                .to_string();
            assert!(refused.contains("automatic engine"), "{refused}");
            assert!(!delete_engine(&conn, &mut live, "never-was").unwrap());

            let roles = vec![axiomata_roster::default_role()];
            let agent = create_agent_on_engine(
                &conn,
                &live,
                &roles,
                project,
                "a",
                AUTO_SONNET,
                "allrounder",
            )
            .unwrap();
            assert_eq!(agent.engine_id.as_deref(), Some(AUTO_SONNET));
            assert_eq!(agent.harness, Harness::ClaudeCode);
            assert_eq!(agent.model.as_deref(), Some("claude-sonnet-5-5"));
            let overview =
                engine_overview(&conn, &live, &crate::model_tiers::ModelSources::load()).unwrap();
            let entry = overview
                .iter()
                .find(|e| e.info.engine.id == AUTO_SONNET)
                .unwrap();
            assert_eq!(
                (entry.sessions, entry.info.origin),
                (1, crate::engines::EngineOrigin::Auto)
            );

            // The card start path resolves it too.
            let mut role = axiomata_roster::default_role();
            role.engine = Some(AUTO_SONNET.into());
            let (id, chosen) = crate::card_session::choose_engine(&live, &role, None).unwrap();
            assert_eq!(
                (id.as_str(), chosen.harness),
                (AUTO_SONNET, Harness::ClaudeCode)
            );
            assert!(crate::card_session::choose_engine(&live, &role, Some("nowhere")).is_err());
            // And a role may name it.
            assert!(
                role.check_engines(|id| crate::engines::Lookup::new(&live).exists(id))
                    .is_ok()
            );
        });
    }

    #[test]
    fn deleting_a_used_engine_is_refused_and_an_unused_one_is_removed() {
        with_temp_home(|_| {
            let (conn, project, _dir) = fixture();
            let mut live = Config::default();
            save_engine(&mut live, engine("used")).unwrap();
            save_engine(&mut live, engine("free")).unwrap();
            let agent = add(&conn, project, "a", Harness::Glut, None, "");
            agent_store::set_engine(&conn, agent.id, Some("used")).unwrap();

            assert!(matches!(
                delete_engine(&conn, &mut live, "used"),
                Err(AxiomataError::Roster(_))
            ));
            assert!(live.agents.engines.contains_key("used"));
            assert!(delete_engine(&conn, &mut live, "free").unwrap());
            assert!(!delete_engine(&conn, &mut live, "free").unwrap());
            assert!(!Config::load().unwrap().agents.engines.contains_key("free"));
        });
    }

    #[test]
    fn syncing_assigns_engines_persists_them_and_updates_the_live_config() {
        with_temp_home(|_| {
            let (conn, project, _dir) = fixture();
            add(&conn, project, "a", Harness::ClaudeCode, None, "");
            let live = RwLock::new(Config::default());

            assert_eq!(sync_live(&conn, &live).unwrap(), 1);
            assert!(
                live.read()
                    .unwrap()
                    .agents
                    .engines
                    .contains_key("claude-code")
            );
            assert!(
                Config::load()
                    .unwrap()
                    .agents
                    .engines
                    .contains_key("claude-code")
            );
            assert!(agent_store::unassigned_agents(&conn).unwrap().is_empty());
            assert_eq!(
                sync_live(&conn, &live).unwrap(),
                0,
                "nothing left to assign"
            );
        });
    }

    #[test]
    fn a_role_is_saved_only_with_engines_that_exist_and_deleted_only_when_nobody_plays_it() {
        with_temp_home(|_| {
            let (conn, project, _dir) = fixture();
            let mut config = Config::default();
            let mut role = axiomata_roster::default_role();
            role.name = "reviewer".into();
            role.engine = Some("missing".into());
            assert!(matches!(
                save_role(&config, role.clone()),
                Err(AxiomataError::Roster(_))
            ));

            save_engine(&mut config, engine("missing")).unwrap();
            save_role(&config, role).unwrap();
            assert_eq!(list_roles().unwrap().roles.len(), 1);

            let agent = add(&conn, project, "a", Harness::Glut, None, "");
            agent_store::set_role(&conn, agent.id, "reviewer").unwrap();
            assert!(matches!(
                delete_role(&conn, "reviewer"),
                Err(AxiomataError::Roster(_))
            ));
            agent_store::set_role(&conn, agent.id, "allrounder").unwrap();
            assert!(delete_role(&conn, "reviewer").unwrap());
            assert!(list_roles().unwrap().roles.is_empty());
        });
    }

    #[test]
    fn a_project_role_applies_only_after_the_owner_confirmed_what_was_shown() {
        with_temp_home(|home| {
            let project = home.join("project");
            let mut role = axiomata_roster::default_role();
            role.description = "the repository's version".into();
            axiomata_roster::save_role(&project.join(".axiomata/agents"), &role).unwrap();
            axiomata_roster::save_role(&paths::agent_roles_dir(), &axiomata_roster::default_role())
                .unwrap();

            let before = project_roles(&project, &Config::default()).unwrap();
            assert!(before.overrides.present && !before.confirmed);
            assert_eq!(
                before.effective[0].description,
                axiomata_roster::default_role().description
            );

            confirm_project_roles(&project, &before.overrides.hash).unwrap();
            let after = project_roles(&project, &Config::default()).unwrap();
            assert!(after.confirmed);
            assert_eq!(after.effective[0].description, "the repository's version");
            assert_eq!(after.effective[0].source, Source::Project);

            assert!(matches!(
                confirm_project_roles(&project, "not the hash that was shown"),
                Err(AxiomataError::Roster(RosterError::Changed))
            ));
        });
    }

    #[test]
    fn a_profile_that_cannot_make_a_valid_engine_stays_unassigned_and_never_poisons_the_config() {
        with_temp_home(|_| {
            let (conn, project, _dir) = fixture();
            // Agent profiles accept any text as `env`; an engine does not.
            let loose = agent_store::create_agent(
                &conn,
                NewAgent {
                    project_id: project,
                    fields: AgentFields {
                        name: "loose".into(),
                        harness: Harness::Glut,
                        command: String::new(),
                        model: None,
                        env: "export FOO=1\n# a note".into(),
                    },
                },
            )
            .unwrap();
            add(&conn, project, "fine", Harness::ClaudeCode, None, "");

            let mut live = Config::default();
            assert_eq!(
                sync_agents(&conn, &mut live).unwrap(),
                1,
                "only the valid one is assigned"
            );

            assert!(
                live.validate_for_save().is_ok(),
                "the Settings dialog must still be able to save"
            );
            assert!(Config::load().unwrap().validate_for_save().is_ok());
            let unassigned = agent_store::unassigned_agents(&conn).unwrap();
            assert_eq!(unassigned.len(), 1);
            assert_eq!(unassigned[0].id, loose.id);
        });
    }

    #[test]
    fn a_failing_save_leaves_the_callers_config_and_the_rows_alone() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let base = unique_temp_dir("axiomata-test-roster-unwritable");
        std::fs::create_dir_all(&base).unwrap();
        // The "home" is below a regular file, so the config can be neither read from nor created.
        let blocker = base.join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        // SAFETY: serialized by `_guard`, see `paths::tests`.
        unsafe {
            std::env::set_var(paths::AXIOMATA_HOME_ENV, blocker.join("home"));
        }
        let (conn, project, _dir) = fixture();
        add(&conn, project, "a", Harness::ClaudeCode, None, "");
        let mut live = Config::default();

        assert!(sync_agents(&conn, &mut live).is_err());

        assert!(
            live.agents.engines.is_empty(),
            "the live catalog must not claim engines that were never saved"
        );
        assert_eq!(
            agent_store::unassigned_agents(&conn).unwrap().len(),
            1,
            "no row may point at an unsaved engine"
        );
        unsafe {
            std::env::remove_var(paths::AXIOMATA_HOME_ENV);
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn editing_a_profile_gives_the_agent_the_engine_of_the_new_profile() {
        with_temp_home(|_| {
            let (conn, project, _dir) = fixture();
            let agent = add(&conn, project, "a", Harness::Glut, Some("x/one"), "");
            let live = RwLock::new(Config::default());
            sync_live(&conn, &live).unwrap();
            let first = agent_store::get_agent(&conn, agent.id)
                .unwrap()
                .unwrap()
                .engine_id
                .unwrap();

            agent_store::update_agent(
                &conn,
                agent.id,
                AgentFields {
                    name: "a".into(),
                    harness: Harness::Glut,
                    command: String::new(),
                    model: Some("x/two".into()),
                    env: String::new(),
                },
            )
            .unwrap();
            assert_eq!(sync_live(&conn, &live).unwrap(), 1);

            let second = agent_store::get_agent(&conn, agent.id)
                .unwrap()
                .unwrap()
                .engine_id
                .unwrap();
            assert_ne!(first, second);
            let catalog = live.read().unwrap();
            assert_eq!(
                catalog.agents.engines[&second].model.as_deref(),
                Some("x/two")
            );
            assert!(
                catalog.agents.engines.contains_key(&first),
                "the old engine is not removed behind the owner's back"
            );
        });
    }

    #[test]
    fn a_change_made_by_another_process_is_merged_not_overwritten() {
        with_temp_home(|_| {
            let (conn, project, _dir) = fixture();
            let mut live = Config::default();
            // The CLI adds an engine to the file while this process still holds a catalog without it.
            let mut elsewhere = Config::default();
            elsewhere
                .agents
                .engines
                .insert("from-cli".into(), engine("from-cli"));
            elsewhere.save().unwrap();

            save_engine(&mut live, engine("mine")).unwrap();
            add(&conn, project, "a", Harness::ClaudeCode, None, "");
            sync_agents(&conn, &mut live).unwrap();

            let on_disk = Config::load().unwrap().agents.engines;
            assert!(
                on_disk.contains_key("from-cli"),
                "the other process's engine survived"
            );
            assert!(on_disk.contains_key("mine") && on_disk.contains_key("claude-code"));
            assert_eq!(
                live.agents.engines, on_disk,
                "the live catalog adopts the file's"
            );
        });
    }

    #[test]
    fn an_engine_a_role_names_cannot_be_deleted() {
        with_temp_home(|_| {
            let (conn, _project, _dir) = fixture();
            let mut config = Config::default();
            save_engine(&mut config, engine("named")).unwrap();
            let mut role = axiomata_roster::default_role();
            role.name = "reviewer".into();
            role.engine = Some("named".into());
            save_role(&config, role).unwrap();

            assert!(matches!(
                delete_engine(&conn, &mut config, "named"),
                Err(AxiomataError::Roster(_))
            ));
            assert!(Config::load().unwrap().agents.engines.contains_key("named"));
        });
    }

    #[test]
    fn the_default_role_cannot_be_deleted() {
        with_temp_home(|_| {
            let (conn, _project, _dir) = fixture();
            axiomata_roster::seed_default_roles(&paths::agent_roles_dir()).unwrap();
            assert!(matches!(
                delete_role(&conn, "allrounder"),
                Err(AxiomataError::Roster(_))
            ));
            // The reviewer (CP-A6b) and the planner (CP-A7) are seeded beside it; like the bundled skills, a missing
            // one
            // is seeded again at the next start.
            let roles = list_roles().unwrap().roles;
            assert_eq!(
                roles.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
                [
                    "allrounder",
                    "documenter",
                    "grill",
                    "implementer-heavy",
                    "implementer-light",
                    "planner",
                    "reviewer",
                    "reviewer-heavy",
                    "reviewer-light",
                    "tester"
                ]
            );
        });
    }

    #[test]
    fn the_confirmation_screen_is_told_what_a_project_replaces_and_which_engines_do_not_exist() {
        with_temp_home(|home| {
            let project = home.join("project");
            let mut brought = axiomata_roster::default_role();
            brought.engine = Some("ghost".into());
            brought.fallback_engines = vec!["real".into(), "other-ghost".into()];
            axiomata_roster::save_role(&project.join(".axiomata/agents"), &brought).unwrap();
            let mut extra = axiomata_roster::default_role();
            extra.name = "brand-new".into();
            axiomata_roster::save_role(&project.join(".axiomata/agents"), &extra).unwrap();
            axiomata_roster::save_role(&paths::agent_roles_dir(), &axiomata_roster::default_role())
                .unwrap();
            let mut config = Config::default();
            config.agents.engines.insert("real".into(), engine("real"));

            let found = project_roles(&project, &config).unwrap();

            assert_eq!(
                found.replaces,
                ["allrounder"],
                "brand-new replaces nothing of the owner's"
            );
            assert_eq!(found.unknown_engines, ["ghost", "other-ghost"]);
        });
    }

    fn engine_on(id: &str, harness: Harness, model: Option<&str>) -> Engine {
        Engine {
            id: id.into(),
            label: id.into(),
            harness,
            command: String::new(),
            model: model.map(String::from),
            env: "A=1".into(),
            billing: axiomata_roster::Billing::Metered,
            tier: None,
        }
    }

    fn plain_role(name: &str) -> Role {
        Role {
            name: name.into(),
            description: String::new(),
            kind: "implement".into(),
            tier: axiomata_roster::Tier::Medium,
            engine: None,
            fallback_engines: vec![],
            permissions: vec![],
            limits: axiomata_roster::Limits::default(),
            creates: vec![],
            instructions: String::new(),
            source: Source::User,
        }
    }

    fn catalog() -> (Config, Vec<Role>) {
        let mut config = Config::default();
        for e in [
            engine_on("opus", Harness::ClaudeCode, Some("opus")),
            engine_on("flash", Harness::Glut, Some("openrouter/x/flash")),
        ] {
            config.agents.engines.insert(e.id.clone(), e);
        }
        (
            config,
            vec![plain_role("allrounder"), plain_role("reviewer")],
        )
    }

    #[test]
    fn an_agent_made_on_an_engine_runs_what_the_engine_says() {
        let (conn, project, _dir) = fixture();
        let (config, roles) = catalog();
        let agent = create_agent_on_engine(
            &conn, &config, &roles, project, "builder", "opus", "reviewer",
        )
        .unwrap();
        assert_eq!(agent.engine_id.as_deref(), Some("opus"));
        assert_eq!(agent.agent_role, "reviewer");
        assert_eq!(
            (agent.harness, agent.model.as_deref(), agent.env.as_str()),
            (Harness::ClaudeCode, Some("opus"), "A=1")
        );
        // The engine is already assigned: nothing is left for the profile-derived path to invent.
        assert!(agent_store::unassigned_agents(&conn).unwrap().is_empty());
    }

    #[test]
    fn an_unknown_engine_or_role_creates_nothing() {
        let (conn, project, _dir) = fixture();
        let (config, roles) = catalog();
        for (engine_id, role_name) in [("nope", "allrounder"), ("opus", "nope")] {
            let err =
                create_agent_on_engine(&conn, &config, &roles, project, "x", engine_id, role_name)
                    .unwrap_err();
            assert!(
                matches!(err, AxiomataError::Roster(RosterError::Invalid { .. })),
                "{err:?}"
            );
        }
        assert!(agent_store::list_agents(&conn, project).unwrap().is_empty());
    }

    #[test]
    fn a_name_clash_leaves_no_half_made_agent() {
        let (conn, project, _dir) = fixture();
        let (config, roles) = catalog();
        create_agent_on_engine(&conn, &config, &roles, project, "dup", "opus", "allrounder")
            .unwrap();
        assert!(
            create_agent_on_engine(
                &conn,
                &config,
                &roles,
                project,
                "DUP",
                "flash",
                "allrounder"
            )
            .is_err()
        );
        assert_eq!(agent_store::list_agents(&conn, project).unwrap().len(), 1);
    }

    #[test]
    fn editing_an_agent_moves_it_to_another_engine_and_keeps_the_assignment() {
        let (conn, project, _dir) = fixture();
        let (config, roles) = catalog();
        let agent = create_agent_on_engine(
            &conn,
            &config,
            &roles,
            project,
            "builder",
            "opus",
            "allrounder",
        )
        .unwrap();
        let moved = update_agent_on_engine(
            &conn, &config, &roles, agent.id, "renamed", "flash", "reviewer",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (
                moved.name.as_str(),
                moved.engine_id.as_deref(),
                moved.harness,
                moved.agent_role.as_str()
            ),
            ("renamed", Some("flash"), Harness::Glut, "reviewer")
        );
        assert_eq!(moved.model.as_deref(), Some("openrouter/x/flash"));
        assert!(
            update_agent_on_engine(&conn, &config, &roles, 9999, "x", "opus", "allrounder")
                .unwrap()
                .is_none()
        );
        // A refused edit changes nothing.
        assert!(
            update_agent_on_engine(&conn, &config, &roles, agent.id, "x", "nope", "allrounder")
                .is_err()
        );
        assert_eq!(
            agent_store::get_agent(&conn, agent.id)
                .unwrap()
                .unwrap()
                .name,
            "renamed"
        );
    }
}
