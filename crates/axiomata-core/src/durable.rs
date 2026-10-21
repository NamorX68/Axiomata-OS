//! Durable runs in the app (`docs/plans/durable.md`, CP3/CP4): a skill run's row is written when it starts, a run the
//! app lost is marked `interrupted` at the next start, and a Glut run that kept its journal goes on from its last step
//! on the owner's click — never by itself (D3).
//!
//! The journal itself is the harness's (`glut::journal`); the app keeps its journals apart from `axh`'s,
//! under `~/.axiomata/harness/runs`, and names each by the id it records in the run's `journal` column.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;

use crate::agents::RunJournal;
use crate::config::{Config, ProviderRole};
use crate::error::AxiomataError;
use crate::skills::model::{RunRecord, RunSource};
use crate::skills::{registry, runlog, runner};

/// Where the app's Glut runs keep their journals.
pub fn journal_dir() -> PathBuf {
    crate::paths::axiomata_home().join("harness").join("runs")
}

/// A journal id no other run of this or an earlier process had.
pub fn new_journal_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!(
        "run-{}-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// The ids of the unfinished journals there are.
fn kept_journals() -> Vec<String> {
    glut::journal::list(&journal_dir())
        .into_iter()
        .map(|unfinished| unfinished.run)
        .collect()
}

/// What the app does about lost runs when it starts: every `running` row whose process is gone becomes
/// `interrupted`, journals past their time are swept (D8), and a run whose journal is gone — swept, or another
/// harness's that never kept one — is no longer offered for a resume. Best effort: a failure is logged, never fatal.
pub fn on_start(db: &Mutex<Connection>) {
    let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
    match runlog::mark_interrupted(&conn) {
        Ok(0) => {}
        Ok(marked) => tracing::info!(marked, "runs the app lost are marked interrupted"),
        Err(err) => tracing::warn!(%err, "could not mark the lost runs"),
    }
    glut::journal::sweep(&journal_dir());
    if let Err(err) = runlog::forget_lost_journals(&conn, &kept_journals()) {
        tracing::warn!(%err, "could not tidy the interrupted runs");
    }
}

/// Runs `execute` — a skill or a routine's target — as the run `name`, its row written first and completed after
/// (CP3). With `journaled` the run is named a journal, so a Glut run can be resumed (CP4) — only a skill's run is,
/// it is rebuilt from its `SKILL.md`. A run refused before it started (already running) leaves no row.
///
/// # Errors
///
/// What `execute` returns, and a failure to write the row.
pub async fn recorded<F>(
    db: &Mutex<Connection>,
    name: &str,
    source: RunSource,
    journaled: bool,
    execute: F,
) -> Result<RunRecord, AxiomataError>
where
    F: Future<Output = Result<RunRecord, AxiomataError>>,
{
    let journal = journaled.then(new_journal_id);
    let id = {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        runlog::begin_run(&conn, name, source, journal.as_deref())?
    };
    let outcome = match journal {
        Some(journal) => {
            let run = RunJournal {
                id: journal,
                resume: false,
            };
            runner::RUN_JOURNAL.scope(run, execute).await
        }
        None => execute.await,
    };
    let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
    match outcome {
        Ok(mut record) => {
            record.source = source;
            runlog::finish_run(&conn, id, record)
        }
        Err(err) => {
            runlog::discard_run(&conn, id)?;
            Err(err)
        }
    }
}

/// Goes on with the interrupted run `id` from its journal (D3): only a skill run on Glut that kept one. The run's row
/// is `running` again while it works, and ends as any run does; a resume that cannot start leaves it `interrupted`.
///
/// # Errors
///
/// No such interrupted run, no journal (another harness, or past [`glut::journal::KEEP`]), a journal of
/// another run, the skill gone or no longer on Glut, the run claimed by another resume, the spend cap reached, or a
/// failure to write the row.
pub async fn resume_run(
    db: &Mutex<Connection>,
    config: &Config,
    id: i64,
) -> Result<RunRecord, AxiomataError> {
    let refused = |reason: String| AxiomataError::AgentApi {
        backend: "durable",
        message: reason,
    };
    let (journal, name, source) = {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        let journal = runlog::interrupted_journal(&conn, id)?.ok_or_else(|| {
            refused(format!(
                "run {id} is not an interrupted run that kept a journal"
            ))
        })?;
        let (name, source): (String, String) = conn.query_row(
            "SELECT skill_name, source FROM runs WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let source = RunSource::from_db_str(&source, 1)?;
        (journal, name, source)
    };
    let Some(unfinished) = glut::journal::list(&journal_dir())
        .into_iter()
        .find(|unfinished| unfinished.run == journal)
    else {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        runlog::forget_lost_journals(&conn, &kept_journals())?;
        return Err(refused(format!(
            "run {id} cannot be resumed: its journal is gone (only a Glut run keeps one, for 7 days)"
        )));
    };
    if unfinished.running {
        return Err(refused(format!("run {id} is going on already")));
    }
    // The journal must be this run's: a skill's, under the row's name.
    if unfinished.meta["kind"] != "skill" || unfinished.meta["name"].as_str() != Some(name.as_str())
    {
        return Err(refused(format!(
            "run {id}'s journal is not a run of {name}"
        )));
    }
    // The run goes on as the skill is now — but only on Glut: another harness would start it all over.
    let skill = registry::find_skill(&name)?;
    let harness = skill
        .backend
        .as_deref()
        .unwrap_or_else(|| config.agents.skill_harness.as_str());
    if harness != crate::config::HarnessId::Axh.as_str() {
        return Err(refused(format!(
            "{name} now runs on {harness}, not on Glut: only Glut can go on from the journal"
        )));
    }
    {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        crate::spend::guard_redirected_turn(&conn, config, ProviderRole::Skill)?;
        // Claimed once: a second click finds the row no longer interrupted and goes no further.
        let claimed = conn.execute(
            "UPDATE runs SET status = 'running', pid = ?2 WHERE id = ?1 AND status = 'interrupted'",
            rusqlite::params![id, std::process::id()],
        )?;
        if claimed != 1 {
            return Err(refused(format!("run {id} is being resumed already")));
        }
    }
    let run = RunJournal {
        id: journal,
        resume: true,
    };
    let outcome = runner::RUN_JOURNAL
        .scope(run, runner::execute_skill(&name, config))
        .await;
    let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
    match outcome {
        Ok(mut record) => {
            record.source = source;
            runlog::finish_run(&conn, id, record)
        }
        Err(err) => {
            conn.execute(
                "UPDATE runs SET status = 'interrupted' WHERE id = ?1 AND status = 'running'",
                [id],
            )?;
            Err(err)
        }
    }
}

/// Gives the interrupted run `id` up (the owner's choice): its journal is deleted, then the row forgets it and stays
/// `interrupted`. A journal a run still holds stays, and so does the row's pointer to it.
///
/// # Errors
///
/// A journal still held, or a failure to read or write the row.
pub fn discard(db: &Mutex<Connection>, id: i64) -> Result<(), AxiomataError> {
    let journal = {
        let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
        runlog::interrupted_journal(&conn, id)?
    };
    let Some(journal) = journal else {
        return Ok(());
    };
    // Gone already is fine: the journal past its time was swept.
    if kept_journals().contains(&journal) {
        glut::journal::discard(&journal_dir(), &journal).map_err(|message| {
            AxiomataError::AgentApi {
                backend: "durable",
                message,
            }
        })?;
    }
    let conn = db.lock().unwrap_or_else(|poison| poison.into_inner());
    conn.execute(
        "UPDATE runs SET journal = NULL WHERE id = ?1 AND status = 'interrupted'",
        [id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::model::RunStatus;
    use crate::test_support::{ENV_MUTEX, unique_temp_dir};

    /// The test's home and database; holds the environment lock for as long as the test lives.
    struct Fixture {
        _guard: std::sync::MutexGuard<'static, ()>,
        db: Mutex<Connection>,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let guard = ENV_MUTEX
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let home = unique_temp_dir(&format!("axiomata-test-durable-{tag}"));
            std::fs::create_dir_all(home.join("logs")).unwrap();
            // SAFETY: serialized by `guard`, see `paths::tests`.
            unsafe {
                std::env::set_var(crate::paths::AXIOMATA_HOME_ENV, &home);
            }
            let db = crate::db::open_and_migrate_at(&home.join("axiomata.db")).unwrap();
            Fixture {
                _guard: guard,
                db: Mutex::new(db),
            }
        }

        fn lost_run(&self, name: &str, journal: Option<&str>) -> i64 {
            let conn = self.db.lock().unwrap();
            let id = runlog::begin_run(&conn, name, RunSource::Manual, journal).unwrap();
            conn.execute("UPDATE runs SET pid = 999999999 WHERE id = ?1", [id])
                .unwrap();
            runlog::mark_interrupted(&conn).unwrap();
            id
        }

        fn status(&self, id: i64) -> RunStatus {
            runlog::get_run(&self.db.lock().unwrap(), id)
                .unwrap()
                .unwrap()
                .status
        }
    }

    fn write_journal(run: &str, meta: serde_json::Value) {
        let spec = glut::journal::JournalSpec {
            dir: journal_dir(),
            run: run.to_owned(),
            meta,
        };
        drop(glut::journal::Journal::create(&spec, std::path::Path::new("/w"), "m").unwrap());
    }

    fn resumable(fx: &Fixture) -> Vec<bool> {
        runlog::list_runs(&fx.db.lock().unwrap(), 10)
            .unwrap()
            .iter()
            .map(|run| run.resumable)
            .collect()
    }

    /// A run refused before it started leaves no row; one that ran is completed with its own source.
    #[tokio::test]
    async fn a_recorded_run_is_completed_or_taken_back() {
        let fx = Fixture::new("recorded");
        let refused = recorded(&fx.db, "racer", RunSource::Manual, true, async {
            Err(AxiomataError::AlreadyRunning {
                name: "racer".into(),
            })
        })
        .await;
        assert!(refused.is_err());
        assert!(
            runlog::list_runs(&fx.db.lock().unwrap(), 10)
                .unwrap()
                .is_empty()
        );

        let journaled = recorded(&fx.db, "news-web", RunSource::Routine, true, async {
            assert!(runner::RUN_JOURNAL.try_with(|run| !run.resume).unwrap());
            Ok(RunRecord {
                skill_name: "news-web".into(),
                ..sample()
            })
        })
        .await
        .unwrap();
        assert_eq!(
            (journaled.status, journaled.source),
            (RunStatus::Success, RunSource::Routine)
        );
        let plain = recorded(&fx.db, "prompt-routine", RunSource::Routine, false, async {
            assert!(
                runner::RUN_JOURNAL.try_with(|_| ()).is_err(),
                "no journal for a prompt target"
            );
            Ok(sample())
        })
        .await
        .unwrap();
        assert!(plain.id.is_some());
    }

    /// A lost run without a journal — another harness's, or one swept — is not offered; one with its journal is, and a
    /// journal of another run or a skill no longer on Glut is refused without touching the row.
    #[tokio::test]
    async fn only_a_lost_glut_run_with_its_own_journal_is_offered_and_resumed() {
        let fx = Fixture::new("resume");
        let opencode = fx.lost_run("calendar-digest", Some("run-oc"));
        let glut = fx.lost_run("news-web", Some("run-glut"));
        write_journal(
            "run-glut",
            serde_json::json!({"kind": "skill", "name": "other-skill"}),
        );
        on_start(&fx.db);
        assert_eq!(resumable(&fx), [true, false]);

        // The journal is not this run's: refused, and the row stays as it was.
        let config = Config::default();
        let err = resume_run(&fx.db, &config, glut).await.unwrap_err();
        assert!(err.to_string().contains("not a run of news-web"), "{err}");
        assert_eq!(fx.status(glut), RunStatus::Interrupted);
        assert!(resume_run(&fx.db, &config, opencode).await.is_err());

        // Given up: the journal goes, the row stays interrupted and is offered no more.
        discard(&fx.db, glut).unwrap();
        assert!(kept_journals().is_empty());
        assert_eq!(fx.status(glut), RunStatus::Interrupted);
        assert_eq!(resumable(&fx), [false, false]);
    }

    fn sample() -> RunRecord {
        let now = chrono::Utc::now();
        RunRecord {
            id: None,
            skill_name: "s".into(),
            backend: "axh".into(),
            status: RunStatus::Success,
            exit_code: Some(0),
            duration_ms: 1,
            stdout: String::new(),
            stderr: String::new(),
            error: None,
            started_at: now,
            finished_at: now,
            provider: None,
            cost_usd: None,
            input_tokens: None,
            cached_input_tokens: None,
            output_tokens: None,
            num_turns: None,
            model: None,
            source: RunSource::Manual,
        }
    }
}
