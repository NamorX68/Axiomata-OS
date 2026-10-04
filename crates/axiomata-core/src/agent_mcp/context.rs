//! Who the MCP server speaks for, and what that session may do.
//!
//! The identity comes from the environment the studio started the harness with (`AXIOMATA_AGENT_ID`, and for a session
//! started for one card `AXIOMATA_CARD_ID`, for a planner `AXIOMATA_PLAN_ID`) and is **never taken from a tool
//! argument**: a model that could name its own sender, card or role could lift every limit by asking. The sender of a
//! message is stamped here, the card a message is about is the session's own card, and the tools offered are the ones
//! the session's role allows (`docs/plans/a2a.md` A39, A28, and the duties the CP-A3 review left for this checkpoint).
//!
//! Like [`crate::session`] this guards against mistakes and against agents that follow their instructions, not
//! against another process of the same user, who could start a server with any id or write the database directly.

use std::sync::{Arc, Mutex, MutexGuard, RwLock};

use axiomata_ide::lifecycle::ChannelRoots;
use axiomata_ide::mailbox::Limits;
use axiomata_ide::model::Agent;
use axiomata_roster::Role;
use rusqlite::Connection;

use crate::AxiomataCore;
use crate::config::Config;
use crate::ide::agent_store;
use crate::session::actor_from;

/// The role kind of a reviewer: the only one that may judge cards.
pub const KIND_REVIEW: &str = "review";
/// The role kind of a planner: the only one that may propose cards of any kind.
pub const KIND_PLAN: &str = "plan";

/// What a role may propose with `create_card`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Creates {
    /// Nothing: the tool is not offered.
    Nothing,
    /// Cards of these kinds only (`creates:` in the role file, A7).
    Kinds(Vec<String>),
    /// Any kind — the planner.
    Any,
}

/// The tools a role gets. Everybody can read and write mail and read the board; the rest follows the role's kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    /// `claim_task` and `report_done`: a session that does the work.
    pub work: bool,
    /// `review_verdict`: a session that judges the work of others.
    pub review: bool,
    pub create: Creates,
}

impl Capabilities {
    /// Least privilege for a session whose role file is missing or unreadable: it can talk and read, nothing else.
    pub const NONE: Capabilities = Capabilities {
        work: false,
        review: false,
        create: Creates::Nothing,
    };

    /// Derives the tools from a role.
    pub fn of(role: &Role) -> Self {
        let kind = role.kind.as_str();
        Capabilities {
            work: kind != KIND_REVIEW && kind != KIND_PLAN,
            review: kind == KIND_REVIEW,
            create: if kind == KIND_PLAN {
                Creates::Any
            } else if role.creates.is_empty() {
                Creates::Nothing
            } else {
                Creates::Kinds(role.creates.clone())
            },
        }
    }
}

/// Why a server could not be set up for a session.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error(
        "AXIOMATA_AGENT_ID is not set to a session id — this server is started by the studio for one session"
    )]
    NoIdentity,
    #[error("there is no agent session {0}")]
    UnknownSession(i64),
    #[error("database error: {0}")]
    Database(String),
}

/// Everything a tool call needs: the database, the session and what it may do.
pub struct Context {
    db: Arc<Mutex<Connection>>,
    config: Arc<RwLock<Config>>,
    pub roots: ChannelRoots,
    pub agent: Agent,
    /// The actor string the board knows this session by (`agent:<name>-<id>`).
    pub actor: String,
    pub caps: Capabilities,
    /// The card the studio started this session for, if it did.
    pub card_env: Option<i64>,
    /// The plan a planner session works on, if the studio said so.
    pub plan_env: Option<i64>,
    pub limits: Limits,
}

/// The roles in force for the project of session `agent_id`; the owner's catalog alone when the project cannot be
/// resolved, and nothing (so no rights) when even that cannot be read.
fn effective_roles(core: &AxiomataCore, agent_id: i64) -> Vec<Role> {
    let root = {
        let db = core.db_lock();
        agent_store::get_agent(&db, agent_id)
            .ok()
            .flatten()
            .and_then(|agent| {
                crate::ide::store::get_project(&db, agent.project_id)
                    .ok()
                    .flatten()
            })
            .map(|project| project.repo_root)
    };
    let config = core
        .config
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    let project = root.and_then(|root| crate::roster::project_roles(&root, &config).ok());
    match project {
        Some(project) => project.effective,
        None => crate::roster::list_roles()
            .map(|loaded| loaded.roles)
            .unwrap_or_default(),
    }
}

fn parse_id(raw: Option<&str>) -> Option<i64> {
    let raw = raw?.trim();
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok().filter(|id| *id > 0)
}

impl Context {
    /// Builds the context of session `agent_id`.
    ///
    /// `roles` is the owner's role catalog; a session whose role is not in it gets [`Capabilities::NONE`].
    ///
    /// # Errors
    ///
    /// [`ContextError::UnknownSession`] when no such agent row exists, [`ContextError::Database`] otherwise.
    pub fn new(
        core: &AxiomataCore,
        roots: ChannelRoots,
        roles: &[Role],
        agent_id: i64,
        card_env: Option<i64>,
        plan_env: Option<i64>,
    ) -> Result<Self, ContextError> {
        let agent = {
            let db = core.db_lock();
            agent_store::get_agent(&db, agent_id)
                .map_err(|err| ContextError::Database(err.to_string()))?
                .ok_or(ContextError::UnknownSession(agent_id))?
        };
        let actor = actor_from(Some(&agent_id.to_string()), Some(&agent.name))
            .ok_or(ContextError::UnknownSession(agent_id))?;
        let caps = roles
            .iter()
            .find(|role| role.name == agent.agent_role)
            .map_or(Capabilities::NONE, Capabilities::of);
        Ok(Context {
            db: Arc::clone(&core.db),
            config: Arc::clone(&core.config),
            roots,
            agent,
            actor,
            caps,
            card_env,
            plan_env,
            limits: Limits::default(),
        })
    }

    /// [`Context::new`] with the identity read from the environment and the roles in force for the session's project:
    /// the owner's, with the project's own files replacing and adding once the owner confirmed them
    /// ([`crate::roster::project_roles`]) — the same roles the session was told it plays.
    ///
    /// # Errors
    ///
    /// [`ContextError::NoIdentity`] when `AXIOMATA_AGENT_ID` is missing or not a number, otherwise as [`Context::new`].
    pub fn from_env(core: &AxiomataCore, roots: ChannelRoots) -> Result<Self, ContextError> {
        let var = |name: &str| std::env::var(name).ok();
        let agent_id =
            parse_id(var("AXIOMATA_AGENT_ID").as_deref()).ok_or(ContextError::NoIdentity)?;
        let roles = effective_roles(core, agent_id);
        Self::new(
            core,
            roots,
            &roles,
            agent_id,
            parse_id(var("AXIOMATA_CARD_ID").as_deref()),
            parse_id(var("AXIOMATA_PLAN_ID").as_deref()),
        )
    }

    /// Locks the database, recovering a poisoned guard like [`AxiomataCore::db_lock`].
    pub fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    /// A snapshot of the live config, for the board mirror.
    pub fn config(&self) -> Config {
        self.config
            .read()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn role(kind: &str, creates: &[&str]) -> Role {
        Role {
            name: "r".into(),
            description: String::new(),
            kind: kind.into(),
            tier: axiomata_roster::Tier::Medium,
            engine: None,
            fallback_engines: vec![],
            permissions: vec![],
            limits: axiomata_roster::Limits::default(),
            creates: creates.iter().map(|s| (*s).to_owned()).collect(),
            instructions: String::new(),
            source: axiomata_roster::Source::User,
        }
    }

    #[test]
    fn a_role_kind_decides_the_tools() {
        let worker = Capabilities::of(&role("implement", &[]));
        assert_eq!(
            (worker.work, worker.review, &worker.create),
            (true, false, &Creates::Nothing)
        );
        let reviewer = Capabilities::of(&role("review", &["test"]));
        assert_eq!((reviewer.work, reviewer.review), (false, true));
        assert_eq!(reviewer.create, Creates::Kinds(vec!["test".into()]));
        let planner = Capabilities::of(&role("plan", &[]));
        assert_eq!(
            (planner.work, planner.review, &planner.create),
            (false, false, &Creates::Any)
        );
        let tester = Capabilities::of(&role("implement", &["test", "doc"]));
        assert!(tester.work);
        assert_eq!(
            tester.create,
            Creates::Kinds(vec!["test".into(), "doc".into()])
        );
    }

    #[test]
    fn ids_from_the_environment_are_plain_positive_numbers() {
        assert_eq!(parse_id(Some("12")), Some(12));
        assert_eq!(parse_id(Some(" 3 ")), Some(3));
        for bad in [
            None,
            Some(""),
            Some("0"),
            Some("-1"),
            Some("+4"),
            Some("x"),
            Some("1 2"),
        ] {
            assert_eq!(parse_id(bad), None, "{bad:?}");
        }
    }
}
