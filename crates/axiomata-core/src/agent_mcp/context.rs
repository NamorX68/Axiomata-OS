//! Who the MCP server speaks for, and what that session may do.
//!
//! The identity comes from the environment the studio started the harness with (`AXIOMATA_AGENT_ID`, and for a session
//! started for one card `AXIOMATA_CARD_ID`, for a planner `AXIOMATA_PLAN_ID`) and is **never taken from a tool
//! argument**: a model that could name its own sender, card or role could lift every limit by asking. The sender of a
//! message is stamped here, the card a message is about is the session's own card, and the tools offered are the ones
//! the session's role allows (`docs/plans/a2a.md` A39, A28, and the duties the CP-A3 review left for this checkpoint).
//!
//! The id alone is not believed: the studio also hands the server a **secret issued at every start**
//! (`AXIOMATA_AGENT_TOKEN`, in the session's MCP configuration and not in the environment the agent's own shell
//! inherits), and a server that cannot present it refuses to start ([`axiomata_ide::session_token`]). So an agent
//! cannot become another session by exporting that session's id. Still no sandbox: a process of the same user can read the
//! configuration the secret sits in, or write the database directly.

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
/// The role kind of a session that grills a plan's goal: it reads the plan and may propose a sharper goal, nothing else.
pub const KIND_GRILL: &str = "grill";

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
    /// `get_plan`: a session that plans — the plan it was started for and the catalog of roles to cut it by.
    pub plan: bool,
    /// `propose_goal`: a session that grills the plan's goal; it reads the plan with `get_plan` too.
    pub grill: bool,
    pub create: Creates,
}

impl Capabilities {
    /// Least privilege for a session whose role file is missing or unreadable: it can talk and read, nothing else.
    pub const NONE: Capabilities = Capabilities {
        work: false,
        review: false,
        plan: false,
        grill: false,
        create: Creates::Nothing,
    };

    /// Derives the tools from a role.
    pub fn of(role: &Role) -> Self {
        let kind = role.kind.as_str();
        Capabilities {
            work: kind != KIND_REVIEW && kind != KIND_PLAN && kind != KIND_GRILL,
            review: kind == KIND_REVIEW,
            plan: kind == KIND_PLAN,
            grill: kind == KIND_GRILL,
            // A grilling session proposes a goal and nothing else, whatever `creates:` its role file names.
            create: if kind == KIND_PLAN {
                Creates::Any
            } else if role.creates.is_empty() || kind == KIND_GRILL {
                Creates::Nothing
            } else {
                Creates::Kinds(role.creates.clone())
            },
        }
    }
}

impl Capabilities {
    /// The names of the tools these capabilities offer, in the order the server lists them — what the studio shows
    /// as "what this session can do". Must match [`super::tools::offered`], which a test checks.
    pub fn tool_names(&self) -> Vec<&'static str> {
        let mut names = vec![
            "list_agents",
            "send_message",
            "read_inbox",
            "get_card",
            "list_cards",
        ];
        if self.work {
            names.extend(["claim_task", "report_done"]);
        }
        if self.review {
            names.push("review_verdict");
        }
        if self.plan || self.grill {
            names.push("get_plan");
        }
        if self.grill {
            names.push("propose_goal");
        }
        if self.create != Creates::Nothing {
            names.extend(["create_card", "update_proposal", "withdraw_proposal"]);
        }
        names
    }
}

/// Why a server could not be set up for a session.
#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error(
        "AXIOMATA_AGENT_ID is not set to a session id — this server is started by the studio for one session"
    )]
    NoIdentity,
    #[error(
        "the session secret is missing or wrong — this server only starts from the MCP configuration the studio wrote \
         for session {0} at its latest start"
    )]
    BadSecret(i64),
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
    /// The roles in force for the session's project, in brief: what a planner assigns cards from.
    pub catalog: Vec<RoleEntry>,
    pub limits: Limits,
}

/// One role of the catalog as a planner sees it: who does what, not how.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RoleEntry {
    pub name: String,
    pub description: String,
    pub kind: String,
    pub tier: String,
}

/// The roles in force for the project of session `agent_id` (see [`crate::roster::roles_for_project`]).
fn effective_roles(core: &AxiomataCore, agent_id: i64) -> Vec<Role> {
    let db = core.db_lock();
    let config = core
        .config
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    match agent_store::get_agent(&db, agent_id).ok().flatten() {
        Some(agent) => crate::roster::roles_for_project(&db, &config, agent.project_id),
        None => Vec::new(),
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
            catalog: roles
                .iter()
                .map(|role| RoleEntry {
                    name: role.name.clone(),
                    description: role.description.clone(),
                    kind: role.kind.clone(),
                    tier: match role.tier {
                        axiomata_roster::Tier::Light => "light",
                        axiomata_roster::Tier::Medium => "medium",
                        axiomata_roster::Tier::Heavy => "heavy",
                    }
                    .to_owned(),
                })
                .collect(),
            limits: Limits::default(),
        })
    }

    /// [`Context::new`] with the identity read from the environment and the roles in force for the session's project:
    /// the owner's, with the project's own files replacing and adding once the owner confirmed them
    /// ([`crate::roster::project_roles`]) — the same roles the session was told it plays. The session secret
    /// (`AXIOMATA_AGENT_TOKEN`) must be the one issued at the session's latest start.
    ///
    /// # Errors
    ///
    /// [`ContextError::NoIdentity`] when `AXIOMATA_AGENT_ID` is missing or not a number, [`ContextError::BadSecret`]
    /// when the secret is missing or not the current one, otherwise as [`Context::new`].
    pub fn from_env(core: &AxiomataCore, roots: ChannelRoots) -> Result<Self, ContextError> {
        Self::from_vars(core, roots, |name| std::env::var(name).ok())
    }

    /// [`Context::from_env`] over any source of variables, so the checks can be tested without touching the process
    /// environment.
    pub fn from_vars(
        core: &AxiomataCore,
        roots: ChannelRoots,
        var: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, ContextError> {
        let agent_id =
            parse_id(var("AXIOMATA_AGENT_ID").as_deref()).ok_or(ContextError::NoIdentity)?;
        let secret = var("AXIOMATA_AGENT_TOKEN").unwrap_or_default();
        if !axiomata_ide::session_token::verify(&roots, agent_id, &secret) {
            return Err(ContextError::BadSecret(agent_id));
        }
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
