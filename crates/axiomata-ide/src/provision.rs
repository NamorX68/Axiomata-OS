//! Giving an agent the things it needs to run: a worktree and a port.
//!
//! The two stores below it each do one thing — `agent_store` writes rows,
//! `worktree` runs git — and this module is the one place that knows they
//! belong together. It is deliberately **idempotent**: running it again for an
//! agent that already has a worktree returns what is there rather than
//! failing, because it runs whenever an agent is started, not only when one is
//! created. An agent that predates CP5, or whose worktree someone deleted by
//! hand, gets one on the next start without anybody having to notice.
//!
//! Like [`crate::worktree`], and unlike [`crate::store`], this touches the
//! file system. It never touches the project's own working copy.

use std::net::TcpListener;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::model::Agent;
use crate::{IdeError, Result, agent_store, store, worktree};

/// The port range agents are given numbers from.
///
/// Above the ranges the usual dev servers pick for themselves (Vite's 5173,
/// Next's 3000, this project's own 1420) so that an agent's reserved port does
/// not collide with whatever a tool grabs on its own before reading
/// `AXIOMATA_PORT`.
pub const PORT_RANGE: std::ops::RangeInclusive<u16> = 4300..=4399;

/// Everything an agent needs before it can be started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provisioned {
    pub agent: Agent,
    /// Where the harness should start: the worktree, or the project folder
    /// when the project is not a git repository.
    pub cwd: PathBuf,
    /// True when the project is not a repository, so callers can say why an
    /// agent is sharing the project folder rather than leaving it a mystery.
    pub shared_folder: bool,
}

/// Finds a port nobody has reserved and nothing is listening on.
///
/// Two checks, because they catch different things: the database knows what
/// *this* app handed out, and a bind attempt knows what the rest of the
/// machine is doing. Neither alone is enough, and even both together are
/// advisory — the agent's own process binds the port later, and something else
/// could take it in between. Reserving it in the database is what stops two
/// *agents* colliding, which is the collision this exists to prevent.
pub fn free_port(db: &Connection) -> Result<Option<u16>> {
    let taken = agent_store::reserved_ports(db)?;
    for port in PORT_RANGE {
        if taken.contains(&port) {
            continue;
        }
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(Some(port));
        }
    }
    Ok(None)
}

/// Makes sure an agent has a worktree and a port, and says where it runs.
///
/// `worktree_base` is where worktrees live (`~/.axiomata/worktrees` for the
/// app) — handed in, because this crate owns no paths of its own.
pub fn prepare(db: &Connection, worktree_base: &Path, agent_id: i64) -> Result<Provisioned> {
    let agent = agent_store::get_agent(db, agent_id)?.ok_or_else(|| IdeError::Invalid {
        field: "agent_id",
        reason: format!("no agent {agent_id}"),
    })?;
    let project = store::get_project(db, agent.project_id)?.ok_or_else(|| IdeError::Invalid {
        field: "project_id",
        reason: format!("no project {}", agent.project_id),
    })?;

    // A project that is not a repository is an ordinary case: its agents share
    // the folder, exactly as they did before worktrees existed. Saying so is
    // better than refusing to start.
    if !worktree::is_repo(&project.repo_root) {
        ensure_port(db, &agent)?;
        return Ok(Provisioned {
            // Re-read, so the caller sees the port that was just written.
            agent: reread(db, agent_id)?,
            cwd: project.repo_root,
            shared_folder: true,
        });
    }

    let path = worktree::worktree_path(worktree_base, &project.name, &agent.name, agent.id);
    let branch = worktree::branch_name(&agent.name, agent.id);
    let created = worktree::add(&project.repo_root, &path, &branch)?;

    agent_store::set_worktree(db, agent.id, Some(&created.path), created.branch.as_deref())?;
    ensure_port(db, &agent)?;

    let agent = reread(db, agent_id)?;
    Ok(Provisioned {
        cwd: created.path,
        agent,
        shared_folder: false,
    })
}

/// Removes an agent's worktree, if it has one.
///
/// `force` throws away uncommitted work in it, which is why the caller decides
/// and the UI asks first. Returns whether there was one to remove. The port is
/// released with the row itself when the agent is deleted.
pub fn discard_worktree(db: &Connection, agent_id: i64, force: bool) -> Result<bool> {
    let Some(agent) = agent_store::get_agent(db, agent_id)? else {
        return Ok(false);
    };
    let (Some(path), Some(project)) = (
        agent.worktree_path.clone(),
        store::get_project(db, agent.project_id)?,
    ) else {
        return Ok(false);
    };

    let removed = worktree::remove(&project.repo_root, &path, force)?;
    if removed || !path.exists() {
        // The second case: the directory was deleted outside the app, so git
        // has nothing to remove but the row still claims a worktree. Clearing
        // it here means the next `prepare` starts from "no worktree" rather
        // than from a path that is not one — which matters, because that is
        // the moment a branch gets re-attached.
        agent_store::set_worktree(db, agent_id, None, None)?;
    }
    Ok(removed)
}

/// Whether an agent's worktree holds work that removing it would throw away.
pub fn worktree_has_changes(db: &Connection, agent_id: i64) -> Result<bool> {
    let Some(agent) = agent_store::get_agent(db, agent_id)? else {
        return Ok(false);
    };
    match agent.worktree_path {
        Some(path) if path.is_dir() => worktree::has_uncommitted_changes(&path),
        _ => Ok(false),
    }
}

fn ensure_port(db: &Connection, agent: &Agent) -> Result<Option<u16>> {
    if agent.port.is_some() {
        return Ok(agent.port);
    }
    let port = free_port(db)?;
    if let Some(port) = port {
        agent_store::set_port(db, agent.id, Some(port))?;
    }
    Ok(port)
}

fn reread(db: &Connection, agent_id: i64) -> Result<Agent> {
    agent_store::get_agent(db, agent_id)?.ok_or_else(|| IdeError::Invalid {
        field: "agent_id",
        reason: format!("agent {agent_id} vanished while being prepared"),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;
    use crate::model::{AgentFields, Harness, NewAgent, NewProject};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-provision-{label}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn run_git(dir: &Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git should run");
        assert!(status.status.success(), "git {args:?} failed");
    }

    fn git_repo() -> PathBuf {
        let dir = temp_dir("repo");
        run_git(&dir, &["init", "--initial-branch=main"]);
        run_git(&dir, &["config", "user.email", "t@example.com"]);
        run_git(&dir, &["config", "user.name", "T"]);
        std::fs::write(dir.join("README.md"), "hi\n").unwrap();
        run_git(&dir, &["add", "."]);
        run_git(&dir, &["commit", "-m", "first"]);
        dir
    }

    fn db_with_project(root: PathBuf) -> (Connection, i64) {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        crate::apply_all_schemas(&db);
        let project = store::create_project(
            &db,
            NewProject {
                name: "Axiomata OS".into(),
                repo_root: root,
            },
        )
        .unwrap();
        (db, project.id)
    }

    fn add_agent(db: &Connection, project_id: i64, name: &str) -> i64 {
        agent_store::create_agent(
            db,
            NewAgent {
                project_id,
                fields: AgentFields {
                    name: name.into(),
                    harness: Harness::Opencode,
                    command: String::new(),
                    model: None,
                    env: String::new(),
                },
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn an_agent_in_a_repository_gets_its_own_worktree_and_branch() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo);
        let base = temp_dir("base");
        let agent = add_agent(&db, project, "Qwen 3.8 27b FP4");

        let ready = prepare(&db, &base, agent).unwrap();
        assert!(!ready.shared_folder);
        assert_eq!(
            ready.agent.branch.as_deref(),
            Some("axiomata/qwen-3-8-27b-fp4-1")
        );
        assert!(ready.cwd.join("README.md").exists(), "a populated checkout");
        assert_eq!(
            ready.agent.worktree_path.as_deref(),
            Some(ready.cwd.as_path())
        );
    }

    #[test]
    fn two_agents_get_two_worktrees_and_two_ports() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo);
        let base = temp_dir("base");
        let first = prepare(&db, &base, add_agent(&db, project, "Builder")).unwrap();
        let second = prepare(&db, &base, add_agent(&db, project, "Reviewer")).unwrap();

        // The whole point: neither one can overwrite the other's work, and
        // neither one's dev server takes the other's port.
        assert_ne!(first.cwd, second.cwd);
        assert_ne!(first.agent.branch, second.agent.branch);
        assert_ne!(first.agent.port, second.agent.port);
        assert!(first.agent.port.is_some());
    }

    #[test]
    fn preparing_twice_is_the_same_answer_not_an_error() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo);
        let base = temp_dir("base");
        let agent = add_agent(&db, project, "Builder");

        let first = prepare(&db, &base, agent).unwrap();
        // Every start calls this; a restart must not need a new worktree.
        let second = prepare(&db, &base, agent).unwrap();
        assert_eq!(first.cwd, second.cwd);
        assert_eq!(first.agent.port, second.agent.port);
    }

    #[test]
    fn a_project_that_is_not_a_repository_shares_its_folder_rather_than_refusing() {
        let plain = temp_dir("plain");
        let (db, project) = db_with_project(plain.clone());
        let base = temp_dir("base");
        let agent = add_agent(&db, project, "Builder");

        let ready = prepare(&db, &base, agent).unwrap();
        assert!(ready.shared_folder);
        // The project store canonicalises on the way in (CP0), so compare
        // against the resolved spelling rather than the one we asked for.
        assert_eq!(ready.cwd, plain.canonicalize().unwrap());
        assert!(ready.agent.worktree_path.is_none());
        // It still gets a port: that has nothing to do with git.
        assert!(ready.agent.port.is_some());
    }

    #[test]
    fn the_identity_reaches_the_environment_and_cannot_be_forged() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo);
        let base = temp_dir("base");
        let agent = agent_store::create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: AgentFields {
                    name: "Builder".into(),
                    harness: Harness::Opencode,
                    command: String::new(),
                    model: None,
                    // A profile claiming to be a different agent.
                    env: "FOO=bar\nAXIOMATA_AGENT_ID=999".into(),
                },
            },
        )
        .unwrap()
        .id;

        let ready = prepare(&db, &base, agent).unwrap();
        let env = ready.agent.effective_env;
        assert!(
            env.contains("FOO=bar"),
            "the profile's own lines survive: {env}"
        );
        assert!(env.contains("AXIOMATA_AGENT_NAME=Builder"), "{env}");
        assert!(env.contains("AXIOMATA_BRANCH=axiomata/builder-"), "{env}");
        assert!(
            env.contains(&format!("AXIOMATA_PORT={}", ready.agent.port.unwrap())),
            "{env}"
        );
        // The forged line is still in the text, but ours comes after it, and
        // later wins — both in `PtySession::spawn` and in the frontend merge.
        let ours = env.rfind(&format!("AXIOMATA_AGENT_ID={agent}")).unwrap();
        let forged = env.find("AXIOMATA_AGENT_ID=999").unwrap();
        assert!(ours > forged, "our identity must come last: {env}");
    }

    #[test]
    fn a_worktree_can_be_discarded_and_refuses_to_throw_away_work() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo);
        let base = temp_dir("base");
        let agent = add_agent(&db, project, "Builder");
        let ready = prepare(&db, &base, agent).unwrap();

        assert!(!worktree_has_changes(&db, agent).unwrap());
        std::fs::write(ready.cwd.join("work.txt"), "unfinished").unwrap();
        assert!(worktree_has_changes(&db, agent).unwrap());

        assert!(
            discard_worktree(&db, agent, false).is_err(),
            "should refuse"
        );
        assert!(discard_worktree(&db, agent, true).unwrap());
        assert!(!ready.cwd.exists());
        assert!(
            agent_store::get_agent(&db, agent)
                .unwrap()
                .unwrap()
                .worktree_path
                .is_none()
        );
    }

    #[test]
    fn a_worktree_deleted_by_hand_stops_being_claimed() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo);
        let base = temp_dir("base");
        let agent = add_agent(&db, project, "Builder");
        let ready = prepare(&db, &base, agent).unwrap();

        // Somebody removes the directory without telling git or us.
        std::fs::remove_dir_all(&ready.cwd).unwrap();

        // Git still has the bookkeeping for it and cleans that up, so whether
        // this reports having removed something is git's business. What must
        // hold either way is that the row stops claiming a worktree.
        discard_worktree(&db, agent, false).unwrap();
        assert!(
            agent_store::get_agent(&db, agent)
                .unwrap()
                .unwrap()
                .worktree_path
                .is_none(),
            "a path that is no longer a worktree must not stay on the row"
        );
    }

    #[test]
    fn an_agent_started_again_finds_the_work_it_committed() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo);
        let base = temp_dir("base");
        let agent = add_agent(&db, project, "Builder");
        let first = prepare(&db, &base, agent).unwrap();

        std::fs::write(first.cwd.join("done.txt"), "shipped\n").unwrap();
        run_git(&first.cwd, &["add", "."]);
        run_git(&first.cwd, &["commit", "-m", "agent work"]);

        // Discarded with a clean tree — no force needed, no warning given.
        assert!(discard_worktree(&db, agent, false).unwrap());
        // …and started again, which is where the work used to disappear.
        let again = prepare(&db, &base, agent).unwrap();
        assert!(
            again.cwd.join("done.txt").exists(),
            "the agent's committed work must come back with its branch"
        );
    }

    #[test]
    fn a_port_is_not_handed_to_two_agents() {
        let plain = temp_dir("plain");
        let (db, project) = db_with_project(plain);
        let base = temp_dir("base");
        let first = prepare(&db, &base, add_agent(&db, project, "One")).unwrap();

        // The same number must now be refused by the unique index, which is
        // what makes the reservation mean anything.
        let second = add_agent(&db, project, "Two");
        assert!(agent_store::set_port(&db, second, first.agent.port).is_err());
    }
}
