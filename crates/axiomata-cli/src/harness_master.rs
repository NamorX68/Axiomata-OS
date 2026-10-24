//! `axiomata-cli harness master`: a project's Glut master in its pane (`../Axiomata-Glut/docs/plans/glut-tui.md`,
//! TUI8c) — the owner's own `glut`, their `~/.glut` (providers, rules, skills, sessions), in the real project folder,
//! with the team server beside the owner's MCP servers (`glut --mcp-config`). No card, no budget, no worktree, no host
//! taking part: the studio only wrote the team server's file into the session's channel, found by the session's id.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::agent_entry::{self, HARNESS_RUN_ENV};
use axiomata_core::ide::lifecycle::Channel;
use axiomata_core::ide::model::{Agent, Harness};
use glut_tui::{Launch, Open, Servers};

/// Runs the master the environment names.
pub async fn master(core: &AxiomataCore) -> Result<()> {
    let agent_id: i64 = std::env::var("AXIOMATA_AGENT_ID")
        .ok()
        .and_then(|id| id.trim().parse().ok())
        .context("`harness master` runs in a master pane of the Studio, which names the session (AXIOMATA_AGENT_ID)")?;
    if std::env::var_os(HARNESS_RUN_ENV).is_some() {
        bail!("a skill or chat run does not start a master");
    }
    let (agent, folder) = {
        let db = core.db.lock().unwrap_or_else(|poison| poison.into_inner());
        let agent = axiomata_core::ide::agent_store::get_agent(&db, agent_id)?
            .context("no such session")?;
        let folder = axiomata_core::ide::store::get_project(&db, agent.project_id)?
            .context("the session's project is gone")?
            .repo_root;
        (agent, folder)
    };
    let roots = axiomata_core::paths::ide_locations().channels;
    // One pane per master, as per card session: a second one would hold a secret the first one's start already replaced.
    let _held = crate::harness_card::hold_session(&Channel::for_agent(&roots, agent_id))?;
    let launch = launch(
        &agent,
        folder,
        agent_entry::glut_master_config(&roots, agent_id),
    )?;
    // Glut said why on its way out; this only gives the pane its failing status.
    if glut_tui::standalone(launch, Open::default()).await != std::process::ExitCode::SUCCESS {
        bail!("the master ended with an error");
    }
    Ok(())
}

/// The owner's own session in `folder`, with the team server of `config` beside the owner's servers — refused for any
/// session but a Glut master, and without the file its start writes.
fn launch(agent: &Agent, folder: PathBuf, config: PathBuf) -> Result<Launch> {
    if agent.harness != Harness::Glut || !agent.is_master() {
        bail!("this session is no Glut master");
    }
    if !Path::new(&config).is_file() {
        bail!("the master's team server was not written; start it from the Studio");
    }
    let mut launch = Launch::owner(folder, None, None);
    launch.servers = Servers::Also(config);
    // The Studio holds the master in its project's folder: switching to another project is the Studio's (Glut T44).
    launch.projects = false;
    Ok(launch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(harness: Harness, role: &str) -> Agent {
        let dir = std::env::temp_dir().join(format!(
            "axiomata-master-{}-{harness:?}-{role}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let db = axiomata_core::db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
        let project = axiomata_core::ide::store::create_project(
            &db,
            axiomata_core::ide::NewProject {
                name: "P".into(),
                repo_root: dir.clone(),
            },
        )
        .unwrap();
        let made = axiomata_core::roster::master_session(&db, project.id, harness).unwrap();
        if role != axiomata_core::ide::model::MASTER_ROLE {
            axiomata_core::ide::agent_store::set_role(&db, made.id, role).unwrap();
        }
        axiomata_core::ide::agent_store::get_agent(&db, made.id)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn a_glut_master_is_the_owners_session_with_the_team_server_added() {
        let master = agent(Harness::Glut, axiomata_core::ide::model::MASTER_ROLE);
        let config =
            std::env::temp_dir().join(format!("axiomata-master-{}.json", std::process::id()));
        std::fs::write(&config, "{\"mcpServers\":{}}").unwrap();
        let folder = PathBuf::from("/work/project");
        let made = launch(&master, folder.clone(), config.clone()).unwrap();
        assert_eq!(made.cwd, folder);
        assert!(matches!(&made.servers, Servers::Also(file) if *file == config));
        assert!(made.owner_trust && made.keep_sessions && made.owner_shell && !made.mode_locked);
        assert!(!made.projects, "the Studio picks the master's folder");
        assert!(
            launch(&master, folder.clone(), config.with_extension("gone")).is_err(),
            "no file, no start"
        );
        let claude = agent(Harness::ClaudeCode, axiomata_core::ide::model::MASTER_ROLE);
        assert!(
            launch(&claude, folder.clone(), config.clone()).is_err(),
            "Claude Code's master is no Glut"
        );
        let card = agent(Harness::Glut, "allrounder");
        assert!(
            launch(&card, folder, config).is_err(),
            "a card session is no master"
        );
    }
}
