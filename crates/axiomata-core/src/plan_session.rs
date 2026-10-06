//! Starting a planner: the owner's click that makes a session to cut a plan into cards (`docs/plans/a2a.md`, A5a, A17,
//! A36, CP-A7).
//!
//! A *planner* is a session of the role of kind `plan` on an engine the owner picks now (the role names none: the owner
//! chooses the model at every start). It is made for exactly one plan — a draft — and works in a **detached, read-only
//! checkout** of the project as it is when the planner starts: it can read the code to cut the work well and writes to
//! no branch, so nothing it does ever reaches a take-over. What it produces are cards in the proposal column
//! (`create_card`), which the owner reads and approves with the plan.
//!
//! This module only *makes* the session. The harness is started when its pane — or the CLI — starts it
//! ([`crate::ide_start::start_agent`]), which sees that the session holds a plan and starts it unattended, with the
//! plan
//! in its MCP server and the start prompt as its first words. The plan's text — its goal — reaches it through
//! `get_plan`, never through a shell line.

use std::sync::{Arc, Mutex};

use axiomata_roster::Role;
use rusqlite::Connection;
use serde::Serialize;

use crate::AxiomataCore;
use crate::AxiomataError;
use crate::board::{PlanStatus, flow};
use crate::card_session::{choose_engine, lock, remove_sessions};
use crate::config::Config;
use crate::ide::agent_store;
use crate::ide::model::Agent;
use crate::ide::{store as project_store, worktree};
use crate::roster::{self, refusal};

/// The role kind of a planner.
const KIND_PLAN: &str = "plan";
/// The role a plan is given when there is more than one of kind `plan`: the one every install is seeded with.
const DEFAULT_PLANNER: &str = "planner";
/// The role kind of a session that grills a plan, and the one every install is seeded with.
const KIND_GRILL: &str = "grill";
const DEFAULT_GRILL: &str = "grill";

type Result<T> = std::result::Result<T, AxiomataError>;

/// What the owner asked for.
#[derive(Debug, Clone)]
pub struct PlanStartRequest {
    pub plan_id: i64,
    /// The repository the planner reads — chosen at the start, like a card's.
    pub project_id: i64,
    /// An engine of the catalog to run on; the planner role names none, so this is what the owner picked.
    pub engine_id: Option<String>,
    /// A session that grills the plan's goal (the role of kind `grill`) instead of one that cuts it into cards.
    pub grill: bool,
}

/// The session that was made for the plan.
#[derive(Debug, Clone, Serialize)]
pub struct PlanSession {
    pub agent: Agent,
    pub plan_id: i64,
    pub role: String,
    pub engine_id: String,
}

/// The role that plans — the one called `planner`, else the first of kind `plan` — or, for `grill`, the one that grills: the
/// one called `grill`, else the first of kind `grill`.
fn pick_planner(roles: &[Role], grill: bool) -> Result<&Role> {
    let (kind, default) = if grill {
        (KIND_GRILL, DEFAULT_GRILL)
    } else {
        (KIND_PLAN, DEFAULT_PLANNER)
    };
    roles
        .iter()
        .find(|role| role.kind == kind && role.name == default)
        .or_else(|| roles.iter().find(|role| role.kind == kind))
        .ok_or_else(|| {
            refusal(
                "role",
                format!(
                    "there is no role of kind {kind} for this project; the {default} role is seeded at the next start of \
                     the app, or add one in Engines & roles"
                ),
            )
        })
}

/// The first of `role-<plan>`, `role-<plan>-2`, … no session of the project has.
fn free_name(taken: &[Agent], role: &str, plan_id: i64) -> Result<String> {
    let base = format!("{role}-{plan_id}");
    for attempt in 1..=50u32 {
        let name = if attempt == 1 {
            base.clone()
        } else {
            format!("{base}-{attempt}")
        };
        if !taken.iter().any(|agent| agent.name == name) {
            return Ok(name);
        }
    }
    Err(refusal(
        "name",
        format!("plan #{plan_id} has had 50 planners; remove the old sessions first"),
    ))
}

/// A plan has one planner and one grilling session, in whichever project they run: two planners would propose the same
/// cards twice. The two duties are asked apart — grilling the goal and cutting it are different sessions.
fn refuse_second_planner(
    db: &Connection,
    plan_id: i64,
    roles: &[Role],
    wanted: &Role,
) -> Result<()> {
    let kind_of = |agent: &Agent| {
        roles
            .iter()
            .find(|role| role.name == agent.agent_role)
            .map_or(KIND_PLAN, |role| role.kind.as_str())
            .to_owned()
    };
    match agent_store::plan_sessions(db)?
        .into_iter()
        .find(|agent| agent.plan_id == Some(plan_id) && kind_of(agent) == wanted.kind)
    {
        Some(existing) => Err(refusal(
            "plan",
            format!(
                "plan #{plan_id} has such a session already, {}; open its pane in the Studio",
                existing.name
            ),
        )),
        None => Ok(()),
    }
}

/// What the start needs, read under the lock so that git can run without it.
struct Plan {
    project_root: std::path::PathBuf,
    roles: Vec<Role>,
    role: Role,
    engine_id: String,
}

/// Makes a planner session for the plan. See the module documentation.
///
/// # Errors
///
/// A refusal that says why: no such project or plan, a plan that is not a draft (an approved plan has had its say), a
/// project that is no repository or has no commit yet, no `axiomata-cli` for the team tools, no role of kind `plan`, no
/// usable engine, a plan that has such a session already. Nothing is left behind then.
pub async fn start_plan_session(
    core: &AxiomataCore,
    request: &PlanStartRequest,
) -> Result<PlanSession> {
    let db = Arc::clone(&core.db);
    let config = core.config_read().clone();
    let request = request.clone();
    let cli_available = crate::agent_entry::cli_path().is_some();
    tokio::task::spawn_blocking(move || {
        plan_blocking(
            &db,
            &config,
            &|conn, project| roster::roles_for_project(conn, &config, project),
            &request,
            cli_available,
        )
    })
    .await
    .map_err(|err| refusal("plan", format!("the planner start failed: {err}")))?
}

/// [`start_plan_session`] without the runtime and with the machine's `axiomata-cli` answered by the caller, so tests
/// need neither.
fn plan_blocking(
    db: &Mutex<Connection>,
    config: &Config,
    roles_of: &dyn Fn(&Connection, i64) -> Vec<Role>,
    request: &PlanStartRequest,
    cli_available: bool,
) -> Result<PlanSession> {
    let plan = prepare(&lock(db), config, roles_of, request, cli_available)?;
    // The state the planner reads: one commit, taken now, outside the lock.
    let commit = worktree::head_commit(&plan.project_root).map_err(|err| {
        refusal(
            "project",
            format!("the planner needs a commit to read, and {err}"),
        )
    })?;
    finish(&lock(db), config, request, &plan, &commit)
}

/// Phase one, under the lock: everything that can be refused, asked before anything is made.
fn prepare(
    db: &Connection,
    config: &Config,
    roles_of: &dyn Fn(&Connection, i64) -> Vec<Role>,
    request: &PlanStartRequest,
    cli_available: bool,
) -> Result<Plan> {
    let plan = flow::get_plan(db, request.plan_id)?
        .ok_or_else(|| refusal("plan", format!("no plan {}", request.plan_id)))?;
    if plan.status != PlanStatus::Draft {
        return Err(refusal(
            "plan",
            format!(
                "plan #{} is {}: only a draft is planned",
                plan.id,
                plan.status.as_str()
            ),
        ));
    }
    let project = project_store::get_project(db, request.project_id)?
        .ok_or_else(|| refusal("project", format!("no project {}", request.project_id)))?;
    if !worktree::is_repo(&project.repo_root) {
        return Err(refusal(
            "project",
            "a planner needs a worktree of its own, and this project is not a git repository"
                .to_string(),
        ));
    }
    if !cli_available {
        return Err(refusal(
            "axiomata-cli",
            "a planner needs the team tools, and no `axiomata-cli` was found next to the app (build the workspace, or \
             set AXIOMATA_CLI)"
                .to_string(),
        ));
    }
    let sessions = agent_store::list_agents(db, request.project_id)?;
    let roles = roles_of(db, request.project_id);
    let role = pick_planner(&roles, request.grill)?.clone();
    refuse_second_planner(db, plan.id, &roles, &role)?;
    let engine_id = choose_engine(config, &role, request.engine_id.as_deref())?
        .0
        .to_owned();
    free_name(&sessions, &role.name, plan.id)?;
    Ok(Plan {
        project_root: project.repo_root,
        roles,
        role,
        engine_id,
    })
}

/// Phase two, under the lock again: asked once more — the owner may have started a planner, or approved the plan, in
/// between — then the session is made and told its plan and the commit it reads.
fn finish(
    db: &Connection,
    config: &Config,
    request: &PlanStartRequest,
    plan: &Plan,
    commit: &str,
) -> Result<PlanSession> {
    let current = flow::get_plan(db, request.plan_id)?
        .ok_or_else(|| refusal("plan", format!("no plan {}", request.plan_id)))?;
    if current.status != PlanStatus::Draft {
        return Err(refusal(
            "plan",
            format!("plan #{} is not a draft any more", current.id),
        ));
    }
    let sessions = agent_store::list_agents(db, request.project_id)?;
    refuse_second_planner(db, current.id, &plan.roles, &plan.role)?;
    let name = free_name(&sessions, &plan.role.name, current.id)?;
    let agent = roster::create_agent_on_engine(
        db,
        config,
        &plan.roles,
        request.project_id,
        &name,
        &plan.engine_id,
        &plan.role.name,
    )?;
    if let Err(err) = agent_store::set_plan(db, agent.id, Some(current.id), Some(commit)) {
        agent_store::delete_agent(db, agent.id)?;
        return Err(err.into());
    }
    Ok(PlanSession {
        agent: agent_store::get_agent(db, agent.id)?.unwrap_or(agent),
        plan_id: current.id,
        role: plan.role.name.clone(),
        engine_id: plan.engine_id.clone(),
    })
}

/// Ends the planner sessions of a plan that has had its say (approved, closed or deleted): the sessions are forgotten
/// with their worktrees, secrets and channels, and their Opencode registrations go. Problems are logged — a planner
/// that
/// lingers is no reason to refuse the owner's yes. Returns how many sessions were ended.
pub async fn forget_plan_sessions(core: &AxiomataCore, plan_id: i64) -> usize {
    let db = Arc::clone(&core.db);
    let roots = crate::paths::ide_locations().channels;
    let ended = tokio::task::spawn_blocking(move || {
        let planners: Vec<Agent> = agent_store::plan_sessions(&lock(&db))
            .unwrap_or_default()
            .into_iter()
            .filter(|agent| agent.plan_id == Some(plan_id))
            .collect();
        let locations = remove_sessions(&db, &roots, &planners);
        (planners.len(), locations)
    })
    .await;
    let Ok((count, locations)) = ended else {
        tracing::warn!("ending the planner sessions of a plan failed");
        return 0;
    };
    for location in &locations {
        crate::agents::opencode::forget_mcp(location).await;
    }
    count
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use axiomata_roster::{Billing, Engine, Harness, Limits, Source, Tier};

    use super::*;
    use crate::board::PlanFields;
    use crate::board::store as board_store;
    use crate::db;
    use crate::ide::NewProject;

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn role(name: &str, kind: &str) -> Role {
        Role {
            name: name.into(),
            description: String::new(),
            kind: kind.into(),
            tier: Tier::Heavy,
            engine: None,
            fallback_engines: vec![],
            permissions: vec![],
            limits: Limits::default(),
            creates: vec![],
            instructions: String::new(),
            source: Source::User,
        }
    }

    fn engine(id: &str, command: &str) -> Engine {
        Engine {
            id: id.into(),
            label: id.into(),
            harness: Harness::ClaudeCode,
            command: command.into(),
            model: None,
            env: String::new(),
            billing: Billing::Subscription,
        }
    }

    struct World {
        db: Mutex<Connection>,
        config: Config,
        roles: Vec<Role>,
        project: i64,
        plan: i64,
        dir: PathBuf,
        repo: PathBuf,
    }

    impl Drop for World {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn git(dir: &std::path::Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git should run");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn world(commit: bool) -> World {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-plansession-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "--initial-branch=main"]);
        git(&repo, &["config", "user.email", "t@example.com"]);
        git(&repo, &["config", "user.name", "T"]);
        if commit {
            std::fs::write(repo.join("README.md"), "hi\n").unwrap();
            git(&repo, &["add", "."]);
            git(&repo, &["commit", "-q", "-m", "first"]);
        }
        let mut conn = db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
        let project = project_store::create_project(
            &conn,
            NewProject {
                name: "P".into(),
                repo_root: repo.clone(),
            },
        )
        .unwrap()
        .id;
        let board = board_store::create_board(&mut conn, "B").unwrap().id;
        let plan = flow::create_plan(
            &conn,
            board,
            &PlanFields {
                project_id: None,
                goal: "Add a dark mode".into(),
                name: "Dark".into(),
                auto_start_max: None,
                max_cost_usd: None,
                max_tokens: None,
            },
        )
        .unwrap()
        .id;
        let mut config = Config::default();
        config.agents.engines = BTreeMap::from([
            ("opus".to_owned(), engine("opus", "")),
            ("custom".to_owned(), engine("custom", "my-harness --go")),
        ]);
        World {
            db: Mutex::new(conn),
            config,
            roles: vec![
                role("allrounder", "implement"),
                role("planner", "plan"),
                role("reviewer", "review"),
                role("grill", "grill"),
            ],
            project,
            plan,
            dir,
            repo,
        }
    }

    impl World {
        fn start(&self, engine: Option<&str>) -> Result<PlanSession> {
            self.start_with(engine, true)
        }

        fn start_grill(&self, engine: Option<&str>) -> Result<PlanSession> {
            let roles = self.roles.clone();
            plan_blocking(
                &self.db,
                &self.config,
                &|_, _| roles.clone(),
                &PlanStartRequest {
                    plan_id: self.plan,
                    project_id: self.project,
                    engine_id: engine.map(str::to_owned),
                    grill: true,
                },
                true,
            )
        }

        fn start_with(&self, engine: Option<&str>, cli: bool) -> Result<PlanSession> {
            let roles = self.roles.clone();
            plan_blocking(
                &self.db,
                &self.config,
                &|_, _| roles.clone(),
                &PlanStartRequest {
                    plan_id: self.plan,
                    project_id: self.project,
                    engine_id: engine.map(str::to_owned),
                    grill: false,
                },
                cli,
            )
        }
    }

    #[test]
    fn a_planner_is_made_for_a_draft_plan_on_the_engine_the_owner_picked_and_reads_the_state_at_the_start()
     {
        let w = world(true);
        let session = w.start(Some("opus")).unwrap();
        assert_eq!(
            (session.role.as_str(), session.engine_id.as_str()),
            ("planner", "opus")
        );
        assert_eq!(session.agent.plan_id, Some(w.plan));
        assert_eq!(session.agent.agent_role, "planner");
        assert_eq!(session.agent.name, format!("planner-{}", w.plan));
        assert_eq!(
            session.agent.start_ref.as_deref(),
            Some(git(&w.repo, &["rev-parse", "HEAD"]).as_str())
        );
        assert!(!session.agent.card_review && session.agent.card_id.is_none());
    }

    #[test]
    fn the_planner_role_names_no_engine_so_one_must_be_picked() {
        let w = world(true);
        let refused = w.start(None).unwrap_err().to_string();
        assert!(refused.contains("names no engine"), "{refused}");
        assert!(w.start(Some("nope")).is_err());
        let own_command = w.start(Some("custom")).unwrap_err().to_string();
        assert!(own_command.contains("command of its own"), "{own_command}");
        assert!(
            agent_store::plan_sessions(&lock(&w.db)).unwrap().is_empty(),
            "nothing left behind"
        );
    }

    #[test]
    fn a_plan_has_one_planner_and_only_a_draft_is_planned() {
        let w = world(true);
        w.start(Some("opus")).unwrap();
        let again = w.start(Some("opus")).unwrap_err().to_string();
        assert!(again.contains("has such a session already"), "{again}");

        let other = world(true);
        flow::close_plan(&lock(&other.db), other.plan).unwrap();
        let closed = other.start(Some("opus")).unwrap_err().to_string();
        assert!(closed.contains("only a draft"), "{closed}");
        assert!(other.start(Some("opus")).is_err());
    }

    #[test]
    fn a_plan_may_have_a_grilling_session_beside_its_planner_but_only_one_of_each() {
        let w = world(true);
        let planner = w.start(Some("opus")).unwrap();
        let grill = w.start_grill(Some("opus")).unwrap();
        assert_eq!(grill.role, "grill");
        assert_eq!(grill.agent.name, format!("grill-{}", w.plan));
        assert_eq!(grill.agent.plan_id, Some(w.plan));
        assert_ne!(grill.agent.id, planner.agent.id);
        let again = w.start_grill(Some("opus")).unwrap_err().to_string();
        assert!(again.contains("has such a session already"), "{again}");
        assert!(w.start(Some("opus")).is_err(), "still only one planner");
    }

    #[test]
    fn a_grilling_session_needs_a_role_of_kind_grill() {
        let mut w = world(true);
        w.roles.retain(|role| role.kind != "grill");
        let refused = w.start_grill(Some("opus")).unwrap_err().to_string();
        assert!(refused.contains("no role of kind grill"), "{refused}");
        assert!(w.start(Some("opus")).is_ok(), "the planner is not affected");
    }

    #[test]
    fn a_plan_has_one_planner_even_when_the_second_would_run_in_another_project() {
        let w = world(true);
        w.start(Some("opus")).unwrap();
        let second_repo = w.dir.join("repo2");
        std::fs::create_dir_all(&second_repo).unwrap();
        git(&second_repo, &["init", "--initial-branch=main"]);
        git(&second_repo, &["config", "user.email", "t@example.com"]);
        git(&second_repo, &["config", "user.name", "T"]);
        std::fs::write(second_repo.join("a"), "a").unwrap();
        git(&second_repo, &["add", "."]);
        git(&second_repo, &["commit", "-q", "-m", "x"]);
        let other = project_store::create_project(
            &lock(&w.db),
            NewProject {
                name: "Q".into(),
                repo_root: second_repo,
            },
        )
        .unwrap()
        .id;
        let roles = w.roles.clone();
        let refused = plan_blocking(
            &w.db,
            &w.config,
            &|_, _| roles.clone(),
            &PlanStartRequest {
                plan_id: w.plan,
                project_id: other,
                engine_id: Some("opus".into()),
                grill: false,
            },
            true,
        )
        .unwrap_err()
        .to_string();
        assert!(refused.contains("has such a session already"), "{refused}");
    }

    fn prepared(w: &World) -> (PlanStartRequest, Plan, String) {
        let request = PlanStartRequest {
            plan_id: w.plan,
            project_id: w.project,
            engine_id: Some("opus".into()),
            grill: false,
        };
        let roles = w.roles.clone();
        let plan = prepare(
            &lock(&w.db),
            &w.config,
            &|_, _| roles.clone(),
            &request,
            true,
        )
        .unwrap();
        let commit = worktree::head_commit(&plan.project_root).unwrap();
        (request, plan, commit)
    }

    #[test]
    fn a_plan_approved_between_the_questions_and_the_making_is_found_out_when_the_session_is_made()
    {
        let w = world(true);
        let (request, plan, commit) = prepared(&w);
        flow::approve_plan(&mut lock(&w.db), w.plan, "human:owner").unwrap();
        let late = finish(&lock(&w.db), &w.config, &request, &plan, &commit)
            .unwrap_err()
            .to_string();
        assert!(late.contains("not a draft"), "{late}");
        assert!(agent_store::plan_sessions(&lock(&w.db)).unwrap().is_empty());
    }

    #[test]
    fn a_planner_that_appeared_in_between_is_found_again_when_the_session_is_made() {
        let w = world(true);
        let (request, plan, commit) = prepared(&w);
        finish(&lock(&w.db), &w.config, &request, &plan, &commit).unwrap();
        let again = finish(&lock(&w.db), &w.config, &request, &plan, &commit)
            .unwrap_err()
            .to_string();
        assert!(again.contains("has such a session already"), "{again}");
    }

    #[test]
    fn a_planner_needs_the_team_tools_a_commit_to_read_and_a_role_that_plans() {
        let w = world(true);
        let no_cli = w.start_with(Some("opus"), false).unwrap_err().to_string();
        assert!(no_cli.contains("axiomata-cli"), "{no_cli}");

        let empty = world(false);
        let no_commit = empty.start(Some("opus")).unwrap_err().to_string();
        assert!(no_commit.contains("commit"), "{no_commit}");
        assert!(
            agent_store::plan_sessions(&lock(&empty.db))
                .unwrap()
                .is_empty()
        );

        let mut no_role = world(true);
        no_role.roles.retain(|role| role.kind != "plan");
        let refused = no_role.start(Some("opus")).unwrap_err().to_string();
        assert!(refused.contains("no role of kind plan"), "{refused}");
    }

    #[test]
    fn a_planner_is_wanted_while_its_plan_is_a_draft_and_not_after_the_owner_said_yes() {
        let w = world(true);
        let session = w.start(Some("opus")).unwrap();
        let conn = lock(&w.db);
        let launch = crate::ide_start::launch_of(&conn, &session.agent).unwrap();
        assert_eq!(launch.plan_id(), Some(w.plan));
        assert!(launch.read_only());
        assert_eq!(launch.card_id(), None);

        let mut conn = conn;
        flow::approve_plan(&mut conn, w.plan, "human:owner").unwrap();
        assert!(
            crate::ide_start::launch_of(&conn, &session.agent).is_none(),
            "a restarted pane must not go on proposing cards into an approved plan"
        );
    }
}
