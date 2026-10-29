//! `axiomata-cli harness watch --agent <id>`: a Glut session of the Studio followed read-only from its event log
//! (`../Axiomata-Glut/docs/plans/glut-tui.md`, TUI8d) — what the Flow tile's "Watch" opens beside the session's own
//! pane. A card's log is in its channel; a master's is the owner's newest Glut session in the project folder.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use axiomata_core::AxiomataCore;
use axiomata_core::ide::model::{Agent, Harness};

use crate::board_flow::owner_only;

/// Follows session `agent_id` until the owner quits.
pub async fn watch(core: &AxiomataCore, agent_id: i64) -> Result<()> {
    owner_only("watching a session")?;
    let (agent, folder) = {
        let db = core.db.lock().unwrap_or_else(|poison| poison.into_inner());
        let agent = axiomata_core::ide::agent_store::get_agent(&db, agent_id)?
            .context("no such session")?;
        let folder = axiomata_core::ide::store::get_project(&db, agent.project_id)?
            .context("the session's project is gone")?
            .repo_root;
        (agent, folder)
    };
    let home = glut::permissions::settings::home_dir().context("no home folder")?;
    let channels = axiomata_core::paths::ide_locations().channels;
    let channel = axiomata_core::ide::lifecycle::Channel::for_agent(&channels, agent_id);
    let log = log_of(&agent, &home, &folder, channel.dir())?;
    if glut_tui::watch(home, log).await != std::process::ExitCode::SUCCESS {
        bail!("the watch ended with an error");
    }
    Ok(())
}

/// The log session `agent` writes: a card's in its channel `channel`, a master's the newest of the owner's sessions in
/// the project `folder` (`home` holds `~/.glut`).
fn log_of(agent: &Agent, home: &Path, folder: &Path, channel: &Path) -> Result<PathBuf> {
    if agent.harness != Harness::Glut {
        bail!("only a Glut session keeps a log to watch");
    }
    if !agent.is_master() {
        return Ok(channel.join(axiomata_core::glut_card::EVENTS_FILE));
    }
    // Sessions are kept by their folder's full path, as the master started there names it. Chosen once, at the
    // start: a session the master opens later is watched by opening Watch again.
    let folder = std::fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    glut_tui::log::newest(home, &folder).context("the master has no session to watch yet")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("axiomata-watch-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn agent(dir: &Path, harness: Harness, master: bool) -> Agent {
        let db = axiomata_core::db::open_and_migrate_at(&dir.join("axiomata.db")).unwrap();
        let project = axiomata_core::ide::store::create_project(
            &db,
            axiomata_core::ide::NewProject {
                name: "P".into(),
                repo_root: dir.to_path_buf(),
            },
        )
        .unwrap();
        let made = axiomata_core::roster::master_session(&db, project.id, harness, &[]).unwrap();
        if !master {
            axiomata_core::ide::agent_store::set_role(&db, made.id, "allrounder").unwrap();
        }
        axiomata_core::ide::agent_store::get_agent(&db, made.id)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn a_card_is_watched_in_its_channel_and_a_master_in_the_owner_s_newest_session() {
        let dir = temp("logs");
        let home = dir.join("home");
        let channel = dir.join("channel");
        let card = agent(&temp("card"), Harness::Glut, false);
        assert_eq!(
            log_of(&card, &home, &dir, &channel).unwrap(),
            channel.join(axiomata_core::glut_card::EVENTS_FILE)
        );
        let master = agent(&temp("master"), Harness::Glut, true);
        assert!(
            log_of(&master, &home, &dir, &channel).is_err(),
            "no session yet"
        );
        let folder = std::fs::canonicalize(&dir).unwrap();
        let log =
            glut_tui::log::EventLog::new(glut_tui::log::Target::Sessions(home.clone()), folder);
        log.follow("s1");
        log.owner("Look at the parser");
        assert_eq!(
            log_of(&master, &home, &dir, &channel).unwrap(),
            glut_tui::log::session_log(&home, "s1")
        );
        let claude = agent(&temp("claude"), Harness::ClaudeCode, true);
        assert!(log_of(&claude, &home, &dir, &channel).is_err());
    }
}
