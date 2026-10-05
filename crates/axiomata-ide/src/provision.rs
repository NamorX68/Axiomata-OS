//! Giving an agent the things it needs to run: a worktree, a port and a
//! status channel.
//!
//! The modules below it each do one thing — `agent_store` writes rows,
//! `worktree` runs git, `lifecycle` owns the channel — and this module is the
//! one place that knows they belong together. It is deliberately **idempotent**: running it again for an
//! agent that already has a worktree returns what is there rather than
//! failing, because it runs whenever an agent is started, not only when one is
//! created. An agent that predates CP5, or whose worktree someone deleted by
//! hand, gets one on the next start without anybody having to notice.
//!
//! Like [`crate::worktree`], and unlike [`crate::store`], this touches the
//! file system. It never touches the project's own working copy.

use std::net::TcpListener;
use std::path::PathBuf;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::git::{AgentRepo, TakeOver, TakeOverMode};
use crate::lifecycle::{AgentState, AgentStatus, Channel, ChannelRoots};
use crate::model::Agent;
use crate::{IdeError, Result, agent_store, store, worktree};

/// The port range agents are given numbers from.
///
/// Above the ranges the usual dev servers pick for themselves (Vite's 5173,
/// Next's 3000, this project's own 1420) so that an agent's reserved port does
/// not collide with whatever a tool grabs on its own before reading
/// `AXIOMATA_PORT`.
pub const PORT_RANGE: std::ops::RangeInclusive<u16> = 4300..=4399;

/// Where the IDE keeps what it owns on disk — every path this module needs,
/// in one value. The crate has no paths of its own; the embedder builds this
/// (`axiomata_core::paths::ide_locations`) and hands it in.
#[derive(Debug, Clone)]
pub struct Locations {
    /// One git worktree per agent (`~/.axiomata/worktrees`).
    pub worktrees: PathBuf,
    /// Where the status channels live (M7.2 CP6).
    pub channels: ChannelRoots,
}

/// Everything an agent needs before it can be started.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provisioned {
    /// Re-read after provisioning, so this reflects the worktree/branch/port
    /// that were just written rather than the caller's stale copy.
    pub agent: Agent,
    /// Where the harness should start: the worktree, or the project folder
    /// when the project is not a git repository.
    pub cwd: PathBuf,
    /// True when the project is not a repository, so callers can say why an
    /// agent is sharing the project folder rather than leaving it a mystery.
    pub shared_folder: bool,
    /// The command line to type into the shell: [`Agent::effective_command`]
    /// plus the status hookup — but the hookup only when the command is the
    /// generated one (E13 in `docs/plans/agent-lifecycle.md`). Comes from Rust,
    /// never from a stored layout (the CP4 rule).
    pub launch_command: String,
    /// The environment to start with: [`Agent::effective_env`] plus the
    /// channel's lines (`AXIOMATA_EVENTS`, `AXIOMATA_CLAUDE_SETTINGS`, …). Ours come
    /// last, like the identity, so a profile cannot redirect them.
    pub launch_env: String,
    /// Whether the harness reports into the status channel at all.
    pub status_connected: bool,
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

/// Makes sure an agent has a worktree, a port and a fresh status channel, and
/// says where and how it runs.
///
/// `locations` says where worktrees and channels live — handed in, because
/// this crate owns no paths of its own.
pub fn prepare(db: &Connection, locations: &Locations, agent_id: i64) -> Result<Provisioned> {
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
    let (cwd, shared_folder) = if worktree::is_repo(&project.repo_root) {
        let path =
            worktree::worktree_path(&locations.worktrees, &project.name, &agent.name, agent.id);
        // A reviewer looks at one state, a planner at the state the project was in; neither writes to a branch.
        let snapshot = agent
            .start_ref
            .as_deref()
            .filter(|_| agent.card_review || agent.plan_id.is_some());
        if let Some(commit) = snapshot {
            let created = worktree::add_detached(&project.repo_root, &path, commit)?;
            agent_store::set_worktree(db, agent.id, Some(&created.path), None)?;
            (created.path, false)
        } else {
            let branch = worktree::branch_name(&agent.name, agent.id);
            // The base is recorded only when the branch is born (G1): a branch
            // that already exists was cut earlier, from something nobody wrote
            // down, and guessing now would be worse than the documented fallback.
            let fresh_branch = !worktree::branch_exists(&project.repo_root, &branch);
            let base = worktree::current_branch(&project.repo_root);
            let created = worktree::add(&project.repo_root, &path, &branch)?;
            agent_store::set_worktree(
                db,
                agent.id,
                Some(&created.path),
                created.branch.as_deref(),
            )?;
            // Two statements, not one transaction: were the app killed between
            // them, the branch would exist without a recorded base and fall back
            // to the project folder's branch (G1) — an accepted, visible
            // degradation, not a corruption.
            if fresh_branch && agent.base_branch.is_none() {
                agent_store::set_base_branch(db, agent.id, base.as_deref())?;
            }
            (created.path, false)
        }
    } else {
        (project.repo_root, true)
    };
    ensure_port(db, &agent)?;

    // Every start, like the worktree: a channel deleted by hand, or one from
    // before CP6, is simply rebuilt.
    let channel = Channel::for_agent(&locations.channels, agent.id);
    channel.reset()?;
    let hookup = channel.install(agent.harness)?;

    // Re-read, so the caller sees the worktree and port that were just written.
    let agent = reread(db, agent_id)?;
    let launch_command = match &hookup.args {
        Some(args) if agent.command.trim().is_empty() => {
            format!("{} {args}", agent.effective_command)
        }
        _ => agent.effective_command.clone(),
    };
    let launch_env = std::iter::once(agent.effective_env.as_str())
        .chain(hookup.env.iter().map(String::as_str))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Provisioned {
        agent,
        cwd,
        shared_folder,
        launch_command,
        launch_env,
        status_connected: hookup.connected,
    })
}

/// Removes an agent's worktree, if it has one.
///
/// `force` throws away uncommitted work in it, which is why the caller decides
/// and the UI asks first. Returns whether there was one to remove. The port is
/// released with the row itself when the agent is deleted.
pub fn discard_worktree(db: &Connection, agent_id: i64, force: bool) -> Result<bool> {
    let Some(target) = WorktreeToDiscard::read(db, agent_id)? else {
        return Ok(false);
    };
    let removed = target.remove(force)?;
    target.forget(db, removed)?;
    Ok(removed)
}

/// [`discard_worktree`] in its three steps, so a caller that shares its
/// database connection can let go of it while `git worktree remove` runs
/// (performance review, M7.3 CP7): read, remove without the connection,
/// then record.
#[derive(Debug, Clone)]
pub struct WorktreeToDiscard {
    agent_id: i64,
    repo_root: PathBuf,
    path: PathBuf,
}

impl WorktreeToDiscard {
    /// The agent's worktree, or `None` when it has none.
    pub fn read(db: &Connection, agent_id: i64) -> Result<Option<Self>> {
        let Some(agent) = agent_store::get_agent(db, agent_id)? else {
            return Ok(None);
        };
        let (Some(path), Some(project)) = (
            agent.worktree_path,
            store::get_project(db, agent.project_id)?,
        ) else {
            return Ok(None);
        };
        Ok(Some(WorktreeToDiscard {
            agent_id,
            repo_root: project.repo_root,
            path,
        }))
    }

    /// Where the worktree is.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Runs `git worktree remove`. Needs no database.
    pub fn remove(&self, force: bool) -> Result<bool> {
        worktree::remove(&self.repo_root, &self.path, force)
    }

    /// Clears the row once the worktree is gone.
    pub fn forget(&self, db: &Connection, removed: bool) -> Result<()> {
        if removed || !self.path.exists() {
            // The second case: the directory was deleted outside the app, so
            // git has nothing to remove but the row still claims a worktree.
            // Clearing it here means the next `prepare` starts from "no
            // worktree" rather than from a path that is not one — which
            // matters, because that is the moment a branch gets re-attached.
            agent_store::set_worktree(db, self.agent_id, None, None)?;
        }
        Ok(())
    }
}

/// The status of each of these agents — what the UI polls, once per second.
///
/// Takes the agents rather than a connection on purpose: the caller lists
/// them, lets go of the database, and only then are the files read, so the
/// app's one connection is not held for N file reads on every tick.
/// A missing or garbled channel reads as "starting", never as an error.
pub fn agent_statuses(agents: &[Agent], roots: &ChannelRoots) -> Vec<AgentStatus> {
    agents
        .iter()
        .map(|agent| Channel::for_agent(roots, agent.id).read_status(agent.harness))
        .collect()
}

/// Removes an agent's status channel and its own Claude Code task list.
///
/// For the caller to run after deleting the row. Kept apart from
/// `agent_store::delete_agent`, which never touches the file system.
pub fn forget_channel(roots: &ChannelRoots, agent_id: i64) -> Result<()> {
    Channel::for_agent(roots, agent_id).forget()
}

/// Where an agent's git work happens — or why it has none (M7.3).
///
/// Three cases, because the Diffs tab says something different for each
/// (architecture review, CP7): a project folder that is not a repository will
/// never give an agent a diff of its own, while an agent that has simply not
/// been started yet will have one after its first start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRepoState {
    /// A worktree and branch exist; [`AgentRepo`] is ready to answer git questions.
    Ready(AgentRepo),
    /// The project folder is not a git repository; its agents share it.
    SharedFolder,
    /// A repository, but this agent has no worktree yet — start it once.
    NotStarted,
}

impl AgentRepoState {
    /// The repository, or a sentence saying why there is none.
    pub fn ready(self) -> Result<AgentRepo> {
        match self {
            AgentRepoState::Ready(repo) => Ok(repo),
            AgentRepoState::SharedFolder => Err(IdeError::Invalid {
                field: "agent",
                reason: "the project folder is not a git repository, so its agents share it and \
                         have no worktree of their own"
                    .into(),
            }),
            AgentRepoState::NotStarted => Err(IdeError::Invalid {
                field: "agent",
                reason: "this agent has not been started yet, so it has no worktree".into(),
            }),
        }
    }
}

/// Reads where an agent's git work happens, once, so the caller can let go of
/// the database connection before running git.
pub fn agent_repo(db: &Connection, agent_id: i64) -> Result<AgentRepoState> {
    let agent = agent_store::get_agent(db, agent_id)?.ok_or_else(|| IdeError::Invalid {
        field: "agent_id",
        reason: format!("no agent {agent_id}"),
    })?;
    repo_state(db, agent)
}

fn repo_state(db: &Connection, agent: Agent) -> Result<AgentRepoState> {
    let project = store::get_project(db, agent.project_id)?.ok_or_else(|| IdeError::Invalid {
        field: "project_id",
        reason: format!("no project {}", agent.project_id),
    })?;
    let (Some(worktree), Some(agent_branch)) = (agent.worktree_path, agent.branch) else {
        return Ok(if worktree::is_repo(&project.repo_root) {
            AgentRepoState::NotStarted
        } else {
            AgentRepoState::SharedFolder
        });
    };
    Ok(AgentRepoState::Ready(AgentRepo {
        repo_root: project.repo_root,
        worktree,
        agent_branch,
        base_branch: agent.base_branch,
    }))
}

/// Everything a take-over needs, read under one brief database lock.
///
/// The only way to take an agent's work over from outside this crate: its
/// [`run`](Self::run) checks the agent is not mid-turn before anything
/// happens (G12), so no entry point — Tauri, CLI, a later dock pane — can
/// skip that check (architecture review, CP7).
#[derive(Debug, Clone)]
pub struct TakeOverTarget {
    agent: Agent,
    repo: AgentRepo,
}

impl TakeOverTarget {
    /// Reads the agent and its repository. Holds `db` only for these reads.
    pub fn read(db: &Connection, agent_id: i64) -> Result<Self> {
        let agent = agent_store::get_agent(db, agent_id)?.ok_or_else(|| IdeError::Invalid {
            field: "agent_id",
            reason: format!("no agent {agent_id}"),
        })?;
        let repo = repo_state(db, agent.clone())?.ready()?;
        Ok(TakeOverTarget { agent, repo })
    }

    /// Refuses while the agent is working or waiting, then takes over.
    pub fn run(&self, roots: &ChannelRoots, mode: TakeOverMode, message: &str) -> Result<TakeOver> {
        ensure_not_busy(roots, &self.agent)?;
        self.repo.take_over(mode, message)
    }
}

/// Refuses while the agent is in the middle of a turn (G12): taking over
/// resets its branch, which would pull the ground from under whatever it is
/// doing. `waiting` counts as busy too — it is mid-turn, blocked on a
/// permission prompt.
fn ensure_not_busy(roots: &ChannelRoots, agent: &Agent) -> Result<()> {
    let state = Channel::for_agent(roots, agent.id)
        .read_status(agent.harness)
        .state;
    if matches!(state, AgentState::Working | AgentState::Waiting) {
        return Err(IdeError::Invalid {
            field: "agent",
            reason: format!(
                "{} is {} — wait until it has finished before taking its work over",
                agent.name,
                state.as_str()
            ),
        });
    }
    Ok(())
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
    use std::path::Path;
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

    fn locations() -> Locations {
        let root = temp_dir("home");
        Locations {
            worktrees: root.join("worktrees"),
            channels: ChannelRoots {
                events: root.join("agent-events"),
                claude_tasks: root.join("claude-tasks"),
                claude_plans: root.join("claude-plans"),
            },
        }
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
        let base = locations();
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
        let base = locations();
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
    fn a_planner_gets_a_detached_checkout_of_the_commit_it_was_started_at_even_after_the_project_moves_on()
     {
        let repo = git_repo();
        let (db, project) = db_with_project(repo.clone());
        let base = locations();
        let agent = add_agent(&db, project, "planner-1");
        let at_start = worktree::head_commit(&repo).unwrap();
        agent_store::set_plan(&db, agent, Some(7), Some(&at_start)).unwrap();

        // The owner commits after the planner was made: what it reads is the state it was started at.
        std::fs::write(repo.join("later.txt"), "later\n").unwrap();
        run_git(&repo, &["add", "."]);
        run_git(&repo, &["commit", "-m", "later"]);

        let ready = prepare(&db, &base, agent).unwrap();
        assert_eq!(ready.agent.branch, None, "a planner writes to no branch");
        assert_eq!(worktree::head_commit(&ready.cwd).unwrap(), at_start);
        assert!(!ready.cwd.join("later.txt").exists());
    }

    #[test]
    fn preparing_twice_is_the_same_answer_not_an_error() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo);
        let base = locations();
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
        let base = locations();
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
        let base = locations();
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
        let base = locations();
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
        let base = locations();
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
        let base = locations();
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
        let base = locations();
        let first = prepare(&db, &base, add_agent(&db, project, "One")).unwrap();

        // The same number must now be refused by the unique index, which is
        // what makes the reservation mean anything.
        let second = add_agent(&db, project, "Two");
        assert!(agent_store::set_port(&db, second, first.agent.port).is_err());
    }

    #[test]
    fn a_generated_claude_command_is_hooked_up_and_an_own_command_is_left_alone() {
        let plain = temp_dir("plain");
        let (db, project) = db_with_project(plain);
        let base = locations();
        let fields = |command: &str| AgentFields {
            name: format!("Claude {command}"),
            harness: Harness::ClaudeCode,
            command: command.into(),
            model: Some("claude-sonnet-5".into()),
            env: String::new(),
        };
        let generated = agent_store::create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: fields(""),
            },
        )
        .unwrap()
        .id;
        let own = agent_store::create_agent(
            &db,
            NewAgent {
                project_id: project,
                fields: fields("claude --resume"),
            },
        )
        .unwrap()
        .id;

        let ready = prepare(&db, &base, generated).unwrap();
        assert!(ready.status_connected);
        assert!(
            ready
                .launch_command
                .starts_with("claude --model 'claude-sonnet-5' --settings '"),
            "{}",
            ready.launch_command
        );
        // The shared folder gets a channel too (E12): nothing is written into it.
        assert!(ready.shared_folder);

        let ready = prepare(&db, &base, own).unwrap();
        assert_eq!(
            ready.launch_command, "claude --resume",
            "an own command is never extended"
        );
        // …but the env still carries the hookup, so it can attach itself.
        assert!(
            ready.launch_env.contains("AXIOMATA_CLAUDE_SETTINGS="),
            "{}",
            ready.launch_env
        );
    }

    #[test]
    fn agent_statuses_covers_several_agents_and_an_empty_project_reads_as_empty() {
        let plain = temp_dir("plain");
        let (db, project) = db_with_project(plain);
        let base = locations();

        assert!(
            agent_statuses(
                &agent_store::list_agents(&db, project).unwrap(),
                &base.channels
            )
            .is_empty(),
            "a project with no agents yet must not error"
        );

        let one = add_agent(&db, project, "One");
        let two = add_agent(&db, project, "Two");
        prepare(&db, &base, one).unwrap();
        prepare(&db, &base, two).unwrap();

        let statuses = agent_statuses(
            &agent_store::list_agents(&db, project).unwrap(),
            &base.channels,
        );
        let ids: std::collections::HashSet<_> = statuses.iter().map(|s| s.agent_id).collect();
        assert_eq!(statuses.len(), 2);
        assert_eq!(ids, std::collections::HashSet::from([one, two]));
    }

    #[test]
    fn forgetting_a_channel_that_was_never_created_is_not_an_error() {
        let plain = temp_dir("plain");
        let (db, project) = db_with_project(plain);
        let base = locations();
        // Never prepared, so no channel directory was ever written.
        let agent = add_agent(&db, project, "Builder");

        forget_channel(&base.channels, agent).unwrap();
        assert!(!base.channels.events.join(agent.to_string()).exists());
    }

    #[test]
    fn the_channel_env_comes_after_the_identity_and_statuses_read_back() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo);
        let base = locations();
        let agent = add_agent(&db, project, "Builder");
        let ready = prepare(&db, &base, agent).unwrap();

        let env = &ready.launch_env;
        let identity = env.find("AXIOMATA_AGENT_ID=").unwrap();
        let channel = env.find("AXIOMATA_EVENTS=").unwrap();
        assert!(channel > identity, "{env}");
        assert!(
            !env.contains("OPENCODE_CONFIG_DIR="),
            "gone with OC3: {env}"
        );
        // Nothing of ours lands in the worktree (E12).
        assert!(!ready.cwd.join(".opencode").exists());
        assert!(!ready.cwd.join(".claude").exists());

        let statuses = agent_statuses(
            &agent_store::list_agents(&db, project).unwrap(),
            &base.channels,
        );
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].agent_id, agent);
        assert_eq!(statuses[0].state, crate::lifecycle::AgentState::Starting);
        assert!(statuses[0].started_at.is_some());

        forget_channel(&base.channels, agent).unwrap();
        assert!(!base.channels.events.join(agent.to_string()).exists());
    }

    #[test]
    fn the_base_is_recorded_when_the_branch_is_born_and_found_again() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo.clone());
        let base = locations();
        let agent = add_agent(&db, project, "Builder");

        let ready = prepare(&db, &base, agent).unwrap();
        assert_eq!(ready.agent.base_branch.as_deref(), Some("main"));

        let repo_of = agent_repo(&db, agent).unwrap().ready().expect("a worktree");
        assert_eq!(repo_of.base_branch.as_deref(), Some("main"));
        std::fs::write(ready.cwd.join("new.txt"), "x").unwrap();
        let changes = repo_of.changes().unwrap();
        assert_eq!(changes.base.branch, "main");
        assert_eq!(changes.files.len(), 1);

        // Switching branches in the project folder does not move the base.
        run_git(&repo, &["switch", "--quiet", "-c", "elsewhere"]);
        prepare(&db, &base, agent).unwrap();
        let again = agent_store::get_agent(&db, agent).unwrap().unwrap();
        assert_eq!(again.base_branch.as_deref(), Some("main"));
    }

    #[test]
    fn a_shared_folder_agent_has_no_repo_of_its_own() {
        let (db, project) = db_with_project(temp_dir("plain"));
        let agent = add_agent(&db, project, "Builder");
        prepare(&db, &locations(), agent).unwrap();
        assert_eq!(
            agent_repo(&db, agent).unwrap(),
            AgentRepoState::SharedFolder
        );
    }

    #[test]
    fn preparing_a_re_attached_branch_does_not_record_a_base() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo);
        let base = locations();
        let agent = add_agent(&db, project, "Builder");
        let ready = prepare(&db, &base, agent).unwrap();
        assert_eq!(ready.agent.base_branch.as_deref(), Some("main"));

        // Simulate an agent from before base tracking existed: its branch is
        // still there in git (worktree removal never deletes it), but nothing
        // was ever recorded for it.
        discard_worktree(&db, agent, false).unwrap();
        db.execute(
            "UPDATE ide_agents SET base_branch = NULL WHERE id = ?1",
            [agent],
        )
        .unwrap();

        let again = prepare(&db, &base, agent).unwrap();
        assert!(
            again.agent.base_branch.is_none(),
            "re-attaching to a branch that already existed must not guess a base"
        );
    }

    #[test]
    fn a_busy_agent_cannot_have_its_work_taken_over() {
        let (db, project) = db_with_project(temp_dir("plain"));
        let base = locations();
        let id = add_agent(&db, project, "Builder");
        prepare(&db, &base, id).unwrap();
        let agent = agent_store::get_agent(&db, id).unwrap().unwrap();
        let state = base.channels.events.join(id.to_string()).join("state");

        assert!(
            ensure_not_busy(&base.channels, &agent).is_ok(),
            "starting is not busy"
        );
        for (word, busy) in [
            ("working", true),
            ("waiting", true),
            ("idle", false),
            ("ended", false),
        ] {
            std::fs::write(&state, format!("{word} 1\n")).unwrap();
            assert_eq!(
                ensure_not_busy(&base.channels, &agent).is_err(),
                busy,
                "{word}"
            );
        }
    }

    #[test]
    fn an_agent_in_a_repository_that_never_started_is_not_a_shared_folder() {
        let (db, project) = db_with_project(git_repo());
        let agent = add_agent(&db, project, "Builder");
        assert_eq!(agent_repo(&db, agent).unwrap(), AgentRepoState::NotStarted);
        assert!(agent_repo(&db, agent).unwrap().ready().is_err());
    }

    #[test]
    fn a_take_over_goes_through_the_busy_check_first() {
        let repo = git_repo();
        let (db, project) = db_with_project(repo.clone());
        let base = locations();
        let id = add_agent(&db, project, "Builder");
        let ready = prepare(&db, &base, id).unwrap();
        std::fs::write(ready.cwd.join("feature.txt"), "feature\n").unwrap();
        run_git(&ready.cwd, &["add", "-A"]);
        run_git(&ready.cwd, &["commit", "-m", "agent work"]);
        let state = base.channels.events.join(id.to_string()).join("state");

        std::fs::write(&state, "working 1\n").unwrap();
        let target = TakeOverTarget::read(&db, id).unwrap();
        let err = target
            .run(&base.channels, TakeOverMode::Squash, "Take over")
            .unwrap_err();
        assert!(err.to_string().contains("working"), "{err}");
        assert!(!repo.join("feature.txt").exists(), "nothing was taken over");

        std::fs::write(&state, "idle 1\n").unwrap();
        let done = target
            .run(&base.channels, TakeOverMode::Squash, "Take over")
            .unwrap();
        assert!(matches!(done, TakeOver::Done { .. }));
        assert!(repo.join("feature.txt").exists());
    }

    #[test]
    fn a_reviewer_gets_a_detached_checkout_of_the_snapshot_and_no_branch() {
        let repo = git_repo();
        let head = {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap();
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        let (db, project) = db_with_project(repo.clone());
        let reviewer = add_agent(&db, project, "reviewer-5");
        agent_store::set_card(&db, reviewer, Some(5), true, Some(&head)).unwrap();

        let places = locations();
        let ready = prepare(&db, &places, reviewer).unwrap();
        assert!(ready.cwd.is_dir());
        assert_eq!(ready.agent.branch, None, "it has no branch to write to");
        assert!(ready.agent.card_review);
        let at = std::process::Command::new("git")
            .arg("-C")
            .arg(&ready.cwd)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(at.stdout).unwrap().trim(), head);
        // Starting it again finds the same checkout.
        assert_eq!(
            prepare(&db, &places, reviewer).unwrap().agent.worktree_path,
            ready.agent.worktree_path
        );
    }
}
